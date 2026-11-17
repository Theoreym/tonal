use std::{
    fs::File,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};

use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};

pub struct AudioPlayer {
    // Keep the stream alive for the full lifetime of playback.
    _stream: OutputStream,
    sink: Sink,
    pub title: String,
    pub artist: String,
    pub cover_url: Option<String>,
    pub volume: f32,
    pub duration: Option<Duration>,
    native: Option<crate::native_stream::NativeControl>,
    pub quality: Option<crate::youtube::AudioQuality>,
    source_path: Option<PathBuf>,
    output_failed: Arc<AtomicBool>,
    paused_at: Option<SystemTime>,
    normalization: bool,
    loudness_db: Option<f32>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, String> {
        use rodio::cpal::traits::HostTrait;
        let output_failed = Arc::new(AtomicBool::new(false));
        let open = |builder: OutputStreamBuilder| {
            let failed = output_failed.clone();
            builder
                .with_error_callback(move |_| failed.store(true, Ordering::SeqCst))
                .open_stream_or_fallback()
        };
        let stream = OutputStreamBuilder::from_default_device()
            .and_then(open)
            .or_else(|original| {
                rodio::cpal::default_host()
                    .output_devices()
                    .ok()
                    .and_then(|mut devices| {
                        devices.find_map(|device| {
                            OutputStreamBuilder::from_device(device).and_then(open).ok()
                        })
                    })
                    .ok_or(original)
            })
            .map_err(|e| format!("Impossible d'ouvrir la sortie audio : {e}"))?;
        let sink = Sink::connect_new(stream.mixer());
        let volume = 0.7;
        sink.set_volume(volume);
        Ok(Self {
            _stream: stream,
            sink,
            title: String::new(),
            artist: String::new(),
            cover_url: None,
            volume,
            duration: None,
            native: None,
            quality: None,
            source_path: None,
            output_failed,
            paused_at: None,
            normalization: false,
            loudness_db: None,
        })
    }

