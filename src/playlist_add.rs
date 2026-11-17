//! Explicit playlist writes with immediate UI feedback and bounded background work.
use crate::{
    provider::{PlaylistChoice, Provider, ProviderError, Track, YouTubeMusic},
    session::Session,
};
use eframe::egui;
use std::{
    collections::{HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Added {
    pub track: Track,
    pub playlist: PlaylistChoice,
    pub inserted: bool,
}
pub struct Notice {
    pub message: String,
    pub success: bool,
    pub at: Instant,
}
struct PendingWrite {
    intent: Added,
    receiver: Receiver<Result<Added, String>>,
}
pub struct WriteResult {
    pub intent: Added,
    pub result: Result<Added, String>,
}
type ChoicesResult = (String, Result<Vec<PlaylistChoice>, ProviderError>);
type CachedChoices = (String, Vec<PlaylistChoice>, Instant);
pub(crate) type MembershipCache = Arc<Mutex<VecDeque<(String, HashSet<String>, Instant)>>>;

#[derive(Default)]
pub struct PlaylistAdd {
    pub current: Option<Track>,
    pub choices: Option<Vec<PlaylistChoice>>,
    pub error: Option<String>,
    pub notice: Option<Notice>,
    lookup: Option<Receiver<ChoicesResult>>,
    pending: Vec<PendingWrite>,
    cache: VecDeque<CachedChoices>,
    membership: MembershipCache,
    cancelled: Arc<AtomicBool>,
}

impl PlaylistAdd {
    #[cfg(feature = "ui-capture")]
    pub fn qa_add(&mut self, ctx: &egui::Context, playlist_id: &str, fail: bool) -> Option<Added> {
        let track = self.current.clone()?;
        let playlist = self
            .choices
            .as_ref()?
            .iter()
            .find(|choice| choice.id == playlist_id && choice.contains == Some(false))?
            .clone();
        let intent = Added {
            track,
            playlist,
            inserted: true,
        };
        self.start_optimistic(&intent);
        let (tx, rx) = mpsc::channel();
        self.pending.push(PendingWrite {
            intent: intent.clone(),
            receiver: rx,
        });
        let work = intent.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(if fail { 200 } else { 10_000 }));
            let _ = tx.send(if fail {
                Err("Échec simulé pour la vérification.".into())
            } else {
                Ok(work)
            });
            ctx.request_repaint();
        });
        Some(intent)
    }

    pub fn busy(&self) -> bool {
        self.current.as_ref().is_some_and(|track| {
            self.pending
                .iter()
                .any(|job| job.intent.track.video_id == track.video_id)
        })
    }
    pub fn visible(&self) -> bool {
        self.current.is_some()
            && !self.busy()
            && !self
                .choices
                .as_ref()
                .is_some_and(|choices| choices.iter().any(|choice| choice.contains == Some(true)))
    }
    pub fn membership_cache(&self) -> MembershipCache {
        self.membership.clone()
    }

    pub fn observe(&mut self, ctx: &egui::Context, session: Option<Session>, track: Option<Track>) {
        if self.current.as_ref().map(|track| &track.video_id)
            != track.as_ref().map(|track| &track.video_id)
        {
            self.current = track;
            self.error = None;
            self.choices = self.current.as_ref().and_then(|track| {
                self.cache
                    .iter()
                    .find(|(id, _, at)| {
                        id == &track.video_id && at.elapsed() < Duration::from_secs(900)
                    })
                    .map(|(_, choices, _)| choices.clone())
            });
        }
        if let Some(rx) = &self.lookup {
            match rx.try_recv() {
                Ok((id, result)) => {
                    self.lookup = None;
                    match result {
                        Ok(choices) => {
                            if self
                                .current
                                .as_ref()
                                .is_some_and(|track| track.video_id == id)
                            {
                                self.choices = Some(choices.clone());
                            }
                            self.cache.retain(|(old, _, _)| old != &id);
                            self.cache.push_back((id, choices, Instant::now()));
                            while self.cache.len() > 24 {
                                self.cache.pop_front();
                            }
                        }
                        Err(error)
                            if self
                                .current
                                .as_ref()
                                .is_some_and(|track| track.video_id == id) =>
                        {
                            self.error = Some(error.to_string())
                        }
                        Err(_) => {}
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.lookup = None;
                    self.error =
                        Some("La vérification des playlists s’est interrompue. Réessaie.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.lookup.is_some() || self.choices.is_some() || self.error.is_some() {
            return;
        }
        let (Some(session), Some(track)) = (session, self.current.as_ref()) else {
            return;
        };
        let id = track.video_id.clone();
        let cancelled = self.cancelled.clone();
        let membership = self.membership.clone();
        let (tx, rx) = mpsc::channel();
        self.lookup = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = YouTubeMusic::new(&session).and_then(|provider| {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(Vec::new());
                }
                lookup_choices(&provider, &session, &id, &membership, &cancelled)
            });
            if !cancelled.load(Ordering::Relaxed) {
                let _ = tx.send((id, result));
                ctx.request_repaint();
            }
        });
    }

    pub fn retry(&mut self) {
        self.error = None;
        self.choices = None;
    }

    fn mark(&mut self, added: &Added, contains: bool) {
        for (id, choices, _) in &mut self.cache {
            if id == &added.track.video_id {
                for choice in choices {
                    if choice.id == added.playlist.id {
                        choice.contains = Some(contains);
                    }
                }
            }
        }
        if self
            .current
            .as_ref()
            .is_some_and(|track| track.video_id == added.track.video_id)
        {
            if let Some(choices) = &mut self.choices {
                for choice in choices {
                    if choice.id == added.playlist.id {
                        choice.contains = Some(contains);
                    }
                }
            }
        }
        for (id, tracks, _) in self.membership.lock().unwrap().iter_mut() {
            if id == &added.playlist.id {
                if contains {
                    tracks.insert(added.track.video_id.clone());
                } else {
                    tracks.remove(&added.track.video_id);
                }
            }
        }
    }

    pub fn add(
        &mut self,
        ctx: &egui::Context,
        session: Session,
        playlist_id: &str,
    ) -> Option<Added> {
        if self.busy() || self.pending.len() >= 8 {
            return None;
        }
        let track = self.current.clone()?;
        let playlist = self
            .choices
            .as_ref()?
            .iter()
            .find(|choice| choice.id == playlist_id && choice.contains == Some(false))?
            .clone();
        let intent = Added {
            track,
            playlist,
            inserted: true,
        };
        self.start_optimistic(&intent);
        let (tx, rx) = mpsc::channel();
        self.pending.push(PendingWrite {
            intent: intent.clone(),
            receiver: rx,
        });
        let cancelled = self.cancelled.clone();
        let membership = self.membership.clone();
        let ctx = ctx.clone();
        let work = intent.clone();
        thread::spawn(move || {
            let result = (|| {
                let provider = YouTubeMusic::new(&session).map_err(|error| error.to_string())?;
                let fresh = provider
                    .playlist_choices(&session, &work.track.video_id)
                    .map_err(|error| error.to_string())?;
                let choice = fresh
                    .iter()
                    .find(|choice| choice.id == work.playlist.id)
                    .ok_or("Cette playlist n’est plus modifiable. Actualise la liste.")?;
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Ajout annulé.".into());
                }
                let contains = match choice.contains {
                    Some(contains) => contains,
                    None => playlist_contains(
                        &provider,
                        &session,
                        &work.playlist.id,
                        &work.track.video_id,
                        &membership,
                        &cancelled,
                        true,
                    )
                    .map_err(|error| error.to_string())?,
                };
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Ajout annulé.".into());
                }
                let mut added = work;
                added.inserted = !contains;
                if added.inserted {
                    if let Err(error) = provider.add_playlist_track(
                        &session,
                        &added.playlist.id,
                        &added.track.video_id,
                    ) {
                        // A lost response may hide a successful write. Reconcile by reading,
                        // never by automatically repeating the mutation.
                        if playlist_contains(
                            &provider,
                            &session,
                            &added.playlist.id,
                            &added.track.video_id,
                            &membership,
                            &cancelled,
                            true,
                        )
                        .ok()
                            != Some(true)
                        {
                            return Err(error.to_string());
                        }
                    }
                }
                Ok(added)
            })();
            if !cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(result);
                ctx.request_repaint();
            }
        });
        Some(intent)
    }

    fn start_optimistic(&mut self, intent: &Added) {
        self.error = None;
        self.mark(intent, true);
        self.notice = Some(Notice {
            message: format!("{} ajouté à {}", intent.track.title, intent.playlist.title),
            success: true,
            at: Instant::now(),
        });
    }

    pub fn poll_added(&mut self) -> Vec<WriteResult> {
        if self
            .notice
            .as_ref()
            .is_some_and(|notice| notice.at.elapsed() > Duration::from_secs(5))
        {
            self.notice = None;
        }
        let mut ready = Vec::new();
        let mut index = 0;
        while index < self.pending.len() {
            let result = match self.pending[index].receiver.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("L’ajout s’est interrompu. Vérifie la playlist avant de réessayer.".into())
                }
            };
            let job = self.pending.remove(index);
            match &result {
                Ok(added) => {
                    self.mark(added, true);
                    if self
                        .current
                        .as_ref()
                        .is_some_and(|track| track.video_id == added.track.video_id)
                    {
                        self.error = None;
                    }
                }
                Err(error) => {
                    self.mark(&job.intent, false);
                    if self
                        .current
                        .as_ref()
                        .is_some_and(|track| track.video_id == job.intent.track.video_id)
                    {
                        self.error = Some(error.clone());
                    }
                    self.notice = Some(Notice {
                        message: format!(
                            "Impossible d’ajouter {} à {}. {}",
                            job.intent.track.title, job.intent.playlist.title, error
                        ),
                        success: false,
                        at: Instant::now(),
                    });
                }
            }
            ready.push(WriteResult {
                intent: job.intent,
                result,
            });
        }
        ready
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn pending_for(&self, id: &str) -> bool {
        self.pending.iter().any(|job| job.intent.playlist.id == id)
    }
    pub fn library_changed(&mut self, removed_id: Option<&str>) {
        self.cache.clear();
        self.choices = None;
        self.lookup = None;
        self.error = None;
        if let Some(id) = removed_id {
            self.membership
                .lock()
                .unwrap()
                .retain(|(key, _, _)| key != id);
        }
    }
}

