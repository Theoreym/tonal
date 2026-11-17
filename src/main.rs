#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app_state;
mod artwork;
mod audio;
mod auth;
mod auth_diagnostics;
mod auth_navigation;
mod backdrop;
mod brands;
mod desktop;
mod direct_audio;
mod http_audio;
mod light_text;
mod native_stream;
#[cfg(windows)]
mod native_theme;
#[cfg(feature = "ui-capture")]
mod performance_probe;
mod playback_queue;
mod playback_store;
mod player_solver;
mod playlist_add;
mod playlist_cache;
mod playlist_management;
mod preferences;
mod provider;
mod queue_panel;
mod radio;
mod resolution_cache;
mod row_motion;
mod session;
mod suggestions;
mod ui;
mod window_chrome;
mod youtube;

use app_state::AppState;
use eframe::egui::{
    self, pos2, vec2, Align, Align2, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind,
};
use std::time::Duration;
use ui::{button, icon, icon_button, ButtonKind, Icon, BLACK, LINE, MUTED, RED, SURFACE, WHITE};

const RADIOS_ID: &str = "tonal://radios";

struct TonalApp {
    state: AppState,
    settings_open: bool,
    preferences: preferences::Preferences,
    backdrop: backdrop::Backdrop,
    filter: String,
    artwork: artwork::ArtworkCache,
    light_text: light_text::LightText,
    seek_preview: Option<f32>,
    unmuted_volume: f32,
    volume_interacting: bool,
    desktop: desktop::Desktop,
    queue_open: bool,
    queue_dragging: Option<usize>,
    suggestions_open: bool,
    radios_open: bool,
    add_menu_open: bool,
    add_menu_track: Option<String>,
    playlist_motion: row_motion::RowMotion,
    suggestion_motion: row_motion::RowMotion,
    #[cfg(feature = "ui-capture")]
    performance: Option<performance_probe::Probe>,
    #[cfg(feature = "ui-capture")]
    capture_started: std::time::Instant,
    #[cfg(feature = "ui-capture")]
    capture_requested: bool,
    #[cfg(feature = "ui-capture")]
    capture_settings_action_done: bool,
    #[cfg(feature = "ui-capture")]
    capture_pointer_release: Option<egui::Pos2>,
    #[cfg(feature = "ui-capture")]
    capture_queue_stage: u8,
    #[cfg(feature = "ui-capture")]
    capture_image: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<egui::ColorImage>>>>,
}