    #[cfg(any(test, feature = "ui-capture"))]
    pub fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let file = File::open(path).map_err(|e| format!("Impossible d'ouvrir le fichier : {e}"))?;
        let source = Decoder::try_from(file)
            .map_err(|e| format!("Format audio non pris en charge : {e}"))?;
        self.duration = source.total_duration();
        self.sink = Sink::connect_new(self._stream.mixer());
        self.native = None;
        self.quality = None;
        self.loudness_db = None;
        self.sink.append(source);
        self.apply_volume();
        self.sink.play();
        self.title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Titre inconnu")
            .to_owned();
        self.artist.clear();
        self.cover_url = None;
        self.source_path = Some(path.to_path_buf());
        self.paused_at = None;
        Ok(())
    }

    pub fn toggle_pause(&mut self) {
        if self.sink.is_paused() {
            if let Some(native) = &self.native {
                native.set_paused(false);
            }
            self.sink.play();
            self.paused_at = None;
        } else {
            self.pause();
        }
    }

    pub fn pause(&mut self) {
        self.sink.pause();
        if let Some(native) = &self.native {
            native.set_paused(true);
        }
        self.paused_at.get_or_insert_with(SystemTime::now);
    }

    pub fn needs_reopen(&self) -> bool {
        self.output_failed.load(Ordering::SeqCst)
            || self
                .paused_at
                .and_then(|paused| paused.elapsed().ok())
                .is_some_and(|elapsed| elapsed >= Duration::from_secs(30 * 60))
    }

    pub fn reopen_output(&mut self) -> Result<(), String> {
        let mut replacement = Self::new()?;
        replacement.set_volume(self.volume);
        replacement.set_normalization(self.normalization);
        *self = replacement;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.loudness_db = None;
        self.sink = Sink::connect_new(self._stream.mixer());
        self.apply_volume();
        self.native = None;
        self.quality = None;
        self.title.clear();
        self.artist.clear();
        self.cover_url = None;
        self.duration = None;
        self.source_path = None;
        self.paused_at = None;
    }

    pub fn set_volume(&mut self, value: f32) {
        self.volume = value.clamp(0.0, 1.0);
        self.apply_volume();
    }

    fn apply_volume(&self) {
        self.sink
            .set_volume(self.volume * normalization_gain(self.normalization, self.loudness_db));
    }
    pub fn set_normalization(&mut self, enabled: bool) {
        self.normalization = enabled;
        self.apply_volume();
    }
    pub fn set_loudness(&mut self, level: Option<f32>) {
        self.loudness_db = level;
        self.apply_volume();
    }
    pub fn loudness_known(&self) -> bool {
        self.loudness_db.is_some()
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }
    pub fn is_empty(&self) -> bool {
        self.native
            .as_ref()
            .map_or_else(|| self.sink.empty(), |native| native.ended())
    }

    pub fn position(&self) -> Duration {
        self.native
            .as_ref()
            .map_or_else(|| self.sink.get_pos(), |native| native.position())
    }

    pub fn load_track(
        &mut self,
        audio: crate::native_stream::NativeStream,
        title: String,
        artist: String,
        cover_url: Option<String>,
        position: Duration,
        paused: bool,
    ) -> Result<(), String> {
        self.duration = audio.duration;
        self.loudness_db = audio.loudness_db;
        self.quality = Some(audio.quality.clone());
        let (control, source) = crate::native_stream::start(audio, paused);
        if !position.is_zero() {
            control.seek(position.min(self.duration.unwrap_or(position)))?;
        }
        self.sink = Sink::connect_new(self._stream.mixer());
        self.native = Some(control);
        self.source_path = None;
        self.apply_volume();
        self.sink.append(source);
        if paused {
            self.sink.pause();
        } else {
            self.sink.play();
        }
        self.paused_at = paused.then(SystemTime::now);
        self.title = title;
        self.artist = artist;
        self.cover_url = cover_url;
        Ok(())
    }

    pub fn load_radio(&mut self, source: crate::radio::Output, name: &str) {
        self.stop();
        self.title = name.into();
        self.artist = "Radio en direct".into();
        self.sink.append(source);
        self.sink.play();
    }

    pub fn seek(&self, position: Duration) -> Result<(), String> {
        let duration = self
            .duration
            .filter(|duration| !duration.is_zero())
            .ok_or("La durée de ce titre est indisponible.")?;
        if let Some(native) = &self.native {
            return native.seek(position.min(duration));
        }
        if self.is_empty() {
            return Err("Aucun titre en lecture.".into());
        }
        self.sink
            .try_seek(position.min(duration))
            .map_err(|_| "Impossible de déplacer la lecture de ce titre.".into())
    }

    pub fn replay(&mut self) -> Result<(), String> {
        if let Some(native) = &self.native {
            if self.is_paused() {
                native.set_paused(false);
            }
            native.seek(Duration::ZERO)?;
            self.sink.play();
            self.paused_at = None;
            return Ok(());
        }
        let path = self.source_path.as_ref().ok_or("Aucun titre à relancer.")?;
        let file = File::open(path).map_err(|_| "Impossible de relire le titre.".to_owned())?;
        let source =
            Decoder::try_from(file).map_err(|_| "Impossible de décoder le titre.".to_owned())?;
        self.duration = source.total_duration();
        self.sink = Sink::connect_new(self._stream.mixer());
        self.apply_volume();
        self.sink.append(source);
        self.sink.play();
        self.paused_at = None;
        Ok(())
    }

    #[cfg(feature = "ui-capture")]
    pub fn preview_position(&self, position: Duration) {
        self.sink
            .try_seek(position)
            .expect("Seek generated preview audio");
    }

    pub fn poll_finished(&mut self) {
        if self.is_empty() && !self.title.is_empty() {
            self.stop();
        }
    }

    pub fn buffering(&self) -> bool {
        self.native
            .as_ref()
            .is_some_and(|native| native.buffering())
    }
    pub fn take_error(&self) -> Option<String> {
        if self.output_failed.swap(false, Ordering::SeqCst) {
            return Some("La sortie audio a été interrompue.".into());
        }
        self.native.as_ref().and_then(|native| native.take_error())
    }
}

