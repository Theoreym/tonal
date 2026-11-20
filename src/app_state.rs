use crate::{
    audio::AudioPlayer,
    auth,
    native_stream::NativeStream,
    playback_queue::{PlaybackQueue, Repeat},
    provider::{
        Account, Page, Playlist, Provider, ProviderError, StreamResolver, Track, YouTubeMusic,
        YouTubeStreamResolver,
    },
    session::{self, Session},
    youtube,
};
use eframe::egui;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
};

enum GoogleResult {
    Connected {
        session: Session,
        playlists: Page<Playlist>,
        account: Account,
    },
    Library {
        page: Page<Playlist>,
        replace: bool,
    },
    Tracks {
        playlist: Playlist,
        page: Page<Track>,
        replace: bool,
        revision: u64,
    },
}

struct PreparedTrack {
    audio: NativeStream,
    queue: PlaybackQueue,
}
struct PlaylistCatalog {
    id: String,
    tracks: Arc<Vec<Track>>,
}
struct OptimisticAdd {
    added: crate::playlist_add::Added,
    appended: bool,
}
struct PlaylistSnapshot {
    playlist: Playlist,
    tracks: Vec<Track>,
}
struct PlaylistRefresh {
    choice: crate::provider::PlaylistChoice,
    revision: u64,
    receiver: Receiver<Result<PlaylistSnapshot, GoogleError>>,
    cancelled: Arc<AtomicBool>,
}
#[derive(Debug)]
enum GoogleError {
    Login(String),
    Provider(ProviderError),
}
impl From<ProviderError> for GoogleError {
    fn from(e: ProviderError) -> Self {
        Self::Provider(e)
    }
}

pub struct AppState {
    pub audio: Option<AudioPlayer>,
    pub audio_error: Option<String>,
    output_devices: Option<crate::audio_devices::Watcher>,
    resume_after_output_change: bool,
    pub import_busy: bool,
    pub now_playing: Option<Track>,
    pub radio_station: Option<crate::radio::Station>,
    radio_paused: bool,
    radio_receiver: Option<Receiver<Result<crate::radio::Prepared, String>>>,
    radio_cancel: Option<Arc<AtomicBool>>,
    radio_control: Option<crate::radio::Control>,
    radio_now_playing: Option<String>,
    retry_position: Option<std::time::Duration>,
    import_position: std::time::Duration,
    pause_after_load: bool,
    import_receiver: Option<Receiver<Result<PreparedTrack, String>>>,
    cancel_import: Option<Arc<AtomicBool>>,
    prefetch_receiver: Option<Receiver<Result<PreparedTrack, String>>>,
    prefetched: Option<PreparedTrack>,
    prefetch_id: Option<String>,
    prefetch_failed: Option<String>,
    cancel_prefetch: Option<Arc<AtomicBool>>,
    pub status: String,
    pub google_status: String,
    session: Option<Session>,
    pub account: Option<Account>,
    google_receiver: Option<Receiver<Result<GoogleResult, GoogleError>>>,
    google_progress_receiver: Option<Receiver<&'static str>>,
    cancel_login: Option<Arc<AtomicBool>>,
    cancel_browse: Option<Arc<AtomicBool>>,
    pub playlists: Vec<Playlist>,
    pub playlists_continuation: Option<String>,
    pub selected_playlist: Option<Playlist>,
    pub tracks: Vec<Track>,
    pub tracks_continuation: Option<String>,
    pub tracks_error: Option<String>,
    playlist_cache: crate::playlist_cache::PlaylistCache,
    pub suggestions: crate::suggestions::Suggestions,
    pub playlist_add: crate::playlist_add::PlaylistAdd,
    pub playlist_management: crate::playlist_management::Manager,
    optimistic_adds: HashMap<String, OptimisticAdd>,
    playlist_revisions: HashMap<String, u64>,
    playlist_refreshes: Vec<PlaylistRefresh>,
    playlist_refresh_requested: HashMap<String, crate::provider::PlaylistChoice>,
    tracks_job: bool,
    tracks_page_requested: Option<String>,
    pub queue: PlaybackQueue,
    catalog: Option<PlaylistCatalog>,
    catalog_receiver: Option<Receiver<Result<PlaylistCatalog, GoogleError>>>,
    catalog_id: Option<String>,
    cancel_catalog: Option<Arc<AtomicBool>>,
    catalog_error: Option<String>,
    pending_next: Option<bool>,
    store: Option<crate::playback_store::Store>,
    restore_pending: Option<crate::playback_store::Snapshot>,
    saved_revision: Option<(u64, u64, u32)>,
    normalization: bool,
    level_receiver: Option<Receiver<(String, Option<f32>)>>,
    level_cancel: Option<Arc<AtomicBool>>,
    level_attempted: Option<String>,
}

impl AppState {
    fn empty() -> Self {
        Self {
            audio: None,
            audio_error: None,
            output_devices: None,
            resume_after_output_change: false,
            import_busy: false,
            now_playing: None,
            radio_station: None,
            radio_paused: true,
            radio_receiver: None,
            radio_cancel: None,
            radio_control: None,
            radio_now_playing: None,
            retry_position: None,
            import_position: std::time::Duration::ZERO,
            pause_after_load: false,
            import_receiver: None,
            cancel_import: None,
            prefetch_receiver: None,
            prefetched: None,
            prefetch_id: None,
            prefetch_failed: None,
            cancel_prefetch: None,
            status: "Prêt".into(),
            google_status: String::new(),
            session: None,
            account: None,
            google_receiver: None,
            google_progress_receiver: None,
            cancel_login: None,
            cancel_browse: None,
            playlists: Vec::new(),
            playlists_continuation: None,
            selected_playlist: None,
            tracks: Vec::new(),
            tracks_continuation: None,
            tracks_error: None,
            playlist_cache: Default::default(),
            suggestions: Default::default(),
            playlist_add: Default::default(),
            playlist_management: Default::default(),
            optimistic_adds: Default::default(),
            playlist_revisions: Default::default(),
            playlist_refreshes: Default::default(),
            playlist_refresh_requested: Default::default(),
            tracks_job: false,
            tracks_page_requested: None,
            queue: PlaybackQueue::default(),
            catalog: None,
            catalog_receiver: None,
            catalog_id: None,
            cancel_catalog: None,
            catalog_error: None,
            pending_next: None,
            store: None,
            restore_pending: None,
            saved_revision: None,
            normalization: false,
            level_receiver: None,
            level_cancel: None,
            level_attempted: None,
        }
    }
    pub fn new(ctx: &egui::Context) -> Self {
        let mut state = Self::empty();
        state.output_devices = crate::audio_devices::Watcher::new(ctx);
        match AudioPlayer::new() {
            Ok(player) => state.audio = Some(player),
            Err(message) => state.audio_error = Some(message),
        }
        state.store = crate::playback_store::Store::new();
        state.restore_pending = crate::playback_store::Store::load();
        match session::load() {
            Ok(Some(session)) => {
                state.google_status = "Vérification de la session enregistrée…".into();
                state.google_job(ctx, move || connect(session));
            }
            Err(error) => state.google_status = error,
            Ok(None) => {}
        }
        state
    }