#[cfg(feature = "ui-capture")]
pub fn qa_mode() -> Option<bool> {
    std::env::var_os("TONAL_CAPTURE_TO")?;
    match std::env::var("TONAL_CAPTURE_ACTION").as_deref() {
        Ok("playlist-add-optimistic") => Some(false),
        Ok("playlist-add-rollback") => Some(true),
        _ => None,
    }
}

pub(crate) fn library_video_ids(
    provider: &impl Provider,
    session: &Session,
    seed: &str,
    cache: &MembershipCache,
    cancelled: &AtomicBool,
) -> Result<HashSet<String>, ProviderError> {
    let choices = provider.playlist_choices(session, seed)?;
    if choices.len() > 128 {
        return Err(ProviderError::Schema);
    }
    let mut ids = HashSet::new();
    for choice in choices {
        playlist_contains(provider, session, &choice.id, seed, cache, cancelled, false)?;
        if let Some((_, tracks, _)) = cache
            .lock()
            .unwrap()
            .iter()
            .find(|(id, _, _)| id == &choice.id)
        {
            ids.extend(tracks.iter().cloned());
        }
        if ids.len() > 50_000 {
            return Err(ProviderError::Schema);
        }
    }
    Ok(ids)
}

fn lookup_choices(
    provider: &impl Provider,
    session: &Session,
    video_id: &str,
    cache: &MembershipCache,
    cancelled: &AtomicBool,
) -> Result<Vec<PlaylistChoice>, ProviderError> {
    let mut choices = provider.playlist_choices(session, video_id)?;
    if choices.len() > 128 {
        return Err(ProviderError::Schema);
    }
    for choice in &mut choices {
        if choice.contains.is_none() {
            choice.contains = Some(playlist_contains(
                provider, session, &choice.id, video_id, cache, cancelled, false,
            )?);
        }
    }
    Ok(choices)
}

