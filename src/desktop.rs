//! Windows media controls and an optional notification icon; no browser or global key hooks.
use crate::app_state::AppState;
use eframe::egui;

#[cfg(windows)]
mod platform {
    use super::*;
    use souvlaki::{
        MediaControlEvent as Media, MediaControls, MediaMetadata, MediaPlayback, MediaPosition,
        PlatformConfig, SeekDirection,
    };
    use std::sync::mpsc::{self, Receiver};
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem},
        MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };
    enum Event {
        Media(Media),
        Show,
        Quit,
    }
    pub struct Desktop {
        controls: Option<MediaControls>,
        receiver: Receiver<Event>,
        sender: mpsc::Sender<Event>,
        tray: Option<TrayIcon>,
        quitting: bool,
        track: Option<crate::provider::Track>,
        playback: Option<(bool, u64, bool)>,
        duration: Option<std::time::Duration>,
        #[cfg(feature = "ui-capture")]
        hwnd: isize,
    }
    impl Desktop {
        pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
            crate::native_theme::dark_menus();
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let hwnd = cc
                .window_handle()
                .ok()
                .and_then(|handle| match handle.as_raw() {
                    RawWindowHandle::Win32(window) => {
                        Some(window.hwnd.get() as *mut std::ffi::c_void)
                    }
                    _ => None,
                });
            let (sender, receiver) = mpsc::channel();
            let controls = hwnd.and_then(|hwnd| {
                let mut controls = MediaControls::new(PlatformConfig {
                    dbus_name: "tonal",
                    display_name: "Tonal",
                    hwnd: Some(hwnd),
                })
                .ok()?;
                let sender = sender.clone();
                let ctx = cc.egui_ctx.clone();
                controls
                    .attach(move |event| {
                        let _ = sender.send(Event::Media(event));
                        ctx.request_repaint();
                    })
                    .ok()?;
                Some(controls)
            });
            #[cfg(feature = "ui-capture")]
            if std::env::var_os("TONAL_CAPTURE_TO").is_some() {
                eprintln!("Windows media controls: {}", controls.is_some());
            }
            Self {
                controls,
                receiver,
                sender,
                tray: None,
                quitting: false,
                track: None,
                playback: None,
                duration: None,
                #[cfg(feature = "ui-capture")]
                hwnd: hwnd.map_or(0, |hwnd| hwnd as isize),
            }
        }
        fn configure_tray(&mut self, ctx: &egui::Context, enabled: bool) {
            if !enabled {
                self.tray = None;
                return;
            }
            if self.tray.is_some() {
                return;
            }
            let make = || -> Result<TrayIcon, Box<dyn std::error::Error>> {
                let menu = Menu::new();
                let show = MenuItem::new("Ouvrir", true, None);
                let quit = MenuItem::new("Quitter", true, None);
                menu.append_items(&[&show, &quit])?;
                let show_id = show.id().clone();
                let quit_id = quit.id().clone();
                let sender = self.sender.clone();
                let wake = ctx.clone();
                MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                    let action = if event.id == show_id {
                        Some(Event::Show)
                    } else if event.id == quit_id {
                        Some(Event::Quit)
                    } else {
                        None
                    };
                    if let Some(action) = action {
                        let _ = sender.send(action);
                        wake.request_repaint();
                    }
                }));
                let sender = self.sender.clone();
                let wake = ctx.clone();
                TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Right,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        crate::native_theme::dark_menus();
                    }
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        let _ = sender.send(Event::Show);
                        wake.request_repaint();
                    }
                }));
                let icon = crate::window_chrome::app_icon();
                Ok(TrayIconBuilder::new()
                    .with_tooltip("Tonal")
                    .with_menu(Box::new(menu))
                    .with_menu_on_left_click(false)
                    .with_icon(tray_icon::Icon::from_rgba(
                        icon.rgba,
                        icon.width,
                        icon.height,
                    )?)
                    .build()?)
            };
            self.tray = make().ok();
        }
        pub fn poll(&mut self, state: &mut AppState, ctx: &egui::Context, close_to_tray: bool) {
            self.configure_tray(ctx, close_to_tray);
            for event in self.receiver.try_iter() {
                match event {
                    Event::Show | Event::Media(Media::Raise) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    Event::Quit | Event::Media(Media::Quit) => {
                        self.quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Event::Media(media) => match media {
                        Media::Play => state.set_playing(ctx, true),
                        Media::Pause => state.set_playing(ctx, false),
                        Media::Toggle => state.toggle_pause(ctx),
                        Media::Next => state.next(ctx, false),
                        Media::Previous => state.previous(ctx),
                        Media::Stop => state.stop(),
                        Media::SetPosition(position) => {
                            let _ = state.seek(position.0);
                        }
                        Media::Seek(direction) | Media::SeekBy(direction, _) => {
                            let amount = match media {
                                Media::SeekBy(_, amount) => amount,
                                _ => std::time::Duration::from_secs(10),
                            };
                            let position = state.position();
                            let position = if direction == SeekDirection::Forward {
                                position.saturating_add(amount)
                            } else {
                                position.saturating_sub(amount)
                            };
                            let _ = state.seek(position);
                        }
                        Media::SetVolume(volume) if volume.is_finite() => {
                            if let Some(audio) = &mut state.audio {
                                audio.set_volume(volume as f32);
                            }
                        }
                        _ => {}
                    },
                }
            }
            if !self.quitting
                && close_to_tray
                && self.tray.is_some()
                && ctx.input(|input| input.viewport().close_requested())
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
        pub fn publish(&mut self, state: &AppState) {
            let Some(controls) = &mut self.controls else {
                return;
            };
            let display_track = state.display_track();
            let duration = state
                .audio
                .as_ref()
                .and_then(|audio| audio.duration)
                .or_else(|| {
                    state
                        .now_playing
                        .as_ref()?
                        .duration_secs
                        .map(|seconds| std::time::Duration::from_secs(seconds.into()))
                });
            if self.track != display_track || self.duration != duration {
                self.track = display_track.clone();
                self.duration = duration;
                if let Some(track) = &display_track {
                    let fallback =
                        format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", track.video_id);
                    let cover = (state.radio_station.is_none()).then(|| {
                        track
                            .cover_url
                            .as_deref()
                            .filter(|url| crate::artwork::allowed_url(url))
                            .unwrap_or(&fallback)
                    });
                    let _ = controls.set_metadata(MediaMetadata {
                        title: Some(&track.title),
                        artist: Some(&track.artist),
                        album: Some(&track.album),
                        cover_url: cover,
                        duration,
                    });
                } else {
                    let _ = controls.set_metadata(MediaMetadata {
                        title: Some(""),
                        artist: Some(""),
                        album: Some(""),
                        ..Default::default()
                    });
                }
            }
            let paused = state.playback_paused();
            let position = state.position();
            let stopped = display_track.is_none();
            let playback = (paused, position.as_secs(), stopped);
            if self.playback != Some(playback) {
                let progress = state
                    .radio_station
                    .is_none()
                    .then_some(MediaPosition(position));
                let _ = controls.set_playback(if stopped {
                    MediaPlayback::Stopped
                } else if paused {
                    MediaPlayback::Paused { progress }
                } else {
                    MediaPlayback::Playing { progress }
                });
                self.playback = Some(playback);
            }
        }
        pub fn close(&self, ctx: &egui::Context, close_to_tray: bool) {
            if close_to_tray && self.tray.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        #[cfg(feature = "ui-capture")]
        pub fn qa_tray_roundtrip(&self, ctx: &egui::Context) {
            assert!(self.tray.is_some());
            self.close(ctx, true);
            let sender = self.sender.clone();
            let ctx = ctx.clone();
            let hwnd = self.hwnd;
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let hidden = unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd as _) == 0
                };
                ctx.data_mut(|data| data.insert_temp(egui::Id::unique("qa_tray_hidden"), hidden));
                let _ = sender.send(Event::Show);
                ctx.request_repaint();
            });
        }
        #[cfg(feature = "ui-capture")]
        pub fn qa_check_tray(&self, ctx: &egui::Context) {
            assert!(ctx
                .data(|data| data.get_temp::<bool>(egui::Id::unique("qa_tray_hidden")))
                .unwrap_or(false));
            assert!(unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(self.hwnd as _) != 0
            });
        }
        #[cfg(feature = "ui-capture")]
        pub fn quit(&mut self, ctx: &egui::Context) {
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
#[cfg(windows)]
pub use platform::Desktop;
#[cfg(not(windows))]
pub struct Desktop;
#[cfg(not(windows))]
impl Desktop {
    pub fn new(_: &eframe::CreationContext<'_>) -> Self {
        Self
    }
    pub fn poll(&mut self, _: &mut AppState, _: &egui::Context, _: bool) {}
    pub fn publish(&mut self, _: &AppState) {}
    pub fn close(&self, ctx: &egui::Context, _: bool) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