    pub fn google_busy(&self) -> bool {
        self.google_receiver.is_some()
    }
    pub fn connected(&self) -> bool {
        self.session.is_some()
    }
    pub fn login(&mut self, ctx: &egui::Context) {
        if self.google_busy() {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancel_login = Some(cancelled.clone());
        self.google_status = "Ouverture de la fenêtre Google…".into();
        let (progress_tx, progress_rx) = mpsc::channel();
        self.google_progress_receiver = Some(progress_rx);
        let progress_ctx = ctx.clone();
        self.google_job(ctx, move || {
            let login_tx = progress_tx.clone();
            let login_ctx = progress_ctx.clone();
            let session = auth::login(cancelled.clone(), move |message| {
                let _ = login_tx.send(message);
                login_ctx.request_repaint();
            })
            .map_err(GoogleError::Login)?;
            if cancelled.load(Ordering::Relaxed) {
                return Err(GoogleError::Login("Connexion annulée.".into()));
            }
            let _ = progress_tx.send("Vérification de la session et chargement des playlists…");
            progress_ctx.request_repaint();
            connect(session)
        });
    }

    pub fn cancel_google_task(&mut self) {
        if let Some(cancelled) = self.cancel_browse.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        if let Some(cancelled) = self.cancel_login.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        // A late login result has no receiver and cannot replace or persist the current account.
        self.google_receiver = None;
        self.google_progress_receiver = None;
        self.tracks_job = false;
        self.tracks_page_requested = None;
        self.google_status = if self.connected() {
            "Chargement annulé."
        } else {
            "Connexion annulée."
        }
        .into();
    }
    pub fn disconnect(&mut self) {
        if let Some(cancelled) = self.cancel_browse.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.playlist_cache.clear();
        self.playlist_management.clear();
        crate::resolution_cache::clear();
        for job in self.playlist_refreshes.drain(..) {
            job.cancelled.store(true, Ordering::Relaxed);
        }
        self.optimistic_adds.clear();
        self.playlist_revisions.clear();
        self.playlist_refresh_requested.clear();
        self.playlist_add.clear();
        self.suggestions.clear();
        let smart = self.queue.smart_shuffle;
        self.clear_level();
        self.restore_pending = None;
        if self.radio_station.is_none() {
            self.stop();
        }
        self.catalog = None;
        if let Some(cancelled) = self.cancel_login.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        // Dropping the receiver discards late network results; workers never persist sessions.
        self.google_receiver = None;
        self.google_progress_receiver = None;
        self.session = None;
        self.queue = PlaybackQueue::default();
        self.queue.set_smart_shuffle(smart);
        self.account = None;
        self.playlists.clear();
        self.playlists_continuation = None;
        self.selected_playlist = None;
        self.tracks.clear();
        self.tracks_continuation = None;
        self.tracks_error = None;
        self.tracks_job = false;
        self.tracks_page_requested = None;
        self.google_status = match session::delete() {
            Ok(()) => "Déconnecté de Tonal. Session locale effacée.".into(),
            Err(error) => error,
        };
    }

    pub fn refresh_library(&mut self, ctx: &egui::Context, more: bool) {
        if self.playlist_management.busy() {
            return;
        }
        if self.google_busy() {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        let continuation = if more {
            self.playlists_continuation.clone()
        } else {
            None
        };
        self.google_status = "Récupération des playlists…".into();
        let known = self.playlists.len();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancel_browse = Some(cancelled.clone());
        self.google_job(ctx, move || {
            let provider = YouTubeMusic::new(&session)?;
            Ok(GoogleResult::Library {
                page: if more {
                    provider.library_playlists(&session, continuation.as_deref())?
                } else {
                    crate::playlist_cache::revalidate(known, |token| {
                        if cancelled.load(Ordering::Relaxed) {
                            return Err(ProviderError::Network);
                        }
                        provider.library_playlists(&session, token)
                    })?
                },
                replace: !more,
            })
        });
    }

    pub fn open_playlist(&mut self, ctx: &egui::Context, playlist: Playlist, more: bool) {
        if self.google_busy() {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        if more && self.tracks_continuation.is_none() {
            return;
        }
        let continuation = if more {
            self.tracks_continuation.clone()
        } else {
            None
        };
        if !more {
            if self
                .catalog
                .as_ref()
                .is_some_and(|catalog| catalog.id == playlist.id)
            {
                self.catalog = None;
            }
            self.selected_playlist = Some(playlist.clone());
            if let Some(snapshot) = self.playlist_cache.get(&playlist.id) {
                self.tracks = snapshot.tracks;
                self.tracks_continuation = snapshot.continuation;
            } else {
                self.tracks.clear();
                self.tracks_continuation = None;
            }
        }
        self.tracks_error = None;
        self.tracks_job = true;
        self.tracks_page_requested = continuation.clone();
        let known = self.tracks.len();
        let revision = self
            .playlist_revisions
            .get(&playlist.id)
            .copied()
            .unwrap_or(0);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancel_browse = Some(cancelled.clone());
        self.google_status = if more {
            String::new()
        } else {
            format!("Chargement de « {} »…", playlist.title)
        };
        self.google_job(ctx, move || {
            let provider = YouTubeMusic::new(&session)?;
            let page = if more {
                provider.playlist_tracks(&session, &playlist.id, continuation.as_deref())?
            } else {
                crate::playlist_cache::revalidate(known, |token| {
                    if cancelled.load(Ordering::Relaxed) {
                        return Err(ProviderError::Network);
                    }
                    provider.playlist_tracks(&session, &playlist.id, token)
                })?
            };
            Ok(GoogleResult::Tracks {
                playlist,
                page,
                replace: !more,
                revision,
            })
        });
    }

    fn cache_selected_playlist(&mut self) {
        if let Some(playlist) = &self.selected_playlist {
            self.playlist_cache.put(
                &playlist.id,
                &self.tracks,
                self.tracks_continuation.as_deref(),
            );
        }
    }
    pub fn leave_playlist(&mut self) {
        if !self.tracks_job {
            self.cache_selected_playlist();
        }
        self.selected_playlist = None;
        self.tracks.clear();
        self.tracks_continuation = None;
        self.tracks_error = None;
    }
    pub fn retry_playlist(&mut self, ctx: &egui::Context) {
        if let Some(playlist) = self.selected_playlist.clone() {
            self.open_playlist(ctx, playlist, self.tracks_page_requested.is_some());
        }
    }

    fn google_job(
        &mut self,
        ctx: &egui::Context,
        job: impl FnOnce() -> Result<GoogleResult, GoogleError> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        self.google_receiver = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
    }

    pub fn load_suggestions(&mut self, ctx: &egui::Context, refresh: bool) {
        if let (Some(session), Some(seed)) = (self.session.clone(), self.now_playing.clone()) {
            self.suggestions.load(
                ctx,
                session,
                seed,
                refresh,
                self.playlist_add.membership_cache(),
            );
        }
    }

    pub fn add_current_to_playlist(&mut self, ctx: &egui::Context, playlist_id: &str) -> bool {
        if let Some(session) = self.session.clone() {
            #[cfg(feature = "ui-capture")]
            let added = if let Some(fail) = crate::playlist_add::qa_mode() {
                self.playlist_add.qa_add(ctx, playlist_id, fail)
            } else {
                self.playlist_add.add(ctx, session, playlist_id)
            };
            #[cfg(not(feature = "ui-capture"))]
            let added = self.playlist_add.add(ctx, session, playlist_id);
            if let Some(added) = added {
                self.apply_optimistic_add(added);
                ctx.request_repaint();
                return true;
            }
        }
        false
    }

    pub fn inspect_playlist(&mut self, ctx: &egui::Context, playlist: &Playlist, retry: bool) {
        self.playlist_management
            .inspect(ctx, self.session.clone(), playlist, retry);
    }
    pub fn submit_playlist_management(&mut self, ctx: &egui::Context) {
        if self.google_busy() {
            return;
        }
        if self
            .playlist_management
            .target()
            .is_some_and(|id| self.playlist_add.pending_for(id))
        {
            self.playlist_management
                .deny("Un ajout est en cours dans cette playlist. Réessaie après sa confirmation.");
            return;
        }
        self.playlist_management.submit(ctx, self.session.clone());
    }
    fn poll_playlist_management(&mut self) {
        use crate::playlist_management::Outcome;
        let Some(result) = self.playlist_management.poll() else {
            return;
        };
        let (message, success) = match result {
            Ok(Outcome::Created(playlist, _)) => {
                let message = format!("{} créée", playlist.title);
                if !self.playlists.iter().any(|p| p.id == playlist.id) {
                    self.playlists.insert(0, playlist);
                }
                self.playlist_add.library_changed(None);
                (message, true)
            }
            Ok(Outcome::Edited(original, details)) => {
                self.invalidate_playlist_refresh(&original.id);
                for playlist in self
                    .playlists
                    .iter_mut()
                    .chain(self.selected_playlist.iter_mut())
                {
                    if playlist.id == original.id {
                        playlist.title.clone_from(&details.title);
                    }
                }
                self.playlist_add.library_changed(None);
                (format!("{} modifiée", details.title), true)
            }
            Ok(Outcome::Deleted(playlist)) => {
                self.invalidate_playlist_refresh(&playlist.id);
                self.playlists.retain(|p| p.id != playlist.id);
                if self
                    .selected_playlist
                    .as_ref()
                    .is_some_and(|p| p.id == playlist.id)
                {
                    self.google_receiver = None;
                    self.google_progress_receiver = None;
                    self.tracks_job = false;
                    self.tracks_page_requested = None;
                    self.google_status.clear();
                    self.selected_playlist = None;
                    self.tracks.clear();
                    self.tracks_continuation = None;
                    self.tracks_error = None;
                }
                if self.queue.playlist_id.as_ref() == Some(&playlist.id) {
                    self.clear_catalog_job();
                    self.catalog = None;
                    self.queue.mark_complete();
                }
                self.playlist_add.library_changed(Some(&playlist.id));
                self.suggestions.clear();
                (format!("{} supprimée", playlist.title), true)
            }
            Err(error) => {
                if self.playlist_management.is_open() {
                    return;
                }
                (error, false)
            }
        };
        self.playlist_add.notice = Some(crate::playlist_add::Notice {
            message,
            success,
            at: std::time::Instant::now(),
        });
    }
    fn invalidate_playlist_refresh(&mut self, id: &str) {
        self.playlist_cache.remove(id);
        self.playlist_refreshes.retain(|job| {
            if job.choice.id == id {
                job.cancelled.store(true, Ordering::Relaxed);
                false
            } else {
                true
            }
        });
        self.playlist_refresh_requested.remove(id);
        let revision = self.playlist_revisions.entry(id.into()).or_default();
        *revision = revision.wrapping_add(1);
    }
    fn playlist_count_delta(&mut self, id: &str, delta: i32) {
        for playlist in self
            .playlists
            .iter_mut()
            .chain(self.selected_playlist.iter_mut())
        {
            if playlist.id == id {
                playlist.track_count = playlist
                    .track_count
                    .map(|count| count.saturating_add_signed(delta));
            }
        }
    }

    fn apply_optimistic_add(&mut self, added: crate::playlist_add::Added) {
        self.playlist_cache.remove(&added.playlist.id);
        *self
            .playlist_revisions
            .entry(added.playlist.id.clone())
            .or_default() += 1;
        self.playlist_count_delta(&added.playlist.id, 1);
        self.suggestions.exclude(&added.track);
        let appended = self
            .selected_playlist
            .as_ref()
            .is_some_and(|playlist| playlist.id == added.playlist.id)
            && self.tracks_continuation.is_none()
            && self.tracks_error.is_none()
            && !self.tracks_job
            && !self
                .tracks
                .iter()
                .any(|track| track.video_id == added.track.video_id);
        if appended {
            self.tracks.push(added.track.clone());
        }
        if let Some(catalog) = self
            .catalog
            .as_mut()
            .filter(|catalog| catalog.id == added.playlist.id)
        {
            if !catalog
                .tracks
                .iter()
                .any(|track| track.video_id == added.track.video_id)
            {
                Arc::make_mut(&mut catalog.tracks).push(added.track.clone());
            }
        }
        self.optimistic_adds.insert(
            added.track.video_id.clone(),
            OptimisticAdd { added, appended },
        );
        self.cache_selected_playlist();
    }

    fn poll_playlist_adds(&mut self, ctx: &egui::Context) {
        for outcome in self.playlist_add.poll_added() {
            let id = outcome.intent.track.video_id.clone();
            if let Some(change) = self.optimistic_adds.remove(&id) {
                match outcome.result {
                    Ok(added) => {
                        self.suggestions.confirm(&id);
                        if !added.inserted {
                            self.playlist_count_delta(&added.playlist.id, -1);
                        }
                    }
                    Err(_) => {
                        self.suggestions.rollback(&id);
                        self.playlist_count_delta(&change.added.playlist.id, -1);
                        if change.appended
                            && self
                                .selected_playlist
                                .as_ref()
                                .is_some_and(|playlist| playlist.id == change.added.playlist.id)
                        {
                            self.tracks.retain(|track| track.video_id != id);
                        }
                        if let Some(catalog) = self
                            .catalog
                            .as_mut()
                            .filter(|catalog| catalog.id == change.added.playlist.id)
                        {
                            Arc::make_mut(&mut catalog.tracks).retain(|track| track.video_id != id);
                        }
                    }
                }
            }
            self.playlist_refresh_requested.insert(
                outcome.intent.playlist.id.clone(),
                outcome.intent.playlist.clone(),
            );
            self.playlist_cache.remove(&outcome.intent.playlist.id);
            self.cache_selected_playlist();
        }
        let mut index = 0;
        while index < self.playlist_refreshes.len() {
            let result = match self.playlist_refreshes[index].receiver.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err(GoogleError::Provider(ProviderError::Network))
                }
            };
            let job = self.playlist_refreshes.remove(index);
            if self
                .playlist_revisions
                .get(&job.choice.id)
                .copied()
                .unwrap_or(0)
                != job.revision
            {
                self.playlist_refresh_requested
                    .insert(job.choice.id.clone(), job.choice);
                continue;
            }
            if let Ok(snapshot) = result {
                self.apply_playlist_snapshot(snapshot);
            }
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        let ready: Vec<_> = self
            .playlist_refresh_requested
            .values()
            .filter(|choice| {
                !self
                    .playlist_refreshes
                    .iter()
                    .any(|job| job.choice.id == choice.id)
                    && !self
                        .optimistic_adds
                        .values()
                        .any(|change| change.added.playlist.id == choice.id)
            })
            .take(2usize.saturating_sub(self.playlist_refreshes.len()))
            .cloned()
            .collect();
        for choice in ready {
            self.playlist_refresh_requested.remove(&choice.id);
            let revision = self
                .playlist_revisions
                .get(&choice.id)
                .copied()
                .unwrap_or(0);
            let cancelled = Arc::new(AtomicBool::new(false));
            let (tx, rx) = mpsc::channel();
            self.playlist_refreshes.push(PlaylistRefresh {
                choice: choice.clone(),
                revision,
                receiver: rx,
                cancelled: cancelled.clone(),
            });
            let session = session.clone();
            let ctx = ctx.clone();
            thread::spawn(move || {
                let result = refresh_added_playlist(&session, &choice, &cancelled);
                if !cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(result);
                    ctx.request_repaint();
                }
            });
        }
    }

    fn apply_playlist_snapshot(&mut self, snapshot: PlaylistSnapshot) {
        self.playlist_cache
            .put(&snapshot.playlist.id, &snapshot.tracks, None);
        let id = &snapshot.playlist.id;
        if let Some(playlist) = self
            .playlists
            .iter_mut()
            .find(|playlist| &playlist.id == id)
        {
            *playlist = snapshot.playlist.clone();
        }
        if self
            .selected_playlist
            .as_ref()
            .is_some_and(|playlist| &playlist.id == id)
        {
            self.selected_playlist = Some(snapshot.playlist.clone());
            if !self.tracks_job && self.tracks_continuation.is_none() && self.tracks_error.is_none()
            {
                self.tracks = snapshot.tracks.clone();
            }
        }
        if self.queue.playlist_id.as_ref() == Some(id) {
            self.queue.extend_from(&snapshot.tracks);
            self.queue.mark_complete();
        }
        for track in &snapshot.tracks {
            self.suggestions.exclude(track);
            self.suggestions.confirm(&track.video_id);
        }
        if self.catalog_id.as_ref() == Some(id) {
            self.clear_catalog_job();
        }
        self.catalog = Some(PlaylistCatalog {
            id: snapshot.playlist.id,
            tracks: Arc::new(snapshot.tracks),
        });
    }

    pub fn play_suggestion(&mut self, ctx: &egui::Context, index: usize) {
        if let Some(mut queue) = PlaybackQueue::selected(
            self.suggestions.tracks.clone(),
            index,
            self.queue.repeat,
            self.queue.shuffle,
        ) {
            queue.inherit_recent(&self.queue);
            queue.mark_complete();
            self.request_queue(ctx, queue);
        }
    }

    pub fn play_track(&mut self, ctx: &egui::Context, index: usize) {
        if let Some(mut queue) = PlaybackQueue::selected(
            self.tracks.clone(),
            index,
            self.queue.repeat,
            self.queue.shuffle,
        ) {
            queue.inherit_recent(&self.queue);
            queue.playlist_id = self
                .selected_playlist
                .as_ref()
                .map(|playlist| playlist.id.clone());
            if !self.tracks_job && self.tracks_continuation.is_none() && self.tracks_error.is_none()
            {
                queue.mark_complete();
            }
            self.request_queue(ctx, queue);
        }
    }

    pub fn next(&mut self, ctx: &egui::Context, automatic: bool) {
        if self.radio_station.is_some() {
            return;
        }
        if automatic && self.import_busy {
            return;
        }
        if automatic && self.queue.repeat == Repeat::One && self.queue.current().is_some() {
            if let Some(audio) = &mut self.audio {
                if let Err(error) = audio.replay() {
                    audio.stop();
                    self.status = error;
                }
            }
            return;
        }
        if !self.ensure_full_queue(ctx, !automatic)
            && (self.queue.shuffle || self.queue.remaining() == 0)
        {
            self.pending_next = Some(automatic);
            return;
        }
        self.pending_next = None;
        let mut proposed = self.queue.clone();
        if proposed.forward(automatic) {
            self.request_queue(ctx, proposed);
        } else if automatic {
            if let Some(audio) = &mut self.audio {
                audio.poll_finished();
            }
            self.now_playing = None;
        }
    }

    pub fn previous(&mut self, ctx: &egui::Context) {
        if self.radio_station.is_some() {
            return;
        }
        self.pending_next = None;
        let restart = self.position().as_secs() > 3;
        let mut proposed = self.queue.clone();
        if (!restart && proposed.previous())
            || self.retry_position.is_some()
            || self.audio.as_ref().is_some_and(AudioPlayer::needs_reopen)
        {
            self.request_queue(ctx, proposed);
        } else if let Some(audio) = &mut self.audio {
            if let Err(error) = audio.replay() {
                self.status = error;
            }
        }
    }

    fn request_queue(&mut self, ctx: &egui::Context, queue: PlaybackQueue) {
        self.request_queue_at(ctx, queue, std::time::Duration::ZERO);
    }

    fn request_queue_at(
        &mut self,
        ctx: &egui::Context,
        queue: PlaybackQueue,
        position: std::time::Duration,
    ) {
        self.clear_level();
        let Some(track) = queue.current() else {
            return;
        };
        let url = format!("https://music.youtube.com/watch?v={}", track.video_id);
        if let Err(error) = youtube::check_url(&url) {
            self.status = error;
            return;
        }
        if self.audio.is_none() {
            self.status = "Sortie audio indisponible".into();
            return;
        }
        self.cancel_radio();
        if self.retry_position.is_some()
            || self.audio.as_ref().is_some_and(AudioPlayer::needs_reopen)
        {
            if let Err(error) = self.audio.as_mut().unwrap().reopen_output() {
                self.status = error;
                return;
            }
        }
        self.retry_position = None;
        self.import_position = position;
        self.pause_after_load = false;
        if let Some(cancelled) = self.cancel_import.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.import_receiver = None;
        self.now_playing = Some(track.clone());
        ctx.request_repaint();
        self.queue = queue.clone();
        self.pending_next = None;
        if self.catalog_id.is_some() && self.catalog_id != self.queue.playlist_id {
            self.clear_catalog_job();
        }
        self.ensure_full_queue(ctx, true);
        self.audio.as_mut().unwrap().stop();
        self.import_busy = true;
        self.status.clear();
        let (tx, rx) = mpsc::channel();
        self.import_receiver = Some(rx);
        if self.prefetch_id.as_deref() == Some(track.video_id.as_str()) {
            self.prefetch_id = None;
            self.cancel_import = self.cancel_prefetch.take();
            if let Some(prepared) = self.prefetched.take() {
                let _ = tx.send(Ok(prepared));
            } else {
                self.import_receiver = self.prefetch_receiver.take();
            }
            return;
        }
        self.clear_prefetch();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancel_import = Some(cancelled.clone());
        let session = self.session.clone();
        let ctx = ctx.clone();
        let normalize = self.normalization;
        thread::spawn(move || {
            let resolver = YouTubeStreamResolver;
            let result = if normalize {
                resolver.prepare_with_level(url, session, cancelled, false, true)
            } else {
                resolver.prepare(url, session, cancelled, false)
            }
            .map(|audio| PreparedTrack { audio, queue });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        self.poll_default_output(ctx);
        self.poll_radio();
        self.poll_playlist_management();
        self.poll_playlist_adds(ctx);
        let suggested =
            self.queue.playlist_id.is_none() && self.queue.current().is_some() && self.connected();
        self.playlist_add.observe(
            ctx,
            self.session.clone(),
            suggested.then(|| self.now_playing.clone()).flatten(),
        );
        self.suggestions.poll();
        self.poll_level(ctx);
        self.save_playback();
        self.poll_catalog(ctx);
        self.ensure_full_queue(ctx, false);
        if let Some((error, output_failed)) =
            self.audio.as_ref().and_then(|audio| audio.take_error())
        {
            if output_failed && !self.playback_paused() {
                self.resume_after_output_change = true;
            }
            if self.radio_station.is_some() {
                self.pause_radio();
                self.retry_position = Some(std::time::Duration::ZERO);
                self.status = format!("{error} Clique sur Lecture pour réessayer.");
            } else {
                let audio = self.audio.as_mut().unwrap();
                self.retry_position = Some(audio.position());
                audio.pause();
                self.clear_prefetch();
                self.status = format!("{error} Clique sur Lecture pour réessayer.");
            }
        }
        if let Some(receiver) = &self.prefetch_receiver {
            match receiver.try_recv() {
                Ok(Ok(prepared)) => {
                    self.prefetched = Some(prepared);
                    self.prefetch_receiver = None;
                }
                Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.prefetch_failed = self.prefetch_id.clone();
                    self.clear_prefetch();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.google_progress_receiver {
            for message in receiver.try_iter() {
                self.google_status = message.into();
            }
        }
        let finished = self
            .audio
            .as_ref()
            .is_some_and(|audio| audio.is_empty() && !audio.title.is_empty());
        if finished
            && self.radio_station.is_none()
            && !self.import_busy
            && self.pending_next.is_none()
        {
            self.next(ctx, true);
        }
        if let Some(receiver) = &self.import_receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    self.import_receiver = None;
                    self.cancel_import = None;
                    self.import_busy = false;
                    self.status = match result {
                        Ok(prepared) => {
                            let mut prepared = prepared;
                            prepared.queue = self.queue.clone();
                            if prepared.queue.playlist_id.is_some()
                                && prepared.queue.playlist_id.as_deref()
                                    == self
                                        .selected_playlist
                                        .as_ref()
                                        .map(|playlist| playlist.id.as_str())
                            {
                                prepared.queue.extend_from(&self.tracks);
                            }
                            let track = prepared
                                .queue
                                .current()
                                .expect("Prepared queue has a current track")
                                .clone();
                            let loaded = self
                                .audio
                                .as_mut()
                                .ok_or_else(|| "Sortie audio indisponible".to_owned())
                                .and_then(|player| {
                                    player.load_track(
                                        prepared.audio,
                                        track.title,
                                        track.artist,
                                        track.cover_url,
                                        self.import_position,
                                        self.pause_after_load,
                                    )
                                });
                            if loaded.is_ok() {
                                self.queue = prepared.queue;
                            } else if let Some(audio) = &mut self.audio {
                                audio.poll_finished();
                                self.retry_position = Some(self.import_position);
                            }
                            loaded.map(|_| String::new()).unwrap_or_else(|error| error)
                        }
                        Err(error) => {
                            self.retry_position = Some(self.import_position);
                            if let Some(audio) = &mut self.audio {
                                audio.poll_finished();
                            }
                            error
                        }
                    };
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.import_receiver = None;
                    self.cancel_import = None;
                    self.import_busy = false;
                    self.status = "La préparation audio s'est interrompue.".into();
                    self.retry_position = Some(self.import_position);
                    if let Some(audio) = &mut self.audio {
                        audio.poll_finished();
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        self.prefetch_next(ctx);
        let Some(receiver) = &self.google_receiver else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(GoogleError::Login(
                "La tâche Google s'est interrompue.".into(),
            )),
        };
        self.google_receiver = None;
        self.google_progress_receiver = None;
        self.cancel_login = None;
        self.cancel_browse = None;
        let tracks_failed = self.tracks_job && result.is_err();
        self.tracks_job = false;
        match result {
            Ok(GoogleResult::Connected {
                session,
                playlists,
                account,
            }) => {
                self.playlist_cache.clear();
                let saved = session::save(&session);
                self.session = Some(session);
                self.account = Some(account);
                self.playlists = playlists.items;
                self.playlists_continuation = playlists.continuation;
                self.google_status = match saved {
                    Ok(()) => String::new(),
                    Err(error) => format!("Session active uniquement en mémoire : {error}"),
                };
                if let Some(snapshot) = self.restore_pending.take().filter(|snapshot| {
                    self.radio_station.is_none()
                        && self
                            .session
                            .as_ref()
                            .is_some_and(|session| snapshot.belongs_to(session))
                }) {
                    self.restore(snapshot);
                }
            }
            Ok(GoogleResult::Library { page, replace }) => {
                if replace {
                    self.playlists.clear();
                }
                for mut item in page.items {
                    let pending = self
                        .optimistic_adds
                        .values()
                        .filter(|change| change.added.playlist.id == item.id)
                        .count() as u32;
                    item.track_count = item.track_count.map(|count| count.saturating_add(pending));
                    if let Some(existing) = self.playlists.iter_mut().find(|p| p.id == item.id) {
                        *existing = item;
                    } else {
                        self.playlists.push(item);
                    }
                }
                self.playlists_continuation = page.continuation;
                self.google_status.clear();
            }
            Ok(GoogleResult::Tracks {
                playlist,
                mut page,
                replace,
                revision,
            }) => {
                if self
                    .playlist_revisions
                    .get(&playlist.id)
                    .copied()
                    .unwrap_or(0)
                    != revision
                {
                    self.google_status.clear();
                    return;
                }
                if page.continuation.is_none() {
                    for change in self
                        .optimistic_adds
                        .values()
                        .filter(|change| change.added.playlist.id == playlist.id)
                    {
                        if !page
                            .items
                            .iter()
                            .any(|track| track.video_id == change.added.track.video_id)
                        {
                            page.items.push(change.added.track.clone());
                        }
                    }
                }
                if page.continuation.is_some() && page.continuation == self.tracks_page_requested {
                    self.tracks_error =
                        Some("YouTube Music n’a pas renvoyé la page suivante. Réessaie.".into());
                    self.google_status = self.tracks_error.clone().unwrap_or_default();
                    return;
                }
                if self
                    .selected_playlist
                    .as_ref()
                    .is_none_or(|current| current.id != playlist.id)
                {
                    self.selected_playlist = Some(playlist);
                }
                if replace {
                    self.tracks = page.items;
                } else {
                    self.tracks.extend(page.items);
                }
                self.tracks_continuation = page.continuation;
                self.cache_selected_playlist();
                if self.queue.playlist_id.is_some()
                    && self.queue.playlist_id.as_deref()
                        == self
                            .selected_playlist
                            .as_ref()
                            .map(|playlist| playlist.id.as_str())
                {
                    self.queue.extend_from(&self.tracks);
                    if self.tracks_continuation.is_none() {
                        self.queue.mark_complete();
                    }
                }
                self.google_status.clear();
            }
            Err(GoogleError::Provider(ProviderError::SessionExpired)) => {
                self.disconnect();
                self.google_status =
                    format!("{} {}", ProviderError::SessionExpired, self.google_status);
            }
            Err(GoogleError::Provider(error)) => self.google_status = error.to_string(),
            Err(GoogleError::Login(error)) => self.google_status = error,
        }
        if tracks_failed && self.selected_playlist.is_some() {
            self.tracks_error = Some(self.google_status.clone());
        }
    }

    fn clear_prefetch(&mut self) {
        if let Some(cancelled) = self.cancel_prefetch.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.prefetch_receiver = None;
        self.prefetched = None;
        self.prefetch_id = None;
    }

    fn prefetch_next(&mut self, ctx: &egui::Context) {
        if self.audio.as_ref().is_some_and(|audio| audio.is_paused()) {
            self.clear_prefetch();
            return;
        }
        if (self.queue.shuffle && !self.queue.is_complete())
            || (!self.queue.is_complete() && self.queue.remaining() == 0)
            || self.import_busy
            || self
                .audio
                .as_ref()
                .is_none_or(|audio| audio.title.is_empty())
        {
            return;
        }
        let Some(track) = self.queue.upcoming() else {
            self.clear_prefetch();
            return;
        };
        let id = track.video_id.clone();
        if self.prefetch_id.as_ref() == Some(&id) || self.prefetch_failed.as_ref() == Some(&id) {
            return;
        }
        let mut queue = self.queue.clone();
        queue.forward(false);
        let url = format!("https://music.youtube.com/watch?v={id}");
        self.clear_prefetch();
        self.prefetch_id = Some(id);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancel_prefetch = Some(cancelled.clone());
        let session = self.session.clone();
        let (tx, rx) = mpsc::channel();
        self.prefetch_receiver = Some(rx);
        let ctx = ctx.clone();
        let normalize = self.normalization;
        thread::spawn(move || {
            let resolver = YouTubeStreamResolver;
            let result = if normalize {
                resolver.prepare_with_level(url, session, cancelled, true, true)
            } else {
                resolver.prepare(url, session, cancelled, true)
            }
            .map(|audio| PreparedTrack { audio, queue });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub fn stop(&mut self) {
        self.resume_after_output_change = false;
        self.cancel_radio();
        self.clear_level();
        self.clear_catalog_job();
        if let Some(cancelled) = self.cancel_import.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.import_receiver = None;
        self.import_busy = false;
        self.now_playing = None;
        self.retry_position = None;
        self.import_position = std::time::Duration::ZERO;
        self.clear_prefetch();
        if let Some(audio) = &mut self.audio {
            audio.stop();
        }
        self.save_playback();
    }

    fn poll_default_output(&mut self, ctx: &egui::Context) {
        if !self
            .output_devices
            .as_mut()
            .is_some_and(|watcher| watcher.ready(ctx))
        {
            return;
        }
        let result = if let Some(audio) = &mut self.audio {
            audio.follow_default_output()
        } else {
            AudioPlayer::new().map(|audio| self.audio = Some(audio))
        };
        match result {
            Ok(()) => {
                self.output_devices.as_mut().unwrap().succeeded();
                self.audio_error = None;
                if self.resume_after_output_change {
                    self.resume_after_output_change = false;
                    self.retry_position = None;
                    if let Some(station) = self.radio_station.clone() {
                        self.play_radio(ctx, station);
                    } else if let Some(audio) = &mut self.audio {
                        if audio.is_paused() && !audio.is_empty() {
                            audio.toggle_pause();
                        }
                    }
                    self.status.clear();
                }
                ctx.request_repaint();
            }
            Err(error) => {
                if !self.output_devices.as_mut().unwrap().retry(ctx) {
                    self.status = error;
                }
            }
        }
    }
    fn cancel_radio(&mut self) {
        self.radio_now_playing = None;
        if let Some(cancel) = self.radio_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.radio_control = None;
        self.radio_receiver = None;
        self.radio_station = None;
        self.radio_paused = true;
    }
    pub fn play_radio(&mut self, ctx: &egui::Context, station: crate::radio::Station) {
        let Some(audio) = &mut self.audio else {
            self.status = "Sortie audio indisponible.".into();
            return;
        };
        if audio.needs_reopen() || self.retry_position.is_some() {
            if let Err(error) = audio.reopen_output() {
                self.status = error;
                return;
            }
        }
        self.stop();
        let smart = self.queue.smart_shuffle;
        let repeat = self.queue.repeat;
        let shuffle = self.queue.shuffle;
        self.queue = PlaybackQueue::default();
        self.queue.set_smart_shuffle(smart);
        self.queue.repeat = repeat;
        self.queue.shuffle = shuffle;
        self.radio_station = Some(station.clone());
        self.radio_paused = false;
        self.import_busy = true;
        self.status.clear();
        let cancel = Arc::new(AtomicBool::new(false));
        self.radio_cancel = Some(cancel.clone());
        let (tx, rx) = mpsc::channel();
        self.radio_receiver = Some(rx);
        let wake = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(crate::radio::prepare(station, cancel, wake.clone()));
            wake.request_repaint();
        });
        ctx.request_repaint();
    }
    fn pause_radio(&mut self) {
        let station = self.radio_station.take();
        let now_playing = self.radio_now_playing.take();
        self.cancel_radio();
        self.radio_station = station;
        self.radio_now_playing = now_playing;
        self.import_busy = false;
        if let Some(audio) = &mut self.audio {
            audio.stop();
        }
    }
    fn poll_radio(&mut self) {
        if let Some(control) = &self.radio_control {
            let now_playing = control.now_playing().and_then(|text| {
                self.radio_station
                    .as_ref()
                    .and_then(|station| station.song_title(&text))
            });
            if self.radio_now_playing != now_playing {
                self.radio_now_playing = now_playing;
                let status = self.radio_status().to_owned();
                if let Some(audio) = &mut self.audio {
                    audio.artist = status;
                }
            }
        }
        let result = self
            .radio_receiver
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("La connexion radio a été interrompue.".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            self.radio_receiver = None;
            self.import_busy = false;
            match result {
                Ok(prepared) => {
                    if let (Some(station), Some(audio)) =
                        (self.radio_station.as_ref(), &mut self.audio)
                    {
                        audio.load_radio(prepared.source, station.name());
                        self.radio_control = Some(prepared.control);
                    }
                }
                Err(error) => {
                    self.pause_radio();
                    self.status = error;
                }
            }
        }
        if let Some(error) = self
            .radio_control
            .as_ref()
            .and_then(|control| control.take_error())
        {
            self.pause_radio();
            self.status = error;
        }
    }
    pub fn radio_status(&self) -> &str {
        self.radio_now_playing
            .as_deref()
            .unwrap_or("Radio en direct")
    }
    pub fn radio_buffering(&self) -> bool {
        self.radio_receiver.is_some()
            || self
                .radio_control
                .as_ref()
                .is_some_and(|control| control.buffering())
    }
    #[cfg(feature = "ui-capture")]
    pub fn qa_radio_paused(&mut self, station: crate::radio::Station) {
        self.cancel_radio();
        self.radio_station = Some(station);
        self.radio_paused = true;
        self.now_playing = None;
        if let Some(audio) = &mut self.audio {
            audio.stop();
        }
    }
    pub fn display_track(&self) -> Option<Track> {
        self.radio_station
            .as_ref()
            .map(|station| Track {
                video_id: format!("radio:{}", station.name()),
                title: station.name().into(),
                artist: self.radio_status().into(),
                ..Default::default()
            })
            .or_else(|| self.now_playing.clone())
    }

    pub fn position(&self) -> std::time::Duration {
        if self.import_busy {
            return self.import_position;
        }
        self.retry_position.unwrap_or_else(|| {
            self.audio
                .as_ref()
                .map_or(std::time::Duration::ZERO, AudioPlayer::position)
        })
    }

    fn restore(&mut self, snapshot: crate::playback_store::Snapshot) {
        let smart = self.queue.smart_shuffle;
        self.queue = snapshot.queue;
        self.queue.set_smart_shuffle(smart);
        self.now_playing = self.queue.current().cloned();
        self.retry_position = Some(snapshot.position);
        if let Some(audio) = &mut self.audio {
            audio.set_volume(snapshot.volume);
            audio.pause();
        }
    }

    fn snapshot(&self) -> Option<crate::playback_store::Snapshot> {
        self.now_playing.as_ref()?;
        Some(crate::playback_store::Snapshot::new(
            self.session.as_ref()?,
            self.queue.clone(),
            self.position(),
            self.audio.as_ref()?.volume,
        ))
    }

    fn save_playback(&mut self) {
        if self.radio_station.is_some() {
            return;
        }
        let Some(store) = &self.store else {
            return;
        };
        if let Some(error) = store.error() {
            self.status = error;
        }
        // Save immediately on queue/mode/volume changes, and about every ten seconds.
        let stamp = self.now_playing.as_ref().map(|_| {
            (
                self.queue.revision(),
                self.position().as_secs() / 10,
                self.audio
                    .as_ref()
                    .map_or(0, |audio| (audio.volume * 1000.0).round() as u32),
            )
        });
        if stamp != self.saved_revision && store.enqueue(self.snapshot()) {
            self.saved_revision = stamp;
        }
    }
    pub fn toggle_pause(&mut self, ctx: &egui::Context) {
        if let Some(station) = self.radio_station.clone() {
            if self.radio_paused {
                self.play_radio(ctx, station);
            } else {
                self.pause_radio();
            }
            ctx.request_repaint();
            return;
        }
        if self.import_busy {
            return;
        }
        let recovery = self.retry_position.or_else(|| {
            self.audio
                .as_ref()
                .filter(|audio| audio.is_paused() && audio.needs_reopen())
                .map(AudioPlayer::position)
        });
        if let Some(position) = recovery {
            self.clear_prefetch();
            self.request_queue_at(ctx, self.queue.clone(), position);
            return;
        }
        if let Some(audio) = &mut self.audio {
            audio.toggle_pause();
        }
        if self.audio.as_ref().is_some_and(|audio| audio.is_paused()) {
            self.clear_prefetch();
        }
    }

    pub fn set_playing(&mut self, ctx: &egui::Context, playing: bool) {
        if !playing {
            self.resume_after_output_change = false;
        }
        if self.radio_station.is_some() {
            if playing == self.radio_paused {
                self.toggle_pause(ctx);
            }
            return;
        }
        if self.import_busy {
            self.pause_after_load = !playing;
            if !playing {
                self.clear_prefetch();
            }
            return;
        }
        if self.now_playing.is_none() {
            return;
        }
        let paused = self
            .audio
            .as_ref()
            .is_none_or(|audio| audio.is_paused() || audio.is_empty());
        if playing == paused {
            self.toggle_pause(ctx);
        }
    }

    pub fn playback_paused(&self) -> bool {
        if self.radio_station.is_some() {
            return self.radio_paused;
        }
        if self.import_busy {
            return self.pause_after_load;
        }
        self.retry_position.is_some()
            || self
                .audio
                .as_ref()
                .is_none_or(|audio| audio.is_paused() || audio.is_empty())
    }

    pub fn seek(&mut self, position: std::time::Duration) -> Result<(), String> {
        let duration = self
            .audio
            .as_ref()
            .and_then(|audio| audio.duration)
            .or_else(|| {
                self.now_playing
                    .as_ref()?
                    .duration_secs
                    .map(|seconds| std::time::Duration::from_secs(seconds.into()))
            })
            .ok_or("Durée indisponible.")?;
        let position = position.min(duration);
        if self.import_busy {
            self.import_position = position;
            return Ok(());
        }
        if self.retry_position.is_some() {
            self.retry_position = Some(position);
            return Ok(());
        }
        self.audio
            .as_ref()
            .ok_or("Sortie audio indisponible.")?
            .seek(position)
    }

    pub fn edit_queue(&mut self, ctx: &egui::Context, action: crate::queue_panel::Action) {
        use crate::queue_panel::Action;
        match action {
            Action::Toggle => self.toggle_pause(ctx),
            Action::Play(position) => {
                let mut queue = self.queue.clone();
                if queue.play_upcoming(position) {
                    self.request_queue(ctx, queue);
                }
            }
            Action::Remove(position) => {
                if self.queue.remove_upcoming(position) {
                    self.clear_prefetch();
                    self.prefetch_failed = None;
                }
            }
            Action::Move(from, to) => {
                if self.queue.move_upcoming(from, to) {
                    self.clear_prefetch();
                    self.prefetch_failed = None;
                }
            }
        }
        self.save_playback();
    }

    pub fn shuffle_busy(&self) -> bool {
        self.queue.shuffle && !self.queue.is_complete() && self.catalog_receiver.is_some()
    }
    pub fn can_next(&self) -> bool {
        if self.radio_station.is_some() {
            return false;
        }
        !(self.queue_busy() && (self.queue.shuffle || self.queue.remaining() == 0))
            && (self.queue.can_next()
                || (!self.queue.is_complete() && self.queue.current().is_some()))
    }

    pub fn queue_busy(&self) -> bool {
        !self.queue.is_complete() && self.catalog_receiver.is_some()
    }
    pub fn toggle_shuffle(&mut self, ctx: &egui::Context) {
        self.queue.toggle_shuffle();
        self.clear_prefetch();
        self.pending_next = None;
        if self.queue.shuffle {
            self.ensure_full_queue(ctx, true);
        } else {
            self.clear_catalog_job();
        }
    }

    pub fn set_smart_shuffle(&mut self, enabled: bool) {
        self.queue.set_smart_shuffle(enabled);
        self.clear_prefetch();
        self.save_playback();
    }
    pub fn set_normalization(&mut self, enabled: bool) {
        self.normalization = enabled;
        self.clear_level();
        if let Some(audio) = &mut self.audio {
            audio.set_normalization(enabled);
        }
    }
    fn clear_level(&mut self) {
        if let Some(cancel) = self.level_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.level_receiver = None;
        self.level_attempted = None;
    }
    fn poll_level(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.level_receiver {
            if let Ok((id, level)) = receiver.try_recv() {
                self.level_receiver = None;
                self.level_cancel = None;
                if self.normalization
                    && self
                        .now_playing
                        .as_ref()
                        .is_some_and(|track| track.video_id == id)
                {
                    if let Some(audio) = &mut self.audio {
                        audio.set_loudness(level);
                    }
                }
            }
        }
        if !self.normalization
            || self.import_busy
            || self
                .audio
                .as_ref()
                .is_none_or(|audio| audio.is_paused() || audio.is_empty() || audio.loudness_known())
        {
            return;
        }
        let Some(track) = &self.now_playing else {
            return;
        };
        if self.level_attempted.as_ref() == Some(&track.video_id) {
            return;
        }
        let id = track.video_id.clone();
        self.level_attempted = Some(id.clone());
        let session = self.session.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.level_cancel = Some(cancelled.clone());
        let (sender, receiver) = mpsc::channel();
        self.level_receiver = Some(receiver);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let source = format!("https://youtube.com/watch?v={id}");
            let level = crate::direct_audio::ensure_loudness(&source, session.as_ref(), &cancelled);
            let _ = sender.send((id, level));
            ctx.request_repaint();
        });
    }
    fn clear_catalog_job(&mut self) {
        if self.catalog_error.as_ref() == Some(&self.status) {
            self.status.clear();
        }
        if let Some(cancelled) = self.cancel_catalog.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.catalog_receiver = None;
        self.catalog_id = None;
        self.catalog_error = None;
        self.pending_next = None;
    }
    fn ensure_full_queue(&mut self, ctx: &egui::Context, retry: bool) -> bool {
        if self.queue.is_complete()
            || self.queue.current().is_none()
            || (!self.queue.shuffle && self.queue.remaining() > 5)
        {
            return true;
        }
        let Some(id) = self.queue.playlist_id.clone() else {
            self.queue.mark_complete();
            return true;
        };
        if let Some(tracks) = self
            .catalog
            .as_ref()
            .filter(|catalog| catalog.id == id)
            .map(|catalog| catalog.tracks.clone())
        {
            self.queue.complete_from(&tracks);
            self.clear_prefetch();
            return true;
        }
        if self.catalog_id.as_ref() == Some(&id) && self.catalog_receiver.is_some() {
            return false;
        }
        if self.catalog_id.as_ref() == Some(&id) && self.catalog_error.is_some() && !retry {
            return false;
        }
        self.clear_catalog_job();
        let Some(session) = self.session.clone() else {
            self.catalog_id = Some(id);
            self.catalog_error = Some("Reconnecte-toi pour préparer la playlist entière.".into());
            self.status = self.catalog_error.clone().unwrap();
            return false;
        };
        self.catalog_id = Some(id.clone());
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancel_catalog = Some(cancelled.clone());
        let (sender, receiver) = mpsc::channel();
        self.catalog_receiver = Some(receiver);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = (|| {
                let provider = YouTubeMusic::new(&session)?;
                let tracks = collect_playlist(&cancelled, |continuation| {
                    provider.playlist_tracks(&session, &id, continuation)
                })?;
                Ok(PlaylistCatalog {
                    id,
                    tracks: Arc::new(tracks),
                })
            })();
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        false
    }
    fn poll_catalog(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.catalog_receiver else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(GoogleError::Login("Préparation interrompue.".into()))
            }
        };
        self.catalog_receiver = None;
        self.cancel_catalog = None;
        match result {
            Ok(mut catalog) => {
                for change in self
                    .optimistic_adds
                    .values()
                    .filter(|change| change.added.playlist.id == catalog.id)
                {
                    if !catalog
                        .tracks
                        .iter()
                        .any(|track| track.video_id == change.added.track.video_id)
                    {
                        Arc::make_mut(&mut catalog.tracks).push(change.added.track.clone());
                    }
                }
                if self.queue.playlist_id.as_ref() == Some(&catalog.id) {
                    self.queue.complete_from(&catalog.tracks);
                    self.clear_prefetch();
                }
                self.catalog = Some(catalog);
                self.catalog_error = None;
                if let Some(automatic) = self.pending_next.take() {
                    self.next(ctx, automatic);
                }
            }
            Err(GoogleError::Provider(ProviderError::SessionExpired)) => {
                self.disconnect();
                self.google_status = ProviderError::SessionExpired.to_string();
            }
            Err(error) => {
                let reason = match error {
                    GoogleError::Provider(error) => error.to_string(),
                    GoogleError::Login(error) => error,
                };
                let message = format!("Impossible de préparer la playlist entière. {reason} Appuie sur Suivant pour réessayer.");
                self.status = message.clone();
                self.catalog_error = Some(message);
            }
        }
    }
}

fn refresh_added_playlist(
    session: &Session,
    choice: &crate::provider::PlaylistChoice,
    cancelled: &AtomicBool,
) -> Result<PlaylistSnapshot, GoogleError> {
    let provider = YouTubeMusic::new(session)?;
    let tracks = collect_playlist(cancelled, |continuation| {
        provider.playlist_tracks(session, &choice.id, continuation)
    })?;
    let mut playlist = Playlist {
        id: choice.id.clone(),
        title: choice.title.clone(),
        cover_url: choice.cover_url.clone(),
        track_count: Some(tracks.len() as u32),
        ..Default::default()
    };
    let mut continuation = None;
    let mut seen = HashSet::new();
    for _ in 0..128 {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GoogleError::Login("Actualisation annulée.".into()));
        }
        let page = provider.library_playlists(session, continuation.as_deref())?;
        if let Some(found) = page
            .items
            .into_iter()
            .find(|playlist| playlist.id == choice.id)
        {
            playlist = found;
            break;
        }
        continuation = page.continuation;
        match &continuation {
            Some(token) if seen.insert(token.clone()) => {}
            _ => break,
        }
    }
    Ok(PlaylistSnapshot { playlist, tracks })
}