impl TonalApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let light_text = ui::configure(&cc.egui_ctx);
        let rounding = window_chrome::round_corners(cc);
        #[cfg(feature = "ui-capture")]
        if std::env::var_os("TONAL_CAPTURE_TO").is_some() {
            eprintln!(
                "Windows rounded corners: {}",
                if rounding.is_ok() {
                    "enabled"
                } else {
                    "unavailable"
                }
            );
        }
        let _ = rounding;
        let mut app = Self {
            state: AppState::new(&cc.egui_ctx),
            settings_open: cfg!(feature = "ui-capture")
                && std::env::var("TONAL_CAPTURE_PAGE").as_deref() == Ok("settings"),
            preferences: preferences::Preferences::load(),
            backdrop: backdrop::Backdrop::default(),
            filter: String::new(),
            artwork: artwork::ArtworkCache::new(),
            light_text,
            seek_preview: None,
            unmuted_volume: 0.7,
            volume_interacting: false,
            desktop: desktop::Desktop::new(cc),
            queue_open: false,
            queue_dragging: None,
            suggestions_open: false,
            radios_open: false,
            add_menu_open: false,
            add_menu_track: None,
            playlist_motion: Default::default(),
            suggestion_motion: Default::default(),
            #[cfg(feature = "ui-capture")]
            performance: performance_probe::Probe::new(),
            #[cfg(feature = "ui-capture")]
            capture_started: std::time::Instant::now(),
            #[cfg(feature = "ui-capture")]
            capture_requested: false,
            #[cfg(feature = "ui-capture")]
            capture_settings_action_done: false,
            #[cfg(feature = "ui-capture")]
            capture_pointer_release: None,
            #[cfg(feature = "ui-capture")]
            capture_queue_stage: 0,
            #[cfg(feature = "ui-capture")]
            capture_image: Default::default(),
        };
        app.state.set_smart_shuffle(app.preferences.smart_shuffle);
        app.state
            .set_normalization(app.preferences.normalize_volume);
        #[cfg(feature = "ui-capture")]
        let app = app.with_capture_audio(&cc.egui_ctx);
        app
    }

    fn header(&mut self, root: &mut egui::Ui) {
        let ctx = &root.ctx().clone();
        let caption_size = 46.0;
        let caption_inset = 6.0;
        egui::Panel::top("header")
            .exact_size(caption_size + caption_inset * 2.0)
            .frame(egui::Frame::new().fill(BLACK))
            .show(root, |ui| {
                let bar = ui.max_rect();
                let controls_left = bar.right() - caption_size * 3.0 - caption_inset;
                let account_rect = Rect::from_min_size(
                    pos2(controls_left - 162.0, bar.center().y - 20.0),
                    vec2(144.0, 40.0),
                );
                let search_width = (account_rect.left() - 24.0 - (bar.left() + 180.0)).min(480.0);
                let search_left = (bar.center().x - search_width / 2.0)
                    .min(account_rect.left() - 24.0 - search_width);
                let search_rect = Rect::from_min_size(
                    pos2(search_left, bar.center().y - 20.0),
                    vec2(search_width, 40.0),
                );

                window_chrome::title_drag(
                    ui,
                    Rect::from_min_max(
                        bar.min + vec2(6.0, 5.0),
                        pos2(search_rect.left() - 8.0, bar.bottom()),
                    ),
                    "title-drag-left",
                );
                window_chrome::title_drag(
                    ui,
                    Rect::from_min_max(
                        pos2(search_rect.right() + 8.0, bar.top() + 5.0),
                        pos2(account_rect.left() - 8.0, bar.bottom()),
                    ),
                    "title-drag-gap",
                );
                ui.painter().text(
                    pos2(bar.left() + 24.0, bar.center().y),
                    Align2::LEFT_CENTER,
                    "tonal.",
                    FontId::new(30.0, egui::FontFamily::Name("display".into())),
                    WHITE,
                );

                let mut search = ui.new_child(egui::UiBuilder::new().max_rect(search_rect));
                egui::Frame::new()
                    .fill(SURFACE)
                    .corner_radius(20)
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(&mut search, |ui| {
                        ui.spacing_mut().interact_size.y = 24.0;
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(18.0, 22.0), Sense::hover());
                            icon(
                                ui.painter(),
                                Icon::Search,
                                Rect::from_center_size(rect.center(), vec2(17.0, 17.0)),
                                MUTED,
                            );
                            let enabled = self.state.connected() && !self.suggestions_open;
                            let response = ui
                                .add_enabled(
                                    enabled,
                                    egui::TextEdit::singleline(&mut self.filter)
                                        .align(Align2::LEFT_CENTER)
                                        .min_size(vec2(0.0, 24.0))
                                        .id(egui::Id::unique("library_search"))
                                        .frame(egui::Frame::NONE)
                                        .desired_width((search_width - 56.0).max(80.0))
                                        .hint_text(
                                            RichText::new("Rechercher dans ta bibliothèque…")
                                                .color(MUTED),
                                        ),
                                )
                                .on_hover_text(
                                    "Filtrer les playlists ou les titres déjà chargés · Ctrl+F",
                                )
                                .on_disabled_hover_text(if self.suggestions_open {
                                    "La recherche reste limitée à ta bibliothèque."
                                } else {
                                    "Connecte YouTube Music pour rechercher dans ta bibliothèque."
                                });
                            if enabled
                                && ctx.input(|input| {
                                    input.modifiers.command && input.key_pressed(egui::Key::F)
                                })
                            {
                                response.request_focus();
                            }
                        });
                    });
                let mut account = ui.new_child(egui::UiBuilder::new().max_rect(account_rect));
                self.account_menu(&mut account, ctx);
                let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                for (index, symbol, label) in [
                    (0, Icon::Minimize, "Minimiser"),
                    (
                        1,
                        if maximized {
                            Icon::Restore
                        } else {
                            Icon::Maximize
                        },
                        if maximized { "Restaurer" } else { "Agrandir" },
                    ),
                    (2, Icon::Close, "Fermer"),
                ] {
                    let rect = Rect::from_min_size(
                        pos2(
                            controls_left + index as f32 * caption_size,
                            bar.top() + caption_inset,
                        ),
                        vec2(caption_size, caption_size),
                    );
                    if window_chrome::caption_button(ui, rect, symbol, label, index == 2).clicked()
                    {
                        match index {
                            0 => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                            1 => window_chrome::toggle_maximize(ctx),
                            _ => self.desktop.close(ctx, self.preferences.close_to_tray),
                        }
                    }
                }
            });
    }

    fn account_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let avatar = self.artwork.texture(
            ctx,
            self.state
                .account
                .as_ref()
                .and_then(|account| account.avatar_url.as_deref()),
        );
        let response = ui::account_button(
            ui,
            self.state.connected(),
            self.state
                .account
                .as_ref()
                .map(|account| account.given_name.as_str()),
            avatar.as_ref(),
        );
        let popup = egui::Popup::menu(&response)
            .frame(ui::popup_frame())
            .width(264.0)
            .gap(10.0);
        #[cfg(feature = "ui-capture")]
        let popup = if std::env::var("TONAL_CAPTURE_PAGE").as_deref() == Ok("menu") {
            popup.open(true)
        } else {
            popup
        };
        popup.show(|ui| {
            ui.set_min_width(248.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(account) = &self.state.account {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(42.0, 42.0), Sense::hover());
                            self.artwork
                                .paint(ui, rect, account.avatar_url.as_deref(), 21);
                            ui.add_space(4.0);
                        }
                        ui.vertical(|ui| {
                            ui.label(ui::heading(
                                self.state
                                    .account
                                    .as_ref()
                                    .map(|account| account.given_name.as_str())
                                    .unwrap_or("Ton compte"),
                                17.0,
                            ));
                            ui.label(
                                RichText::new(if self.state.connected() {
                                    "YouTube Music · Connecté"
                                } else {
                                    "YouTube Music · Non connecté"
                                })
                                .size(12.0)
                                .color(MUTED),
                            );
                        });
                    });
                });
            ui.separator();
            if self.state.google_busy() {
                if ui::menu_item(ui, "Annuler la connexion", Icon::Close, false).clicked() {
                    self.state.cancel_google_task();
                    ui.close();
                }
            } else if !self.state.connected()
                && ui::menu_item(ui, "Se connecter à Google", Icon::User, false).clicked()
            {
                self.state.login(ctx);
                ui.close();
            }
            if ui::menu_item(ui, "Paramètres", Icon::Settings, false).clicked() {
                self.settings_open = true;
                ui.close();
            }
            if self.state.connected() {
                ui.separator();
                if ui::menu_item(ui, "Se déconnecter", Icon::Logout, true).clicked() {
                    self.state.disconnect();
                    ui.close();
                }
            }
        });
    }
    fn hero(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, 286.0), Sense::hover());
        ui.painter().rect(
            rect,
            16,
            SURFACE,
            Stroke::new(1.0_f32, LINE),
            StrokeKind::Inside,
        );
        let origin = rect.min + vec2(32.0, 30.0);
        brands::paint(
            ui,
            brands::Brand::YouTubeMusic,
            Rect::from_min_size(origin, vec2(24.0, 24.0)),
            egui::Color32::WHITE,
        );
        ui.painter().text(
            origin + vec2(34.0, 12.0),
            Align2::LEFT_CENTER,
            "YOUTUBE MUSIC",
            FontId::proportional(11.0),
            MUTED,
        );
        ui.painter().text(
            origin + vec2(0.0, 48.0),
            Align2::LEFT_TOP,
            "Ta musique.\nÀ ta façon.",
            FontId::new(
                if width > 650.0 { 40.0 } else { 34.0 },
                egui::FontFamily::Name("display".into()),
            ),
            WHITE,
        );
        ui.painter().text(
            origin + vec2(0.0, 156.0),
            Align2::LEFT_TOP,
            "Retrouve tes playlists et tes morceaux.",
            FontId::proportional(13.0),
            MUTED,
        );
        let mut controls = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
            origin + vec2(0.0, 199.0),
            vec2(width - 64.0, 40.0),
        )));
        controls.horizontal(|ui| {
            if self.state.google_busy() {
                ui.spinner();
                ui.label(RichText::new("Connexion en cours…").color(MUTED));
                if button(ui, "Annuler", Icon::Close, ButtonKind::Quiet, true).clicked() {
                    self.state.cancel_google_task();
                }
            } else if button(ui, "Se connecter", Icon::Arrow, ButtonKind::Primary, true).clicked() {
                self.state.login(ctx);
            }
        });
        if width > 690.0 {
            let center = pos2(rect.right() - 150.0, rect.center().y);
            for radius in [98.0, 81.0, 63.0] {
                ui.painter().circle_stroke(
                    center,
                    radius,
                    Stroke::new(1.0_f32, egui::Color32::from_rgb(43, 32, 35)),
                );
            }
            ui.painter()
                .circle_filled(center, 43.0, egui::Color32::from_rgb(32, 20, 24));
            brands::paint(
                ui,
                brands::Brand::YouTubeMusic,
                Rect::from_center_size(center, vec2(46.0, 46.0)),
                egui::Color32::WHITE,
            );
        }
    }

    fn settings(&mut self, ctx: &egui::Context) {
        let id = egui::Id::unique("settings_modal");
        self.backdrop
            .paint(ctx, egui::LayerId::new(egui::Order::Foreground, id));
        let mut close = false;
        let response = egui::Modal::new(id)
            .area(
                egui::Modal::default_area(id)
                    .default_size(vec2(512.0, (ctx.content_rect().height() - 32.0).min(740.0))),
            )
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_gray(13))
                    .inner_margin(26)
                    .corner_radius(20)
                    .stroke(Stroke::new(1.0_f32, egui::Color32::from_gray(39)))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 12],
                        blur: 42,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(140),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(460.0_f32.min(ctx.content_rect().width() - 88.0));
                ui.spacing_mut().item_spacing.y = 6.0;
                let (header, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                ui.painter().text(
                    header.left_center(),
                    Align2::LEFT_CENTER,
                    "Paramètres",
                    FontId::new(24.0, egui::FontFamily::Name("bold".into())),
                    WHITE,
                );
                let mut close_ui =
                    ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(
                        pos2(header.right() - 16.0, header.center().y),
                        vec2(32.0, 32.0),
                    )));
                #[cfg(feature = "ui-capture")]
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::unique("qa_settings_close"), close_ui.max_rect())
                });
                close |= ui::transport_button(
                    &mut close_ui,
                    Icon::Close,
                    "Fermer les paramètres",
                    32.0,
                    20.0,
                    true,
                    false,
                )
                .clicked();
                ui.add_space(22.0);
                ui.label(RichText::new("COMPTES").size(11.0).color(MUTED));
                ui.add_space(8.0);
                let account = self.state.account.clone();
                let (card, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 90.0), Sense::hover());
                ui.painter().rect_filled(card, 12, SURFACE);
                let avatar_rect = Rect::from_center_size(
                    pos2(card.left() + 40.0, card.center().y),
                    vec2(48.0, 48.0),
                );
                if let Some(avatar) = self.artwork.texture(
                    ctx,
                    account
                        .as_ref()
                        .and_then(|account| account.avatar_url.as_deref()),
                ) {
                    artwork::paint_texture(ui, avatar_rect, &avatar, 24);
                } else {
                    ui.painter().circle_filled(avatar_rect.center(), 24.0, LINE);
                    icon(ui.painter(), Icon::User, avatar_rect.shrink(12.0), MUTED);
                }
                let text_rect = Rect::from_min_max(
                    pos2(card.left() + 80.0, card.top() + 23.0),
                    pos2(
                        card.right() - if account.is_some() { 56.0 } else { 134.0 },
                        card.bottom() - 12.0,
                    ),
                );
                let mut account_text = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
                account_text.spacing_mut().item_spacing.y = 6.0;
                account_text.spacing_mut().interact_size.y = 23.0;
                account_text.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let name = account
                        .as_ref()
                        .map(|a| {
                            if a.given_name.is_empty() {
                                a.display_name.as_str()
                            } else {
                                a.given_name.as_str()
                            }
                        })
                        .unwrap_or("Aucun compte");
                    let font = FontId::new(16.0, egui::FontFamily::Name("bold".into()));
                    let width = ui
                        .painter()
                        .layout_no_wrap(name.into(), font.clone(), WHITE)
                        .size()
                        .x
                        .min(
                            (ui.available_width() - if account.is_some() { 114.0 } else { 0.0 })
                                .max(0.0),
                        );
                    ui.add_sized(
                        vec2(width, 23.0),
                        egui::Label::new(RichText::new(name).font(font))
                            .truncate()
                            .show_tooltip_when_elided(false),
                    );
                    if let Some(account) = &account {
                        let (label, color) = match account.premium {
                            Some(true) => ("PREMIUM", egui::Color32::from_rgb(231, 193, 104)),
                            Some(false) => ("STANDARD", MUTED),
                            None => ("NON VÉRIFIÉ", MUTED),
                        };
                        let (pill, _) = ui.allocate_exact_size(vec2(106.0, 23.0), Sense::hover());
                        ui.painter()
                            .rect_filled(pill, 6, egui::Color32::from_gray(30));
                        ui.painter().text(
                            pill.center(),
                            Align2::CENTER_CENTER,
                            label,
                            FontId::proportional(10.0),
                            color,
                        );
                    }
                });
                account_text.horizontal(|ui| {
                    brands::image(ui, brands::Brand::YouTubeMusic, 14.0);
                    ui.label(RichText::new("YouTube Music").size(12.0).color(MUTED));
                });
                if account.is_some() {
                    let mut logout =
                        ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(
                            pos2(card.right() - 32.0, card.center().y),
                            vec2(32.0, 32.0),
                        )));
                    if ui::transport_button(
                        &mut logout,
                        Icon::Logout,
                        "Se déconnecter de YouTube Music",
                        32.0,
                        17.0,
                        true,
                        false,
                    )
                    .clicked()
                    {
                        self.state.disconnect();
                    }
                } else {
                    let mut login =
                        ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                            pos2(card.right() - 122.0, card.center().y - 20.0),
                            vec2(110.0, 40.0),
                        )));
                    let busy = self.state.google_busy();
                    if button(
                        &mut login,
                        if busy { "Annuler" } else { "Connexion" },
                        Icon::User,
                        ButtonKind::Secondary,
                        true,
                    )
                    .clicked()
                    {
                        if busy {
                            self.state.cancel_google_task();
                        } else {
                            self.state.login(ctx);
                            close = true;
                        }
                    }
                }
                ui.add_space(8.0);
                ui.add_enabled_ui(false, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 12, egui::Color32::from_gray(17));
                    brands::paint(
                        ui,
                        brands::Brand::Spotify,
                        Rect::from_center_size(
                            pos2(rect.left() + 30.0, rect.center().y),
                            vec2(23.0, 23.0),
                        ),
                        egui::Color32::from_gray(180),
                    );
                    ui.painter().text(
                        pos2(rect.left() + 54.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        "Connecter Spotify",
                        FontId::proportional(13.0),
                        egui::Color32::from_gray(190),
                    );
                    ui.painter().text(
                        pos2(rect.right() - 18.0, rect.center().y),
                        Align2::RIGHT_CENTER,
                        "SOON",
                        FontId::proportional(10.0),
                        egui::Color32::from_gray(180),
                    );
                });
                ui.add_space(22.0);
                ui.separator();
                ui.add_space(18.0);
                ui.label(RichText::new("OPTIONS").size(11.0).color(MUTED));
                ui.add_space(9.0);
                if ui::setting_toggle(
                    ui,
                    "Masquer dans la zone de notification",
                    "Continuer la lecture quand la fenêtre est fermée.",
                    &mut self.preferences.close_to_tray,
                )
                .changed()
                {
                    if let Err(error) = self.preferences.save() {
                        self.state.status = error;
                    }
                }
                if ui::setting_toggle(
                    ui,
                    "Aléatoire intelligent",
                    "Espace les artistes et les titres récemment joués.",
                    &mut self.preferences.smart_shuffle,
                )
                .changed()
                {
                    self.state.set_smart_shuffle(self.preferences.smart_shuffle);
                    if let Err(error) = self.preferences.save() {
                        self.state.status = error;
                    }
                }
                if ui::setting_toggle(
                    ui,
                    "Normaliser le volume",
                    "Atténue les titres trop forts, selon YouTube.",
                    &mut self.preferences.normalize_volume,
                )
                .changed()
                {
                    self.state
                        .set_normalization(self.preferences.normalize_volume);
                    if let Err(error) = self.preferences.save() {
                        self.state.status = error;
                    }
                }
                let (row, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
                ui.painter().text(
                    row.left_top(),
                    Align2::LEFT_TOP,
                    "Afficher la qualité du titre",
                    FontId::proportional(14.0),
                    WHITE,
                );
                ui.painter().text(
                    row.left_top() + vec2(0.0, 23.0),
                    Align2::LEFT_TOP,
                    "Un badge à côté du titre dans le lecteur.",
                    FontId::proportional(12.0),
                    MUTED,
                );
                let mut toggle =
                    ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                        pos2(row.right() - 44.0, row.top() + 4.0),
                        vec2(44.0, 26.0),
                    )));
                #[cfg(feature = "ui-capture")]
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::unique("qa_settings_toggle"), toggle.max_rect())
                });
                if ui::toggle(
                    &mut toggle,
                    &mut self.preferences.show_quality,
                    "Afficher la qualité dans le lecteur",
                )
                .changed()
                {
                    if let Err(error) = self.preferences.save() {
                        self.state.status = error;
                    }
                }
                ui.add_space(18.0);
                ui.label(
                    RichText::new("tonal v0.3")
                        .size(11.0)
                        .color(egui::Color32::from_gray(95)),
                );
            });
        if close || response.should_close() {
            self.settings_open = false;
        }
    }
    fn playlist_grid(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, limit: Option<usize>) {
        let filter = self.filter.to_lowercase();
        let mut playlists: Vec<_> = self
            .state
            .playlists
            .iter()
            .filter(|p| {
                filter.is_empty()
                    || format!("{} {}", p.title, p.subtitle)
                        .to_lowercase()
                        .contains(&filter)
            })
            .take(limit.unwrap_or(usize::MAX))
            .cloned()
            .collect();
        if filter.is_empty() || "radios".contains(&filter) {
            playlists.push(provider::Playlist {
                id: RADIOS_ID.into(),
                title: "Radios".into(),
                subtitle: format!("{} stations", radio::STATIONS.len()),
                ..Default::default()
            });
        }
        let columns = ((ui.available_width() + 16.0) / 174.0)
            .floor()
            .clamp(2.0, 5.0) as usize;
        let width = (ui.available_width() - (columns - 1) as f32 * 16.0) / columns as f32;
        let mut selected = None;
        for chunk in playlists.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for playlist in chunk {
                    let radios = playlist.id == RADIOS_ID;
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(width, width + 60.0), Sense::click());
                    #[cfg(feature = "ui-capture")]
                    if radios {
                        ctx.data_mut(|data| {
                            data.insert_temp(egui::Id::unique("qa_radio_collection"), rect)
                        });
                    }
                    #[cfg(feature = "ui-capture")]
                    if std::env::var("TONAL_CAPTURE_PAGE").as_deref() == Ok("playlist-context")
                        && playlist.id == playlists[0].id
                    {
                        ctx.data_mut(|data| {
                            data.insert_temp(egui::Id::unique("qa_playlist_context_rect"), rect)
                        });
                    }
                    let cover = Rect::from_min_size(rect.min, vec2(width, width));
                    if radios {
                        Self::radio_cover(ui, cover);
                    } else {
                        self.artwork
                            .paint(ui, cover, playlist.cover_url.as_deref(), 8);
                    }
                    let amount = ctx.animate_bool_with_time(
                        ui.make_persistent_id(("playlist-hover", &playlist.id)),
                        response.hovered() || response.has_focus(),
                        0.16,
                    );
                    ui::playlist_play_overlay(ui.painter(), cover, amount);
                    let mut text = ui.new_child(
                        egui::UiBuilder::new()
                            .layout(Layout::top_down(Align::Min))
                            .max_rect(Rect::from_min_max(
                                rect.min + vec2(0.0, width + 10.0),
                                rect.max,
                            )),
                    );
                    text.spacing_mut().item_spacing.y = 3.0;
                    text.add(
                        egui::Label::new(RichText::new(&playlist.title).size(14.0).strong())
                            .truncate()
                            .show_tooltip_when_elided(false),
                    );
                    text.add(
                        egui::Label::new(
                            RichText::new(if playlist.subtitle.is_empty() {
                                "Playlist"
                            } else {
                                &playlist.subtitle
                            })
                            .size(12.0)
                            .color(MUTED),
                        )
                        .truncate()
                        .show_tooltip_when_elided(false),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            radios || !self.state.google_busy(),
                            &playlist.title,
                        )
                    });
                    if !radios && response.secondary_clicked() {
                        self.state.inspect_playlist(ctx, playlist, false);
                    }
                    if !radios {
                        let access = self.state.playlist_management.access(&playlist.id).cloned();
                        response.context_menu(|ui| {
                            let width = ["Modifier", "Supprimer"]
                                .iter()
                                .map(|label| {
                                    ui.painter()
                                        .layout_no_wrap(
                                            (*label).into(),
                                            FontId::proportional(13.0),
                                            WHITE,
                                        )
                                        .size()
                                        .x
                                })
                                .fold(0.0_f32, f32::max)
                                .ceil()
                                + 58.0;
                            ui.set_width(width);
                            let readonly =
                                matches!(&access, Some(Ok(details)) if !details.editable);
                            ui.add_enabled_ui(
                                !self.state.playlist_management.busy() && !readonly,
                                |ui| {
                                    if ui::menu_item_width(
                                        ui,
                                        "Modifier",
                                        Icon::Pencil,
                                        false,
                                        width,
                                    )
                                    .clicked()
                                    {
                                        self.state.inspect_playlist(ctx, playlist, false);
                                        self.state
                                            .playlist_management
                                            .edit(playlist.clone(), false);
                                        ui.close();
                                    }
                                    if ui::menu_item_width(
                                        ui,
                                        "Supprimer",
                                        Icon::Trash,
                                        true,
                                        width,
                                    )
                                    .clicked()
                                    {
                                        self.state.inspect_playlist(ctx, playlist, false);
                                        self.state.playlist_management.edit(playlist.clone(), true);
                                        ui.close();
                                    }
                                },
                            );
                        });
                    }
                    if response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                        && (radios || !self.state.google_busy())
                    {
                        selected = Some(playlist.clone());
                    }
                }
            });
            ui.add_space(8.0);
        }
        if let Some(playlist) = selected {
            self.filter.clear();
            if playlist.id == RADIOS_ID {
                self.radios_open = true;
                self.suggestions_open = false;
            } else {
                self.state.open_playlist(ctx, playlist, false);
            }
        }
        if playlists.is_empty() {
            ui.label(
                RichText::new(if filter.is_empty() {
                    "Aucune playlist dans ta bibliothèque pour le moment."
                } else {
                    "Aucune playlist ne correspond."
                })
                .color(MUTED),
            );
        }
    }

    fn suggested_tracks(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let back = if self.state.selected_playlist.is_some() {
            "Revenir à la playlist"
        } else {
            "Bibliothèque"
        };
        if button(ui, back, Icon::Back, ButtonKind::Quiet, true).clicked() {
            self.suggestions_open = false;
            return;
        }
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.label(ui::heading("Suggestions", 30.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.state.suggestions.busy() {
                    ui.add(egui::Spinner::new().size(18.0));
                }
                if icon_button(
                    ui,
                    Icon::Refresh,
                    "Actualiser les suggestions",
                    36.0,
                    false,
                    !self.state.suggestions.busy() && self.state.now_playing.is_some(),
                )
                .clicked()
                {
                    self.state.load_suggestions(ctx, true);
                }
            });
        });
        if let Some(seed) = &self.state.suggestions.seed {
            ui.add(
                egui::Label::new(
                    RichText::new(format!("Autour de {} · {}", seed.title, seed.artist))
                        .size(13.0)
                        .color(MUTED),
                )
                .truncate()
                .show_tooltip_when_elided(false),
            );
        }
        ui.add_space(22.0);
        if self.state.suggestions.busy() && self.state.suggestions.tracks.is_empty() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Quelques découvertes se préparent…").color(MUTED));
            });
        }
        if let Some(error) = self.state.suggestions.error.clone() {
            ui.label(RichText::new(error).color(MUTED));
            if button(ui, "Réessayer", Icon::Refresh, ButtonKind::Secondary, true).clicked() {
                self.state.load_suggestions(ctx, true);
            }
        } else if self.state.suggestions.tracks.is_empty() && !self.state.suggestions.busy() {
            ui.label(
                RichText::new(if self.state.suggestions.seed.is_none() {
                    "Écoute un premier morceau pour découvrir des titres qui lui ressemblent."
                } else {
                    "Aucune suggestion pour ce morceau. Essaie après une autre écoute."
                })
                .color(MUTED),
            );
        }
        if !self.state.suggestions.tracks.is_empty() {
            ui.push_id("suggested-tracks", |ui| {
                if let Some(index) = self.track_rows(ui, ctx, true) {
                    self.state.play_suggestion(ctx, index);
                }
            });
        }
    }

    fn track_rows(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        suggested: bool,
    ) -> Option<usize> {
        let tracks = if suggested {
            &self.state.suggestions.tracks
        } else {
            &self.state.tracks
        };
        let filter = self.filter.to_lowercase();
        let mut play = None;
        let mut toggle_current = false;
        let mut occurrences = std::collections::HashMap::new();
        let rows: Vec<_> = tracks
            .iter()
            .enumerate()
            .filter_map(|(index, track)| {
                let occurrence = occurrences
                    .entry(track.video_id.as_str())
                    .or_insert(0_usize);
                let key = egui::Id::unique((&track.video_id, *occurrence));
                *occurrence += 1;
                (filter.is_empty()
                    || format!("{} {}", track.title, track.artist)
                        .to_lowercase()
                        .contains(&filter))
                .then_some((index, key))
            })
            .collect();
        let scope = format!(
            "{}:{filter}",
            if suggested {
                "suggestions"
            } else {
                self.state
                    .selected_playlist
                    .as_ref()
                    .map_or("playlist", |playlist| playlist.id.as_str())
            }
        );
        let top = ui.cursor().top();
        let first = (((ui.clip_rect().top() - top) / 76.0).floor() - 1.0).max(0.0) as usize;
        let last = (((ui.clip_rect().bottom() - top) / 76.0).ceil() + 2.0).max(0.0) as usize;
        let now = ctx.input(|input| input.time);
        let motion = if suggested {
            &mut self.suggestion_motion
        } else {
            &mut self.playlist_motion
        };
        motion.sync(
            &scope,
            rows.iter().map(|(_, key)| *key).collect(),
            first..last,
            now,
            !suggested,
        );
        let current_id = self
            .state
            .now_playing
            .as_ref()
            .map(|track| track.video_id.as_str());
        let paused = self
            .state
            .audio
            .as_ref()
            .is_some_and(|audio| audio.is_paused());
        let current_loaded = self
            .state
            .audio
            .as_ref()
            .is_some_and(|audio| !audio.is_empty());
        ui.spacing_mut().item_spacing.y = 0.0;
        for (index, key) in rows {
            let track = &tracks[index];
            let transition = motion.sample(key, now);
            ui.push_id(key, |ui| {
                let (allocated, response) = ui.allocate_exact_size(
                    vec2(ui.available_width(), 72.0 * transition.height),
                    if transition.active {
                        Sense::hover()
                    } else {
                        Sense::click()
                    },
                );
                let rect = Rect::from_min_size(
                    allocated.min + vec2(0.0, transition.offset),
                    vec2(allocated.width(), 72.0),
                );
                if !ui.is_rect_visible(rect) {
                    return;
                }
                if transition.active {
                    ctx.request_repaint();
                }
                if transition.opacity <= 0.0 {
                    return;
                }
                ui.multiply_opacity(transition.opacity);
                if transition.height < 1.0 {
                    ui.set_clip_rect(ui.clip_rect().intersect(allocated));
                }
                let active = current_id == Some(track.video_id.as_str());
                if active || response.hovered() || response.has_focus() {
                    ui.painter().rect_filled(
                        rect,
                        8,
                        if active {
                            egui::Color32::from_gray(38)
                        } else {
                            SURFACE
                        },
                    );
                }
                let cover = Rect::from_min_size(rect.min + vec2(10.0, 12.0), vec2(48.0, 48.0));
                self.artwork.paint(ui, cover, track.cover_url.as_deref(), 4);
                if active || response.hovered() || response.has_focus() {
                    ui.painter().rect_filled(
                        cover,
                        4,
                        egui::Color32::from_black_alpha(if active { 165 } else { 125 }),
                    );
                    icon(
                        ui.painter(),
                        if active && current_loaded && !paused {
                            Icon::Pause
                        } else {
                            Icon::Play
                        },
                        Rect::from_center_size(cover.center(), vec2(28.0, 28.0)),
                        WHITE,
                    );
                    if active && self.state.import_busy {
                        ui.painter().rect_filled(
                            cover.shrink(8.0),
                            5,
                            egui::Color32::from_black_alpha(225),
                        );
                        ui.put(
                            cover.shrink(10.0),
                            egui::Spinner::new().size(26.0).color(WHITE),
                        );
                    }
                }
                ui.painter().text(
                    pos2(rect.right() - 12.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    track
                        .duration_secs
                        .map(|seconds| time(Duration::from_secs(seconds.into())))
                        .unwrap_or_else(|| "—".into()),
                    FontId::proportional(14.0),
                    MUTED,
                );
                let text_rect = Rect::from_min_max(
                    pos2(cover.right() + 16.0, rect.center().y - 23.0),
                    pos2(rect.right() - 90.0, rect.center().y + 25.0),
                );
                #[cfg(feature = "ui-capture")]
                if index == 0 {
                    ctx.data_mut(|data| {
                        data.insert_temp(egui::Id::unique("qa_track_title"), text_rect);
                        data.insert_temp(egui::Id::unique("qa_track_hovered"), response.hovered());
                    });
                }
                let mut text = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
                text.spacing_mut().item_spacing.y = 4.0;
                text.add(
                    egui::Label::new(
                        RichText::new(&track.title)
                            .font(FontId::new(16.0, egui::FontFamily::Name("bold".into()))),
                    )
                    .truncate()
                    .show_tooltip_when_elided(false),
                );
                let subtitle = if track.album.is_empty() {
                    track.artist.clone()
                } else {
                    format!("{} · {}", track.artist, track.album)
                };
                text.add(
                    egui::Label::new(RichText::new(subtitle).size(14.0).color(MUTED))
                        .truncate()
                        .show_tooltip_when_elided(false),
                );
                let enabled = self.state.audio.is_some();
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, enabled, &track.title)
                });
                if response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                    && enabled
                {
                    if active && current_loaded && !self.state.import_busy {
                        toggle_current = true;
                    } else {
                        play = Some(index);
                    }
                }
            });
            ui.add_space(4.0 * transition.height);
        }
        if toggle_current {
            self.state.toggle_pause(ctx);
        }
        play
    }

    fn library(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.suggestions_open && self.state.connected() {
            self.suggested_tracks(ui, ctx);
            return;
        }
        if self.radios_open {
            self.radio_playlist(ui, ctx);
            return;
        }
        if !self.state.connected() {
            self.hero(ui, ctx);
            ui.add_space(22.0);
            ui.label(
                RichText::new("Tes playlists t'attendent ici.")
                    .size(20.0)
                    .strong(),
            );
            ui.label(RichText::new("Connecte YouTube Music pour les retrouver.").color(MUTED));
            ui.add_space(22.0);
            ui.label(ui::heading("Bibliothèque", 30.0));
            ui.add_space(24.0);
            self.playlist_grid(ui, ctx, None);
            return;
        }
        if let Some(playlist) = self.state.selected_playlist.clone() {
            if button(
                ui,
                "Bibliothèque",
                Icon::Back,
                ButtonKind::Quiet,
                !self.state.google_busy(),
            )
            .clicked()
            {
                self.state.leave_playlist();
                self.filter.clear();
                return;
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(100.0, 100.0), Sense::hover());
                self.artwork
                    .paint(ui, rect, playlist.cover_url.as_deref(), 8);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("PLAYLIST").size(10.0).strong().color(MUTED));
                    ui.label(RichText::new(&playlist.title).size(27.0).strong());
                    let total = playlist.track_count.or_else(|| {
                        (!self.state.google_busy()
                            && self.state.tracks_continuation.is_none()
                            && self.state.tracks_error.is_none())
                        .then_some(self.state.tracks.len() as u32)
                    });
                    if let Some(total) = total {
                        ui.label(
                            RichText::new(format!(
                                "{total} titre{}",
                                if total == 1 { "" } else { "s" }
                            ))
                            .size(13.0)
                            .color(MUTED),
                        );
                    }
                });
            });
            ui.add_space(22.0);
            ui.separator();
            if let Some(index) = self.track_rows(ui, ctx, false) {
                self.state.play_track(ctx, index);
            }
            if self.state.tracks_error.is_some() {
                if button(
                    ui,
                    "Réessayer le chargement",
                    Icon::Refresh,
                    ButtonKind::Quiet,
                    !self.state.google_busy(),
                )
                .clicked()
                {
                    self.state.retry_playlist(ctx);
                }
            } else if self.state.tracks_continuation.is_some()
                && ui.cursor().top() < ui.clip_rect().bottom() + 400.0
            {
                if self.state.google_busy() {
                    ui.spinner();
                } else {
                    self.state.open_playlist(ctx, playlist, true);
                }
            }
        } else {
            ui.horizontal(|ui| {
                ui.label(ui::heading("Bibliothèque", 30.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(
                        ui,
                        Icon::Refresh,
                        "Actualiser les playlists",
                        36.0,
                        false,
                        !self.state.google_busy(),
                    )
                    .clicked()
                    {
                        self.state.refresh_library(ctx, false);
                    }
                    if icon_button(
                        ui,
                        Icon::Plus,
                        "Créer une playlist",
                        36.0,
                        false,
                        !self.state.google_busy() && !self.state.playlist_management.busy(),
                    )
                    .clicked()
                    {
                        self.state.playlist_management.create();
                    }
                });
            });
            ui.add_space(24.0);
            self.playlist_grid(ui, ctx, None);
            if self.state.playlists_continuation.is_some()
                && button(
                    ui,
                    "Plus de playlists",
                    Icon::Arrow,
                    ButtonKind::Secondary,
                    !self.state.google_busy(),
                )
                .clicked()
            {
                self.state.refresh_library(ctx, true);
            }
        }
    }

    fn radio_cover(ui: &egui::Ui, rect: Rect) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        let id = egui::Id::unique("radios-collection-cover");
        let texture = ui
            .ctx()
            .data_mut(|data| data.get_temp::<egui::TextureHandle>(id))
            .unwrap_or_else(|| {
                let image = egui_extras::image::load_svg_bytes_with_size(
                    include_bytes!("../assets/radios-cover.svg"),
                    egui::load::SizeHint::Height(512),
                    &Default::default(),
                )
                .expect("Bundled Radios cover");
                let texture = ui.ctx().load_texture(
                    "Radios cover",
                    image,
                    egui::TextureOptions {
                        mipmap_mode: Some(egui::TextureFilter::Linear),
                        ..egui::TextureOptions::LINEAR
                    },
                );
                ui.ctx()
                    .data_mut(|data| data.insert_temp(id, texture.clone()));
                texture
            });
        egui::Image::new((texture.id(), texture.size_vec2()))
            .corner_radius(8)
            .paint_at(ui, rect);
    }

    fn radio_playlist(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if button(ui, "Bibliothèque", Icon::Back, ButtonKind::Quiet, true).clicked() {
            self.radios_open = false;
            self.filter.clear();
            return;
        }
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            let (cover, _) = ui.allocate_exact_size(vec2(100.0, 100.0), Sense::hover());
            Self::radio_cover(ui, cover);
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("PLAYLIST").size(10.0).strong().color(MUTED));
                ui.label(ui::heading("Radios", 27.0));
                ui.label(
                    RichText::new(format!("{} stations", radio::STATIONS.len()))
                        .size(13.0)
                        .color(MUTED),
                );
            });
        });
        ui.add_space(22.0);
        ui.separator();
        let filter = self.filter.to_lowercase();
        let stations: Vec<_> = radio::STATIONS
            .into_iter()
            .filter(|station| filter.is_empty() || station.name().to_lowercase().contains(&filter))
            .collect();
        if stations.is_empty() {
            ui.label(RichText::new("Aucune radio ne correspond.").color(MUTED));
            return;
        }
        let mut selected = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for station in stations {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click());
                let cover = Rect::from_min_size(rect.min + vec2(10.0, 12.0), vec2(48.0, 48.0));
                let active = self.state.radio_station == Some(station);
                let paused = self.state.playback_paused();
                if active || response.hovered() || response.has_focus() {
                    ui.painter().rect_filled(
                        rect,
                        8,
                        if active {
                            egui::Color32::from_gray(38)
                        } else {
                            SURFACE
                        },
                    );
                }
                station.paint(ui, cover);
                if active || response.hovered() || response.has_focus() {
                    ui.painter().rect_filled(
                        cover,
                        8,
                        egui::Color32::from_black_alpha(if active { 165 } else { 125 }),
                    );
                    icon(
                        ui.painter(),
                        if active && !paused {
                            Icon::Pause
                        } else {
                            Icon::Play
                        },
                        Rect::from_center_size(cover.center(), vec2(28.0, 28.0)),
                        WHITE,
                    );
                }
                ui.painter().text(
                    pos2(cover.right() + 16.0, rect.center().y - 11.0),
                    Align2::LEFT_CENTER,
                    station.name(),
                    FontId::new(16.0, egui::FontFamily::Name("bold".into())),
                    WHITE,
                );
                ui.painter().text(
                    pos2(cover.right() + 16.0, rect.center().y + 11.0),
                    Align2::LEFT_CENTER,
                    if active && paused {
                        "En pause"
                    } else {
                        "En direct"
                    },
                    FontId::proportional(14.0),
                    MUTED,
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        self.state.audio.is_some(),
                        station.name(),
                    )
                });
                if response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    selected = Some((station, active));
                }
            }
        });
        if let Some((station, active)) = selected {
            self.queue_open = false;
            self.add_menu_open = false;
            if active {
                self.state.toggle_pause(ctx);
            } else {
                self.state.play_radio(ctx, station);
            }
        }
    }

    fn playlist_add_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, rect: Rect) -> bool {
        let track_id = self
            .state
            .playlist_add
            .current
            .as_ref()
            .map(|track| track.video_id.clone());
        let visible = self.state.playlist_add.visible();
        if !visible || self.add_menu_track != track_id || self.settings_open {
            self.add_menu_open = false;
        }
        if !visible {
            return false;
        }
        let mut add_ui = ui.new_child(
            egui::UiBuilder::new()
                .scope_id(ui.make_persistent_id("player-playlist-add"))
                .max_rect(rect),
        );
        let response = ui::transport_button(
            &mut add_ui,
            Icon::PlaylistAdd,
            "Ajouter à une playlist",
            32.0,
            21.0,
            true,
            self.add_menu_open,
        );
        #[cfg(feature = "ui-capture")]
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::unique("qa_playlist_add_button"), response.rect)
        });
        if response.clicked() {
            self.add_menu_open = !self.add_menu_open;
            self.add_menu_track = track_id;
            self.queue_open = false;
        }
        let busy = self.state.playlist_add.busy();
        let mut added = false;
        let popup_width = 320.0;
        egui::Popup::menu(&response).open_bool(&mut self.add_menu_open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .align(egui::RectAlign::TOP).align_alternatives(&[]).width(popup_width).gap(10.0).frame(ui::popup_frame())
            .show(|ui| {
                ui.set_width(popup_width - 16.0);
                #[cfg(feature = "ui-capture")]
                if playlist_add::qa_mode().is_some()
                    && ctx.data(|data| data.get_temp::<std::time::Instant>(egui::Id::unique("qa_optimistic_at"))).is_none()
                {
                    if let Some(choice) = self.state.playlist_add.choices.as_ref().and_then(|choices| choices.iter().find(|choice| choice.contains == Some(false))).cloned() {
                        let track_id = self.state.now_playing.as_ref().unwrap().video_id.clone();
                        let before = self.state.playlists.iter().find(|playlist| playlist.id == choice.id).and_then(|playlist| playlist.track_count);
                        added = self.state.add_current_to_playlist(ctx, &choice.id);
                        assert!(added && !self.state.suggestions.tracks.iter().any(|track| track.video_id == track_id));
                        ctx.data_mut(|data| {
                            data.insert_temp(egui::Id::unique("qa_optimistic_at"), std::time::Instant::now());
                            data.insert_temp(egui::Id::unique("qa_optimistic_intent"), (track_id, choice.id, before));
                        });
                        self.capture_started = std::time::Instant::now() - Duration::from_secs(60);
                    }
                }
                if busy {
                    ui.horizontal(|ui| { ui.spinner(); ui.label("Ajout en cours…"); });
                } else if let Some(error) = self.state.playlist_add.error.clone() {
                    ui.label(RichText::new(error).size(12.0).color(MUTED));
                    if button(ui, "Actualiser", Icon::Refresh, ButtonKind::Quiet, true).clicked() { self.state.playlist_add.retry(); }
                } else if let Some(choices) = self.state.playlist_add.choices.clone() {
                    if choices.is_empty() {
                        ui.label(RichText::new("Aucune playlist modifiable. Crée une playlist dans YouTube Music, puis actualise.").size(12.0).color(MUTED));
                        if button(ui, "Actualiser", Icon::Refresh, ButtonKind::Quiet, true).clicked() { self.state.playlist_add.retry(); }
                    } else {
                        egui::ScrollArea::vertical().id_salt("playlist-add-choices").max_height(240.0).show(ui, |ui| {
                            for choice in choices {
                                ui.push_id(&choice.id, |ui| {
                                    let enabled = choice.contains == Some(false);
                                    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
                                    if !ui.is_rect_visible(row) { return; }
                                    if enabled && (response.hovered() || response.has_focus()) {
                                        ui.painter().rect_filled(row, 8, egui::Color32::from_gray(31));
                                    }
                                    let cover = Rect::from_min_size(row.min + vec2(8.0, 8.0), vec2(40.0, 40.0));
                                    let cover_url = choice.cover_url.as_deref().or_else(|| self.state.playlists.iter()
                                        .find(|playlist| playlist.id == choice.id).and_then(|playlist| playlist.cover_url.as_deref()));
                                    self.artwork.paint(ui, cover, cover_url, 4);
                                    let mut text = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(
                                        pos2(cover.right() + 12.0, row.center().y - 10.0), pos2(row.right() - 8.0, row.center().y + 12.0))));
                                    text.add(egui::Label::new(RichText::new(&choice.title).size(14.0).color(if enabled { WHITE } else { MUTED }))
                                        .truncate().show_tooltip_when_elided(false));
                                    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &choice.title));
                                    let response = if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response };
                                    if enabled && response.clicked() {
                                        added = self.state.add_current_to_playlist(ctx, &choice.id);
                                    }
                                });
                            }
                        });
                    }
                } else {
                    ui.horizontal(|ui| { ui.spinner(); ui.label("Vérification des playlists…"); });
                }
            });
        if added {
            self.add_menu_open = false;
        }
        true
    }

    fn playlist_notification(&mut self, ctx: &egui::Context) {
        #[cfg(feature = "ui-capture")]
        if std::env::var_os("TONAL_CAPTURE_TO").is_some()
            && std::env::var("TONAL_CAPTURE_PAGE").as_deref() == Ok("notification")
        {
            self.state
                .playlist_add
                .notice
                .get_or_insert_with(|| crate::playlist_add::Notice {
                    message: "Un titre ajouté à Mes découvertes".into(),
                    success: true,
                    at: std::time::Instant::now(),
                });
        }
        let Some(notice) = &self.state.playlist_add.notice else {
            return;
        };
        if notice.at.elapsed() > Duration::from_secs(5) {
            return;
        }
        egui::Area::new(egui::Id::unique("playlist-add-notification"))
            .order(egui::Order::Foreground)
            .interactable(false)
            .anchor(Align2::RIGHT_TOP, vec2(-16.0, 74.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .corner_radius(9)
                    .inner_margin(egui::Margin {
                        right: 14,
                        ..egui::Margin::same(6)
                    })
                    .show(ui, |ui| {
                        ui.set_max_width(368.0);
                        ui.spacing_mut().item_spacing.x = 8.75;
                        ui.spacing_mut().interact_size.y = 24.5;
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(24.5, 24.5), Sense::hover());
                            icon(
                                ui.painter(),
                                if notice.success {
                                    Icon::Check
                                } else {
                                    Icon::Close
                                },
                                rect,
                                BLACK,
                            );
                            ui.add_space(3.5);
                            self.light_text.label(
                                ui,
                                &notice.message,
                                FontId::new(14.0, egui::FontFamily::Name("display".into())),
                            );
                        });
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn player(&mut self, root: &mut egui::Ui) {
        let ctx = &root.ctx().clone();
        let live = self.state.radio_station.is_some();
        egui::Panel::bottom("player")
            .exact_size(112.0)
            .frame(egui::Frame::new().fill(BLACK))
            .show(root, |ui| {
                let bar = ui.max_rect();
                let (position, duration) = self
                    .state
                    .audio
                    .as_ref()
                    .map(|audio| (self.state.position(), audio.duration))
                    .unwrap_or_default();
                let duration = duration.or_else(|| {
                    self.state
                        .now_playing
                        .as_ref()
                        .and_then(|track| track.duration_secs)
                        .map(|seconds| Duration::from_secs(seconds.into()))
                });
                let playing = self
                    .state
                    .audio
                    .as_ref()
                    .is_some_and(|audio| !audio.is_empty());
                let paused = self.state.playback_paused();
                let total = duration.map(time).unwrap_or_else(|| {
                    if playing {
                        "--:--".into()
                    } else {
                        "0:00".into()
                    }
                });

                let rail = Rect::from_min_size(bar.min + vec2(0.0, 4.0), vec2(bar.width(), 3.0));
                let can_seek =
                    !live && playing && duration.is_some_and(|duration| !duration.is_zero());
                let actual = duration
                    .filter(|duration| !duration.is_zero())
                    .map(|duration| {
                        (position.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
                    })
                    .unwrap_or(0.0);
                let mut progress = if live {
                    1.0
                } else {
                    self.seek_preview.unwrap_or(actual)
                };
                let response = ui::seek_bar(ui, rail, &mut progress, can_seek);
                if can_seek && response.dragged() {
                    self.seek_preview = Some(progress);
                }
                if can_seek
                    && (response.clicked()
                        || response.drag_stopped()
                        || (response.changed() && !response.dragged()))
                {
                    if let (Some(audio), Some(duration)) = (&self.state.audio, duration) {
                        if let Err(error) = audio.seek(duration.mul_f32(progress)) {
                            self.state.status = error;
                        }
                    }
                    self.seek_preview = None;
                }
                if !can_seek {
                    self.seek_preview = None;
                }
                let displayed_position = self
                    .seek_preview
                    .and_then(|preview| duration.map(|duration| duration.mul_f32(preview)))
                    .unwrap_or(position);
                let elapsed = time(displayed_position);
                let hover_position = response.hover_pos().and_then(|pointer| {
                    duration.map(|duration| {
                        duration.mul_f32(((pointer.x - rail.left()) / rail.width()).clamp(0.0, 1.0))
                    })
                });
                if !live {
                    response.on_hover_text(format!(
                        "{} / {total}",
                        time(hover_position.unwrap_or(displayed_position))
                    ));
                }
                let content = Rect::from_min_max(pos2(bar.left(), rail.bottom()), bar.max);
                let inset = 8.0;
                let artwork_size = content.height() - inset * 2.0;
                let artwork = Rect::from_min_size(
                    content.min + vec2(inset, inset),
                    vec2(artwork_size, artwork_size),
                );
                if let Some(station) = self.state.radio_station {
                    station.paint(ui, artwork);
                } else {
                    self.artwork.paint(
                        ui,
                        artwork,
                        self.state
                            .now_playing
                            .as_ref()
                            .and_then(|track| track.cover_url.as_deref())
                            .or_else(|| {
                                self.state
                                    .audio
                                    .as_ref()
                                    .and_then(|audio| audio.cover_url.as_deref())
                            }),
                        8,
                    );
                }
                let text_rect = Rect::from_min_max(
                    pos2(artwork.right() + 18.0, content.center().y - 26.0),
                    pos2(bar.center().x - 148.0, content.center().y + 26.0),
                );
                let mut metadata = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
                metadata.spacing_mut().item_spacing.y = 4.0;
                let current = self
                    .state
                    .audio
                    .as_ref()
                    .filter(|audio| !audio.title.is_empty());
                let title = self
                    .state
                    .radio_station
                    .map(|station| station.name())
                    .or_else(|| {
                        self.state
                            .now_playing
                            .as_ref()
                            .map(|track| track.title.as_str())
                    })
                    .or_else(|| current.map(|audio| audio.title.as_str()))
                    .unwrap_or("Aucun titre en lecture");
                let artist = if live {
                    "Radio en direct"
                } else {
                    self.state
                        .now_playing
                        .as_ref()
                        .map(|track| track.artist.as_str())
                        .or_else(|| {
                            current.map(|audio| {
                                if audio.artist.is_empty() {
                                    "Artiste inconnu"
                                } else {
                                    audio.artist.as_str()
                                }
                            })
                        })
                        .unwrap_or("—")
                };
                let quality = self
                    .preferences
                    .show_quality
                    .then(|| {
                        self.state
                            .audio
                            .as_ref()
                            .and_then(|audio| audio.quality.as_ref())
                    })
                    .flatten();
                metadata.horizontal(|ui| {
                    let tier = quality.and_then(|quality| quality.tier());
                    let badge_width = if tier.is_some() {
                        ui::QUALITY_BADGE_SIZE + ui.spacing().item_spacing.x
                    } else {
                        0.0
                    };
                    let title_limit = (ui.available_width() - badge_width).max(0.0);
                    let title_galley = ui.painter().layout_no_wrap(
                        title.into(),
                        FontId::new(18.0, egui::FontFamily::Name("bold".into())),
                        WHITE,
                    );
                    let title_width = title_galley.size().x.min(title_limit);
                    let title_response = ui.add_sized(
                        vec2(title_width, 22.0),
                        egui::Label::new(
                            RichText::new(title)
                                .font(FontId::new(18.0, egui::FontFamily::Name("bold".into()))),
                        )
                        .truncate()
                        .show_tooltip_when_elided(false),
                    );
                    if let Some(tier) = tier {
                        let baseline = title_galley
                            .rows
                            .first()
                            .and_then(|row| {
                                row.glyphs.first().map(|glyph| {
                                    title_response.rect.top() + row.pos.y + glyph.pos.y
                                })
                            })
                            .unwrap_or(title_response.rect.bottom());
                        ui::quality_badge(ui, tier, Some(baseline), &mut self.light_text)
                            .on_hover_text(quality.unwrap().label());
                    }
                });
                metadata.add(
                    egui::Label::new(RichText::new(artist).size(13.0).color(MUTED))
                        .truncate()
                        .show_tooltip_when_elided(false),
                );

                let center = pos2(bar.center().x, content.center().y);
                let compact = bar.width() < 1040.0;
                let time_label = if live {
                    String::new()
                } else {
                    format!("{elapsed} / {total}")
                };
                let natural_time_width = if live {
                    14.0
                } else {
                    ui.painter()
                        .layout_no_wrap(time_label.clone(), FontId::proportional(14.0), MUTED)
                        .size()
                        .x
                };
                let has_add = self.state.playlist_add.visible();
                let duration_right = bar.right() - 164.0;
                let min_controls_right = center.x + if has_add { 162.0 } else { 146.0 };
                let time_scale = if compact {
                    ((duration_right - min_controls_right - 8.0) / natural_time_width.max(1.0))
                        .clamp(0.5, 1.0)
                } else {
                    1.0
                };
                let add_offset = if compact && has_add {
                    (duration_right - natural_time_width * time_scale - 24.0 - center.x)
                        .clamp(146.0, 170.0)
                } else {
                    170.0
                };
                let queue_offset = if compact && has_add {
                    (add_offset - 36.0).min(130.0)
                } else {
                    130.0
                };
                let mut suggestions_button = ui.new_child(
                    egui::UiBuilder::new()
                        .scope_id(ui.make_persistent_id("player-suggestions"))
                        .max_rect(Rect::from_center_size(
                            center - vec2(130.0, 0.0),
                            vec2(32.0, 32.0),
                        )),
                );
                if ui::transport_button(
                    &mut suggestions_button,
                    Icon::Music,
                    "Suggestions",
                    32.0,
                    21.0,
                    self.state.connected() && !live,
                    self.suggestions_open,
                )
                .clicked()
                {
                    self.suggestions_open = !self.suggestions_open;
                    if self.suggestions_open {
                        self.queue_open = false;
                        self.filter.clear();
                        self.state.load_suggestions(ctx, false);
                    }
                }
                let busy = self.state.import_busy;
                let mut previous = ui.new_child(
                    egui::UiBuilder::new()
                        .scope_id(ui.make_persistent_id("player-previous"))
                        .max_rect(Rect::from_center_size(
                            center - vec2(70.0, 0.0),
                            vec2(48.0, 48.0),
                        )),
                );
                if ui::transport_button(
                    &mut previous,
                    Icon::Previous,
                    "Précédent / recommencer",
                    48.0,
                    34.0,
                    !live && (playing || self.state.queue.can_previous()),
                    false,
                )
                .clicked()
                {
                    self.state.previous(ctx);
                }
                let mut play = ui.new_child(
                    egui::UiBuilder::new()
                        .scope_id(ui.make_persistent_id("player-play"))
                        .max_rect(Rect::from_center_size(center, vec2(78.0, 78.0))),
                );
                let play_response = ui::transport_button(
                    &mut play,
                    if paused || !playing {
                        Icon::Play
                    } else {
                        Icon::Pause
                    },
                    if paused {
                        "Reprendre"
                    } else {
                        "Lecture / pause"
                    },
                    78.0,
                    56.0,
                    (!busy || live) && (playing || self.state.now_playing.is_some() || live),
                    true,
                );
                if busy
                    || self.state.radio_buffering()
                    || (!paused
                        && self
                            .state
                            .audio
                            .as_ref()
                            .is_some_and(|audio| audio.buffering()))
                {
                    let spinner = Rect::from_center_size(center, vec2(32.0, 32.0));
                    ui.painter().rect_filled(
                        Rect::from_center_size(center, vec2(62.0, 62.0)),
                        10,
                        BLACK,
                    );
                    ui.put(spinner, egui::Spinner::new().size(32.0).color(WHITE));
                }
                if play_response.clicked() {
                    self.state.toggle_pause(ctx);
                }
                play_response.context_menu(|ui| {
                    if ui::menu_item(ui, "Arrêter", Icon::Stop, false).clicked() {
                        self.state.stop();
                        self.state.status = "Lecture arrêtée".into();
                        ui.close();
                    }
                });
                let mut next = ui.new_child(
                    egui::UiBuilder::new()
                        .scope_id(ui.make_persistent_id("player-next"))
                        .max_rect(Rect::from_center_size(
                            center + vec2(70.0, 0.0),
                            vec2(48.0, 48.0),
                        )),
                );
                if ui::transport_button(
                    &mut next,
                    Icon::Next,
                    "Suivant",
                    48.0,
                    34.0,
                    self.state.can_next(),
                    false,
                )
                .clicked()
                {
                    self.state.next(ctx, false);
                }

                let mut queue_button = ui.new_child(
                    egui::UiBuilder::new()
                        .scope_id(ui.make_persistent_id("player-queue"))
                        .max_rect(Rect::from_center_size(
                            center + vec2(queue_offset, 0.0),
                            vec2(32.0, 32.0),
                        )),
                );
                if ui::transport_button(
                    &mut queue_button,
                    Icon::Queue,
                    "File de lecture",
                    32.0,
                    21.0,
                    !live && self.state.queue.current().is_some(),
                    self.queue_open,
                )
                .clicked()
                {
                    self.queue_open = !self.queue_open;
                    self.add_menu_open = false;
                }

                let add_visible = self.playlist_add_button(
                    ui,
                    ctx,
                    Rect::from_center_size(center + vec2(add_offset, 0.0), vec2(32.0, 32.0)),
                );

                // Order follows YouTube Music: volume, repeat, shuffle.
                let volume_rect =
                    Rect::from_center_size(pos2(bar.right() - 130.0, center.y), vec2(40.0, 40.0));
                let hotzone = Rect::from_min_max(pos2(bar.right() - 300.0, content.top()), bar.max);
                let hovered = ctx.input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|p| hotzone.contains(p))
                });
                let reveal = hovered
                    || self.volume_interacting
                    || (cfg!(feature = "ui-capture")
                        && std::env::var_os("TONAL_CAPTURE_VOLUME").is_some());
                let amount = ctx.animate_bool_with_time(
                    ui.make_persistent_id("volume-reveal"),
                    reveal,
                    0.14,
                );
                let slider_width = if bar.width() < 960.0 {
                    if add_visible {
                        64.0
                    } else {
                        76.0
                    }
                } else {
                    112.0
                };
                let slider_rect = Rect::from_min_size(
                    pos2(volume_rect.left() - 12.0 - slider_width, center.y - 20.0),
                    vec2(slider_width, 40.0),
                );
                // In compact windows, lift the duration above the revealed slider
                // instead of sliding it into the queue and transport controls.
                let time_x = volume_rect.left()
                    - 14.0
                    - if compact {
                        0.0
                    } else {
                        (slider_width + 12.0) * amount
                    };
                let time_y = center.y - if compact { 28.0 * amount } else { 0.0 };
                #[cfg(feature = "ui-capture")]
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::unique("qa_volume_slider"), slider_rect);
                    data.insert_temp(egui::Id::unique("qa_volume_reveal"), amount);
                    data.insert_temp(egui::Id::unique("qa_duration_lift"), center.y - time_y);
                });
                if live {
                    ui::live_indicator(ui, pos2(time_x - 5.0, time_y), !paused);
                } else {
                    ui.painter().text(
                        pos2(time_x, time_y),
                        Align2::RIGHT_CENTER,
                        time_label,
                        FontId::proportional(14.0 * time_scale),
                        MUTED,
                    );
                }
                self.volume_interacting = false;
                if let Some(audio) = &mut self.state.audio {
                    if amount > 0.01 {
                        let mut slider = ui.new_child(
                            egui::UiBuilder::new()
                                .scope_id(ui.make_persistent_id("player-volume-slider"))
                                .max_rect(slider_rect),
                        );
                        slider.set_opacity(amount);
                        // Fade in the slider while the duration makes room to its left.
                        slider.set_clip_rect(Rect::from_min_max(
                            pos2(
                                volume_rect.left() - 12.0 - slider_width * amount,
                                content.top(),
                            ),
                            pos2(volume_rect.left() - 12.0, content.bottom()),
                        ));
                        let mut volume = audio.volume;
                        let response = ui::volume_slider(&mut slider, &mut volume, slider_width);
                        if response.changed() {
                            audio.set_volume(volume);
                        }
                        self.volume_interacting = response.dragged();
                        #[cfg(feature = "ui-capture")]
                        if response.has_focus() {
                            ctx.data_mut(|data| {
                                data.insert_temp(egui::Id::unique("qa_volume_had_focus"), true)
                            });
                        }
                        if response.has_focus() && !hovered && !response.dragged() {
                            response.surrender_focus();
                        }
                    }
                    let mut volume_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .scope_id(ui.make_persistent_id("player-volume"))
                            .max_rect(volume_rect),
                    );
                    if ui::transport_button(
                        &mut volume_ui,
                        if audio.volume == 0.0 {
                            Icon::Muted
                        } else {
                            Icon::Volume
                        },
                        "Activer / couper le son",
                        40.0,
                        21.0,
                        true,
                        false,
                    )
                    .clicked()
                    {
                        if audio.volume > 0.0 {
                            self.unmuted_volume = audio.volume;
                            audio.set_volume(0.0);
                        } else {
                            audio.set_volume(self.unmuted_volume.max(0.05));
                        }
                    }
                }
                use playback_queue::Repeat;
                let mut repeat = ui.new_child(
                    egui::UiBuilder::new()
                        .scope_id(ui.make_persistent_id("player-repeat"))
                        .max_rect(volume_rect.translate(vec2(46.0, 0.0))),
                );
                let mode = self.state.queue.repeat;
                if ui::transport_button(
                    &mut repeat,
                    if mode == Repeat::One {
                        Icon::RepeatOne
                    } else {
                        Icon::Repeat
                    },
                    match mode {
                        Repeat::Off => "Répétition désactivée · activer la boucle playlist",
                        Repeat::All => "Boucle playlist · répéter ce titre",
                        Repeat::One => "Boucle sur ce titre · désactiver",
                    },
                    40.0,
                    21.0,
                    !busy && !live,
                    !live && mode != Repeat::Off,
                )
                .clicked()
                {
                    self.state.queue.repeat = mode.cycle();
                }
                let mut shuffle = ui.new_child(
                    egui::UiBuilder::new()
                        .scope_id(ui.make_persistent_id("player-shuffle"))
                        .max_rect(volume_rect.translate(vec2(92.0, 0.0))),
                );
                #[cfg(feature = "ui-capture")]
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::unique("qa_shuffle"), shuffle.max_rect())
                });
                let shuffle_response = ui::transport_button(
                    &mut shuffle,
                    Icon::Shuffle,
                    if self.state.shuffle_busy() {
                        "Préparation de la playlist entière…"
                    } else {
                        "Lecture aléatoire"
                    },
                    40.0,
                    21.0,
                    !busy && !live,
                    !live && self.state.queue.shuffle,
                );
                if shuffle_response.clicked() {
                    self.state.toggle_shuffle(ctx);
                }
                if self.state.shuffle_busy() {
                    let spinner = Rect::from_center_size(
                        shuffle.max_rect().right_top() + vec2(-5.0, 5.0),
                        vec2(10.0, 10.0),
                    );
                    shuffle.put(spinner, egui::Spinner::new().size(10.0).color(WHITE));
                }
            });
    }
}

