//! Live stations, isolated from Google. Bounded compressed/PCM queues, no audio files.
use eframe::egui;
use rodio::{Decoder, Source};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Station {
    Nrj,
    Skyrock,
    Custom(Arc<CustomStation>),
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CustomStation {
    pub id: String,
    pub name: String,
    pub stream_url: String,
    pub country: String,
    pub favicon: String,
}
pub const STATIONS: [Station; 2] = [Station::Nrj, Station::Skyrock];
impl Station {
    pub fn song_title(&self, metadata: &str) -> Option<String> {
        let (artist, title) = [" - ", " — ", " – "]
            .into_iter()
            .find_map(|separator| metadata.split_once(separator))?;
        let artist = artist.trim();
        let title = title.trim();
        if artist.is_empty() || title.is_empty() {
            return None;
        }
        let station = self.name().to_lowercase();
        let label = artist.to_lowercase();
        // A station name followed by a show name is not an artist / song pair.
        if station == label
            || station.starts_with(&format!("{label} "))
            || station.ends_with(&format!(" {label}"))
            || label.starts_with(&format!("{station} "))
            || ["nrj", "skyrock", "publicité", "publicite", "unknown"].contains(&label.as_str())
            || metadata.to_lowercase().contains("émission")
            || metadata.to_lowercase().contains("emission")
        {
            return None;
        }
        Some(format!("{artist} — {title}"))
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Nrj => "NRJ",
            Self::Skyrock => "Skyrock",
            Self::Custom(station) => &station.name,
        }
    }
    pub fn url(&self) -> &str {
        match self {
            Self::Nrj => "https://streaming.nrjaudio.fm/oumvmk8fnozc?origine=redirection_https_nrj",
            Self::Skyrock => "https://icecast.skyrock.net/s/natio_mp3_128k",
            Self::Custom(station) => &station.stream_url,
        }
    }
    fn logo(&self) -> &'static [u8] {
        match self {
            Self::Nrj => include_bytes!("../assets/brands/nrj.png"),
            Self::Skyrock => include_bytes!("../assets/brands/skyrock.svg"),
            Self::Custom(_) => &[],
        }
    }
    pub fn paint(&self, ui: &egui::Ui, rect: egui::Rect) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        if matches!(self, Self::Custom(_)) {
            ui.painter()
                .rect_filled(rect, 8, egui::Color32::from_gray(32));
            let initials: String = self
                .name()
                .split_whitespace()
                .filter_map(|word| word.chars().next())
                .take(2)
                .collect();
            let first = self.name().split_whitespace().next().unwrap_or("");
            let initials = if first.chars().count() <= 4 {
                first.to_uppercase()
            } else {
                initials.to_uppercase()
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                initials,
                egui::FontId::proportional(rect.height() * 0.3),
                crate::ui::WHITE,
            );
            return;
        }
        // Fixed textures avoid accumulating a new large image at every resize.
        let id = egui::Id::unique(("radio-logo", self));
        let texture = ui
            .ctx()
            .data_mut(|data| data.get_temp::<egui::TextureHandle>(id))
            .unwrap_or_else(|| {
                let image = if self == &Self::Skyrock {
                    egui_extras::image::load_svg_bytes_with_size(
                        self.logo(),
                        egui::load::SizeHint::Height(512),
                        &Default::default(),
                    )
                    .expect("Bundled radio SVG")
                } else {
                    let image = image::load_from_memory(self.logo())
                        .expect("Bundled radio logo")
                        .to_rgba8();
                    let mut bounds = (image.width(), image.height(), 0, 0);
                    for (x, y, _) in image
                        .enumerate_pixels()
                        .filter(|(_, _, pixel)| pixel[3] > 8)
                    {
                        bounds = (
                            bounds.0.min(x),
                            bounds.1.min(y),
                            bounds.2.max(x + 1),
                            bounds.3.max(y + 1),
                        );
                    }
                    let image = image::imageops::crop_imm(
                        &image,
                        bounds.0,
                        bounds.1,
                        bounds.2 - bounds.0,
                        bounds.3 - bounds.1,
                    )
                    .to_image();
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    )
                };
                let texture = ui.ctx().load_texture(
                    self.name(),
                    image,
                    egui::TextureOptions {
                        mipmap_mode: Some(egui::TextureFilter::Linear),
                        ..egui::TextureOptions::LINEAR
                    },
                );
                ui.ctx()
                    .data_mut(|data| data.insert_temp(id, texture.clone()));
                texture
            });
        ui.painter().rect_filled(rect, 8, egui::Color32::WHITE);
        let size = texture.size_vec2();
        let scale = (rect.width() / size.x).min(rect.height() / size.y) * 0.82;
        ui.painter().image(
            texture.id(),
            egui::Rect::from_center_size(rect.center(), size * scale),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}
