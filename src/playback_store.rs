//! Account-bound playback metadata only, encrypted by DPAPI. No audio URLs or audio files.
use crate::{
    playback_queue::PlaybackQueue,
    session::{self, Session},
};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::mpsc::{self, SyncSender},
    thread,
    time::Duration,
};
use zeroize::Zeroizing;
const LIMIT: u64 = 40 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone)]
pub struct Snapshot {
    version: u8,
    owner: String,
    pub queue: PlaybackQueue,
    pub position: Duration,
    pub volume: f32,
}
impl Snapshot {
    pub fn new(session: &Session, queue: PlaybackQueue, position: Duration, volume: f32) -> Self {
        Self {
            version: 1,
            owner: owner(session),
            queue,
            position,
            volume,
        }
    }
    pub fn belongs_to(&self, session: &Session) -> bool {
        self.owner == owner(session)
    }
    fn valid(&self) -> bool {
        self.version == 1
            && self.owner.len() == 40
            && self.queue.valid_snapshot()
            && self.position <= Duration::from_secs(604800)
            && self.volume.is_finite()
            && (0.0..=1.0).contains(&self.volume)
    }
}
fn owner(session: &Session) -> String {
    let mut hash = Sha1::new();
    hash.update(b"Tonal.Playback.v1");
    hash.update(session.cookie_header());
    format!("{:x}", hash.finalize())
}
fn path() -> Option<PathBuf> {
    #[cfg(feature = "ui-capture")]
    if let Some(path) = std::env::var_os("TONAL_CAPTURE_STORE") {
        return Some(PathBuf::from(path));
    }
    Some(
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
            .join("Tonal")
            .join("playback.dpapi"),
    )
}

pub struct Store {
    sender: SyncSender<Option<Snapshot>>,
    worker: Option<thread::JoinHandle<()>>,
    errors: mpsc::Receiver<String>,
}
impl Store {
    pub fn new() -> Option<Self> {
        if cfg!(test)
            || (cfg!(feature = "ui-capture") && std::env::var_os("TONAL_PROFILE_TO").is_some())
            || (std::env::var_os("TONAL_CAPTURE_TO").is_some()
                && std::env::var_os("TONAL_CAPTURE_STORE").is_none())
        {
            return None;
        }
        let path = path()?;
        let (sender, receiver) = mpsc::sync_channel::<Option<Snapshot>>(1);
        let (error_sender, errors) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            while let Ok(snapshot) = receiver.recv() {
                if let Err(error) = save(&path, snapshot.as_ref()) {
                    let _ = error_sender.try_send(error);
                }
            }
        });
        Some(Self {
            sender,
            worker: Some(worker),
            errors,
        })
    }
    pub fn load() -> Option<Snapshot> {
        if cfg!(test)
            || (cfg!(feature = "ui-capture") && std::env::var_os("TONAL_PROFILE_TO").is_some())
            || (std::env::var_os("TONAL_CAPTURE_TO").is_some()
                && std::env::var_os("TONAL_CAPTURE_STORE").is_none())
        {
            return None;
        }
        load(&path()?).ok().flatten()
    }
    pub fn enqueue(&self, snapshot: Option<Snapshot>) -> bool {
        self.sender.try_send(snapshot).is_ok()
    }
    pub fn error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
    pub fn flush(mut self, snapshot: Option<Snapshot>) {
        let _ = self.sender.send(snapshot);
        drop(self.sender);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn load(path: &std::path::Path) -> Result<Option<Snapshot>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Lecture de la reprise impossible.".into()),
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Lecture de la reprise impossible.")?;
    if bytes.len() as u64 > LIMIT {
        return Err("Reprise trop volumineuse.".into());
    }
    let plain = Zeroizing::new(session::unprotect(&bytes)?);
    let snapshot: Snapshot = serde_json::from_slice(&plain).map_err(|_| "Reprise invalide.")?;
    if !snapshot.valid() {
        return Err("Reprise invalide.".into());
    }
    Ok(Some(snapshot))
}
fn save(path: &std::path::Path, snapshot: Option<&Snapshot>) -> Result<(), String> {
    let Some(snapshot) = snapshot else {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Effacement de la reprise impossible.".into()),
        };
    };
    if !snapshot.valid() {
        return Err("Reprise invalide.".into());
    }
    let bytes = Zeroizing::new(serde_json::to_vec(snapshot).map_err(|_| "Reprise invalide.")?);
    if bytes.len() as u64 > LIMIT {
        return Err("Reprise trop volumineuse.".into());
    }
    let encrypted = session::protect(&bytes)?;
    let directory = path.parent().ok_or("Dossier de reprise indisponible.")?;
    std::fs::create_dir_all(directory).map_err(|_| "Dossier de reprise indisponible.")?;
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .map_err(|_| "Sauvegarde de la reprise impossible.")?;
    file.write_all(&encrypted)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "Sauvegarde de la reprise impossible.")?;
    file.persist(path)
        .map_err(|_| "Sauvegarde de la reprise impossible.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_restart_preserves_shuffle_order_history_and_position_without_credentials_or_audio_urls(
    ) {
        let session = Session::from_cookies([
            ("SAPISID".into(), "fake-a".into()),
            ("SID".into(), "fake-b".into()),
        ])
        .unwrap();
        let tracks = (0..8)
            .map(|index| crate::provider::Track {
                video_id: format!("video_{index}"),
                title: format!("Titre {index}"),
                ..Default::default()
            })
            .collect();
        let mut queue =
            PlaybackQueue::selected(tracks, 3, crate::playback_queue::Repeat::All, true).unwrap();
        queue.forward(false);
        assert!(queue.remove_upcoming(3));
        queue.mark_complete();
        let snapshot = Snapshot::new(&session, queue.clone(), Duration::from_secs(43), 0.37);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("playback.dpapi");
        save(&path, Some(&snapshot)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.windows(5).any(|slice| slice == b"Titre"));
        let mut restored = load(&path).unwrap().unwrap();
        assert!(restored.belongs_to(&session));
        assert_eq!(restored.position, Duration::from_secs(43));
        assert_eq!(restored.volume, 0.37);
        assert_eq!(restored.queue.revision(), queue.revision());
        loop {
            assert_eq!(restored.queue.current(), queue.current());
            if queue.remaining() == 0 {
                break;
            }
            restored.queue.forward(false);
            queue.forward(false);
        }
        assert!(restored.queue.previous());
        let other = Session::from_cookies([
            ("SAPISID".into(), "another-a".into()),
            ("SID".into(), "another-b".into()),
        ])
        .unwrap();
        assert!(!restored.belongs_to(&other));
        let mut corrupt = serde_json::to_value(&snapshot).unwrap();
        corrupt["queue"]["order"] = serde_json::json!([999]);
        let invalid: Snapshot = serde_json::from_value(corrupt).unwrap();
        assert!(!invalid.valid());
        save(&path, None).unwrap();
        assert!(load(&path).unwrap().is_none());
    }
}
