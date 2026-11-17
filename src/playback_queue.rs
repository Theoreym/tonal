use crate::provider::Track;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Clone, Hash, serde::Serialize, serde::Deserialize)]
struct Recent {
    video_id: String,
    artist: String,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}
impl Repeat {
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlaybackQueue {
    pub playlist_id: Option<String>,
    tracks: Arc<Vec<Track>>,
    order: Vec<usize>,
    cursor: usize,
    history_start: usize,
    complete: bool,
    #[serde(default)]
    omitted: Vec<(String, usize)>,
    pub repeat: Repeat,
    pub shuffle: bool,
    #[serde(default)]
    pub smart_shuffle: bool,
    #[serde(default)]
    recent: VecDeque<Recent>,
}

impl PlaybackQueue {
    pub fn selected(
        tracks: Vec<Track>,
        index: usize,
        repeat: Repeat,
        shuffle: bool,
    ) -> Option<Self> {
        if index >= tracks.len() {
            return None;
        }
        let mut order: Vec<_> = (0..tracks.len()).collect();
        let cursor = if shuffle {
            order.remove(index);
            fastrand::shuffle(&mut order);
            order.insert(0, index);
            0
        } else {
            index
        };
        Some(Self {
            playlist_id: None,
            tracks: Arc::new(tracks),
            order,
            cursor,
            history_start: cursor,
            complete: false,
            omitted: Vec::new(),
            repeat,
            shuffle,
            smart_shuffle: false,
            recent: VecDeque::new(),
        })
    }

    pub fn current(&self) -> Option<&Track> {
        self.order
            .get(self.cursor)
            .and_then(|index| self.tracks.get(*index))
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }
    pub fn mark_complete(&mut self) {
        self.complete = true;
    }

    pub fn remaining(&self) -> usize {
        self.order.len().saturating_sub(self.cursor + 1)
    }

    pub fn valid_snapshot(&self) -> bool {
        if self.tracks.len() > 50_000
            || self.recent.len() > 20
            || self
                .recent
                .iter()
                .any(|track| track.video_id.len() > 160 || track.artist.len() > 16_384)
            || self.omitted.len() > 50_000
            || self.order.len() > self.tracks.len()
            || self.cursor >= self.order.len()
            || self.history_start > self.cursor
        {
            return false;
        }
        let mut seen = std::collections::HashSet::new();
        self.order
            .iter()
            .all(|index| *index < self.tracks.len() && seen.insert(*index))
            && self.tracks.iter().all(|track| {
                !track.video_id.is_empty()
                    && track.video_id.len() <= 160
                    && track
                        .video_id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
                    && track.title.len() <= 16_384
                    && track.artist.len() <= 16_384
                    && track.album.len() <= 16_384
                    && track
                        .cover_url
                        .as_ref()
                        .is_none_or(|url| url.len() <= 16_384 && crate::artwork::allowed_url(url))
            })
    }

    pub fn revision(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.playlist_id.hash(&mut hash);
        self.tracks.len().hash(&mut hash);
        self.order.hash(&mut hash);
        self.cursor.hash(&mut hash);
        self.history_start.hash(&mut hash);
        self.complete.hash(&mut hash);
        self.omitted.hash(&mut hash);
        self.shuffle.hash(&mut hash);
        self.smart_shuffle.hash(&mut hash);
        self.recent.hash(&mut hash);
        (self.repeat as u8).hash(&mut hash);
        hash.finish()
    }

