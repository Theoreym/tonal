//! Short-lived, account-scoped stream addresses. Memory only; no audio or cookies.
use crate::{session::Session, youtube::MediaInfo};
use sha1::{Digest, Sha1};
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CAPACITY: usize = 32;
const LIFETIME: Duration = Duration::from_secs(20 * 60);
const EXPIRY_MARGIN: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Anonymous,
    Account([u8; 20]),
}
impl Scope {
    fn for_session(session: Option<&Session>) -> Self {
        session.map_or(Self::Anonymous, |session| {
            Self::Account(Sha1::digest(session.cookie_header().as_bytes()).into())
        })
    }
}
struct Entry {
    scope: Scope,
    info: MediaInfo,
    at: Instant,
    expires: SystemTime,
}
struct Cache {
    epoch: u64,
    entries: VecDeque<Entry>,
}
static CACHE: Mutex<Cache> = Mutex::new(Cache {
    epoch: 0,
    entries: VecDeque::new(),
});

impl Cache {
    fn get(
        &mut self,
        scope: Scope,
        source: &str,
        at: Instant,
        now: SystemTime,
    ) -> Option<MediaInfo> {
        self.entries.retain(|entry| {
            at.saturating_duration_since(entry.at) < LIFETIME
                && entry
                    .expires
                    .duration_since(now)
                    .is_ok_and(|left| left > EXPIRY_MARGIN)
        });
        let index = self
            .entries
            .iter()
            .position(|entry| entry.scope == scope && entry.info.source_url == source)?;
        let entry = self.entries.remove(index)?;
        let info = entry.info.clone();
        self.entries.push_back(entry);
        Some(info)
    }

    fn put(&mut self, scope: Scope, info: MediaInfo, epoch: u64, at: Instant, now: SystemTime) {
        if self.epoch != epoch
            || info.url.len() > 64 * 1024
            || info.source_url.len() > 512
            || info.user_agent.len() > 4096
            || info.quality.format_id.len() > 64
            || info.quality.codec.len() > 128
            || info.bytes > 2 * 1024 * 1024 * 1024
            || !crate::youtube::media_url_allowed(&info.url)
        {
            return;
        }
        let Some(expires) = url::Url::parse(&info.url)
            .ok()
            .and_then(|url| {
                url.query_pairs()
                    .find(|(key, _)| key == "expire")
                    .and_then(|(_, value)| value.parse::<u64>().ok())
            })
            .and_then(|seconds| UNIX_EPOCH.checked_add(Duration::from_secs(seconds)))
        else {
            return;
        };
        if !expires
            .duration_since(now)
            .is_ok_and(|left| left > EXPIRY_MARGIN)
        {
            return;
        }
        self.entries
            .retain(|entry| entry.scope != scope || entry.info.source_url != info.source_url);
        self.entries.push_back(Entry {
            scope,
            info,
            at,
            expires,
        });
        while self.entries.len() > CAPACITY {
            self.entries.pop_front();
        }
    }
}

pub(crate) fn lookup(
    source: &str,
    session: Option<&Session>,
    fresh: bool,
) -> (Option<MediaInfo>, Scope, u64) {
    let scope = Scope::for_session(session);
    let mut cache = CACHE.lock().unwrap();
    if fresh {
        cache
            .entries
            .retain(|entry| entry.scope != scope || entry.info.source_url != source);
        cache.epoch = cache.epoch.wrapping_add(1);
    }
    let info = if fresh {
        None
    } else {
        cache.get(scope, source, Instant::now(), SystemTime::now())
    };
    (info, scope, cache.epoch)
}

pub(crate) fn store(scope: Scope, info: &MediaInfo, epoch: u64) {
    CACHE.lock().unwrap().put(
        scope,
        info.clone(),
        epoch,
        Instant::now(),
        SystemTime::now(),
    );
}

pub(crate) fn clear() {
    let mut cache = CACHE.lock().unwrap();
    cache.entries.clear();
    cache.epoch = cache.epoch.wrapping_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    fn info(video: &str, expires: u64) -> MediaInfo {
        MediaInfo {
            url: Zeroizing::new(format!(
                "https://test.googlevideo.com/audio?expire={expires}"
            )),
            source_url: format!("https://youtube.com/watch?v={video}"),
            quality: crate::youtube::AudioQuality {
                format_id: "774".into(),
                codec: "opus".into(),
                bitrate_kbps: Some(261.0),
            },
            bytes: 12345,
            duration: Some(Duration::from_secs(180)),
            user_agent: "test".into(),
        }
    }

    #[test]
    fn cache_respects_account_expiry_age_capacity_and_logout_generation() {
        let mut cache = Cache {
            epoch: 0,
            entries: VecDeque::new(),
        };
        let at = Instant::now();
        let now = UNIX_EPOCH + Duration::from_secs(1000);
        let premium = Scope::Account([1; 20]);
        let other = Scope::Account([2; 20]);
        let source = info("track", 2000).source_url;
        cache.put(premium, info("track", 2000), 0, at, now);
        assert!(cache.get(Scope::Anonymous, &source, at, now).is_none());
        assert!(cache.get(other, &source, at, now).is_none());
        let cached = cache.get(premium, &source, at, now).unwrap();
        assert_eq!(cached.quality.format_id, "774");
        assert_eq!(cached.quality.bitrate_kbps, Some(261.0));
        assert!(cache
            .get(premium, &source, at, UNIX_EPOCH + Duration::from_secs(1880))
            .is_none());
        cache.put(premium, info("track", 5000), 0, at, now);
        assert!(cache.get(premium, &source, at + LIFETIME, now).is_none());
        for index in 0..40 {
            cache.put(premium, info(&index.to_string(), 5000), 0, at, now);
        }
        assert_eq!(cache.entries.len(), CAPACITY);
        assert!(cache
            .get(premium, &info("0", 5000).source_url, at, now)
            .is_none());
        cache.entries.clear();
        cache.epoch += 1;
        cache.put(premium, info("old-worker", 5000), 0, at, now);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn cache_rejects_unknown_expiry_unsafe_addresses_and_nearly_expired_links() {
        let mut cache = Cache {
            epoch: 0,
            entries: VecDeque::new(),
        };
        let at = Instant::now();
        let now = UNIX_EPOCH + Duration::from_secs(1000);
        let mut unknown = info("unknown", 5000);
        unknown.url = Zeroizing::new("https://test.googlevideo.com/audio".into());
        cache.put(Scope::Anonymous, unknown, 0, at, now);
        let mut unsafe_url = info("unsafe", 5000);
        unsafe_url.url = Zeroizing::new("https://evil.test/audio?expire=5000".into());
        cache.put(Scope::Anonymous, unsafe_url, 0, at, now);
        cache.put(Scope::Anonymous, info("soon", 1120), 0, at, now);
        assert!(cache.entries.is_empty());
    }
}