fn collect_playlist(
    cancelled: &AtomicBool,
    mut fetch: impl FnMut(Option<&str>) -> Result<Page<Track>, ProviderError>,
) -> Result<Vec<Track>, GoogleError> {
    let mut tracks = Vec::new();
    let mut continuation: Option<String> = None;
    let mut seen = HashSet::new();
    let mut bytes = 0usize;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GoogleError::Login("Préparation annulée.".into()));
        }
        let page = fetch(continuation.as_deref())?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(GoogleError::Login("Préparation annulée.".into()));
        }
        bytes += page
            .items
            .iter()
            .map(|track| {
                track.video_id.len()
                    + track.title.len()
                    + track.artist.len()
                    + track.album.len()
                    + track.cover_url.as_ref().map_or(0, String::len)
            })
            .sum::<usize>();
        if tracks.len() + page.items.len() > 50_000 || bytes > 32 * 1024 * 1024 {
            return Err(GoogleError::Login(
                "Playlist trop volumineuse pour la file.".into(),
            ));
        }
        tracks.extend(page.items);
        let Some(token) = page.continuation else {
            return Ok(tracks);
        };
        if !seen.insert(token.clone()) || seen.len() > 1000 {
            return Err(GoogleError::Login(
                "La pagination de la playlist ne progresse plus.".into(),
            ));
        }
        continuation = Some(token);
    }
}

