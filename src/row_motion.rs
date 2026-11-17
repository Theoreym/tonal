//! Short, visible-row-only transitions. No timers or repaint requests at rest.
use eframe::egui::Id;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

const DURATION: f64 = 0.22;
#[derive(Clone, Copy)]
struct Arrival {
    at: f64,
    expand: bool,
}
#[derive(Default)]
pub struct RowMotion {
    scope: String,
    keys: Vec<Id>,
    arrivals: HashMap<Id, Arrival>,
    moves: HashMap<Id, (f64, f32)>,
    hidden_until: HashMap<Id, f64>,
}
pub struct Sample {
    pub height: f32,
    pub opacity: f32,
    pub offset: f32,
    pub active: bool,
}
impl RowMotion {
    pub fn sync(
        &mut self,
        scope: &str,
        keys: Vec<Id>,
        visible: Range<usize>,
        now: f64,
        expand: bool,
    ) {
        self.arrivals.retain(|_, a| now < a.at + DURATION);
        self.moves.retain(|_, (at, _)| now < *at + DURATION);
        self.hidden_until.retain(|_, until| now < *until);
        if self.scope != scope {
            self.scope = scope.into();
            self.keys = keys;
            self.arrivals.clear();
            self.moves.clear();
            self.hidden_until.clear();
            return;
        }
        if self.keys == keys {
            return;
        }
        let previous: HashMap<_, _> = self
            .keys
            .iter()
            .enumerate()
            .map(|(i, key)| (*key, i))
            .collect();
        let current: HashSet<_> = keys.iter().copied().collect();
        let old_common: Vec<_> = self
            .keys
            .iter()
            .filter(|key| current.contains(key))
            .copied()
            .collect();
        let new_common: Vec<_> = keys
            .iter()
            .filter(|key| previous.contains_key(key))
            .copied()
            .collect();
        let reordered = old_common != new_common;
        self.arrivals.retain(|key, _| current.contains(key));
        self.moves.retain(|key, _| current.contains(key));
        self.hidden_until.retain(|key, _| current.contains(key));
        let mut stagger = 0;
        for (index, key) in keys
            .iter()
            .enumerate()
            .filter(|(i, _)| visible.contains(i))
            .take(24)
        {
            if let Some(old) = previous.get(key) {
                if reordered && *old != index {
                    self.moves.insert(
                        *key,
                        (
                            now,
                            ((*old as f32 - index as f32) * 76.0).clamp(-152.0, 152.0),
                        ),
                    );
                }
            } else {
                self.hidden_until.remove(key);
                self.arrivals.insert(
                    *key,
                    Arrival {
                        at: now + (stagger as f64 * 0.018).min(0.072),
                        expand: expand && !self.keys.is_empty(),
                    },
                );
                stagger += 1;
            }
        }
        if stagger > 0 {
            let until = now + DURATION + ((stagger - 1) as f64 * 0.018).min(0.072);
            for key in keys
                .iter()
                .filter(|key| !previous.contains_key(key) && !self.arrivals.contains_key(key))
            {
                self.hidden_until.insert(*key, until);
            }
        }
        self.keys = keys;
    }
    pub fn sample(&self, key: Id, now: f64) -> Sample {
        let ease = |at: f64| {
            let t = ((now - at) / DURATION).clamp(0.0, 1.0) as f32;
            1.0 - (1.0 - t).powi(3)
        };
        let arrival = self.arrivals.get(&key);
        let progress = arrival.map_or(1.0, |a| ease(a.at));
        let offset = self
            .moves
            .get(&key)
            .map_or(0.0, |(at, offset)| offset * (1.0 - ease(*at)));
        Sample {
            height: if arrival.is_some_and(|a| a.expand) {
                progress
            } else {
                1.0
            },
            opacity: if self
                .hidden_until
                .get(&key)
                .is_some_and(|until| now < *until)
            {
                0.0
            } else {
                progress
            },
            offset: offset + (1.0 - progress) * 7.0,
            active: arrival.is_some_and(|a| now < a.at + DURATION)
                || self
                    .moves
                    .get(&key)
                    .is_some_and(|(at, _)| now < at + DURATION),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn insertions_expand_only_visible_rows_then_settle_without_repaints() {
        let (a, b, c, offscreen) = (
            Id::unique("a"),
            Id::unique("b"),
            Id::unique("c"),
            Id::unique("offscreen"),
        );
        let mut motion = RowMotion::default();
        motion.sync("playlist", vec![a, c], 0..4, 0.0, true);
        assert!(!motion.sample(a, 0.0).active);
        motion.sync("playlist", vec![a, b, c, offscreen], 0..3, 1.0, true);
        assert_eq!(motion.sample(b, 1.0).height, 0.0);
        assert!(!motion.sample(c, 1.0).active);
        assert!(!motion.sample(offscreen, 1.0).active);
        assert!(motion.sample(b, 1.1).height > 0.0);
        assert!(!motion.sample(b, 1.4).active);
        motion.sync("playlist", vec![a, b, c, offscreen], 0..4, 1.4, true);
        assert!(motion.arrivals.is_empty());
        motion.sync("other playlist", vec![b, a], 0..2, 2.0, true);
        assert!(!motion.sample(b, 2.0).active);
    }
    #[test]
    fn suggestions_fade_without_collapsing_and_reorders_keep_their_identity() {
        let (a, b, c) = (Id::unique("a"), Id::unique("b"), Id::unique("c"));
        let mut motion = RowMotion::default();
        motion.sync("suggestions", vec![a, b], 0..4, 0.0, false);
        motion.sync("suggestions", vec![b, c, a], 0..4, 1.0, false);
        assert_eq!(motion.sample(c, 1.0).height, 1.0);
        assert_eq!(motion.sample(c, 1.0).opacity, 0.0);
        assert!(motion.sample(b, 1.0).offset > 0.0);
        assert_eq!(motion.sample(b, 1.4).offset, 0.0);
    }
}