fn time(duration: Duration) -> String {
    format!("{}:{:02}", duration.as_secs() / 60, duration.as_secs() % 60)
}

impl eframe::App for TonalApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.settings_open
            && !self.state.playlist_management.is_open()
            && !ctx.egui_wants_keyboard_input()
            && !egui::Popup::is_any_open(ctx)
        {
            let (play, previous, next, down, up) = ctx.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Space),
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowLeft),
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowRight),
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowDown),
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowUp),
                )
            });
            if play {
                self.state.set_playing(ctx, self.state.playback_paused());
            }
            if previous {
                self.state.previous(ctx);
            }
            if next {
                self.state.next(ctx, false);
            }
            if let Some(audio) = &mut self.state.audio {
                if down {
                    audio.set_volume((audio.volume - 0.05).max(0.0));
                }
                if up {
                    audio.set_volume((audio.volume + 0.05).min(1.0));
                }
            }
        }
        // egui 0.37 runs logic without drawing when the window is minimized.
        // Keep stream results and automatic queue progression alive in that state.
        let close_to_tray = self.preferences.close_to_tray
            || (cfg!(feature = "ui-capture")
                && std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("tray"));
        self.desktop.poll(&mut self.state, ctx, close_to_tray);
        self.state.poll(ctx);
        #[cfg(feature = "ui-capture")]
        if let Some(probe) = &mut self.performance {
            if probe.tick(&mut self.state, ctx) {
                self.desktop.quit(ctx);
            }
        }
        self.desktop.publish(&self.state);
        if self.state.import_busy
            || self
                .state
                .audio
                .as_ref()
                .is_some_and(|audio| !audio.is_empty() && !audio.is_paused())
        {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl {
            self.backdrop.destroy(gl);
        }
    }
    #[cfg(feature = "ui-capture")]
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if std::env::var("TONAL_CAPTURE_PAGE").as_deref() == Ok("playlist-context")
            && !self.capture_settings_action_done
            && self.capture_started.elapsed() > Duration::from_millis(1800)
        {
            if let Some(rect) =
                ctx.data(|data| data.get_temp::<Rect>(egui::Id::unique("qa_playlist_context_rect")))
            {
                let pos = rect.center();
                input.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true, false] {
                    input.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                self.capture_settings_action_done = true;
            }
        }
        if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("volume-leave")
            && !self.capture_settings_action_done
            && self.capture_started.elapsed() > Duration::from_millis(2200)
            && self.state.playlist_add.choices.is_some()
            && self.state.playlist_add.visible()
        {
            let slider =
                ctx.data(|data| data.get_temp::<Rect>(egui::Id::unique("qa_volume_slider")));
            if let Some(slider) = slider {
                let stage = self.capture_queue_stage;
                if stage == 1
                    && ctx
                        .data(|data| data.get_temp::<f32>(egui::Id::unique("qa_volume_reveal")))
                        .unwrap_or(0.0)
                        < 0.99
                {
                    ctx.request_repaint_after(Duration::from_millis(50));
                    return;
                }
                let pos = match stage {
                    0 => pos2(
                        ctx.content_rect().right() - 130.0,
                        ctx.content_rect().bottom() - 52.0,
                    ),
                    1 | 2 => slider.center(),
                    _ => ctx.content_rect().center(),
                };
                input.events.push(egui::Event::PointerMoved(pos));
                if stage == 1 || stage == 2 {
                    input.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: stage == 1,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                self.capture_queue_stage += 1;
                if stage == 3 {
                    self.capture_settings_action_done = true;
                }
                ctx.request_repaint();
                return;
            }
        }
        if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("queue-move")
            && self.capture_started.elapsed() > Duration::from_millis(2200)
            && self.capture_queue_stage < 4
        {
            let source = ctx
                .data(|data| data.get_temp::<Rect>(egui::Id::unique(("qa_queue_handle", 2usize))));
            let target =
                ctx.data(|data| data.get_temp::<Rect>(egui::Id::unique(("qa_queue_row", 3usize))));
            if let (Some(source), Some(target)) = (source, target) {
                let stage = self.capture_queue_stage;
                let pos = if stage == 0 {
                    source.center()
                } else if stage == 1 {
                    source.center() + vec2(0.0, 14.0)
                } else {
                    target.center()
                };
                input.events.push(egui::Event::PointerMoved(pos));
                if stage == 0 || stage == 3 {
                    input.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: stage == 0,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                self.capture_queue_stage += 1;
                if stage == 3 {
                    self.capture_settings_action_done = true;
                }
                ctx.request_repaint();
                return;
            }
        }
        if let Some(pos) = self.capture_pointer_release.take() {
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
            self.capture_settings_action_done = true;
        } else if !self.capture_settings_action_done
            && self.capture_started.elapsed() > Duration::from_millis(2200)
        {
            if let Ok(action) = std::env::var("TONAL_CAPTURE_ACTION")
                .or_else(|_| std::env::var("TONAL_CAPTURE_SETTINGS_ACTION"))
            {
                if action == "tray" {
                    self.desktop.qa_tray_roundtrip(ctx);
                    self.capture_settings_action_done = true;
                    return;
                }
                let position = match action.as_str() {
                    "radio-open" => ctx
                        .data(|data| data.get_temp::<Rect>(egui::Id::unique("qa_radio_collection")))
                        .map(|rect| rect.center()),
                    "playlist-add-open" | "playlist-add-optimistic" | "playlist-add-rollback"
                        if self.state.playlist_add.choices.is_some()
                            && self.state.playlist_add.visible() =>
                    {
                        ctx.data(|data| {
                            data.get_temp::<Rect>(egui::Id::unique("qa_playlist_add_button"))
                        })
                        .map(|rect| rect.center())
                    }
                    "shuffle" if self.state.queue.current().is_some() => ctx
                        .data(|data| data.get_temp::<Rect>(egui::Id::unique("qa_shuffle")))
                        .map(|rect| rect.center()),
                    "close" | "toggle" => ctx
                        .data(|data| {
                            data.get_temp::<Rect>(egui::Id::unique(format!("qa_settings_{action}")))
                        })
                        .map(|rect| rect.center()),
                    "outside" => Some(pos2(12.0, ctx.content_rect().center().y)),
                    "queue-close" => ctx
                        .data(|data| data.get_temp::<Rect>(egui::Id::unique("qa_queue_close")))
                        .map(|rect| rect.center()),
                    _ => None,
                };
                if let Some(pos) = position {
                    input.events.push(egui::Event::PointerMoved(pos));
                    input.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    });
                    self.capture_pointer_release = Some(pos);
                } else if action == "escape" {
                    input.events.push(egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                    self.capture_settings_action_done = true;
                }
            }
        }
        if std::env::var_os("TONAL_CAPTURE_SEEK_HOVER").is_some() {
            let rect = input.screen_rect.unwrap_or_else(|| ctx.content_rect());
            input.events.push(egui::Event::PointerMoved(pos2(
                rect.width() * 0.4,
                rect.bottom() - 106.5,
            )));
        }
        if std::env::var_os("TONAL_CAPTURE_TRACK_HOVER").is_some() {
            if let Some(rect) =
                ctx.data(|data| data.get_temp::<Rect>(egui::Id::unique("qa_track_title")))
            {
                input
                    .events
                    .push(egui::Event::PointerMoved(rect.min + vec2(20.0, 7.0)));
            }
        }
        if std::env::var_os("TONAL_CAPTURE_PLAYLIST_HOVER").is_some() {
            input
                .events
                .push(egui::Event::PointerMoved(pos2(150.0, 250.0)));
        }
        if let Ok(scrollbar) = std::env::var("TONAL_CAPTURE_SCROLLBAR_HOVER") {
            let pos = if scrollbar == "queue" {
                ctx.data(|data| data.get_temp::<Rect>(egui::Id::unique("qa_queue_scrollbar")))
                    .map(|rect| pos2(rect.right() - 3.0, rect.center().y))
            } else {
                Some(pos2(ctx.content_rect().right() - 17.0, 180.0))
            };
            if let Some(pos) = pos {
                input.events.push(egui::Event::PointerMoved(pos));
            }
        }
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        self.artwork.poll(ctx);
        #[cfg(feature = "ui-capture")]
        if std::env::var_os("TONAL_CAPTURE_PENDING_TRACK").is_some()
            && self.state.now_playing.is_none()
            && !self.state.tracks.is_empty()
        {
            self.state.play_track(ctx, 0);
            assert!(self.state.import_busy);
            assert!(self.state.audio.as_ref().unwrap().title.is_empty());
            self.capture_started = std::time::Instant::now() - Duration::from_secs(60);
        }
        #[cfg(feature = "ui-capture")]
        if std::env::var_os("TONAL_CAPTURE_ACTIVE_TRACK").is_some()
            && self.state.queue.current().is_none()
        {
            if let Some(track) = self
                .state
                .tracks
                .get(1)
                .or_else(|| self.state.tracks.first())
                .cloned()
            {
                if let Some(audio) = &mut self.state.audio {
                    self.state.now_playing = Some(track.clone());
                    audio.title = track.title;
                    audio.artist = track.artist;
                    audio.cover_url = track.cover_url;
                    audio.duration = track
                        .duration_secs
                        .map(|seconds| Duration::from_secs(seconds.into()));
                    self.state.queue = playback_queue::PlaybackQueue::selected(
                        self.state.tracks.clone(),
                        1.min(self.state.tracks.len() - 1),
                        playback_queue::Repeat::Off,
                        false,
                    )
                    .unwrap();
                    self.state.queue.playlist_id = self
                        .state
                        .selected_playlist
                        .as_ref()
                        .map(|playlist| playlist.id.clone());
                }
            }
        }
        #[cfg(feature = "ui-capture")]
        if matches!(
            std::env::var("TONAL_CAPTURE_PAGE").as_deref(),
            Ok("playlist" | "queue" | "suggestions" | "playlist-add")
        ) && self.state.connected()
            && !self.state.google_busy()
            && self.state.selected_playlist.is_none()
        {
            if let Some(playlist) = self.state.playlists.first().cloned() {
                self.state.open_playlist(ctx, playlist, false);
            }
        }
        #[cfg(feature = "ui-capture")]
        if matches!(
            std::env::var("TONAL_CAPTURE_PAGE").as_deref(),
            Ok("suggestions" | "playlist-add")
        ) && !self.suggestions_open
            && !self.state.tracks.is_empty()
        {
            if self.state.now_playing.is_none() {
                self.state.now_playing = self.state.tracks.first().cloned();
            }
            self.filter.clear();
            self.suggestions_open = true;
            self.state.load_suggestions(ctx, false);
        }
        #[cfg(feature = "ui-capture")]
        if std::env::var("TONAL_CAPTURE_PAGE").as_deref() == Ok("playlist-add")
            && self.state.queue.current().is_none()
            && !self.state.suggestions.tracks.is_empty()
        {
            let mut queue = playback_queue::PlaybackQueue::selected(
                self.state.suggestions.tracks.clone(),
                0,
                playback_queue::Repeat::Off,
                false,
            )
            .unwrap();
            queue.mark_complete();
            self.state.now_playing = queue.current().cloned();
            self.state.queue = queue;
        }
        if self
            .state
            .audio
            .as_ref()
            .is_some_and(|audio| !audio.is_empty() && !audio.is_paused())
        {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        #[cfg(feature = "ui-capture")]
        self.prepare_radio_capture(ctx);
        self.player(root);
        self.header(root);
        #[cfg(feature = "ui-capture")]
        self.prepare_motion_capture(ctx);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BLACK).inner_margin(egui::Margin { left: 32, right: 12, top: 22, bottom: 22 })).show(root, |ui| {
            let scroll = egui::ScrollArea::vertical().id_salt("content").auto_shrink([false, false]);
            #[cfg(feature = "ui-capture")]
            let scroll = if std::env::var_os("TONAL_CAPTURE_SCROLL_END").is_some() { scroll.vertical_scroll_offset((self.state.tracks.len() as f32 * 76.0 + 300.0 - (ctx.content_rect().height() - 170.0)).max(0.0)) } else { scroll };
            #[cfg(feature = "ui-capture")]
            let scroll = if let Some(offset) = std::env::var("TONAL_CAPTURE_SCROLL_Y").ok().and_then(|value| value.parse::<f32>().ok()) { scroll.vertical_scroll_offset(offset.max(0.0)) } else { scroll };
            let scrolling = scroll.show(ui, |ui| {
                // Keep the content centered as before, with a separate 20-point
                // gutter for the scrollbar on its right.
                let content_width = (ui.available_width() - 20.0).max(0.0);
                let width = content_width.min(1040.0);
                let inset = (content_width - width) / 2.0;
                ui.horizontal(|ui| {
                    ui.add_space(inset);
                    ui.vertical(|ui| {
                        ui.set_width(width);


                        self.library(ui, ctx);
                        if !self.state.google_status.is_empty() && self.state.google_status != "Connexion Google expérimentale · non validée avec un compte réel" {
                            ui.add_space(16.0); ui.label(RichText::new(&self.state.google_status).size(12.0).color(MUTED));
                        }
                        if self.state.status != "Prêt" { ui.add_space(10.0); ui.label(RichText::new(&self.state.status).size(12.0).color(MUTED)); }
                        if let Some(error) = &self.state.audio_error { ui.add_space(10.0); ui.label(RichText::new(error).size(12.0).color(RED)); }
                        ui.add_space(16.0);
                    });
                });
            });
            let content = ui.max_rect();
            let offset = scrolling.state.clamped_offset().y;
            let above = (offset / 24.0).clamp(0.0, 1.0);
            let below = ((scrolling.content_size.y - scrolling.inner_rect.height() - offset) / 24.0).clamp(0.0, 1.0);
            ui::content_edge_fades(ui.painter(), Rect::from_min_max(content.min, pos2(content.right() - 20.0, content.bottom())), 96.0, above, below);
        });
        if self.settings_open {
            self.queue_open = false;
            self.settings(ctx);
        }
        #[cfg(feature = "ui-capture")]
        if let Ok(page) = std::env::var("TONAL_CAPTURE_PAGE") {
            if matches!(
                page.as_str(),
                "playlist-create" | "playlist-edit" | "playlist-delete"
            ) && !self.state.google_busy()
                && !ctx
                    .data(|data| {
                        data.get_temp::<bool>(egui::Id::unique("qa_playlist_manager_opened"))
                    })
                    .unwrap_or(false)
            {
                self.state.playlist_management.qa_dialog(&page);
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::unique("qa_playlist_manager_opened"), true)
                });
            }
        }
        if self.state.playlist_management.is_open() {
            self.queue_open = false;
            self.add_menu_open = false;
            playlist_management::show(ctx, &mut self.state, &mut self.backdrop);
        }
        #[cfg(feature = "ui-capture")]
        if std::env::var("TONAL_CAPTURE_PAGE").as_deref() == Ok("queue")
            && !ctx
                .data(|data| data.get_temp::<bool>(egui::Id::unique("qa_queue_opened")))
                .unwrap_or(false)
            && self.state.queue.current().is_some()
        {
            self.queue_open = true;
            ctx.data_mut(|data| data.insert_temp(egui::Id::unique("qa_queue_opened"), true));
        }
        if self.queue_open {
            let paused = self
                .state
                .audio
                .as_ref()
                .is_none_or(|audio| audio.is_paused() || audio.is_empty());
            let output = queue_panel::show(
                ctx,
                &self.state.queue,
                &mut self.artwork,
                &mut self.queue_dragging,
                paused,
                self.state.queue_busy(),
            );
            if output.close {
                self.queue_open = false;
                self.queue_dragging = None;
            }
            if let Some(action) = output.action {
                self.state.edit_queue(ctx, action);
            }
        }
        self.playlist_notification(ctx);
        window_chrome::resize_borders(ctx);
        #[cfg(feature = "ui-capture")]
        self.capture_frame(ctx);
    }
}