fn connect(session: Session) -> Result<GoogleResult, GoogleError> {
    crate::player_solver::warm();
    let provider = YouTubeMusic::new(&session)?;
    let account = provider.validate_session(&session)?;
    let playlists = provider.library_playlists(&session, None)?;
    Ok(GoogleResult::Connected {
        session,
        playlists,
        account,
    })
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.cancel_radio();
        if let Some(cancelled) = &self.cancel_browse {
            cancelled.store(true, Ordering::Relaxed);
        }
        for job in &self.playlist_refreshes {
            job.cancelled.store(true, Ordering::Relaxed);
        }
        self.clear_level();
        if let Some(store) = self.store.take() {
            store.flush(self.snapshot().or_else(|| self.restore_pending.take()));
        }
        if let Some(cancelled) = &self.cancel_catalog {
            cancelled.store(true, Ordering::Relaxed);
        }
        if let Some(cancelled) = &self.cancel_import {
            cancelled.store(true, Ordering::Relaxed);
        }
        if let Some(cancelled) = &self.cancel_prefetch {
            cancelled.store(true, Ordering::Relaxed);
        }
        if let Some(cancelled) = &self.cancel_login {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    #[test]
    #[ignore = "Réseau et sortie Windows muette : injecte les notifications de routage, sans modifier les périphériques Windows."]
    fn default_output_events_transfer_live_radio_and_respect_explicit_pause_after_loss() {
        let ctx = egui::Context::default();
        let mut state = AppState::empty();
        state.audio = Some(AudioPlayer::new().unwrap());
        state.audio.as_mut().unwrap().set_volume(0.0);
        state.output_devices = Some(crate::audio_devices::Watcher::new(&ctx).unwrap());
        state.play_radio(&ctx, crate::radio::Station::Nrj);
        until(&mut state, &ctx, |state| !state.import_busy);
        assert!(!state.playback_paused());
        let original_connection = state.radio_cancel.clone().unwrap();
        let generation = state.audio.as_ref().unwrap().routing_generation();
        state.output_devices.as_ref().unwrap().notify_test_change();
        until(&mut state, &ctx, |state| {
            state.audio.as_ref().unwrap().routing_generation() > generation
        });
        assert!(Arc::ptr_eq(
            &original_connection,
            state.radio_cancel.as_ref().unwrap()
        ));
        assert!(!state.playback_paused() && !original_connection.load(Ordering::SeqCst));
        // A genuine output failure pauses the source; a later default-output event recovers intent.
        state.audio.as_ref().unwrap().simulate_output_loss();
        state.poll(&ctx);
        assert!(state.playback_paused() && state.resume_after_output_change);
        let generation = state.audio.as_ref().unwrap().routing_generation();
        state.output_devices.as_ref().unwrap().notify_test_change();
        until(&mut state, &ctx, |state| {
            state.audio.as_ref().unwrap().routing_generation() > generation && !state.import_busy
        });
        assert!(!state.playback_paused());
        assert_eq!(state.audio.as_ref().unwrap().volume, 0.0);
        // An explicit Pause after a loss must cancel automatic resumption.
        state.audio.as_ref().unwrap().simulate_output_loss();
        state.poll(&ctx);
        state.set_playing(&ctx, false);
        assert!(!state.resume_after_output_change);
        let generation = state.audio.as_ref().unwrap().routing_generation();
        state.output_devices.as_ref().unwrap().notify_test_change();
        until(&mut state, &ctx, |state| {
            state.audio.as_ref().unwrap().routing_generation() > generation
        });
        assert!(state.playback_paused() && state.radio_control.is_none() && !state.import_busy);
        state.stop();
    }
    #[test]
    #[ignore = "Réseau et sortie audio Windows muette ; teste les deux radios sans session Google ni écriture de fichier musical."]
    fn radios_play_pause_resume_and_switch_without_google() {
        let ctx = egui::Context::default();
        let mut state = AppState::empty();
        state.audio = Some(AudioPlayer::new().unwrap());
        state.audio.as_mut().unwrap().set_volume(0.0);
        for station in crate::radio::STATIONS {
            state.play_radio(&ctx, station.clone());
            until(&mut state, &ctx, |state| !state.import_busy);
            assert!(
                !state.playback_paused(),
                "{}: {}",
                station.name(),
                state.status
            );
            assert!(state.audio.as_ref().unwrap().duration.is_none());
            assert_eq!(state.display_track().unwrap().title, station.name());
            assert!(state.now_playing.is_none() && !state.connected());
            state.set_playing(&ctx, false);
            assert!(state.playback_paused() && state.radio_control.is_none());
            state.set_playing(&ctx, true);
            until(&mut state, &ctx, |state| !state.import_busy);
            assert!(
                !state.playback_paused(),
                "Reprise {}: {}",
                station.name(),
                state.status
            );
        }
        state.stop();
        assert!(state.radio_station.is_none());
        assert!(state.audio.as_ref().unwrap().is_empty());
        eprintln!("Radios : lecture, pause, retour au direct, changement de station et arrêt validés sur la sortie native muette, sans compte Google.");
    }
    #[test]
    fn pausing_a_loading_radio_cancels_the_request_and_keeps_its_station() {
        let ctx = egui::Context::default();
        let mut state = AppState::empty();
        state.radio_station = Some(crate::radio::Station::Nrj);
        state.radio_paused = false;
        state.import_busy = true;
        let cancel = Arc::new(AtomicBool::new(false));
        state.radio_cancel = Some(cancel.clone());
        let (tx, rx) = mpsc::channel();
        state.radio_receiver = Some(rx);
        state.set_playing(&ctx, false);
        assert!(cancel.load(Ordering::SeqCst));
        assert!(!state.import_busy);
        assert!(state.playback_paused());
        assert_eq!(state.radio_station, Some(crate::radio::Station::Nrj));
        assert!(tx.send(Err("late result".into())).is_err());
        assert!(!state.can_next());
        assert!(state.now_playing.is_none());
        assert_eq!(state.display_track().unwrap().title, "NRJ");
        state.stop();
        assert!(state.radio_station.is_none());
        assert!(state.display_track().is_none());
    }
    #[test]
    fn revalidation_keeps_an_add_which_is_still_being_confirmed() {
        let ctx = egui::Context::default();
        let mut state = AppState::empty();
        let playlist = Playlist {
            id: "PLtest".into(),
            title: "Playlist".into(),
            track_count: Some(1),
            ..Default::default()
        };
        let old = Track {
            video_id: "old".into(),
            ..Default::default()
        };
        let new = Track {
            video_id: "new".into(),
            ..Default::default()
        };
        state.selected_playlist = Some(playlist.clone());
        state.tracks = vec![old.clone()];
        state.apply_optimistic_add(crate::playlist_add::Added {
            track: new.clone(),
            playlist: crate::provider::PlaylistChoice {
                id: playlist.id.clone(),
                title: playlist.title.clone(),
                cover_url: None,
                contains: Some(false),
            },
            inserted: true,
        });
        let (tx, rx) = mpsc::channel();
        state.google_receiver = Some(rx);
        state.tracks_job = true;
        tx.send(Ok(GoogleResult::Tracks {
            playlist,
            page: Page {
                items: vec![old.clone()],
                continuation: None,
            },
            replace: true,
            revision: 1,
        }))
        .unwrap();
        state.poll(&ctx);
        assert_eq!(state.tracks, vec![old, new]);
        assert_eq!(
            state.playlist_cache.get("PLtest").unwrap().tracks,
            state.tracks
        );
    }
    #[test]
    #[ignore = "Session DPAPI et réseau ; cache, revalidation et bibliothèque, sans son ni écriture."]
    fn cached_browsing_keeps_rows_and_library_visible_live() {
        let ctx = egui::Context::default();
        let mut state = AppState::new(&ctx);
        state.store = None;
        state.restore_pending = None;
        until(&mut state, &ctx, |state| {
            state.connected() && !state.google_busy()
        });
        let playlist = state
            .playlists
            .iter()
            .find(|p| p.track_count.is_some_and(|n| n > 100))
            .unwrap()
            .clone();
        state.open_playlist(&ctx, playlist.clone(), false);
        until(&mut state, &ctx, |state| !state.google_busy());
        assert!(!state.tracks.is_empty());
        state.open_playlist(&ctx, playlist.clone(), true);
        until(&mut state, &ctx, |state| !state.google_busy());
        let known = state.tracks.clone();
        assert!(known.len() > 100);
        state.leave_playlist();
        state.open_playlist(&ctx, playlist, false);
        assert!(state.google_busy());
        assert_eq!(
            state.tracks, known,
            "Cache must be visible before the worker returns"
        );
        until(&mut state, &ctx, |state| !state.google_busy());
        assert_eq!(
            state.tracks, known,
            "Revalidation must not append cached rows twice"
        );
        state.leave_playlist();
        let library = state.playlists.clone();
        state.refresh_library(&ctx, false);
        assert!(state.google_busy());
        assert_eq!(state.playlists, library);
        until(&mut state, &ctx, |state| !state.google_busy());
        assert!(!state.playlists.is_empty());
        eprintln!("Réouverture immédiate : {} titres conservés pendant vérification ; bibliothèque conservée pendant actualisation. Aucune écriture ni lecture audio.", known.len());
    }
    #[test]
    fn refresh_preserves_visible_data_on_error_and_replaces_atomically_without_duplicates() {
        let ctx = egui::Context::default();
        let mut state = AppState::empty();
        let old = Playlist {
            id: "old".into(),
            title: "Ancienne".into(),
            ..Default::default()
        };
        let new = Playlist {
            id: "new".into(),
            title: "Nouvelle".into(),
            ..Default::default()
        };
        state.playlists = vec![old.clone()];
        let (tx, rx) = mpsc::channel();
        state.google_receiver = Some(rx);
        state.poll(&ctx);
        assert_eq!(state.playlists, vec![old.clone()]);
        tx.send(Err(GoogleError::Provider(ProviderError::Network)))
            .unwrap();
        state.poll(&ctx);
        assert_eq!(state.playlists, vec![old]);
        let (tx, rx) = mpsc::channel();
        state.google_receiver = Some(rx);
        tx.send(Ok(GoogleResult::Library {
            page: Page {
                items: vec![new.clone()],
                continuation: None,
            },
            replace: true,
        }))
        .unwrap();
        state.poll(&ctx);
        assert_eq!(state.playlists, vec![new.clone()]);
        state.selected_playlist = Some(new.clone());
        let track = Track {
            video_id: "same".into(),
            ..Default::default()
        };
        state.tracks = vec![track.clone()];
        let (tx, rx) = mpsc::channel();
        state.google_receiver = Some(rx);
        state.tracks_job = true;
        tx.send(Ok(GoogleResult::Tracks {
            playlist: new,
            page: Page {
                items: vec![track.clone(), track.clone()],
                continuation: None,
            },
            replace: true,
            revision: 0,
        }))
        .unwrap();
        state.poll(&ctx);
        assert_eq!(
            state.tracks.len(),
            2,
            "Two real entries are preserved, the cached one is not appended"
        );
        state.leave_playlist();
        let cached = state.playlist_cache.get("new").unwrap();
        assert_eq!(cached.tracks, vec![track.clone(), track]);
        state.playlist_revisions.insert("new".into(), 2);
        let (tx, rx) = mpsc::channel();
        state.google_receiver = Some(rx);
        tx.send(Ok(GoogleResult::Tracks {
            playlist: Playlist {
                id: "new".into(),
                ..Default::default()
            },
            page: Page {
                items: vec![Track::default()],
                continuation: None,
            },
            replace: true,
            revision: 1,
        }))
        .unwrap();
        state.poll(&ctx);
        assert!(
            state.tracks.is_empty(),
            "A read started before a mutation cannot overwrite newer data"
        );
    }
    #[test]
    #[ignore = "Session DPAPI et réseau ; relit la playlist complète et ses métadonnées sans écrire."]
    fn playlist_refresh_reads_authoritative_metadata_without_writing() {
        let session = session::load().unwrap().expect("Session enregistrée");
        let provider = YouTubeMusic::new(&session).unwrap();
        let playlist = provider
            .library_playlists(&session, None)
            .unwrap()
            .items
            .remove(0);
        let choice = crate::provider::PlaylistChoice {
            id: playlist.id.clone(),
            title: playlist.title,
            cover_url: playlist.cover_url,
            contains: Some(false),
        };
        let snapshot = refresh_added_playlist(&session, &choice, &AtomicBool::new(false)).unwrap();
        assert_eq!(snapshot.playlist.id, choice.id);
        assert!(!snapshot.tracks.is_empty());
        assert!(snapshot
            .playlist
            .track_count
            .is_some_and(|count| count >= snapshot.tracks.len() as u32));
        eprintln!(
            "Actualisation complète : {} titres et total serveur relus, aucune écriture.",
            snapshot.tracks.len()
        );
    }

    #[test]
    #[ignore = "Session DPAPI, réseau et sortie Windows ; suggestions puis lecture muette, sans modifier les playlists."]
    fn suggestions_play_in_an_independent_queue_without_changing_the_open_playlist() {
        let ctx = egui::Context::default();
        let mut state = AppState::new(&ctx);
        state.audio.as_mut().unwrap().set_volume(0.0);
        until(&mut state, &ctx, |state| {
            state.connected() && !state.playlists.is_empty()
        });
        let playlist = state.playlists[0].clone();
        state.open_playlist(&ctx, playlist.clone(), false);
        until(&mut state, &ctx, |state| !state.google_busy());
        let source = state.tracks.clone();
        state.now_playing = source.first().cloned();
        state.load_suggestions(&ctx, false);
        until(&mut state, &ctx, |state| !state.suggestions.busy());
        assert!(
            state.suggestions.error.is_none(),
            "{:?}",
            state.suggestions.error
        );
        assert!(!state.suggestions.tracks.is_empty());
        let expected = state.suggestions.tracks[0].clone();
        assert_ne!(expected.video_id, source[0].video_id);
        let count = state.suggestions.tracks.len();
        state.play_suggestion(&ctx, 0);
        assert_eq!(state.now_playing.as_ref(), Some(&expected));
        assert!(state.import_busy);
        assert!(state.queue.is_complete() && state.queue.playlist_id.is_none());
        assert_eq!(state.selected_playlist.as_ref(), Some(&playlist));
        assert_eq!(state.tracks, source);
        until(&mut state, &ctx, |state| !state.import_busy);
        assert_eq!(
            state.audio.as_ref().unwrap().title,
            expected.title,
            "{}",
            state.status
        );
        state.set_playing(&ctx, false);
        assert_eq!(state.queue.entries().count(), count);
        assert!(state.audio.as_ref().unwrap().is_paused());
        assert_eq!(state.tracks, source);
        eprintln!("Suggestions : {count} titres ; sélection immédiate, audio natif et pause vérifiés ; playlist inchangée.");
        state.stop();
    }

    #[test]
    #[ignore = "Session DPAPI, réseau et sortie Windows ; restaure en pause puis étend la lecture séquentielle de 100 à toute la playlist sans défiler."]
    fn restored_paused_queue_continues_sequentially_past_the_visible_page() {
        let ctx = egui::Context::default();
        let mut state = AppState::new(&ctx);
        state.audio.as_mut().unwrap().set_volume(0.0);
        until(&mut state, &ctx, |state| {
            state.connected() && !state.playlists.is_empty()
        });
        let playlist = state.playlists[0].clone();
        state.open_playlist(&ctx, playlist.clone(), false);
        until(&mut state, &ctx, |state| state.tracks.len() >= 100);
        assert!(state.tracks_continuation.is_some());
        let mut queue =
            PlaybackQueue::selected(state.tracks.clone(), 95, Repeat::Off, false).unwrap();
        queue.playlist_id = Some(playlist.id.clone());
        let snapshot = crate::playback_store::Snapshot::new(
            state.session.as_ref().unwrap(),
            queue,
            std::time::Duration::from_secs(43),
            0.0,
        );
        state.restore(snapshot);
        assert!(state.audio.as_ref().unwrap().is_paused());
        assert!(state.audio.as_ref().unwrap().is_empty());
        assert!(!state.import_busy);
        assert_eq!(state.position(), std::time::Duration::from_secs(43));
        assert_eq!(state.now_playing.as_ref(), state.tracks.get(95));
        until(&mut state, &ctx, |state| state.queue.is_complete());
        assert_eq!(state.tracks.len(), 100);
        assert!(state.tracks_continuation.is_some());
        let full = state.catalog.as_ref().unwrap().tracks.clone();
        assert!(full.len() > 100);
        for expected in full.iter().skip(96) {
            assert!(state.queue.forward(false));
            assert_eq!(state.queue.current(), Some(expected));
        }
        assert!(!state.queue.forward(false));
        assert!(!state.import_busy);
        assert!(state.audio.as_ref().unwrap().is_empty());
        eprintln!(
            "Sequential queue: {} visible / {} playable, restored paused at 43 s.",
            state.tracks.len(),
            full.len()
        );
        state.stop();
    }
    use std::time::{Duration, Instant};

    #[test]
    fn shuffle_catalog_collects_all_pages_in_order_without_truncating_to_the_visible_page() {
        let source: Vec<_> = (0..611)
            .map(|index| Track {
                video_id: index.to_string(),
                ..Default::default()
            })
            .collect();
        let mut calls = 0;
        let tracks = collect_playlist(&AtomicBool::new(false), |continuation| {
            calls += 1;
            let start = continuation.unwrap_or("0").parse::<usize>().unwrap();
            let end = (start + 100).min(source.len());
            Ok(Page {
                items: source[start..end].to_vec(),
                continuation: (end < source.len()).then(|| end.to_string()),
            })
        })
        .unwrap();
        assert_eq!(tracks, source);
        assert_eq!(calls, 7);
    }

    #[test]
    fn shuffle_catalog_stops_on_cancellation_errors_and_cyclic_pagination() {
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        let result = collect_playlist(&cancel, |_| {
            calls += 1;
            cancel.store(true, Ordering::Relaxed);
            Ok(Page {
                items: vec![],
                continuation: Some("next".into()),
            })
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
        let mut calls = 0;
        let result = collect_playlist(&AtomicBool::new(false), |_| {
            calls += 1;
            Ok(Page {
                items: vec![],
                continuation: Some(if calls % 2 == 0 { "b" } else { "a" }.into()),
            })
        });
        assert!(result.is_err());
        assert_eq!(calls, 3);
        let mut calls = 0;
        let result = collect_playlist(&AtomicBool::new(false), |_| {
            calls += 1;
            if calls == 1 {
                Ok(Page {
                    items: vec![Track::default()],
                    continuation: Some("next".into()),
                })
            } else {
                Err(ProviderError::Network)
            }
        });
        assert!(matches!(
            result,
            Err(GoogleError::Provider(ProviderError::Network))
        ));
        assert_eq!(calls, 2);
    }

    #[test]
    #[ignore = "Session enregistrée, Internet et sortie audio ; vérifie l'aléatoire sur toute la playlist sans défilement, sans jouer d'audio."]
    fn shuffle_reaches_the_full_real_playlist_while_only_one_page_is_visible() {
        let ctx = egui::Context::default();
        let mut state = AppState::new(&ctx);
        state.audio.as_mut().unwrap().set_volume(0.0);
        until(&mut state, &ctx, |state| state.connected());
        let playlist = state.playlists.first().unwrap().clone();
        state.open_playlist(&ctx, playlist.clone(), false);
        until(&mut state, &ctx, |state| !state.tracks.is_empty());
        assert!(
            state.tracks_continuation.is_some(),
            "Ce contrôle nécessite une playlist de plusieurs pages."
        );
        let visible = state.tracks.len();
        let mut queue =
            PlaybackQueue::selected(state.tracks.clone(), visible / 2, Repeat::Off, false).unwrap();
        queue.playlist_id = Some(playlist.id);
        let initial = queue.current().unwrap().video_id.clone();
        state.queue = queue;
        state.toggle_shuffle(&ctx);
        assert!(state.shuffle_busy());
        state.next(&ctx, true);
        assert_eq!(state.queue.current().unwrap().video_id, initial);
        assert!(!state.import_busy);
        until(&mut state, &ctx, |state| {
            state.queue.is_complete() || state.catalog_error.is_some()
        });
        assert!(state.queue.is_complete(), "{}", state.status);
        assert_eq!(state.tracks.len(), visible);
        let full = state.catalog.as_ref().unwrap().tracks.clone();
        assert!(full.len() > visible);
        assert!(state.queue.previous());
        assert_eq!(state.queue.current().unwrap().video_id, initial);
        assert!(state.queue.forward(false));
        let mut observed = std::collections::HashMap::<String, usize>::new();
        *observed.entry(initial).or_default() += 1;
        loop {
            *observed
                .entry(state.queue.current().unwrap().video_id.clone())
                .or_default() += 1;
            if !state.queue.forward(false) {
                break;
            }
        }
        let mut expected = std::collections::HashMap::<String, usize>::new();
        for track in full.iter() {
            *expected.entry(track.video_id.clone()).or_default() += 1;
        }
        assert!(
            observed == expected,
            "Chaque entrée du catalogue complet doit être parcourue exactement une fois."
        );
        eprintln!(
            "Shuffle validated: visible={visible}, complete={}",
            full.len()
        );
        state.play_track(&ctx, 2);
        assert!(state.queue.is_complete());
        assert!(state.catalog_receiver.is_none());
        state.stop();
        assert!(!state.shuffle_busy());
    }

    fn until(state: &mut AppState, ctx: &egui::Context, ready: impl Fn(&AppState) -> bool) {
        let started = Instant::now();
        while !ready(state) && started.elapsed() < Duration::from_secs(50) {
            state.poll(ctx);
            thread::sleep(Duration::from_millis(20));
        }
        assert!(ready(state), "La transition attendue n'a pas abouti.");
    }

    #[test]
    #[ignore = "Nécessite la session enregistrée, Internet, yt-dlp et la sortie Windows ; contrôle sélection immédiate, annulation et préchargement."]
    fn immediate_selection_cancels_previous_and_uses_prefetched_audio() {
        let ctx = egui::Context::default();
        let mut state = AppState::new(&ctx);
        state.audio.as_mut().unwrap().set_volume(0.0);
        until(&mut state, &ctx, |state| state.connected());
        let playlist = state.playlists.first().unwrap().clone();
        state.open_playlist(&ctx, playlist, false);
        until(&mut state, &ctx, |state| state.tracks.len() >= 3);
        let first = state.tracks[0].clone();
        let second = state.tracks[1].clone();
        let third = state.tracks[2].clone();
        state.play_track(&ctx, 0);
        let first_cancel = state.cancel_import.clone().unwrap();
        assert_eq!(state.now_playing.as_ref(), Some(&first));
        assert!(state.import_busy && state.audio.as_ref().unwrap().title.is_empty());
        state.play_track(&ctx, 1);
        assert!(first_cancel.load(Ordering::Relaxed));
        assert_eq!(state.now_playing.as_ref(), Some(&second));
        assert!(state.import_busy && state.audio.as_ref().unwrap().title.is_empty());
        until(&mut state, &ctx, |state| !state.import_busy);
        assert_eq!(
            state.audio.as_ref().unwrap().title,
            second.title,
            "{}",
            state.status
        );
        until(&mut state, &ctx, |state| {
            state.prefetched.is_some() || state.prefetch_failed.is_some()
        });
        assert!(
            state.prefetched.is_some(),
            "Le prochain titre doit avoir un flux et son premier bloc audio."
        );
        let cancelled = state.cancel_prefetch.clone().unwrap();
        state.toggle_pause(&ctx);
        assert!(state.audio.as_ref().unwrap().is_paused());
        assert!(cancelled.load(Ordering::Relaxed));
        for _ in 0..3 {
            state.poll(&ctx);
            assert!(state.prefetch_receiver.is_none() && state.prefetched.is_none());
            thread::sleep(Duration::from_millis(30));
        }
        state.toggle_pause(&ctx);
        until(&mut state, &ctx, |state| {
            state.prefetched.is_some() || state.prefetch_failed.is_some()
        });
        assert!(state.prefetched.is_some());
        let started = Instant::now();
        state.next(&ctx, false);
        assert_eq!(state.now_playing.as_ref(), Some(&third));
        assert!(state.import_busy && state.audio.as_ref().unwrap().title.is_empty());
        state.poll(&ctx);
        assert!(!state.import_busy);
        assert_eq!(state.audio.as_ref().unwrap().title, third.title);
        eprintln!("Prepared next-track transition: {:?}", started.elapsed());
        assert!(started.elapsed() < Duration::from_millis(300));
        state.stop();
        assert!(state.now_playing.is_none() && !state.import_busy && state.prefetched.is_none());
        assert!(state.audio.as_ref().unwrap().is_empty());
    }
}