fn normalization_gain(enabled: bool, loudness: Option<f32>) -> f32 {
    if !enabled {
        return 1.0;
    }
    loudness
        .filter(|level| level.is_finite() && (-60.0..=60.0).contains(level))
        .map_or(1.0, |level| 10_f32.powf(-level.max(0.0) / 20.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_attenuates_loud_tracks_without_boosting_or_changing_user_volume() {
        assert!((normalization_gain(true, Some(6.0)) - 0.5011872).abs() < 0.00001);
        for level in [
            None,
            Some(-8.0),
            Some(0.0),
            Some(f32::NAN),
            Some(f32::INFINITY),
        ] {
            assert_eq!(normalization_gain(true, level), 1.0);
        }
        assert_eq!(normalization_gain(false, Some(12.0)), 1.0);
    }

    #[test]
    #[ignore = "Session Google, réseau et sortie Windows ; vérifie le niveau réel YouTube et les bascules, lecture muette puis en pause."]
    fn normalization_uses_live_metadata_and_toggles_without_reloading() {
        let ctx = eframe::egui::Context::default();
        let mut state = crate::app_state::AppState::new(&ctx);
        state.audio.as_mut().unwrap().set_volume(0.0);
        state.set_normalization(true);
        let until = |state: &mut crate::app_state::AppState,
                     ready: &dyn Fn(&crate::app_state::AppState) -> bool| {
            let started = Instant::now();
            while !ready(state) && started.elapsed() < Duration::from_secs(45) {
                state.poll(&ctx);
                thread::sleep(Duration::from_millis(20));
            }
            assert!(ready(state), "{}", state.status);
        };
        until(&mut state, &|state| {
            state.connected() && !state.playlists.is_empty()
        });
        state.open_playlist(&ctx, state.playlists[0].clone(), false);
        until(&mut state, &|state| !state.tracks.is_empty());
        state.play_track(&ctx, 0);
        until(&mut state, &|state| {
            !state.import_busy && state.audio.as_ref().is_some_and(|audio| !audio.is_empty())
        });
        let audio = state.audio.as_mut().unwrap();
        let loudness = audio
            .loudness_db
            .expect("YouTube must supply loudness for this music fixture");
        audio.pause();
        let position = audio.position();
        let title = audio.title.clone();
        audio.set_volume(0.4);
        assert!(
            (audio.sink.volume() - 0.4 * normalization_gain(true, Some(loudness))).abs() < 0.00001
        );
        audio.set_normalization(false);
        assert!((audio.sink.volume() - 0.4).abs() < 0.00001);
        audio.set_normalization(true);
        assert_eq!(audio.volume, 0.4);
        assert!(audio.is_paused());
        assert_eq!(audio.position(), position);
        assert_eq!(audio.title, title);
        eprintln!("YouTube loudness={loudness:.2} dB, gain={:.3}; toggle preserves volume, pause and position.",normalization_gain(true,Some(loudness)));
        state.stop();
    }
    #[test]
    #[ignore = "Session DPAPI, Internet, yt-dlp et sortie Windows ; simule une pause de huit heures et une sortie déconnectée, volume coupé."]
    fn overnight_and_output_failure_recover_the_current_track_at_its_position() {
        use crate::app_state::AppState;
        let ctx = eframe::egui::Context::default();
        let mut state = AppState::new(&ctx);
        state.audio.as_mut().unwrap().set_volume(0.0);
        let until = |state: &mut AppState, ready: &dyn Fn(&AppState) -> bool| {
            let start = Instant::now();
            while !ready(state) && start.elapsed() < Duration::from_secs(50) {
                state.poll(&ctx);
                thread::sleep(Duration::from_millis(20));
            }
            assert!(ready(state), "Recovery did not complete: {}", state.status);
        };
        until(&mut state, &|state| {
            state.connected() && !state.playlists.is_empty()
        });
        let playlist = state.playlists[0].clone();
        state.open_playlist(&ctx, playlist, false);
        until(&mut state, &|state| !state.tracks.is_empty());
        state.play_track(&ctx, 0);
        until(&mut state, &|state| {
            !state.import_busy
                && state.audio.as_ref().unwrap().position() >= Duration::from_millis(100)
        });
        let selected = state.now_playing.clone();
        state
            .audio
            .as_ref()
            .unwrap()
            .seek(Duration::from_secs(5))
            .unwrap();
        state.toggle_pause(&ctx);
        state.audio.as_mut().unwrap().paused_at =
            Some(SystemTime::now() - Duration::from_secs(8 * 3600));
        assert!(state.audio.as_ref().unwrap().needs_reopen());
        state.toggle_pause(&ctx);
        assert!(state.import_busy);
        assert_eq!(state.now_playing, selected);
        until(&mut state, &|state| {
            !state.import_busy && state.audio.as_ref().unwrap().position() > Duration::from_secs(5)
        });
        assert_eq!(state.audio.as_ref().unwrap().volume, 0.0);
        state
            .audio
            .as_ref()
            .unwrap()
            .output_failed
            .store(true, Ordering::SeqCst);
        state.poll(&ctx);
        assert!(state.audio.as_ref().unwrap().is_paused());
        let frozen = state.audio.as_ref().unwrap().position();
        assert_eq!(state.now_playing, selected);
        assert!(!state.audio.as_ref().unwrap().title.is_empty());
        state.toggle_pause(&ctx);
        assert!(state.import_busy);
        until(&mut state, &|state| {
            !state.import_busy && state.audio.as_ref().unwrap().position() > frozen
        });
        assert_eq!(state.now_playing, selected);
        assert_eq!(
            state.queue.current().unwrap().video_id,
            selected.unwrap().video_id
        );
        assert_eq!(state.audio.as_ref().unwrap().volume, 0.0);
        state.stop();
    }
    use std::{
        process::Command,
        thread,
        time::{Duration, Instant},
    };

    #[test]
    #[ignore = "Nécessite Internet, yt-dlp et une sortie Windows ; utilise la session DPAPI existante sans l'afficher."]
    fn authenticated_native_stream_smoke() {
        let session = crate::session::load()
            .unwrap()
            .expect("Session Google enregistrée");
        native_stream_smoke(Some(session));
    }

    #[test]
    #[ignore = "Nécessite Internet, yt-dlp et une sortie Windows ; lecture standard sans session Google."]
    fn public_native_stream_smoke() {
        native_stream_smoke(None);
    }

    fn native_stream_smoke(session: Option<crate::session::Session>) {
        use crate::provider::{StreamResolver, YouTubeStreamResolver};
        use std::sync::{atomic::AtomicBool, Arc};
        let authenticated = session.is_some();
        let started = Instant::now();
        let stream = YouTubeStreamResolver
            .prepare(
                "https://music.youtube.com/watch?v=jNQXAC9IVRw".into(),
                session,
                Arc::new(AtomicBool::new(false)),
                true,
            )
            .unwrap();
        eprintln!(
            "Native authenticated={authenticated}: {} (format {}), resolution+prebuffer={:?}",
            stream.quality.label(),
            stream.quality.format_id,
            started.elapsed()
        );
        if !authenticated {
            assert!(!matches!(stream.quality.format_id.as_str(), "774" | "141"));
        }
        let mut player = AudioPlayer::new().unwrap();
        player.set_volume(0.01);
        player
            .load_track(
                stream,
                "Test streaming".into(),
                "Test".into(),
                None,
                Duration::ZERO,
                false,
            )
            .unwrap();
        let audio_started = Instant::now();
        while player.position() < Duration::from_millis(100)
            && audio_started.elapsed() < Duration::from_secs(10)
        {
            assert!(player.take_error().is_none(), "Erreur de décodage natif");
            thread::sleep(Duration::from_millis(20));
        }
        assert!(player.position() >= Duration::from_millis(100));
        eprintln!("First audio after prebuffer: {:?}", audio_started.elapsed());
        player.toggle_pause();
        assert!(player.is_paused());
        player.seek(Duration::from_secs(10)).unwrap();
        thread::sleep(Duration::from_millis(200));
        assert!(player.is_paused());
        assert_eq!(player.position(), Duration::from_secs(10));
        player.toggle_pause();
        let seek_started = Instant::now();
        while player.position() <= Duration::from_secs(10)
            && seek_started.elapsed() < Duration::from_secs(10)
        {
            assert!(player.take_error().is_none(), "Erreur après déplacement");
            thread::sleep(Duration::from_millis(20));
        }
        assert!(player.position() > Duration::from_secs(10));
        player.seek(Duration::from_secs(2)).unwrap();
        let seek_started = Instant::now();
        while player.position() <= Duration::from_secs(2)
            && seek_started.elapsed() < Duration::from_secs(10)
        {
            assert!(player.take_error().is_none());
            thread::sleep(Duration::from_millis(20));
        }
        assert!(player.position() > Duration::from_secs(2));
        player.stop();
        assert!(player.is_empty());
    }

    #[test]
    #[ignore = "Nécessite FFmpeg et une sortie audio ; déplace la lecture d'un WAV silencieux."]
    fn seeking_preserves_pause_and_track_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("seek.wav");
        assert!(Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=44100:cl=stereo",
                "-t",
                "3",
                "-y"
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success());
        let mut player = AudioPlayer::new().unwrap();
        player.load(&path).unwrap();
        player.artist = "Artiste test".into();
        player.cover_url = Some("https://i.ytimg.com/test.jpg".into());
        player.toggle_pause();
        player.seek(Duration::from_millis(1500)).unwrap();
        assert!(player.is_paused());
        assert!(
            player.position().abs_diff(Duration::from_millis(1500)) < Duration::from_millis(100)
        );
        player.seek(Duration::from_millis(200)).unwrap();
        assert!(player.position() < Duration::from_millis(400));
        assert_eq!(player.artist, "Artiste test");
        assert!(player.cover_url.is_some());
        player.toggle_pause();
        thread::sleep(Duration::from_millis(150));
        assert!(player.position() > Duration::from_millis(250));
        player.stop();
        assert!(player.cover_url.is_none());
        assert!(player.seek(Duration::ZERO).is_err());
    }

    #[test]
    #[ignore = "Nécessite FFmpeg et une sortie audio Windows ; joue de courts sons de test."]
    fn local_playback_smoke() {
        let directory = tempfile::tempdir().unwrap();
        let mut player = AudioPlayer::new().expect("Sortie audio système");
        player.set_volume(0.02);
        for extension in ["wav", "flac", "mp3", "ogg"] {
            let path = directory.path().join(format!("test.{extension}"));
            let status = Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=0.7",
                    "-y",
                ])
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success());
            player.load(&path).unwrap();
            thread::sleep(Duration::from_millis(120));
            assert!(!player.is_empty(), "{extension}");
            assert!(
                player.sink.get_pos() > Duration::ZERO,
                "Le périphérique doit consommer les échantillons"
            );
            player.toggle_pause();
            assert!(player.is_paused());
            player.toggle_pause();
            assert!(!player.is_paused());
            // A bad file must not stop the currently playing track.
            player.artist = "Artiste de test".into();
            assert!(player.load(&directory.path().join("missing.wav")).is_err());
            assert_eq!(player.artist, "Artiste de test");
            player.stop();
            assert!(player.is_empty());
            assert!(player.title.is_empty());
            assert!(player.artist.is_empty());
            player.load(&path).unwrap();
            let started = Instant::now();
            while !player.is_empty() && started.elapsed() < Duration::from_secs(3) {
                thread::sleep(Duration::from_millis(20));
            }
            player.artist = "Artiste conservé en boucle".into();
            let title = player.title.clone();
            player.replay().unwrap();
            thread::sleep(Duration::from_millis(100));
            assert!(!player.is_empty());
            assert!(player.position() > Duration::ZERO);
            assert_eq!(player.title, title);
            assert_eq!(player.artist, "Artiste conservé en boucle");
            assert_eq!(player.volume, 0.02);
            let restarted = Instant::now();
            while !player.is_empty() && restarted.elapsed() < Duration::from_secs(3) {
                thread::sleep(Duration::from_millis(20));
            }
            player.poll_finished();
            assert!(player.is_empty());
            assert!(player.title.is_empty());
        }
    }
}