fn allowed(url: &url::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.host_str().is_some_and(|host| {
            ["nrjaudio.fm", "skyrock.net"]
                .iter()
                .any(|base| host == *base || host.ends_with(&format!(".{base}")))
        })
}
struct Shared {
    cancel: Arc<AtomicBool>,
    ended: AtomicBool,
    buffering: AtomicBool,
    error: Mutex<Option<String>>,
    now_playing: Mutex<Option<String>>,
}
pub struct Control {
    shared: Arc<Shared>,
}
impl Control {
    pub fn now_playing(&self) -> Option<String> {
        self.shared.now_playing.lock().unwrap().clone()
    }
    pub fn buffering(&self) -> bool {
        self.shared.buffering.load(Ordering::Relaxed)
    }
    pub fn take_error(&self) -> Option<String> {
        self.shared.error.lock().unwrap().take()
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        self.shared.cancel.store(true, Ordering::SeqCst);
    }
}
struct Frame {
    samples: Vec<f32>,
    channels: u16,
    rate: u32,
}
pub struct Output {
    receiver: Receiver<Frame>,
    shared: Arc<Shared>,
    frame: Frame,
    at: usize,
}
impl Output {
    fn refill(&mut self) {
        match self.receiver.try_recv() {
            Ok(frame) => {
                self.frame = frame;
                self.shared.buffering.store(false, Ordering::Relaxed);
            }
            Err(_) => {
                self.frame.samples.clear();
                self.frame
                    .samples
                    .resize(512 * usize::from(self.frame.channels), 0.0);
                self.shared.buffering.store(true, Ordering::Relaxed);
            }
        }
        self.at = 0;
    }
}
impl Iterator for Output {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.shared.cancel.load(Ordering::SeqCst) || self.shared.ended.load(Ordering::Relaxed) {
            return None;
        }
        if self.at >= self.frame.samples.len() {
            self.refill();
        }
        let sample = self.frame.samples[self.at];
        self.at += 1;
        if self.at == self.frame.samples.len() {
            self.refill();
        }
        Some(sample)
    }
}
impl Source for Output {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.frame.samples.len().saturating_sub(self.at).max(1))
    }
    fn channels(&self) -> u16 {
        self.frame.channels
    }
    fn sample_rate(&self) -> u32 {
        self.frame.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
pub struct Prepared {
    pub control: Control,
    pub source: Output,
}

// ICY blocks are at most 4080 bytes. None means no update; Some(None) clears a stale title.
fn metadata_title(bytes: &[u8]) -> Option<Option<String>> {
    let bytes = bytes.split(|byte| *byte == 0).next()?;
    let text = std::str::from_utf8(bytes)
        .map(str::to_owned)
        .unwrap_or_else(|_| {
            bytes
                .iter()
                .map(|byte| match byte {
                    0x80 => '€',
                    0x85 => '…',
                    0x91 | 0x92 => '’',
                    0x93 => '“',
                    0x94 => '”',
                    0x96 => '–',
                    0x97 => '—',
                    0x9c => 'œ',
                    _ => char::from(*byte),
                })
                .collect()
        });
    let lower = text.to_ascii_lowercase();
    let start = lower.find("streamtitle=")? + "streamtitle=".len();
    let rest = text[start..].trim_start();
    let quote = rest
        .chars()
        .next()
        .filter(|quote| matches!(quote, '\'' | '"'))?;
    let rest = &rest[quote.len_utf8()..];
    let end = rest
        .find(&format!("{quote};"))
        .or_else(|| rest.strip_suffix(quote).map(str::len))?;
    let value: String = rest[..end]
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .take(256)
        .collect();
    let value = value
        .replace("&amp;", "&")
        .replace("&apos;", "'")
        .replace("&quot;", "\"");
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let value = match value.rsplit_once('§') {
        Some((title, id))
            if id.trim().len() >= 5 && id.trim().chars().all(|c| c.is_ascii_digit()) =>
        {
            title.trim_end().to_owned()
        }
        _ => value,
    };
    // Some broadcasters send internal numeric IDs during ads, not artist/track names.
    let numeric_ids = value.split_once(" - ").is_some_and(|(a, b)| {
        !a.is_empty()
            && !b.is_empty()
            && a.chars().all(|c| c.is_ascii_digit())
            && b.chars().all(|c| c.is_ascii_digit())
    });
    if value.is_empty() || numeric_ids || value.eq_ignore_ascii_case("unknown - unknown") {
        Some(None)
    } else {
        Some(Some(value))
    }
}

enum Packet {
    Header(usize),
    Data(Vec<u8>),
}
struct Reader {
    receiver: Mutex<Receiver<Packet>>,
    shared: Arc<Shared>,
    attempt: Arc<AtomicBool>,
    data: Vec<u8>,
    at: usize,
    meta_interval: usize,
    remaining: usize,
    position: u64,
    initialized: bool,
}
impl Reader {
    fn raw(&mut self, output: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.shared.cancel.load(Ordering::SeqCst) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            if self.at < self.data.len() {
                let count = output.len().min(self.data.len() - self.at);
                output[..count].copy_from_slice(&self.data[self.at..self.at + count]);
                self.at += count;
                return Ok(count);
            }
            let packet = self
                .receiver
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_millis(100));
            match packet {
                Ok(Packet::Data(data)) => {
                    self.data = data;
                    self.at = 0;
                }
                Ok(Packet::Header(interval)) => {
                    self.meta_interval = interval;
                    self.remaining = interval;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
    }
    fn read_metadata(&mut self) -> io::Result<()> {
        let mut length = [0];
        if self.raw(&mut length)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let mut bytes = vec![0; usize::from(length[0]) * 16];
        let mut at = 0;
        while at < bytes.len() {
            let n = self.raw(&mut bytes[at..])?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            at += n;
        }
        if let Some(title) = metadata_title(&bytes) {
            *self.shared.now_playing.lock().unwrap() = title;
        }
        self.remaining = self.meta_interval;
        Ok(())
    }
}
impl Read for Reader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        while !self.initialized {
            if self.shared.cancel.load(Ordering::SeqCst) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            match self
                .receiver
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_millis(100))
            {
                Ok(Packet::Header(interval)) => {
                    self.meta_interval = interval;
                    self.remaining = interval;
                    self.initialized = true;
                }
                Ok(Packet::Data(_)) => return Err(io::ErrorKind::InvalidData.into()),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        if self.meta_interval > 0 && self.remaining == 0 {
            self.read_metadata()?;
        }
        let wanted = if self.meta_interval > 0 {
            output.len().min(self.remaining)
        } else {
            output.len()
        };
        let n = self.raw(&mut output[..wanted])?;
        if self.meta_interval > 0 {
            self.remaining = self.remaining.saturating_sub(n);
        }
        self.position += n as u64;
        Ok(n)
    }
}
impl Seek for Reader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        if matches!(from, SeekFrom::Current(0))
            || (matches!(from, SeekFrom::Start(0)) && self.position == 0)
        {
            Ok(self.position)
        } else {
            Err(io::ErrorKind::Unsupported.into())
        }
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        self.attempt.store(true, Ordering::SeqCst);
    }
}

