//! Explicit playlist management. One write at a time; no automatic mutation retries.
use crate::{
    app_state::AppState,
    backdrop::Backdrop,
    provider::{self, Playlist, PlaylistDetails, PlaylistPrivacy, YouTubeMusic},
    session::Session,
    ui::{self, ButtonKind, Icon, MUTED, RED, WHITE},
};
use eframe::egui::{self, vec2, Align, Layout, RichText, Stroke};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
enum Mode {
    Create,
    Edit(Playlist, Option<PlaylistDetails>),
    Delete(Playlist),
}
struct Form {
    mode: Mode,
    title: String,
    privacy: PlaylistPrivacy,
    focused: bool,
}
#[derive(Clone)]
enum Action {
    Create(String, PlaylistPrivacy),
    Edit(Playlist, Option<String>, Option<PlaylistPrivacy>),
    Delete(Playlist),
}
pub enum Outcome {
    Created(Playlist, PlaylistPrivacy),
    Edited(Playlist, PlaylistDetails),
    Deleted(Playlist),
}
type Inspection = Result<PlaylistDetails, String>;
#[derive(Default)]
pub struct Manager {
    form: Option<Form>,
    error: Option<String>,
    lookup: Option<(String, Receiver<Inspection>)>,
    info: Option<(String, Inspection, Instant)>,
    pending: Option<Receiver<Result<Outcome, String>>>,
    cancelled: Arc<AtomicBool>,
}
impl Drop for Manager {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl Manager {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn is_open(&self) -> bool {
        self.form.is_some()
    }
    pub fn target(&self) -> Option<&str> {
        match &self.form.as_ref()?.mode {
            Mode::Edit(playlist, _) | Mode::Delete(playlist) => Some(&playlist.id),
            Mode::Create => None,
        }
    }
    pub fn deny(&mut self, message: &str) {
        self.error = Some(message.into());
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn create(&mut self) {
        if !self.busy() {
            self.error = None;
            self.form = Some(Form {
                mode: Mode::Create,
                title: String::new(),
                privacy: PlaylistPrivacy::Public,
                focused: false,
            });
        }
    }
    pub fn access(&self, id: &str) -> Option<&Inspection> {
        self.info
            .as_ref()
            .filter(|(key, _, at)| key == id && at.elapsed() < Duration::from_secs(120))
            .map(|(_, info, _)| info)
    }
    pub fn edit(&mut self, playlist: Playlist, delete: bool) {
        if self.busy() {
            return;
        }
        let details = self
            .access(&playlist.id)
            .and_then(|result| result.as_ref().ok())
            .cloned();
        if details.as_ref().is_some_and(|details| !details.editable) {
            return;
        }
        self.error = None;
        let title = details
            .as_ref()
            .map(|details| details.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| playlist.title.clone());
        let mut playlist = playlist;
        playlist.title = title.clone();
        self.form = Some(Form {
            title,
            privacy: details
                .as_ref()
                .map_or(PlaylistPrivacy::Public, |details| details.privacy),
            focused: false,
            mode: if delete {
                Mode::Delete(playlist)
            } else {
                Mode::Edit(playlist, details)
            },
        });
    }
    pub fn inspect(
        &mut self,
        ctx: &egui::Context,
        session: Option<Session>,
        playlist: &Playlist,
        retry: bool,
    ) {
        if !retry
            && (self.access(&playlist.id).is_some_and(Result::is_ok)
                || self
                    .lookup
                    .as_ref()
                    .is_some_and(|(id, _)| id == &playlist.id))
        {
            return;
        }
        let Some(session) = session else {
            return;
        };
        self.info = None;
        let id = playlist.id.clone();
        let (tx, rx) = mpsc::channel();
        self.lookup = Some((id.clone(), rx));
        let cancelled = self.cancelled.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Vérification annulée.".into());
                }
                let provider = YouTubeMusic::new(&session).map_err(|e| e.to_string())?;
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Vérification annulée.".into());
                }
                provider
                    .playlist_details(&session, &id)
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self) -> Option<Result<Outcome, String>> {
        if let Some((id, rx)) = &self.lookup {
            let result = match rx.try_recv() {
                Ok(info) => Some(info),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Vérification interrompue. Réessaie.".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(info) = result {
                let id = id.clone();
                self.resolve_form(&id, &info);
                self.info = Some((id, info, Instant::now()));
                self.lookup = None;
            }
        }
        let result = match self.pending.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(
                "L’action n’a pas été confirmée. Actualise la bibliothèque avant de réessayer."
                    .into(),
            ),
        };
        self.pending = None;
        match &result {
            Ok(Outcome::Created(playlist, privacy)) => {
                self.form = None;
                self.error = None;
                // The create receipt proves ownership, even while browse is still propagating it.
                self.info = Some((
                    playlist.id.clone(),
                    Ok(PlaylistDetails {
                        editable: true,
                        title: playlist.title.clone(),
                        privacy: *privacy,
                    }),
                    Instant::now(),
                ));
            }
            Ok(_) => {
                self.form = None;
                self.error = None;
                self.info = None;
            }
            Err(error) => self.error = Some(error.clone()),
        }
        Some(result)
    }
    fn resolve_form(&mut self, id: &str, info: &Inspection) {
        let Some(form) = &mut self.form else {
            return;
        };
        match &mut form.mode {
            Mode::Edit(playlist, original) if playlist.id == id && original.is_none() => match info
            {
                Ok(details) if details.editable => {
                    if !details.title.is_empty() {
                        if form.title == playlist.title {
                            form.title.clone_from(&details.title);
                        }
                        playlist.title.clone_from(&details.title);
                    }
                    form.privacy = details.privacy;
                    *original = Some(details.clone());
                }
                Ok(_) => {
                    self.error =
                        Some("Cette playlist ne peut pas être modifiée avec ce compte.".into())
                }
                Err(error) => self.error = Some(error.clone()),
            },
            Mode::Delete(playlist) if playlist.id == id => {
                if matches!(info, Ok(details) if !details.editable) {
                    self.error =
                        Some("Cette playlist ne peut pas être supprimée avec ce compte.".into());
                }
            }
            _ => {}
        }
    }
    pub fn submit(&mut self, ctx: &egui::Context, session: Option<Session>) {
        if self.busy() {
            return;
        }
        let Some(action) = self.form.as_ref().and_then(Form::action) else {
            return;
        };
        let Some(session) = session else {
            self.error = Some("Reconnecte-toi à Google.".into());
            return;
        };
        #[cfg(feature = "ui-capture")]
        if std::env::var_os("TONAL_CAPTURE_TO").is_some() {
            self.error = Some("Écriture désactivée pendant la capture.".into());
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.error = None;
        let cancelled = self.cancelled.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Action annulée.".into());
                }
                let provider = YouTubeMusic::new(&session).map_err(|e| e.to_string())?;
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Action annulée.".into());
                }
                match action {
                    Action::Create(title, privacy) => provider
                        .create_playlist(&session, &title, privacy)
                        .map(|playlist| Outcome::Created(playlist, privacy)),
                    Action::Edit(playlist, title, privacy) => provider
                        .update_playlist_guarded(
                            &session,
                            &playlist.id,
                            title.as_deref(),
                            privacy,
                            &cancelled,
                        )
                        .map(|details| Outcome::Edited(playlist, details)),
                    Action::Delete(playlist) => provider
                        .delete_playlist_guarded(&session, &playlist.id, &cancelled)
                        .map(|()| Outcome::Deleted(playlist)),
                }
                .map_err(|error| error.to_string())
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    #[cfg(feature = "ui-capture")]
    pub fn qa_dialog(&mut self, mode: &str) {
        let playlist = Playlist {
            id: "PL_tonal_preview".into(),
            title: "Mes découvertes".into(),
            ..Default::default()
        };
        self.info = Some((
            playlist.id.clone(),
            Ok(PlaylistDetails {
                editable: true,
                title: playlist.title.clone(),
                privacy: PlaylistPrivacy::Private,
            }),
            Instant::now(),
        ));
        match mode {
            "playlist-create" => self.create(),
            "playlist-edit" => self.edit(playlist, false),
            "playlist-delete" => self.edit(playlist, true),
            _ => {}
        }
    }
}
impl Form {
    fn action(&self) -> Option<Action> {
        match &self.mode {
            Mode::Create => Some(Action::Create(
                provider::playlist_title(&self.title).ok()?.into(),
                self.privacy,
            )),
            Mode::Delete(playlist) => Some(Action::Delete(playlist.clone())),
            Mode::Edit(playlist, original) => {
                let original = original.as_ref()?;
                let title = provider::playlist_title(&self.title).ok()?;
                let changed_title = (title != playlist.title).then(|| title.into());
                let privacy = (self.privacy != original.privacy).then_some(self.privacy);
                (changed_title.is_some() || privacy.is_some())
                    .then(|| Action::Edit(playlist.clone(), changed_title, privacy))
            }
        }
    }
}

