//! Bounded, account-local metadata snapshots. Never stores audio or credentials.
use crate::provider::{Page, ProviderError, Track};
use std::collections::VecDeque;

#[derive(Clone)]
pub struct Snapshot {
    pub tracks: Vec<Track>,
    pub continuation: Option<String>,
}

#[derive(Default)]
pub struct PlaylistCache {
    entries: VecDeque<(String, Snapshot)>,
}
impl PlaylistCache {
    pub fn get(&mut self, id: &str) -> Option<Snapshot> {
        let index = self.entries.iter().position(|(key, _)| key == id)?;
        let entry = self.entries.remove(index)?;
        let result = entry.1.clone();
        self.entries.push_back(entry);
        Some(result)
    }
    pub fn put(&mut self, id: &str, tracks: &[Track], continuation: Option<&str>) {
        self.remove(id);
        if tracks.len() > 8_000 {
            return;
        }
        self.entries.push_back((
            id.into(),
            Snapshot {
                tracks: tracks.to_vec(),
                continuation: continuation.map(str::to_owned),
            },
        ));
        while self.entries.len() > 8
            || self
                .entries
                .iter()
                .map(|(_, snapshot)| snapshot.tracks.len())
                .sum::<usize>()
                > 8_000
        {
            self.entries.pop_front();
        }
    }
    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|(key, _)| key != id);
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Revalidate only the range already displayed, then replace it atomically. Mixing
/// a fresh first page with an old tail would duplicate/omit rows after insertions.
pub fn revalidate<T>(
    known: usize,
    mut fetch: impl FnMut(Option<&str>) -> Result<Page<T>, ProviderError>,
) -> Result<Page<T>, ProviderError> {
    let mut result = fetch(None)?;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..128 {
        if result.items.len() >= known {
            break;
        }
        let Some(token) = result.continuation.clone() else {
            break;
        };
        if !seen.insert(token.clone()) {
            return Err(ProviderError::Schema);
        }
        let page = fetch(Some(&token))?;
        result.items.extend(page.items);
        result.continuation = page.continuation;
    }
    if result.items.len() < known && result.continuation.is_some() {
        return Err(ProviderError::Schema);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(id: &str) -> Track {
        Track {
            video_id: id.into(),
            ..Default::default()
        }
    }
    #[test]
    fn cached_pages_keep_their_token_and_are_bounded_by_lru() {
        let mut cache = PlaylistCache::default();
        cache.put("first", &[track("a")], Some("next"));
        for n in 0..7 {
            cache.put(&n.to_string(), &[], None);
        }
        assert_eq!(
            cache.get("first").unwrap().continuation.as_deref(),
            Some("next")
        );
        cache.put("new", &[], None);
        assert!(cache.get("0").is_none());
        assert!(cache.get("first").is_some());
        cache.put("large", &vec![track("b"); 8_000], None);
        assert!(cache.get("first").is_none());
        cache.clear();
        assert!(cache.get("large").is_none());
    }
    #[test]
    fn refreshing_a_loaded_range_never_splices_old_and_new_pages() {
        let page = revalidate(3, |token| {
            Ok(match token {
                None => Page {
                    items: vec![track("inserted"), track("a")],
                    continuation: Some("second".into()),
                },
                Some("second") => Page {
                    items: vec![track("a"), track("b")],
                    continuation: Some("third".into()),
                },
                _ => panic!("Must not fetch beyond the known range"),
            })
        })
        .unwrap();
        assert_eq!(
            page.items
                .iter()
                .map(|t| t.video_id.as_str())
                .collect::<Vec<_>>(),
            vec!["inserted", "a", "a", "b"]
        );
        assert_eq!(page.continuation.as_deref(), Some("third"));
        assert!(revalidate(3, |token| if token.is_none() {
            Ok(Page {
                items: vec![track("a")],
                continuation: Some("next".into()),
            })
        } else {
            Err(ProviderError::Network)
        })
        .is_err());
        assert!(revalidate::<Track>(5, |_| Ok(Page {
            items: vec![],
            continuation: Some("cyclic".into())
        }))
        .is_err());
    }
}
