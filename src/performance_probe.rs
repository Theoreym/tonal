//! Opt-in development scenario. No playlist writes, audible output or playback-state persistence.
use crate::app_state::AppState;
use eframe::egui;
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) struct Probe {
    path: PathBuf,
    stage: u8,
    since: Instant,
}
impl Probe {
    pub(crate) fn new() -> Option<Self> {
        let path = PathBuf::from(std::env::var_os("TONAL_PROFILE_TO")?);
        Some(Self {
            path,
            stage: 0,
            since: Instant::now(),
        })
    }
    fn phase(&mut self, label: &str, duration: Duration, ctx: &egui::Context) {
        self.since = Instant::now();
        self.stage += 1;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            let _ = writeln!(file, "{}", serde_json::json!({"phase":label,"time":time}));
        }
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            ctx.request_repaint();
        });
    }
    pub(crate) fn tick(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        let elapsed = self.since.elapsed();
        let sample = Duration::from_secs(20);
        match self.stage {
            0 => {
                if let Some(audio) = &mut state.audio {
                    audio.set_volume(0.0);
                }
                if !state.connected() || state.google_busy() {
                    if elapsed > Duration::from_secs(30) {
                        return true;
                    }
                    ctx.request_repaint_after(Duration::from_millis(200));
                } else if state.tracks.is_empty() {
                    if let Some(playlist) = state.playlists.first().cloned() {
                        state.open_playlist(ctx, playlist, false);
                    } else {
                        return true;
                    }
                    ctx.request_repaint_after(Duration::from_millis(200));
                } else {
                    self.phase("idle_visible", sample, ctx);
                }
            }
            1 if elapsed >= sample => {
                state.play_track(ctx, 0);
                self.phase("loading", Duration::from_millis(200), ctx);
            }
            2 => {
                if state.import_busy {
                    if elapsed > Duration::from_secs(30) {
                        return true;
                    }
                    ctx.request_repaint_after(Duration::from_millis(200));
                } else if state
                    .audio
                    .as_ref()
                    .is_some_and(|a| !a.is_empty() && !a.is_paused())
                {
                    self.phase("playing_visible", sample, ctx);
                } else {
                    return true;
                }
            }
            3 if elapsed >= sample => {
                state.set_playing(ctx, false);
                self.phase("paused_visible", sample, ctx);
            }
            4 if elapsed >= sample => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.phase("paused_hidden", sample, ctx);
            }
            5 if elapsed >= sample => {
                state.set_playing(ctx, true);
                self.phase("playing_hidden", sample, ctx);
            }
            6 if elapsed >= sample => {
                state.set_playing(ctx, false);
                self.phase("settled_hidden", Duration::from_secs(100), ctx);
            }
            7 if elapsed >= Duration::from_secs(100) => {
                self.phase("done", Duration::ZERO, ctx);
                return true;
            }
            _ => {}
        }
        false
    }
}