pub fn show(ctx: &egui::Context, state: &mut AppState, backdrop: &mut Backdrop) {
    if !state.playlist_management.is_open() {
        return;
    }
    let busy = state.playlist_management.busy();
    let readonly = state
        .playlist_management
        .target()
        .and_then(|id| state.playlist_management.access(id))
        .is_some_and(|info| matches!(info, Ok(details) if !details.editable));
    let enabled = state.connected() && !state.google_busy() && !busy && !readonly;
    let error = state.playlist_management.error.clone();
    let deleting = state
        .playlist_management
        .form
        .as_ref()
        .is_some_and(|form| matches!(form.mode, Mode::Delete(_)));
    let id = egui::Id::unique(if deleting {
        "playlist_delete_modal"
    } else {
        "playlist_management_modal"
    });
    backdrop.paint(ctx, egui::LayerId::new(egui::Order::Foreground, id));
    let mut close = false;
    let mut submit = false;
    let mut title_enter = false;
    let response = egui::Modal::new(id)
        .area(
            egui::Modal::default_area(id)
                .default_size(vec2(if deleting { 320.0 } else { 380.0 }, 0.0)),
        )
        .backdrop_color(egui::Color32::from_black_alpha(110))
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_gray(13))
                .inner_margin(if deleting {
                    egui::Margin {
                        left: 18,
                        right: 18,
                        top: 16,
                        bottom: 12,
                    }
                } else {
                    egui::Margin::same(22)
                })
                .corner_radius(20)
                .stroke(Stroke::new(1.0, egui::Color32::from_gray(39))),
        )
        .show(ctx, |ui| {
            ui.set_width(
                (if deleting { 320.0_f32 } else { 380.0_f32 })
                    .min(ctx.content_rect().width() - 88.0),
            );
            let form = state.playlist_management.form.as_mut().unwrap();
            ui.horizontal(|ui| {
                ui.label(ui::heading(
                    match form.mode {
                        Mode::Create => "Nouvelle playlist",
                        Mode::Edit(..) => "Modifier la playlist",
                        Mode::Delete(_) => "Supprimer la playlist ?",
                    },
                    if deleting { 19.0 } else { 21.0 },
                ));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::icon_button(ui, Icon::Close, "Fermer", 30.0, false, true).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(if deleting { 12.0 } else { 16.0 });
            if deleting {
                ui.label(
                    RichText::new(format!("« {} »", form.title))
                        .size(14.0)
                        .strong()
                        .color(WHITE),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Suppression définitive sur YouTube Music.")
                        .size(12.0)
                        .color(MUTED),
                );
            } else {
                ui.label(RichText::new("Titre").size(12.0).color(MUTED));
                let input = ui.add_enabled(
                    !busy,
                    egui::TextEdit::singleline(&mut form.title)
                        .hint_text("Nom de la playlist")
                        .desired_width(ui.available_width())
                        .desired_rows(1)
                        .min_size(vec2(0.0, 38.0))
                        .align(egui::Align2::LEFT_CENTER)
                        .font(egui::FontId::proportional(15.0))
                        .char_limit(150)
                        .frame(
                            egui::Frame::new()
                                .fill(egui::Color32::from_gray(24))
                                .inner_margin(egui::Margin::symmetric(12, 8))
                                .corner_radius(8),
                        ),
                );
                #[cfg(feature = "ui-capture")]
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::unique("qa_playlist_title_rect"), input.rect)
                });
                title_enter =
                    input.lost_focus() && ctx.input(|input| input.key_pressed(egui::Key::Enter));
                if !form.focused {
                    input.request_focus();
                    form.focused = true;
                }
                if !form.title.trim().is_empty() && provider::playlist_title(&form.title).is_err() {
                    ui.label(
                        RichText::new("Le titre ne doit pas contenir < ou >.")
                            .size(12.0)
                            .color(RED),
                    );
                }
                ui.add_space(12.0);
                let privacy_ready = !matches!(form.mode, Mode::Edit(_, None));
                let mut public = privacy_ready && form.privacy == PlaylistPrivacy::Public;
                let description = if !privacy_ready {
                    "Chargement de la confidentialité…"
                } else {
                    match form.privacy {
                        PlaylistPrivacy::Public => "Visible par tout le monde.",
                        PlaylistPrivacy::Private => "Visible uniquement par toi.",
                        PlaylistPrivacy::Unlisted => "Accessible aux personnes qui ont le lien.",
                    }
                };
                if ui
                    .add_enabled_ui(!busy && privacy_ready, |ui| {
                        ui::setting_toggle(ui, "Visibilité", description, &mut public)
                    })
                    .inner
                    .changed()
                {
                    form.privacy = if public {
                        PlaylistPrivacy::Public
                    } else {
                        PlaylistPrivacy::Private
                    };
                }
            }
            if let Some(error) = &error {
                ui.add_space(12.0);
                ui.label(RichText::new(error).size(12.0).color(RED));
            }
            ui.add_space(16.0);
            if !deleting && enabled && form.action().is_some() && title_enter {
                submit = true;
            }
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 40.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    if ui::button(
                        ui,
                        if deleting {
                            "Supprimer"
                        } else if matches!(form.mode, Mode::Create) {
                            "Créer"
                        } else {
                            "Enregistrer"
                        },
                        if deleting { Icon::Trash } else { Icon::Check },
                        if deleting {
                            ButtonKind::Danger
                        } else {
                            ButtonKind::Primary
                        },
                        enabled && form.action().is_some(),
                    )
                    .clicked()
                    {
                        submit = true;
                    }
                    if busy {
                        ui.spinner();
                    }
                },
            );
        });
    if close || response.should_close() {
        state.playlist_management.form = None;
    }
    if submit {
        state.submit_playlist_management(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_playlists_default_to_public() {
        let mut manager = Manager::default();
        manager.create();
        let form = manager.form.as_mut().unwrap();
        assert_eq!(form.privacy, PlaylistPrivacy::Public);
        form.title = "Nouvelle playlist".into();
        assert!(matches!(
            form.action(),
            Some(Action::Create(_, PlaylistPrivacy::Public))
        ));
    }
    #[test]
    fn editor_opens_immediately_and_late_metadata_preserves_the_draft() {
        let mut manager = Manager::default();
        let playlist = Playlist {
            id: "PL_test".into(),
            title: "Titre connu".into(),
            ..Default::default()
        };
        manager.edit(playlist, false);
        assert!(manager.is_open());
        let form = manager.form.as_mut().unwrap();
        assert_eq!(form.title, "Titre connu");
        assert!(
            form.action().is_none(),
            "Unknown privacy must not be written"
        );
        form.title = "Mon nouveau titre".into();
        let (tx, rx) = mpsc::channel();
        manager.lookup = Some(("PL_test".into(), rx));
        tx.send(Ok(PlaylistDetails {
            editable: true,
            title: "Nom côté serveur".into(),
            privacy: PlaylistPrivacy::Private,
        }))
        .unwrap();
        assert!(manager.poll().is_none());
        let form = manager.form.as_ref().unwrap();
        assert_eq!(form.title, "Mon nouveau titre");
        assert_eq!(form.privacy, PlaylistPrivacy::Private);
        assert!(matches!(
            form.action(),
            Some(Action::Edit(_, Some(_), None))
        ));
    }
    #[test]
    fn rename_preserves_unlisted_privacy_and_unchanged_forms_do_not_write() {
        let original = Playlist {
            id: "PL_test".into(),
            title: "Original".into(),
            ..Default::default()
        };
        let details = PlaylistDetails {
            editable: true,
            title: original.title.clone(),
            privacy: PlaylistPrivacy::Unlisted,
        };
        let mut form = Form {
            mode: Mode::Edit(original, Some(details)),
            title: "Original".into(),
            privacy: PlaylistPrivacy::Unlisted,
            focused: false,
        };
        assert!(form.action().is_none());
        form.title = "Nouveau titre".into();
        assert!(matches!(
            form.action(),
            Some(Action::Edit(_, Some(_), None))
        ));
        form.privacy = PlaylistPrivacy::Private;
        assert!(matches!(
            form.action(),
            Some(Action::Edit(_, Some(_), Some(PlaylistPrivacy::Private)))
        ));
    }
    #[test]
    fn disconnect_discards_late_write_results() {
        let mut manager = Manager::default();
        let (sender, receiver) = mpsc::channel();
        manager.pending = Some(receiver);
        let cancel = manager.cancelled.clone();
        manager.clear();
        assert!(cancel.load(Ordering::Relaxed));
        assert!(sender
            .send(Ok(Outcome::Created(
                Playlist::default(),
                PlaylistPrivacy::Private
            )))
            .is_err());
        assert!(manager.poll().is_none());
    }
}