#[cfg(feature = "ui-capture")]
impl TonalApp {
    fn prepare_radio_capture(&mut self, ctx: &egui::Context) {
        if std::env::var_os("TONAL_CAPTURE_TO").is_none() {
            return;
        }
        match std::env::var("TONAL_CAPTURE_PAGE").as_deref() {
            Ok("radios") => {
                self.state.selected_playlist = None;
                self.suggestions_open = false;
                self.radios_open = false;
            }
            Ok("radio-folder") => {
                self.state.selected_playlist = None;
                self.suggestions_open = false;
                self.radios_open = true;
            }
            Ok("radio-player") => {
                self.state.selected_playlist = None;
                self.suggestions_open = false;
                self.radios_open = true;
                self.state.qa_radio_paused(crate::radio::Station::Nrj);
            }
            Ok("radio-live") => {
                self.radios_open = true;
                self.suggestions_open = false;
                if !self.capture_settings_action_done {
                    self.state.audio.as_mut().unwrap().set_volume(0.0);
                    self.state.play_radio(ctx, crate::radio::Station::Nrj);
                    self.capture_settings_action_done = true;
                }
            }
            _ => {}
        }
    }

    fn prepare_motion_capture(&mut self, ctx: &egui::Context) {
        let Ok(page) = std::env::var("TONAL_CAPTURE_PAGE") else {
            return;
        };
        if !matches!(page.as_str(), "playlist-motion" | "suggestion-motion")
            || std::env::var_os("TONAL_CAPTURE_TO").is_none()
            || !self.state.connected()
            || self.state.google_busy()
        {
            return;
        }
        let id = egui::Id::unique("qa_motion_stage");
        let stage = ctx.data(|data| data.get_temp::<u8>(id)).unwrap_or(0);
        let track = |id: &str, title: &str| provider::Track {
            video_id: id.into(),
            title: title.into(),
            artist: "Artiste de démonstration".into(),
            duration_secs: Some(205),
            ..Default::default()
        };
        if stage == 0 {
            let tracks = vec![
                track("a", "Premier titre"),
                track("b", "Deuxième titre"),
                track("c", "Troisième titre"),
                track("d", "Quatrième titre"),
            ];
            if page == "playlist-motion" {
                self.state.selected_playlist = Some(provider::Playlist {
                    id: "PLtonal_motion_preview".into(),
                    title: "Mes découvertes".into(),
                    track_count: Some(4),
                    ..Default::default()
                });
                self.state.tracks = tracks;
                self.state.tracks_continuation = None;
            } else {
                self.suggestions_open = true;
                self.state.suggestions.tracks = tracks;
                self.state.suggestions.seed = Some(track("seed", "Titre de départ"));
            }
            ctx.data_mut(|data| data.insert_temp(id, 1_u8));
        }
        let wait = std::env::var("TONAL_CAPTURE_WAIT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(3000);
        let lead = if std::env::var_os("TONAL_CAPTURE_MOTION_SETTLED").is_some() {
            650
        } else {
            130
        };
        let change_at = Duration::from_millis(wait.saturating_sub(lead));
        if stage == 1 && self.capture_started.elapsed() >= change_at {
            if page == "playlist-motion" {
                self.state
                    .tracks
                    .insert(1, track("new_a", "Nouveau titre ajouté"));
                self.state
                    .tracks
                    .insert(3, track("new_b", "Une autre découverte"));
                self.state.selected_playlist.as_mut().unwrap().track_count = Some(6);
            } else {
                self.state.suggestions.tracks = vec![
                    track("new_a", "Une nouvelle suggestion"),
                    track("b", "Deuxième titre"),
                    track("new_b", "Une autre découverte"),
                    track("new_c", "À écouter ensuite"),
                ];
            }
            ctx.data_mut(|data| data.insert_temp(id, 2_u8));
        }
        if stage < 2 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn with_capture_audio(mut self, ctx: &egui::Context) -> Self {
        if std::env::var_os("TONAL_CAPTURE_SHOW_QUALITY").is_some() {
            self.preferences.show_quality = true;
        }
        if let Some(scale) = capture_scale() {
            ctx.set_zoom_factor(scale);
        }
        if let Some(path) = std::env::var_os("TONAL_CAPTURE_AUDIO") {
            let player = self.state.audio.as_mut().expect("Preview audio output");
            player.set_volume(0.0);
            player
                .load(std::path::Path::new(&path))
                .expect("Generated preview WAV");
            player.title = std::env::var("TONAL_CAPTURE_TITLE")
                .unwrap_or_else(|_| "Titre de démonstration".into());
            player.artist = std::env::var("TONAL_CAPTURE_ARTIST")
                .unwrap_or_else(|_| "Artiste de démonstration".into());
            if let Ok(rate) = std::env::var("TONAL_CAPTURE_BITRATE")
                .unwrap_or_default()
                .parse::<f64>()
            {
                player.quality = Some(youtube::AudioQuality {
                    format_id: "capture".into(),
                    codec: "mp4a.40.2".into(),
                    bitrate_kbps: Some(rate),
                });
            }
            player.preview_position(Duration::from_secs(8));
            if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("shuffle")
                || std::env::var_os("TONAL_CAPTURE_PAUSED").is_some()
            {
                player.toggle_pause();
            }
        }
        self
    }

    fn capture_frame(&mut self, ctx: &egui::Context) {
        let Ok(path) = std::env::var("TONAL_CAPTURE_TO") else {
            return;
        };
        if playlist_add::qa_mode().is_some() {
            let at = ctx.data(|data| {
                data.get_temp::<std::time::Instant>(egui::Id::unique("qa_optimistic_at"))
            });
            if at.is_some_and(|at| at.elapsed() < Duration::from_millis(700))
                || (at.is_none() && self.capture_started.elapsed() < Duration::from_secs(30))
            {
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            }
        }
        if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("shuffle")
            && self.capture_started.elapsed() < Duration::from_secs(30)
            && (!self.capture_settings_action_done || self.state.shuffle_busy())
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        let capture = self.capture_image.lock().unwrap().take();
        if let Some(image) = capture {
            if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("radio-open") {
                assert!(self.capture_settings_action_done && self.radios_open);
                assert!(self.state.selected_playlist.is_none());
                eprintln!("Navigation Radios : clic sur la carte locale, ouverture de sa liste sans playlist Google.");
            }
            if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("volume-leave") {
                assert!(self.capture_settings_action_done && self.capture_queue_stage == 4);
                assert!(
                    ctx.data(|data| data.get_temp::<bool>(egui::Id::unique("qa_volume_had_focus")))
                        .unwrap_or(false),
                    "The slider must actually receive focus before the pointer leaves"
                );
                assert!(
                    self.state.playlist_add.visible(),
                    "The add button must remain visible in this compact layout"
                );
                assert!(
                    ctx.data(|data| data.get_temp::<f32>(egui::Id::unique("qa_volume_reveal")))
                        .unwrap()
                        < 0.01
                );
                assert!(
                    ctx.data(|data| data.get_temp::<f32>(egui::Id::unique("qa_duration_lift")))
                        .unwrap()
                        .abs()
                        < 0.1
                );
                eprintln!("Compact volume: hover, click, release and leave verified; slider hidden and duration back on baseline with add button visible.");
            }
            if let Some(fail) = playlist_add::qa_mode() {
                let (track_id, playlist_id, before) = ctx
                    .data(|data| {
                        data.get_temp::<(String, String, Option<u32>)>(egui::Id::unique(
                            "qa_optimistic_intent",
                        ))
                    })
                    .expect("An optimistic add must have started");
                let count = self
                    .state
                    .playlists
                    .iter()
                    .find(|playlist| playlist.id == playlist_id)
                    .and_then(|playlist| playlist.track_count);
                assert!(!self.add_menu_open && !self.state.import_busy);
                assert_eq!(
                    self.state
                        .suggestions
                        .tracks
                        .iter()
                        .any(|track| track.video_id == track_id),
                    fail
                );
                assert_eq!(
                    count,
                    if fail {
                        before
                    } else {
                        before.map(|count| count + 1)
                    }
                );
                assert_eq!(
                    self.state.playlist_add.notice.as_ref().unwrap().success,
                    !fail
                );
                eprintln!("Optimistic add QA: immediate close, count, suggestion exclusion, notification and rollback={fail} verified; simulated write only.");
            }
            if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("playlist-add-open") {
                assert!(self.add_menu_open && self.state.playlist_add.visible());
                assert!(self
                    .state
                    .playlist_add
                    .choices
                    .as_ref()
                    .is_some_and(|choices| !choices.is_empty()));
                assert!(!self.state.playlist_add.busy() && !self.state.import_busy);
                eprintln!("Playlist picker: real button click, destinations visible, no write or audio preparation.");
            }
            if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("queue-move") {
                assert_eq!(
                    self.state.queue.upcoming().map(|track| &track.video_id),
                    self.state.tracks.get(3).map(|track| &track.video_id)
                );
                eprintln!("Queue drag passed; current song preserved, upcoming order changed.");
            }
            if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("queue-close") {
                assert!(!self.queue_open);
            }
            if std::env::var("TONAL_CAPTURE_ACTION").as_deref() == Ok("tray") {
                self.desktop.qa_check_tray(ctx);
            }
            if std::env::var_os("TONAL_CAPTURE_RESTORE_CHECK").is_some() {
                assert!(self.state.now_playing.is_some() && !self.state.import_busy);
                assert!(self
                    .state
                    .audio
                    .as_ref()
                    .is_some_and(|audio| audio.is_paused() && audio.is_empty()));
                assert!(self.state.position() > Duration::ZERO);
                eprintln!(
                    "Restart restored paused at {:?}, without preparing audio.",
                    self.state.position()
                );
            }
            if std::env::var_os("TONAL_CAPTURE_TRACK_HOVER").is_some() {
                assert!(
                    ctx.data(|data| data.get_temp::<bool>(egui::Id::unique("qa_track_hovered")))
                        .unwrap_or(false),
                    "The track row must stay hovered over its title"
                );
                assert_eq!(
                    ctx.output(|output| output.cursor_icon),
                    egui::CursorIcon::PointingHand
                );
            }
            eprintln!(
                "UI capture: tracks={}, continuation={}, pending={}, selected={}, audio_loaded={}, settings={}, show_quality={}, shuffle={}, complete={}",
                self.state.tracks.len(),
                self.state.tracks_continuation.is_some(),
                self.state.import_busy,
                self.state.now_playing.is_some(),
                self.state
                    .audio
                    .as_ref()
                    .is_some_and(|audio| !audio.title.is_empty()),
                self.settings_open,
                self.preferences.show_quality,
                self.state.queue.shuffle,
                self.state.queue.is_complete()
            );
            let pixels: Vec<_> = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            image::save_buffer(
                path,
                &pixels,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .expect("Save native UI capture");
            self.desktop.quit(ctx);
        } else if !self.capture_requested
            && self.capture_started.elapsed()
                > Duration::from_millis(
                    std::env::var("TONAL_CAPTURE_WAIT_MS")
                        .ok()
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(500)
                        .clamp(500, 30_000),
                )
        {
            self.capture_requested = true;
            let output = self.capture_image.clone();
            let ctx = ctx.clone();
            let repaint = ctx.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                egui::ScreenshotCallback::new(move |image| {
                    *output.lock().unwrap() = Some(image);
                    repaint.request_repaint();
                }),
            ));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
fn main() -> eframe::Result {
    let size = [1180.0, 820.0];
    #[cfg(feature = "ui-capture")]
    let size = if std::env::var_os("TONAL_CAPTURE_COMPACT").is_some() {
        [840.0, 640.0]
    } else {
        size
    };
    #[cfg(feature = "ui-capture")]
    let size = if let Some(scale) = capture_scale() {
        [size[0] * scale, size[1] * scale]
    } else {
        size
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Tonal — Music")
            .with_decorations(false)
            .with_icon(window_chrome::app_icon())
            .with_inner_size(size)
            .with_min_inner_size([840.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Tonal",
        options,
        Box::new(|cc| Ok(Box::new(TonalApp::new(cc)))),
    )
}

#[cfg(feature = "ui-capture")]
fn capture_scale() -> Option<f32> {
    std::env::var("TONAL_CAPTURE_SCALE")
        .ok()?
        .parse::<f32>()
        .ok()
        .filter(|scale| (1.0..=2.0).contains(scale))
}