fn playlist_contains(
    provider: &impl Provider,
    session: &Session,
    playlist_id: &str,
    video_id: &str,
    cache: &MembershipCache,
    cancelled: &AtomicBool,
    fresh: bool,
) -> Result<bool, ProviderError> {
    if !fresh {
        let cache = cache.lock().unwrap();
        if let Some((_, tracks, _)) = cache
            .iter()
            .find(|(id, _, at)| id == playlist_id && at.elapsed() < Duration::from_secs(900))
        {
            return Ok(tracks.contains(video_id));
        }
    }
    // Music sometimes omits containsSelectedVideos. Check all metadata pages once,
    // retaining only IDs in a bounded cache, never downloading tracks or artwork.
    let ids = collect_membership(cancelled, |continuation| {
        provider.playlist_tracks(session, playlist_id, continuation)
    })?;
    let contains = ids.contains(video_id);
    let mut cache = cache.lock().unwrap();
    cache.retain(|(id, _, _)| id != playlist_id);
    cache.push_back((playlist_id.to_owned(), ids, Instant::now()));
    while cache.len() > 24
        || cache
            .iter()
            .map(|(_, tracks, _)| tracks.len())
            .sum::<usize>()
            > 50_000
    {
        cache.pop_front();
    }
    Ok(contains)
}

