//! A small local collection; directory searches run only when explicitly requested.
use crate::{
    artwork::ArtworkCache,
    backdrop::Backdrop,
    radio::{CustomStation, Station, STATIONS},
    ui::{self, Icon, MUTED, RED, WHITE},
};
use eframe::egui::{self, vec2, Align, Layout, RichText, Stroke};
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};

const MAX_STATIONS: usize = 200;
const MAX_FILE: u64 = 512 * 1024;
const MAX_RESULTS: usize = 40;

#[derive(Default, Deserialize)]
#[serde(default)]
struct Entry {
    stationuuid: String,
    name: String,
    url_resolved: String,
    countrycode: String,
    favicon: String,
    codec: String,
    hls: u8,
    lastcheckok: u8,
}
fn valid(station: &Station) -> bool {
    match station {
        Station::Custom(station) => {
            !station.id.is_empty()
                && station.id.len() <= 80
                && !station.name.trim().is_empty()
                && station.name.len() <= 256
                && !station.name.chars().any(char::is_control)
                && station.country.len() <= 80
                && station.favicon.len() <= 2048
                && crate::radio_net::allowed(&station.stream_url)
        }
        _ => true,
    }
}
fn same(a: &Station, b: &Station) -> bool {
    if a == b || a.url().trim_end_matches('/') == b.url().trim_end_matches('/') {
        return true;
    }
    match (a, b) {
        (Station::Custom(a), Station::Custom(b)) => a.id == b.id,
        // National stations already shipped by Tonal should not reappear as duplicates.
        (Station::Nrj, other) | (other, Station::Nrj) => {
            other.name().trim().eq_ignore_ascii_case("nrj")
        }
        (Station::Skyrock, other) | (other, Station::Skyrock) => {
            other.name().trim().eq_ignore_ascii_case("skyrock")
        }
    }
}
fn parse_results(bytes: &[u8]) -> Result<Vec<Station>, String> {
    let entries: Vec<Entry> =
        serde_json::from_slice(bytes).map_err(|_| "Réponse de recherche non reconnue.")?;
    let mut stations = Vec::new();
    for entry in entries.into_iter().take(100) {
        if entry.hls != 0 || entry.lastcheckok != 1 || !entry.codec.eq_ignore_ascii_case("mp3") {
            continue;
        }
        let station = Station::Custom(Arc::new(CustomStation {
            id: entry.stationuuid,
            name: entry.name.trim().into(),
            stream_url: entry.url_resolved,
            country: entry.countrycode,
            favicon: entry.favicon,
        }));
        if valid(&station) && !stations.iter().any(|other| same(other, &station)) {
            stations.push(station);
        }
        if stations.len() >= MAX_RESULTS {
            break;
        }
    }
    Ok(stations)
}
async fn read_limited(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err("Réponse de recherche trop volumineuse.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Recherche interrompue.")?
    {
        if bytes.len() + chunk.len() > limit {
            return Err("Réponse de recherche trop volumineuse.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn search(query: &str) -> Result<Vec<Station>, String> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(7))
        .user_agent(concat!("Tonal/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "Recherche indisponible.")?;
    // The round-robin host discovers current mirrors, rather than binding the app to one server.
    let discovery = client
        .get("https://all.api.radio-browser.info/json/servers")
        .send()
        .await;
    let mut mirrors = Vec::<String>::new();
    if let Ok(response) = discovery.and_then(reqwest::Response::error_for_status) {
        if let Ok(bytes) = read_limited(response, 32768).await {
            if let Ok(entries) = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes) {
                for name in entries
                    .iter()
                    .filter_map(|entry| entry.get("name").and_then(|name| name.as_str()))
                {
                    if name.len() < 100
                        && name.ends_with(".api.radio-browser.info")
                        && name
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
                        && !mirrors.iter().any(|mirror| mirror == name)
                    {
                        mirrors.push(name.into());
                    }
                }
            }
        }
    }
    if mirrors.is_empty() {
        mirrors.push("de1.api.radio-browser.info".into());
    }
    fastrand::shuffle(&mut mirrors);
    for mirror in mirrors.into_iter().take(3) {
        let response = client
            .get(format!("https://{mirror}/json/stations/search"))
            .query(&[
                ("name", query),
                ("codec", "MP3"),
                ("hidebroken", "true"),
                ("limit", "60"),
                ("order", "clickcount"),
                ("reverse", "true"),
            ])
            .send()
            .await;
        if let Ok(response) = response.and_then(reqwest::Response::error_for_status) {
            if let Ok(bytes) = read_limited(response, 512 * 1024).await {
                return parse_results(&bytes);
            }
        }
    }
    Err("Recherche indisponible. Réessaie dans un instant.".into())
}

pub struct Library {
    pub stations: Vec<Station>,
    pub open: bool,
    query: String,
    results: Vec<Station>,
    pending: Option<Receiver<Result<Vec<Station>, String>>>,
    task: Option<tokio::task::JoinHandle<()>>,
    search_at: Option<Instant>,
    focused: bool,
    searched: bool,
    pub error: Option<String>,
    path: Option<PathBuf>,
}
impl Drop for Library {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
impl Library {
    #[cfg(feature = "ui-capture")]
    pub fn qa_search(&mut self, ctx: &egui::Context) {
        self.open();
        self.query = "FIP".into();
        self.start_search(ctx);
    }
    pub fn load() -> Self {
        let path = std::env::var_os("LOCALAPPDATA")
            .map(|root| PathBuf::from(root).join("Tonal").join("radios.json"));
        let stations = path
            .as_deref()
            .and_then(|path| load_file(path).ok())
            .unwrap_or_else(|| STATIONS.to_vec());
        Self {
            stations,
            path,
            open: false,
            query: String::new(),
            results: Vec::new(),
            pending: None,
            task: None,
            search_at: None,
            focused: false,
            searched: false,
            error: None,
        }
    }
    pub fn open(&mut self) {
        self.open = true;
        self.focused = false;
        self.error = None;
    }
    pub fn close(&mut self) {
        self.open = false;
        self.cancel_search();
        self.search_at = None;
    }
    fn cancel_search(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.pending = None;
    }
    pub fn contains(&self, station: &Station) -> bool {
        self.stations.iter().any(|other| same(other, station))
    }
    pub fn add(&mut self, station: Station) -> Result<bool, String> {
        if !valid(&station) {
            return Err("Cette station ne peut pas être ajoutée.".into());
        }
        if self.contains(&station) {
            return Ok(false);
        }
        if self.stations.len() >= MAX_STATIONS {
            return Err("Ta collection est limitée à 200 stations.".into());
        }
        let mut stations = self.stations.clone();
        stations.push(station);
        self.persist(&stations)?;
        self.stations = stations;
        Ok(true)
    }
    pub fn remove(&mut self, station: &Station) -> Result<(), String> {
        let stations: Vec<_> = self
            .stations
            .iter()
            .filter(|other| !same(other, station))
            .cloned()
            .collect();
        self.persist(&stations)?;
        self.stations = stations;
        Ok(())
    }
    fn persist(&self, stations: &[Station]) -> Result<(), String> {
        #[cfg(feature = "ui-capture")]
        if std::env::var_os("TONAL_CAPTURE_TO").is_some() {
            return Ok(());
        }
        save_file(
            self.path
                .as_deref()
                .ok_or("Dossier des radios indisponible.")?,
            stations,
        )
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(result) = self
            .pending
            .as_ref()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Recherche interrompue. Réessaie.".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            })
        {
            self.pending = None;
            self.task = None;
            self.searched = true;
            match result {
                Ok(stations) => self.results = stations,
                Err(error) => self.error = Some(error),
            }
            ctx.request_repaint();
        }
        if self.open && self.search_at.is_some_and(|at| Instant::now() >= at) {
            self.search_at = None;
            self.start_search(ctx);
        }
    }
    fn start_search(&mut self, ctx: &egui::Context) {
        self.cancel_search();
        let query = self.query.trim().to_owned();
        if query.chars().count() < 2 {
            return;
        }
        self.error = None;
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        let Ok(runtime) = crate::http_audio::network_runtime() else {
            self.error = Some("Recherche indisponible.".into());
            return;
        };
        self.pending = Some(receiver);
        self.task = Some(runtime.spawn(async move {
            let result = tokio::time::timeout(Duration::from_secs(20), search(&query))
                .await
                .unwrap_or_else(|_| Err("La recherche prend trop de temps. Réessaie.".into()));
            let _ = sender.send(result);
            ctx.request_repaint();
        }));
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        backdrop: &mut Backdrop,
        artwork: &mut ArtworkCache,
    ) {
        if !self.open {
            return;
        }
        let id = egui::Id::unique("radio_add_modal");
        backdrop.paint(ctx, egui::LayerId::new(egui::Order::Foreground, id));
        let mut close = false;
        let mut add = None;
        let mut request = false;
        let response = egui::Modal::new(id)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_gray(13))
                    .inner_margin(22)
                    .corner_radius(20)
                    .stroke(Stroke::new(1.0, egui::Color32::from_gray(39))),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().scroll.dormant_background_opacity = 0.0;
                ui.spacing_mut().scroll.active_background_opacity = 0.0;
                ui.spacing_mut().scroll.interact_background_opacity = 0.0;
                ui.set_width(440.0_f32.min(ctx.content_rect().width() - 88.0));
                ui.horizontal(|ui| {
                    ui.label(ui::heading("Ajouter une radio", 21.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        close =
                            ui::icon_button(ui, Icon::Close, "Fermer", 30.0, false, true).clicked();
                    });
                });
                ui.add_space(14.0);
                let input = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Rechercher une station…")
                        .desired_width(ui.available_width())
                        .min_size(vec2(0.0, 38.0))
                        .margin(vec2(12.0, 9.0))
                        .char_limit(100),
                );
                if !self.focused {
                    input.request_focus();
                    self.focused = true;
                }
                if input.changed() {
                    self.cancel_search();
                    self.results.clear();
                    self.error = None;
                    self.searched = false;
                    self.search_at = (self.query.trim().chars().count() >= 2)
                        .then(|| Instant::now() + Duration::from_millis(450));
                }
                if input.lost_focus() && ctx.input(|input| input.key_pressed(egui::Key::Enter)) {
                    self.search_at = None;
                    request = true;
                }
                if let Some(at) = self.search_at {
                    ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
                }
                ui.add_space(12.0);
                if let Some(error) = &self.error {
                    ui.label(RichText::new(error).size(13.0).color(RED));
                    if ui::button(ui, "Réessayer", Icon::Refresh, ui::ButtonKind::Quiet, true)
                        .clicked()
                    {
                        request = true;
                    }
                } else if self.pending.is_some() || self.search_at.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Recherche…").size(13.0).color(MUTED));
                    });
                } else if self.results.is_empty() {
                    ui.label(
                        RichText::new(if self.searched {
                            "Aucune station compatible trouvée."
                        } else {
                            "Cherche une station par son nom, puis ajoute-la à ta collection."
                        })
                        .size(13.0)
                        .color(MUTED),
                    );
                }
                let results = self.results.clone();
                let result_height = (results.len() as f32 * 72.0)
                    .min((ctx.content_rect().height() - 280.0).clamp(100.0, 340.0));
                egui::ScrollArea::vertical()
                    .id_salt("radio_search_results")
                    .max_height(result_height)
                    .min_scrolled_height(result_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (_index, station) in results.iter().enumerate() {
                            let added = self.contains(station);
                            let (row, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 62.0),
                                egui::Sense::hover(),
                            );
                            {
                                let mut row_ui = ui.new_child(egui::UiBuilder::new().max_rect(row));
                                let ui = &mut row_ui;
                                let cover = egui::Rect::from_min_size(
                                    row.min + vec2(0.0, 11.0),
                                    vec2(40.0, 40.0),
                                );
                                paint_station(station, ui, cover, artwork);
                                let action = egui::Rect::from_center_size(
                                    egui::pos2(row.right() - 32.0, row.center().y),
                                    vec2(32.0, 32.0),
                                );
                                let text = egui::Rect::from_min_max(
                                    cover.min + vec2(50.0, 0.0),
                                    egui::pos2(action.left() - 10.0, cover.bottom()),
                                );
                                let mut labels =
                                    ui.new_child(egui::UiBuilder::new().max_rect(text));
                                labels.add(
                                    egui::Label::new(
                                        RichText::new(station.name())
                                            .size(14.0)
                                            .strong()
                                            .color(WHITE),
                                    )
                                    .truncate()
                                    .show_tooltip_when_elided(false),
                                );
                                if let Station::Custom(station) = station {
                                    labels.label(
                                        RichText::new(&station.country).size(12.0).color(MUTED),
                                    );
                                }
                                let mut action_ui = ui.new_child(
                                    egui::UiBuilder::new()
                                        .scope_id(ui.make_persistent_id(("radio_result", _index)))
                                        .max_rect(action),
                                );
                                if added {
                                    ui::icon(ui.painter(), Icon::Check, action.shrink(6.0), WHITE);
                                    continue;
                                }
                                let response = ui::icon_button(
                                    &mut action_ui,
                                    Icon::Plus,
                                    "Ajouter",
                                    32.0,
                                    false,
                                    true,
                                );
                                #[cfg(feature = "ui-capture")]
                                if _index == 0 && !added {
                                    ctx.data_mut(|data| {
                                        data.insert_temp(
                                            egui::Id::unique("qa_radio_add_result"),
                                            response.rect,
                                        )
                                    });
                                }
                                if response.clicked() {
                                    add = Some(station.clone());
                                }
                            }
                        }
                    });
            });
        if request {
            self.start_search(ctx);
        }
        if let Some(station) = add {
            if let Err(error) = self.add(station) {
                self.error = Some(error);
            }
            ctx.request_repaint();
        }
        if close || response.should_close() {
            self.close();
        }
    }
}
pub fn paint_station(
    station: &Station,
    ui: &egui::Ui,
    rect: egui::Rect,
    artwork: &mut ArtworkCache,
) {
    station.paint(ui, rect);
    if ui.is_rect_visible(rect) {
        if let Station::Custom(station) = station {
            if let Some(texture) = artwork.radio_texture(ui.ctx(), &station.favicon) {
                crate::artwork::paint_texture(ui, rect, &texture, 8);
            }
        }
    }
}
fn load_file(path: &Path) -> Result<Vec<Station>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "Collection indisponible.")?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Collection illisible.")?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("Collection trop volumineuse.".into());
    }
    let stations: Vec<Station> =
        serde_json::from_slice(&bytes).map_err(|_| "Collection illisible.")?;
    if stations.len() > MAX_STATIONS || stations.iter().any(|station| !valid(station)) {
        return Err("Collection invalide.".into());
    }
    let mut result = Vec::new();
    for station in stations {
        if !result.iter().any(|other| same(other, &station)) {
            result.push(station);
        }
    }
    Ok(result)
}
fn save_file(path: &Path, stations: &[Station]) -> Result<(), String> {
    use std::io::Write;
    let bytes = serde_json::to_vec(stations).map_err(|_| "Impossible d'enregistrer les radios.")?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("Collection trop volumineuse.".into());
    }
    let parent = path.parent().ok_or("Dossier des radios indisponible.")?;
    std::fs::create_dir_all(parent).map_err(|_| "Impossible d'enregistrer les radios.")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Impossible d'enregistrer les radios.")?;
    file.write_all(&bytes)
        .map_err(|_| "Impossible d'enregistrer les radios.")?;
    file.persist(path)
        .map_err(|_| "Impossible d'enregistrer les radios.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn station() -> Station {
        Station::Custom(Arc::new(CustomStation {
            id: "station-fip".into(),
            name: "FIP".into(),
            stream_url: "https://stream.radiofrance.fr/fip.mp3".into(),
            country: "FR".into(),
            favicon: String::new(),
        }))
    }
    #[test]
    fn collection_round_trip_preserves_empty_collection_and_prevents_duplicate_sources() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("radios.json");
        let mut library = Library::load();
        library.path = Some(path.clone());
        library.stations = STATIONS.to_vec();
        assert!(library.add(station()).unwrap());
        let mut duplicate = match station() {
            Station::Custom(station) => (*station).clone(),
            _ => unreachable!(),
        };
        duplicate.id = "other-uuid".into();
        assert!(!library.add(Station::Custom(Arc::new(duplicate))).unwrap());
        assert_eq!(load_file(&path).unwrap(), library.stations);
        for station in library.stations.clone() {
            library.remove(&station).unwrap();
        }
        assert!(load_file(&path).unwrap().is_empty());
        std::fs::write(&path, br#"[{"Custom":{"id":"x","name":"Unsafe","stream_url":"http://127.0.0.1/","country":"","favicon":""}}]"#).unwrap();
        assert!(load_file(&path).is_err());
    }
    #[test]
    fn directory_filters_hls_bad_codecs_private_urls_and_duplicate_ids() {
        let entry = serde_json::json!({"stationuuid":"fip", "name":"FIP", "url_resolved":"https://stream.radiofrance.fr/fip.mp3", "codec":"MP3", "lastcheckok":1});
        let mut hls = entry.clone();
        hls["hls"] = 1.into();
        let mut local = entry.clone();
        local["url_resolved"] = "http://localhost/audio".into();
        let mut aac = entry.clone();
        aac["codec"] = "AAC".into();
        let mut broken = entry.clone();
        broken["lastcheckok"] = 0.into();
        let bytes =
            serde_json::to_vec(&vec![entry.clone(), entry, hls, local, aac, broken]).unwrap();
        assert_eq!(parse_results(&bytes).unwrap().len(), 1);
    }
    #[test]
    #[ignore = "Réseau : recherche un nombre borné de radios et décode une station ajoutable sans son ni fichier musical."]
    fn directory_station_search_and_custom_stream_decode_live() {
        let stations = crate::http_audio::network_runtime()
            .unwrap()
            .block_on(search("FIP"))
            .unwrap();
        assert!(!stations.is_empty() && stations.len() <= MAX_RESULTS);
        let station = stations.into_iter().next().unwrap();
        let prepared = crate::radio::prepare(
            station.clone(),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            egui::Context::default(),
        )
        .unwrap();
        let mut source = prepared.source;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut heard = false;
        while Instant::now() < deadline && !heard {
            heard = source
                .by_ref()
                .take(4096)
                .any(|sample| sample.abs() > 0.00001);
            if !heard {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        assert!(heard, "{} must decode into actual audio", station.name());
        eprintln!(
            "{} : recherche et décodage natif validés, sortie muette, aucun fichier musical.",
            station.name()
        );
        drop(prepared.control);
    }
}