    pub fn inherit_recent(&mut self, previous: &Self) {
        self.recent = previous.planning_history();
        self.set_smart_shuffle(previous.smart_shuffle);
    }
    pub fn set_smart_shuffle(&mut self, enabled: bool) {
        if self.smart_shuffle == enabled {
            return;
        }
        self.smart_shuffle = enabled;
        if self.shuffle {
            self.arrange_future();
        }
    }
    fn remember_current(&mut self) {
        if let Some(track) = self.current() {
            let recent = Recent {
                video_id: track.video_id.clone(),
                artist: track.artist.trim().to_lowercase(),
            };
            self.recent.push_back(recent);
            if self.recent.len() > 20 {
                self.recent.pop_front();
            }
        }
    }
    fn planning_history(&self) -> VecDeque<Recent> {
        let mut recent = self.recent.clone();
        if let Some(track) = self.current() {
            recent.push_back(Recent {
                video_id: track.video_id.clone(),
                artist: track.artist.trim().to_lowercase(),
            });
            if recent.len() > 20 {
                recent.pop_front();
            }
        }
        recent
    }
    fn arrange_future(&mut self) {
        let recent = self.planning_history();
        if let Some(future) = self.order.get_mut(self.cursor + 1..) {
            fastrand::shuffle(future);
            if self.smart_shuffle {
                smart_order(future, &self.tracks, &recent);
            }
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = (usize, &Track)> {
        self.order
            .iter()
            .enumerate()
            .skip(self.cursor)
            .filter_map(|(position, index)| Some((position, self.tracks.get(*index)?)))
    }
    pub fn move_upcoming(&mut self, from: usize, to: usize) -> bool {
        if from <= self.cursor
            || to <= self.cursor
            || from >= self.order.len()
            || to >= self.order.len()
            || from == to
        {
            return false;
        }
        let index = self.order.remove(from);
        self.order.insert(to, index);
        true
    }
    pub fn remove_upcoming(&mut self, position: usize) -> bool {
        if position <= self.cursor || position >= self.order.len() {
            return false;
        }
        let index = self.order.remove(position);
        let track = &self.tracks[index];
        let occurrence = self.tracks[..index]
            .iter()
            .filter(|other| other.video_id == track.video_id)
            .count();
        self.omitted.push((track.video_id.clone(), occurrence));
        true
    }
    pub fn play_upcoming(&mut self, position: usize) -> bool {
        if position <= self.cursor || position >= self.order.len() {
            return false;
        }
        self.remember_current();
        let index = self.order.remove(position);
        self.order.insert(self.cursor + 1, index);
        self.cursor += 1;
        true
    }
    fn omitted_indices(&self, tracks: &[Track]) -> std::collections::HashSet<usize> {
        let mut counts = std::collections::HashMap::<&str, usize>::new();
        let omitted: std::collections::HashSet<_> = self
            .omitted
            .iter()
            .map(|(id, n)| (id.as_str(), *n))
            .collect();
        tracks
            .iter()
            .enumerate()
            .filter_map(|(index, track)| {
                let count = counts.entry(&track.video_id).or_default();
                let excluded = omitted.contains(&(track.video_id.as_str(), *count));
                *count += 1;
                excluded.then_some(index)
            })
            .collect()
    }

    /// Reconcile a full catalog while preserving actual playback history, even
    /// if the playlist was edited since the first visible page was requested.
    pub fn complete_from(&mut self, tracks: &[Track]) {
        use std::collections::{HashMap, HashSet, VecDeque};
        if self.current().is_none() {
            return;
        }
        // Sequential pagination keeps canonical ordering, including the indices before
        // the selected song. They must not be appended after it as shuffled candidates.
        if !self.shuffle
            && tracks.len() >= self.tracks.len()
            && self
                .tracks
                .iter()
                .zip(tracks)
                .all(|(old, new)| old.video_id == new.video_id)
        {
            self.extend_from(tracks);
            self.complete = true;
            return;
        }
        let mut positions: HashMap<&str, VecDeque<usize>> = HashMap::new();
        for (index, track) in tracks.iter().enumerate() {
            positions
                .entry(&track.video_id)
                .or_default()
                .push_back(index);
        }
        let mut all = tracks.to_vec();
        let mut history = Vec::new();
        for index in &self.order[self.history_start..=self.cursor] {
            let old = &self.tracks[*index];
            let position = positions
                .get_mut(old.video_id.as_str())
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| {
                    all.push(old.clone());
                    all.len() - 1
                });
            history.push(position);
        }
        let played: HashSet<_> = history.iter().copied().collect();
        let omitted = self.omitted_indices(tracks);
        let current_position = *history.last().unwrap();
        let sequential_start = if current_position < tracks.len() {
            current_position + 1
        } else {
            self.order[self.cursor].min(tracks.len())
        };
        let mut future: Vec<_> = (0..tracks.len())
            .filter(|index| {
                !played.contains(index)
                    && !omitted.contains(index)
                    && (self.shuffle || *index >= sequential_start)
            })
            .collect();
        if self.shuffle {
            fastrand::shuffle(&mut future);
            if self.smart_shuffle {
                smart_order(&mut future, &all, &self.planning_history());
            }
        }
        self.cursor = history.len() - 1;
        self.history_start = 0;
        history.extend(future);
        self.order = history;
        self.tracks = Arc::new(all);
        self.complete = true;
    }

    pub fn extend_from(&mut self, tracks: &[Track]) {
        if self.complete {
            return;
        }
        if tracks.len() <= self.tracks.len()
            || !self
                .tracks
                .iter()
                .zip(tracks)
                .all(|(old, new)| old.video_id == new.video_id)
        {
            return;
        }
        let previous_len = self.tracks.len();
        Arc::make_mut(&mut self.tracks).extend_from_slice(&tracks[previous_len..]);
        self.order.extend(previous_len..tracks.len());
        if self.shuffle {
            self.arrange_future();
        }
    }
    pub fn can_next(&self) -> bool {
        !self.order.is_empty() && (self.cursor + 1 < self.order.len() || self.repeat == Repeat::All)
    }
    // A shuffled wrap chooses a new order at navigation time; do not predict it.
    pub fn upcoming(&self) -> Option<&Track> {
        let next = self.order.get(self.cursor + 1).or_else(|| {
            (self.repeat == Repeat::All && !self.shuffle)
                .then(|| self.order.first())
                .flatten()
        })?;
        self.tracks.get(*next)
    }
    pub fn can_previous(&self) -> bool {
        !self.order.is_empty() && (self.cursor > 0 || self.repeat == Repeat::All)
    }

    pub fn forward(&mut self, automatic: bool) -> bool {
        if self.order.is_empty() {
            return false;
        }
        if automatic && self.repeat == Repeat::One {
            return true;
        }
        if self.cursor + 1 < self.order.len() {
            self.remember_current();
            self.cursor += 1;
            return true;
        }
        if self.repeat != Repeat::All {
            return false;
        }
        self.remember_current();
        if self.shuffle && self.order.len() > 1 {
            let last = self.order[self.cursor];
            fastrand::shuffle(&mut self.order);
            if self.smart_shuffle {
                smart_order(&mut self.order, &self.tracks, &self.recent);
            }
            if self.order[0] == last {
                self.order.swap(0, 1);
            }
        }
        self.cursor = 0;
        self.history_start = 0;
        true
    }

    pub fn previous(&mut self) -> bool {
        if self.order.is_empty() {
            return false;
        }
        if self.cursor > 0 {
            self.remember_current();
            self.cursor -= 1;
            self.history_start = self.history_start.min(self.cursor);
            true
        } else if self.repeat == Repeat::All {
            self.remember_current();
            self.cursor = self.order.len() - 1;
            self.history_start = self.cursor;
            true
        } else {
            false
        }
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if self.shuffle && self.current().is_some() {
            // Indices before the initial selection are not playback history.
            // Include them too when shuffle is first enabled mid-playlist.
            let mut history = self.order[self.history_start..=self.cursor].to_vec();
            let played: std::collections::HashSet<_> = history.iter().copied().collect();
            let omitted = self.omitted_indices(&self.tracks);
            let mut future: Vec<_> = (0..self.tracks.len())
                .filter(|index| !played.contains(index) && !omitted.contains(index))
                .collect();
            fastrand::shuffle(&mut future);
            self.cursor = history.len() - 1;
            self.history_start = 0;
            history.extend(future);
            self.order = history;
            if self.smart_shuffle {
                self.arrange_future();
            }
            return;
        }
        // Keep the current title and playback history stable when switching modes.
        if let Some(future) = self.order.get_mut(self.cursor + 1..) {
            if self.shuffle {
                fastrand::shuffle(future);
            } else {
                future.sort_unstable();
            }
        }
    }
}

fn smart_order(order: &mut [usize], tracks: &[Track], recent: &VecDeque<Recent>) {
    use std::collections::HashMap;
    let artists: Vec<_> = tracks
        .iter()
        .map(|track| track.artist.trim().to_lowercase())
        .collect();
    let mut videos: VecDeque<String> = recent.iter().map(|track| track.video_id.clone()).collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for video in &videos {
        *counts.entry(video.clone()).or_default() += 1;
    }
    let mut previous: VecDeque<String> = recent
        .iter()
        .rev()
        .take(2)
        .map(|track| track.artist.clone())
        .collect();
    let mut candidates = order.to_vec();
    let mut arranged = Vec::with_capacity(order.len());
    while !candidates.is_empty() {
        let score = |index: usize| {
            let track = &tracks[index];
            let artist = &artists[index];
            usize::from(counts.contains_key(&track.video_id)) * 4
                + if artist.is_empty() {
                    0
                } else if previous.front() == Some(artist) {
                    2
                } else if previous.get(1) == Some(artist) {
                    1
                } else {
                    0
                }
        };
        let best = (0..candidates.len().min(64))
            .min_by_key(|position| score(candidates[*position]))
            .unwrap();
        let index = candidates.swap_remove(best);
        let track = &tracks[index];
        arranged.push(index);
        videos.push_back(track.video_id.clone());
        *counts.entry(track.video_id.clone()).or_default() += 1;
        if videos.len() > 20 {
            let old = videos.pop_front().unwrap();
            let count = counts.get_mut(&old).unwrap();
            *count -= 1;
            if *count == 0 {
                counts.remove(&old);
            }
        }
        previous.push_front(artists[index].clone());
        previous.truncate(2);
    }
    order.copy_from_slice(&arranged);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intelligent_shuffle_spaces_artists_and_recent_tracks_without_losing_entries_or_history() {
        let tracks: Vec<_> = (0..32)
            .map(|index| Track {
                video_id: index.to_string(),
                artist: format!("Artiste {}", index % 8),
                ..Default::default()
            })
            .collect();
        let mut previous = PlaybackQueue::selected(tracks.clone(), 0, Repeat::Off, false).unwrap();
        previous.set_smart_shuffle(true);
        for _ in 0..3 {
            previous.forward(false);
        }
        let mut queue = PlaybackQueue::selected(tracks, 4, Repeat::Off, true).unwrap();
        queue.inherit_recent(&previous);
        assert!(queue.smart_shuffle);
        assert_eq!(queue.current().unwrap().video_id, "4");
        let entries: Vec<_> = queue.entries().map(|(_, track)| track.clone()).collect();
        assert_ne!(entries[0].artist, entries[1].artist);
        assert_ne!(entries[1].artist, entries[2].artist);
        assert_ne!(entries[0].artist, entries[2].artist);
        assert!(entries.iter().skip(1).take(12).all(|track| track
            .video_id
            .parse::<usize>()
            .unwrap()
            >= 5));
        assert_eq!(
            entries
                .iter()
                .map(|track| &track.video_id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            32
        );
        queue.forward(false);
        let current = queue.current().cloned();
        queue.set_smart_shuffle(false);
        assert_eq!(queue.current(), current.as_ref());
        assert!(queue.previous());
        assert_eq!(queue.current().unwrap().video_id, "4");
        assert!(queue.valid_snapshot());
    }
    fn tracks() -> Vec<Track> {
        (0..5)
            .map(|n| Track {
                video_id: n.to_string(),
                title: format!("Titre {n}"),
                artist: "Artiste".into(),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn sequential_completion_continues_past_the_first_page_instead_of_restarting_at_zero() {
        let full: Vec<_> = (0..611)
            .map(|index| Track {
                video_id: format!("video_{index}"),
                ..Default::default()
            })
            .collect();
        let mut queue =
            PlaybackQueue::selected(full[..100].to_vec(), 95, Repeat::Off, false).unwrap();
        queue.complete_from(&full);
        assert!(queue.is_complete());
        assert_eq!(queue.current().unwrap().video_id, "video_95");
        for index in 96..611 {
            assert!(queue.forward(false));
            assert_eq!(queue.current().unwrap().video_id, format!("video_{index}"));
        }
        assert!(!queue.forward(false));
    }

    #[test]
    fn queue_edits_keep_the_current_song_and_do_not_restore_removed_entries_on_shuffle_or_pagination(
    ) {
        let full = tracks();
        let mut queue = PlaybackQueue::selected(full[..4].to_vec(), 0, Repeat::Off, false).unwrap();
        assert!(queue.remove_upcoming(2));
        assert!(queue.move_upcoming(2, 1));
        assert_eq!(
            queue
                .entries()
                .map(|(_, track)| track.video_id.as_str())
                .collect::<Vec<_>>(),
            vec!["0", "3", "1"]
        );
        queue.complete_from(&full);
        assert_eq!(
            queue
                .entries()
                .map(|(_, track)| track.video_id.as_str())
                .collect::<Vec<_>>(),
            vec!["0", "3", "1", "4"]
        );
        queue.toggle_shuffle();
        assert_eq!(queue.current().unwrap().video_id, "0");
        assert!(!queue.entries().any(|(_, track)| track.video_id == "2"));
        let current = queue.current().unwrap().clone();
        assert!(!queue.remove_upcoming(0));
        assert!(!queue.move_upcoming(0, 1));
        assert_eq!(queue.current(), Some(&current));
        assert!(queue.play_upcoming(2));
        assert!(queue.previous());
        assert_eq!(queue.current(), Some(&current));
        assert!(queue.valid_snapshot());
    }

    #[test]
    fn enabling_shuffle_includes_unplayed_titles_before_the_selected_index() {
        let mut queue = PlaybackQueue::selected(tracks(), 3, Repeat::Off, false).unwrap();
        assert!(queue.forward(false));
        queue.toggle_shuffle();
        assert!(queue.previous());
        assert_eq!(queue.current().unwrap().video_id, "3");
        assert!(queue.forward(false));
        let mut seen = std::collections::HashSet::from(["3".to_owned()]);
        loop {
            assert!(seen.insert(queue.current().unwrap().video_id.clone()));
            if !queue.forward(false) {
                break;
            }
        }
        assert_eq!(seen.len(), 5);
    }

    #[test]
    fn full_catalog_adds_unseen_pages_without_replaying_history() {
        let full: Vec<_> = (0..611)
            .map(|index| Track {
                video_id: index.to_string(),
                ..Default::default()
            })
            .collect();
        let mut queue =
            PlaybackQueue::selected(full[..100].to_vec(), 73, Repeat::Off, true).unwrap();
        let first = queue.current().unwrap().video_id.clone();
        queue.forward(false);
        let second = queue.current().unwrap().video_id.clone();
        queue.forward(false);
        let current = queue.current().unwrap().video_id.clone();
        queue.complete_from(&full);
        assert!(queue.is_complete());
        assert_eq!(queue.current().unwrap().video_id, current);
        assert!(queue.previous());
        assert_eq!(queue.current().unwrap().video_id, second);
        assert!(queue.forward(false));
        let mut seen = std::collections::HashSet::from([first, second, current]);
        while queue.forward(false) {
            assert!(seen.insert(queue.current().unwrap().video_id.clone()));
        }
        assert_eq!(seen.len(), 611);
        assert!(seen.contains("610"));
        queue.extend_from(&full[..100]);
        assert!(!queue.forward(false));
    }

    #[test]
    fn catalog_reconciliation_preserves_removed_current_and_duplicate_entries() {
        let mut queue = PlaybackQueue::selected(tracks(), 1, Repeat::Off, true).unwrap();
        let full = vec![
            Track {
                video_id: "new".into(),
                ..Default::default()
            },
            Track {
                video_id: "new".into(),
                ..Default::default()
            },
            Track {
                video_id: "last".into(),
                ..Default::default()
            },
        ];
        queue.complete_from(&full);
        assert_eq!(queue.current().unwrap().video_id, "1");
        let mut upcoming = Vec::new();
        while queue.forward(false) {
            upcoming.push(queue.current().unwrap().video_id.clone());
        }
        upcoming.sort();
        assert_eq!(upcoming, vec!["last", "new", "new"]);
    }

    #[test]
    fn subsequent_pages_extend_the_queue_without_changing_current_or_history() {
        let mut queue = PlaybackQueue::selected(tracks(), 4, Repeat::Off, false).unwrap();
        assert!(!queue.can_next());
        let mut expanded = tracks();
        expanded.push(Track {
            video_id: "next_page".into(),
            ..Default::default()
        });
        queue.extend_from(&expanded);
        assert_eq!(queue.current().unwrap().video_id, "4");
        assert!(queue.forward(false));
        assert_eq!(queue.current().unwrap().video_id, "next_page");
        assert!(queue.previous());
        assert_eq!(queue.current().unwrap().video_id, "4");
        queue.extend_from(&expanded);
        assert!(queue.forward(false));
        assert!(!queue.forward(false));
        expanded[0].video_id = "changed_playlist".into();
        expanded.push(Track::default());
        queue.extend_from(&expanded);
        assert!(!queue.forward(false));
    }

    #[test]
    fn off_stops_at_end_and_all_wraps() {
        let mut queue = PlaybackQueue::selected(tracks(), 4, Repeat::Off, false).unwrap();
        assert!(!queue.forward(true));
        queue.repeat = Repeat::All;
        assert!(queue.forward(true));
        assert_eq!(queue.current().unwrap().video_id, "0");
        assert!(queue.previous());
        assert_eq!(queue.current().unwrap().video_id, "4");
    }

    #[test]
    fn repeat_one_replays_automatically_but_manual_next_moves_on() {
        let mut queue = PlaybackQueue::selected(tracks(), 1, Repeat::One, false).unwrap();
        assert!(queue.forward(true));
        assert_eq!(queue.current().unwrap().video_id, "1");
        assert!(queue.forward(false));
        assert_eq!(queue.current().unwrap().video_id, "2");
    }

    #[test]
    fn shuffle_visits_each_title_once_and_preserves_history() {
        let mut queue = PlaybackQueue::selected(tracks(), 2, Repeat::Off, true).unwrap();
        assert_eq!(queue.current().unwrap().video_id, "2");
        let mut seen = std::collections::HashSet::new();
        loop {
            assert!(seen.insert(queue.current().unwrap().video_id.clone()));
            if !queue.forward(false) {
                break;
            }
        }
        assert_eq!(seen.len(), 5);
        let current = queue.current().unwrap().clone();
        queue.toggle_shuffle();
        assert_eq!(queue.current(), Some(&current));
        assert!(queue.previous());
    }

    #[test]
    fn browsing_another_playlist_cannot_change_a_running_snapshot() {
        let source = tracks();
        let queue = PlaybackQueue::selected(source.clone(), 0, Repeat::Off, false).unwrap();
        let mut pending = queue.clone();
        assert!(pending.forward(false));
        assert_eq!(queue.current().unwrap().video_id, "0");
        assert_eq!(pending.current().unwrap().video_id, "1");
        assert!(PlaybackQueue::selected(Vec::new(), 0, Repeat::Off, false).is_none());
    }
}