fn collect_membership(
    cancelled: &AtomicBool,
    mut fetch: impl FnMut(Option<&str>) -> Result<crate::provider::Page<Track>, ProviderError>,
) -> Result<HashSet<String>, ProviderError> {
    let mut ids = HashSet::new();
    let mut continuation = None;
    let mut tokens = HashSet::new();
    let mut count = 0;
    for _ in 0..1000 {
        if cancelled.load(Ordering::Relaxed) {
            return Err(ProviderError::Network);
        }
        let page = fetch(continuation.as_deref())?;
        count += page.items.len();
        if count > 50_000 {
            return Err(ProviderError::Schema);
        }
        ids.extend(page.items.into_iter().map(|track| track.video_id));
        continuation = page.continuation;
        if let Some(token) = &continuation {
            if !tokens.insert(token.clone()) {
                return Err(ProviderError::Schema);
            }
        } else {
            return Ok(ids);
        }
    }
    Err(ProviderError::Schema)
}

impl Drop for PlaylistAdd {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn membership_reaches_later_pages_and_stops_on_errors_cycles_and_cancellation() {
        use crate::provider::Page;
        let mut calls = 0;
        let ids = collect_membership(&AtomicBool::new(false), |token| {
            calls += 1;
            Ok(Page {
                items: vec![Track {
                    video_id: if token.is_none() { "first" } else { "later" }.into(),
                    ..Default::default()
                }],
                continuation: token.is_none().then(|| "page2".into()),
            })
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert!(ids.contains("later") && !ids.contains("absent"));
        assert!(collect_membership(&AtomicBool::new(true), |_| panic!(
            "No request after cancellation"
        ))
        .is_err());
        assert!(
            collect_membership(&AtomicBool::new(false), |_| Err(ProviderError::Network)).is_err()
        );
        assert!(collect_membership(&AtomicBool::new(false), |_| Ok(Page {
            items: Vec::new(),
            continuation: Some("cycle".into())
        }))
        .is_err());
    }

    #[test]
    fn late_results_and_success_stay_bound_to_the_original_track() {
        let mut state = PlaylistAdd::default();
        let first = Track {
            video_id: "first".into(),
            ..Default::default()
        };
        let second = Track {
            video_id: "second".into(),
            ..Default::default()
        };
        let choice = PlaylistChoice {
            id: "PLmine".into(),
            title: "Playlist".into(),
            cover_url: None,
            contains: Some(false),
        };
        let (tx, rx) = mpsc::channel();
        state.lookup = Some(rx);
        tx.send((first.video_id.clone(), Ok(vec![choice.clone()])))
            .unwrap();
        state.observe(&egui::Context::default(), None, Some(second.clone()));
        assert!(state.choices.is_none());
        assert_eq!(state.current.as_ref().unwrap().video_id, "second");
        state.choices = Some(vec![choice.clone()]);
        let (tx, rx) = mpsc::channel();
        let intent = Added {
            track: first.clone(),
            playlist: choice.clone(),
            inserted: true,
        };
        state.start_optimistic(&intent);
        state.pending.push(PendingWrite {
            intent,
            receiver: rx,
        });
        assert!(tx
            .send(Ok(Added {
                track: first,
                playlist: choice,
                inserted: true
            }))
            .is_ok());
        assert_eq!(state.poll_added().len(), 1);
        assert!(
            state.visible(),
            "Saving the old track cannot mark the new track as saved"
        );
        assert_eq!(state.cache[0].1[0].contains, Some(true));
        let cancelled = state.cancelled.clone();
        state.clear();
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(!state.visible());
    }

    #[test]
    fn optimistic_feedback_is_immediate_and_failure_restores_membership() {
        let mut state = PlaylistAdd::default();
        let track = Track {
            video_id: "first".into(),
            title: "Titre".into(),
            ..Default::default()
        };
        let choice = PlaylistChoice {
            id: "PLmine".into(),
            title: "Playlist".into(),
            cover_url: None,
            contains: Some(false),
        };
        state.current = Some(track.clone());
        state.choices = Some(vec![choice.clone()]);
        let intent = Added {
            track,
            playlist: choice,
            inserted: true,
        };
        let (tx, rx) = mpsc::channel();
        state.pending.push(PendingWrite {
            intent: intent.clone(),
            receiver: rx,
        });
        state.start_optimistic(&intent);
        assert!(!state.visible());
        let notice = state.notice.as_ref().unwrap();
        assert!(notice.success && notice.message == "Titre ajouté à Playlist");
        assert!(
            state.poll_added().is_empty(),
            "Pending network cannot delay optimistic feedback"
        );
        tx.send(Err("Réseau indisponible".into())).unwrap();
        let outcome = state.poll_added();
        assert!(outcome[0].result.is_err());
        assert!(state.visible() && !state.busy());
        assert!(!state.notice.as_ref().unwrap().success);
    }

    #[test]
    #[ignore = "Session DPAPI et réseau ; vérifie les destinations et la présence réelle, sans écrire."]
    fn playlist_membership_live_read_only() {
        let session = crate::session::load()
            .unwrap()
            .expect("Session enregistrée");
        let provider = YouTubeMusic::new(&session).unwrap();
        let playlists = provider.library_playlists(&session, None).unwrap();
        let seed = provider
            .playlist_tracks(&session, &playlists.items[0].id, None)
            .unwrap()
            .items
            .remove(0);
        let cache = MembershipCache::default();
        let choices = lookup_choices(
            &provider,
            &session,
            &seed.video_id,
            &cache,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(choices
            .iter()
            .any(|choice| choice.id == playlists.items[0].id && choice.contains == Some(true)));
        eprintln!(
            "{} destinations ; présence confirmée sur toutes les pages ; aucune écriture.",
            choices.len()
        );
    }
}