async fn send_packet(
    tx: &SyncSender<Packet>,
    mut packet: Packet,
    cancel: &AtomicBool,
    attempt: &AtomicBool,
) -> bool {
    loop {
        if cancel.load(Ordering::SeqCst) || attempt.load(Ordering::SeqCst) {
            return false;
        }
        match tx.try_send(packet) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(value)) => packet = value,
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn reader(station: &Station, shared: Arc<Shared>, retry: u32) -> Result<Reader, String> {
    *shared.now_playing.lock().unwrap() = None;
    let (tx, receiver) = mpsc::sync_channel(8);
    let attempt = Arc::new(AtomicBool::new(false));
    let attempt_task = attempt.clone();
    let cancel = shared.cancel.clone();
    let address = if station == &Station::Nrj && retry % 2 == 1 {
        "https://scdn.nrjaudio.fm/adwz2/fr/30001/mp3_128.mp3"
    } else {
        station.url()
    }
    .to_owned();
    let custom = matches!(station, Station::Custom(_));
    crate::http_audio::network_runtime().map_err(|_| "Connexion radio indisponible.")?.spawn(async move {
        let task = async {
            let mut response = if custom {
                crate::radio_net::stream(&address).await.ok()?
            } else {
            let client = reqwest::Client::builder().https_only(true).connect_timeout(Duration::from_secs(8)).read_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::custom(|next| if next.previous().len() < 6 && allowed(next.url()) { next.follow() } else { next.stop() }))
                .build().ok()?;
            let response = client.get(address).header("User-Agent", "Tonal/0.3").header("Icy-MetaData", "1").send().await;
            response.ok()?.error_for_status().ok()?
            };
            if !response.status().is_success() { return None; }
            let interval = response.headers().get("icy-metaint").and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
            if interval > 1024 * 1024 || !send_packet(&tx, Packet::Header(interval), &cancel, &attempt_task).await { return None; }
            while let Ok(Some(chunk)) = response.chunk().await {
                for bytes in chunk.chunks(16 * 1024) {
                    if !send_packet(&tx, Packet::Data(bytes.to_vec()), &cancel, &attempt_task).await { return None; }
                }
            }
            Some(())
        };
        tokio::select! {
            _ = task => {},
            _ = async { while !cancel.load(Ordering::SeqCst) && !attempt_task.load(Ordering::SeqCst) { tokio::time::sleep(Duration::from_millis(20)).await; } } => {},
        }
    });
    Ok(Reader {
        receiver: Mutex::new(receiver),
        shared,
        attempt,
        data: Vec::new(),
        at: 0,
        meta_interval: 0,
        remaining: 0,
        position: 0,
        initialized: false,
    })
}
pub fn prepare(
    station: Station,
    cancelled: Arc<AtomicBool>,
    ctx: egui::Context,
) -> Result<Prepared, String> {
    let shared = Arc::new(Shared {
        cancel: cancelled,
        ended: AtomicBool::new(false),
        buffering: AtomicBool::new(true),
        error: Mutex::new(None),
        now_playing: Mutex::new(None),
    });
    let control = Control {
        shared: shared.clone(),
    };
    let (tx, receiver) = mpsc::sync_channel(24);
    let (ready_tx, ready) = mpsc::channel();
    let worker = shared.clone();
    thread::spawn(move || {
        let mut announced = false;
        let mut failures = 0;
        while !worker.cancel.load(Ordering::SeqCst) && failures < 5 {
            let result = (|| -> Result<(), String> {
                let input = reader(&station, worker.clone(), failures)?;
                let mut decoder = Decoder::builder()
                    .with_data(input)
                    .with_seekable(false)
                    .with_hint("mp3")
                    .build()
                    .map_err(|_| "Flux radio non reconnu.")?;
                let channels = decoder.channels();
                let rate = decoder.sample_rate();
                while !worker.cancel.load(Ordering::SeqCst) {
                    let samples: Vec<f32> = decoder
                        .by_ref()
                        .take(1024 * usize::from(channels))
                        .collect();
                    if samples.is_empty() {
                        break;
                    }
                    let mut frame = Frame {
                        samples,
                        channels,
                        rate,
                    };
                    loop {
                        if worker.cancel.load(Ordering::SeqCst) {
                            return Ok(());
                        }
                        match tx.try_send(frame) {
                            Ok(()) => break,
                            Err(TrySendError::Disconnected(_)) => return Ok(()),
                            Err(TrySendError::Full(value)) => frame = value,
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    if !announced {
                        let _ = ready_tx.send((channels, rate));
                        announced = true;
                        ctx.request_repaint();
                    }
                    failures = 0;
                }
                Err("Le flux radio s'est interrompu.".into())
            })();
            if worker.cancel.load(Ordering::SeqCst) {
                break;
            }
            failures += 1;
            worker.buffering.store(true, Ordering::Relaxed);
            ctx.request_repaint();
            let until = Instant::now() + Duration::from_secs((1_u64 << failures.min(3)).min(8));
            while Instant::now() < until && !worker.cancel.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(50));
            }
            if failures >= 5 {
                *worker.error.lock().unwrap() =
                    Some(result.err().unwrap_or_else(|| "Radio indisponible.".into()));
            }
        }
        worker.ended.store(true, Ordering::Relaxed);
        ctx.request_repaint();
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if shared.cancel.load(Ordering::SeqCst) {
            return Err("Connexion radio annulée.".into());
        }
        match ready.recv_timeout(Duration::from_millis(100)) {
            Ok((channels, rate)) => {
                return Ok(Prepared {
                    control,
                    source: Output {
                        receiver,
                        shared,
                        frame: Frame {
                            samples: Vec::new(),
                            channels,
                            rate,
                        },
                        at: 0,
                    },
                })
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Radio indisponible. Réessaie dans un instant.".into())
            }
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                return Err("La radio ne répond pas. Réessaie dans un instant.".into())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shared() -> Arc<Shared> {
        Arc::new(Shared {
            cancel: Arc::new(AtomicBool::new(false)),
            ended: AtomicBool::new(false),
            buffering: AtomicBool::new(true),
            error: Mutex::new(None),
            now_playing: Mutex::new(None),
        })
    }
    #[test]
    fn icy_titles_preserve_apostrophes_accents_and_explicit_clear() {
        assert_eq!(
            metadata_title("StreamTitle='Tame Impala - Loser §7768503';".as_bytes()),
            Some(Some("Tame Impala - Loser".into()))
        );
        assert_eq!(
            metadata_title(b"StreamTitle='Lighthouse Family - Ain't No Sunshine';StreamUrl='';\0"),
            Some(Some("Lighthouse Family - Ain't No Sunshine".into()))
        );
        assert_eq!(
            metadata_title(b"StreamTitle='Beyonc\xe9 - Halo';"),
            Some(Some("Beyoncé - Halo".into()))
        );
        assert_eq!(metadata_title(b"StreamTitle='';"), Some(None));
        assert_eq!(metadata_title(b"StreamTitle='5639 - 5639';"), Some(None));
        assert_eq!(metadata_title(b"StreamUrl='https://example.com/';"), None);
        assert_eq!(metadata_title(&[]), None);
    }
    #[test]
    fn station_programmes_are_not_displayed_as_songs() {
        assert_eq!(Station::Skyrock.song_title("SKYROCK - Le 16/20"), None);
        assert_eq!(Station::Nrj.song_title("NRJ - Le morning"), None);
        assert_eq!(Station::Skyrock.song_title("Le 16/20"), None);
        assert_eq!(
            Station::Skyrock.song_title("Animateur - émission du soir"),
            None
        );
        assert_eq!(
            Station::Skyrock.song_title("Lighthouse Family - Ain't No Sunshine"),
            Some("Lighthouse Family — Ain't No Sunshine".into())
        );
        let station = Station::Custom(Arc::new(CustomStation {
            id: "fip".into(),
            name: "FIP autour du groove".into(),
            stream_url: "https://example.com/live".into(),
            country: "FR".into(),
            favicon: String::new(),
        }));
        assert_eq!(station.song_title("FIP - Le club jazz"), None);
        assert_eq!(
            station.song_title("Beyoncé — Halo"),
            Some("Beyoncé — Halo".into())
        );
    }
    #[test]
    fn fragmented_icy_title_is_published_without_reaching_the_audio_decoder() {
        let (tx, receiver) = mpsc::sync_channel(32);
        tx.send(Packet::Header(3)).unwrap();
        let title = b"StreamTitle='Artist - Song';StreamUrl='';";
        let size = title.len().div_ceil(16) * 16;
        let mut bytes = b"abc".to_vec();
        bytes.push((size / 16) as u8);
        bytes.extend_from_slice(title);
        bytes.resize(4 + size, 0);
        bytes.extend_from_slice(b"def\0");
        for chunk in bytes.chunks(3) {
            tx.send(Packet::Data(chunk.to_vec())).unwrap();
        }
        drop(tx);
        let state = shared();
        let mut reader = Reader {
            receiver: Mutex::new(receiver),
            shared: state.clone(),
            attempt: Arc::new(AtomicBool::new(false)),
            data: vec![],
            at: 0,
            meta_interval: 0,
            remaining: 0,
            position: 0,
            initialized: false,
        };
        let mut audio = vec![];
        reader.read_to_end(&mut audio).unwrap();
        assert_eq!(audio, b"abcdef");
        assert_eq!(
            state.now_playing.lock().unwrap().as_deref(),
            Some("Artist - Song")
        );
    }
    #[test]
    fn station_hosts_are_isolated_from_google_and_other_destinations() {
        for station in STATIONS {
            assert!(allowed(&url::Url::parse(station.url()).unwrap()));
        }
        for url in [
            "http://icecast.skyrock.net/audio",
            "https://nrjaudio.fm.evil.test/audio",
            "https://user@icecast.skyrock.net/audio",
            "https://localhost/audio",
            "https://music.youtube.com/audio",
            "https://icecast.skyrock.net:444/audio",
        ] {
            assert!(!allowed(&url::Url::parse(url).unwrap()));
        }
    }
    #[test]
    fn streaming_reader_removes_interleaved_metadata_across_packet_boundaries() {
        let (tx, rx) = mpsc::sync_channel(8);
        tx.send(Packet::Header(3)).unwrap();
        let mut bytes = b"abc".to_vec();
        bytes.push(1);
        bytes.extend_from_slice(&[b'x'; 16]);
        bytes.extend_from_slice(b"def\0");
        for chunk in bytes.chunks(4) {
            tx.send(Packet::Data(chunk.to_vec())).unwrap();
        }
        drop(tx);
        let state = shared();
        let mut reader = Reader {
            receiver: Mutex::new(rx),
            shared: state.clone(),
            attempt: Arc::new(AtomicBool::new(false)),
            data: vec![],
            at: 0,
            meta_interval: 0,
            remaining: 0,
            position: 0,
            initialized: false,
        };
        let mut audio = vec![];
        reader.read_to_end(&mut audio).unwrap();
        assert_eq!(audio, b"abcdef");
        assert!(reader.seek(SeekFrom::Start(1)).is_err());
        state.cancel.store(true, Ordering::SeqCst);
        assert_eq!(
            reader.read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn audio_callback_never_waits_on_the_network_and_cancellation_ends_it() {
        let (_tx, receiver) = mpsc::sync_channel(24);
        let shared = shared();
        let mut output = Output {
            receiver,
            shared: shared.clone(),
            frame: Frame {
                samples: vec![],
                channels: 2,
                rate: 44100,
            },
            at: 0,
        };
        let started = Instant::now();
        assert_eq!(output.next(), Some(0.0));
        assert!(started.elapsed() < Duration::from_millis(50));
        assert!(output.total_duration().is_none());
        shared.cancel.store(true, Ordering::SeqCst);
        assert_eq!(output.next(), None);
    }
    #[test]
    #[ignore = "Réseau ; décode brièvement les deux radios en mémoire, sans périphérique audio ni fichier musical."]
    fn both_stations_decode_live_and_release_their_workers_when_stopped() {
        for station in STATIONS {
            let prepared = prepare(
                station.clone(),
                Arc::new(AtomicBool::new(false)),
                egui::Context::default(),
            )
            .unwrap();
            let weak = Arc::downgrade(&prepared.control.shared);
            let mut source = prepared.source;
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut heard = false;
            while Instant::now() < deadline && !heard {
                heard = source
                    .by_ref()
                    .take(4096)
                    .any(|sample| sample.abs() > 0.00001);
                if !heard {
                    thread::sleep(Duration::from_millis(20));
                }
            }
            assert!(heard, "{} must provide actual decoded PCM", station.name());
            eprintln!(
                "{}: PCM décodé, {} Hz / {} canaux ; aucun son joué.",
                station.name(),
                source.sample_rate(),
                source.channels()
            );
            eprintln!(
                "Métadonnée ICY : {:?} ; morceau exploitable : {:?}",
                prepared.control.now_playing(),
                prepared
                    .control
                    .now_playing()
                    .and_then(|text| station.song_title(&text))
            );
            drop(prepared.control);
            drop(source);
            let deadline = Instant::now() + Duration::from_secs(2);
            while weak.upgrade().is_some() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            assert!(
                weak.upgrade().is_none(),
                "Stop must release decoder and network reader"
            );
        }
    }
}
