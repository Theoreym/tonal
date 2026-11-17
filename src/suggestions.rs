//! On-demand recommendations. Only metadata is fetched; playback remains an explicit action.
use crate::{
    provider::{Provider, ProviderError, Track, YouTubeMusic},
    session::Session,
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

#[derive(Default)]
pub struct Suggestions {
    pub seed: Option<Track>,
    pending_seed: Option<Track>,
    pub tracks: Vec<Track>,
    pub error: Option<String>,
    receiver: Option<Receiver<Result<Vec<Track>, ProviderError>>>,
    cancelled: Option<Arc<AtomicBool>>,
    excluded: HashSet<String>,
    removed: HashMap<String, (Track, usize, Option<String>)>,
}

impl Suggestions {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn load(
        &mut self,
        ctx: &egui::Context,
        session: Session,
        seed: Track,
        refresh: bool,
        membership: crate::playlist_add::MembershipCache,
    ) {
        if self.busy()
            || (!refresh
                && self
                    .seed
                    .as_ref()
                    .is_some_and(|old| old.video_id == seed.video_id))
        {
            return;
        }
        self.cancel_load();
        self.error = None;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancelled = Some(cancelled.clone());
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.pending_seed = Some(seed.clone());
        if self.tracks.is_empty() {
            self.seed = Some(seed.clone());
        }
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = YouTubeMusic::new(&session).and_then(|provider| {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(Vec::new());
                }
                let mut tracks = provider.suggestions(&session, &seed.video_id)?;
                let saved = crate::playlist_add::library_video_ids(
                    &provider,
                    &session,
                    &seed.video_id,
                    &membership,
                    &cancelled,
                )?;
                tracks.retain(|track| !saved.contains(&track.video_id));
                tracks.truncate(12);
                Ok(tracks)
            });
            if !cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(result);
                ctx.request_repaint();
            }
        });
    }

    pub fn poll(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(ProviderError::Network),
        };
        self.receiver = None;
        self.cancelled = None;
        match result {
            Ok(mut tracks) => {
                tracks.retain(|track| !self.excluded.contains(&track.video_id));
                self.tracks = tracks;
                self.seed = self.pending_seed.take().or_else(|| self.seed.take());
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        self.pending_seed = None;
    }

    pub fn exclude(&mut self, track: &Track) {
        self.excluded.insert(track.video_id.clone());
        if let Some(position) = self
            .tracks
            .iter()
            .position(|item| item.video_id == track.video_id)
        {
            let removed = self.tracks.remove(position);
            self.removed.insert(
                track.video_id.clone(),
                (
                    removed,
                    position,
                    self.seed.as_ref().map(|seed| seed.video_id.clone()),
                ),
            );
        }
    }

    pub fn confirm(&mut self, video_id: &str) {
        self.removed.remove(video_id);
    }

    pub fn rollback(&mut self, video_id: &str) {
        self.excluded.remove(video_id);
        if let Some((track, position, seed)) = self.removed.remove(video_id) {
            if self.seed.as_ref().map(|seed| &seed.video_id) == seed.as_ref()
                && !self.tracks.iter().any(|item| item.video_id == video_id)
            {
                self.tracks.insert(position.min(self.tracks.len()), track);
            }
        }
    }

    fn cancel_load(&mut self) {
        if let Some(cancelled) = self.cancelled.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.receiver = None;
        self.pending_seed = None;
    }

    pub fn clear(&mut self) {
        self.cancel_load();
        self.seed = None;
        self.tracks.clear();
        self.error = None;
        self.excluded.clear();
        self.removed.clear();
    }
}

impl Drop for Suggestions {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renewal_keeps_playable_rows_and_their_seed_until_success_even_on_error() {
        let old = Track {
            video_id: "old".into(),
            ..Default::default()
        };
        let new = Track {
            video_id: "new".into(),
            ..Default::default()
        };
        let mut suggestions = Suggestions::default();
        suggestions.seed = Some(old.clone());
        suggestions.tracks = vec![old.clone()];
        suggestions.cancel_load();
        let (tx, rx) = mpsc::channel();
        suggestions.receiver = Some(rx);
        suggestions.pending_seed = Some(new.clone());
        suggestions.poll();
        assert_eq!(suggestions.tracks, vec![old.clone()]);
        assert_eq!(suggestions.seed, Some(old.clone()));
        tx.send(Err(ProviderError::Network)).unwrap();
        suggestions.poll();
        assert_eq!(suggestions.tracks, vec![old.clone()]);
        assert_eq!(suggestions.seed, Some(old));
        let (tx, rx) = mpsc::channel();
        suggestions.receiver = Some(rx);
        suggestions.pending_seed = Some(new.clone());
        tx.send(Ok(vec![new.clone()])).unwrap();
        suggestions.poll();
        assert_eq!(suggestions.tracks, vec![new.clone()]);
        assert_eq!(suggestions.seed, Some(new));
        assert!(suggestions.error.is_none());
    }

    #[test]
    fn optimistic_exclusion_survives_late_results_and_refresh_but_failure_restores_the_row() {
        let first = Track {
            video_id: "first".into(),
            ..Default::default()
        };
        let second = Track {
            video_id: "second".into(),
            ..Default::default()
        };
        let mut suggestions = Suggestions::default();
        suggestions.seed = Some(Track {
            video_id: "seed".into(),
            ..Default::default()
        });
        suggestions.tracks = vec![first.clone(), second.clone()];
        suggestions.exclude(&first);
        assert_eq!(suggestions.tracks, vec![second.clone()]);
        let (tx, rx) = mpsc::channel();
        suggestions.receiver = Some(rx);
        tx.send(Ok(vec![first.clone(), second.clone()])).unwrap();
        suggestions.poll();
        assert_eq!(suggestions.tracks, vec![second.clone()]);
        suggestions.rollback("first");
        assert_eq!(suggestions.tracks, vec![first.clone(), second.clone()]);
        suggestions.exclude(&first);
        suggestions.confirm("first");
        suggestions.cancel_load();
        let (tx, rx) = mpsc::channel();
        suggestions.receiver = Some(rx);
        tx.send(Ok(vec![first, second.clone()])).unwrap();
        suggestions.poll();
        assert_eq!(suggestions.tracks, vec![second]);
        suggestions.clear();
        assert!(suggestions.excluded.is_empty());
    }

    #[test]
    fn clear_discards_late_results_and_does_not_retry_errors_automatically() {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut suggestions = Suggestions {
            receiver: Some(rx),
            cancelled: Some(cancel.clone()),
            seed: None,
            pending_seed: None,
            tracks: Vec::new(),
            error: None,
            excluded: Default::default(),
            removed: Default::default(),
        };
        suggestions.clear();
        assert!(cancel.load(Ordering::Relaxed));
        assert!(tx.send(Ok(vec![Track::default()])).is_err());
        suggestions.poll();
        assert!(suggestions.tracks.is_empty());
        let (tx, rx) = mpsc::channel();
        suggestions.receiver = Some(rx);
        tx.send(Err(ProviderError::SessionExpired)).unwrap();
        suggestions.poll();
        assert!(suggestions.error.is_some());
        assert!(!suggestions.busy());
        suggestions.poll();
        assert!(!suggestions.busy());
    }
}
