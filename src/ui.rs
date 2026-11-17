use eframe::egui::{
    self, pos2, vec2, Color32, FontId, Painter, Rect, Response, Sense, Stroke, StrokeKind,
};

pub const BLACK: Color32 = Color32::BLACK;
pub const WHITE: Color32 = Color32::from_rgb(250, 250, 250);
pub const MUTED: Color32 = Color32::from_rgb(161, 161, 170);
pub const SURFACE: Color32 = Color32::from_rgb(20, 20, 20);
pub const LINE: Color32 = Color32::from_rgb(34, 34, 34);
pub const RED: Color32 = Color32::from_rgb(255, 68, 68);

pub fn heading(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(size, egui::FontFamily::Name("display".into())))
}

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    Volume,
    Muted,
    Arrow,
    Back,
    Search,
    User,
    Logout,
    Settings,
    Refresh,
    Music,
    Close,
    Previous,
    Next,
    Minimize,
    Maximize,
    Restore,
    ChevronDown,
    Repeat,
    RepeatOne,
    Shuffle,
    Queue,
    PlaylistAdd,
    Check,
    Plus,
    Pencil,
    Trash,
}

pub fn icon(painter: &Painter, symbol: Icon, rect: Rect, color: Color32) {
    let crisp = matches!(symbol, Icon::Plus | Icon::Pencil | Icon::Trash);
    let ppp = painter.ctx().pixels_per_point();
    let rect = if crisp {
        let side = (rect.width().min(rect.height()) * ppp).round().max(1.0);
        Rect::from_min_size(
            pos2(
                (rect.center().x * ppp - side * 0.5).round() / ppp,
                (rect.center().y * ppp - side * 0.5).round() / ppp,
            ),
            vec2(side / ppp, side / ppp),
        )
    } else {
        rect
    };
    let (name, bytes): (&str, &[u8]) = match symbol {
        Icon::Plus => ("plus", include_bytes!("../assets/icons/plus.svg")),
        Icon::Pencil => ("pencil", include_bytes!("../assets/icons/pencil.svg")),
        Icon::Trash => ("trash", include_bytes!("../assets/icons/trash-2.svg")),
        Icon::Play => ("play", include_bytes!("../assets/icons/play.svg")),
        Icon::Pause => ("pause", include_bytes!("../assets/icons/pause.svg")),
        Icon::Stop | Icon::Maximize => ("square", include_bytes!("../assets/icons/square.svg")),
        Icon::Volume => ("volume", include_bytes!("../assets/icons/volume-2.svg")),
        Icon::Muted => ("muted", include_bytes!("../assets/icons/volume-x.svg")),
        Icon::Arrow => ("arrow", include_bytes!("../assets/icons/arrow-right.svg")),
        Icon::Back => ("back", include_bytes!("../assets/icons/chevron-left.svg")),
        Icon::Search => ("search", include_bytes!("../assets/icons/search.svg")),
        Icon::User => ("user", include_bytes!("../assets/icons/user-round.svg")),
        Icon::Logout => ("logout", include_bytes!("../assets/icons/log-out.svg")),
        Icon::Settings => ("settings", include_bytes!("../assets/icons/settings.svg")),
        Icon::Refresh => ("refresh", include_bytes!("../assets/icons/rotate-cw.svg")),
        Icon::Music => ("music", include_bytes!("../assets/icons/audio-lines.svg")),
        Icon::Close => ("close", include_bytes!("../assets/icons/x.svg")),
        Icon::Previous => ("previous", include_bytes!("../assets/icons/skip-back.svg")),
        Icon::Next => ("next", include_bytes!("../assets/icons/skip-forward.svg")),
        Icon::Minimize => ("minimize", include_bytes!("../assets/icons/minus.svg")),
        Icon::Restore => ("restore", include_bytes!("../assets/icons/copy.svg")),
        Icon::ChevronDown => (
            "chevron",
            include_bytes!("../assets/icons/chevron-down.svg"),
        ),
        Icon::Repeat => ("repeat", include_bytes!("../assets/icons/repeat.svg")),
        Icon::RepeatOne => ("repeat-one", include_bytes!("../assets/icons/repeat-1.svg")),
        Icon::Shuffle => ("shuffle", include_bytes!("../assets/icons/shuffle.svg")),
        Icon::Queue => ("queue", include_bytes!("../assets/icons/list-music.svg")),
        Icon::PlaylistAdd => (
            "playlist-add",
            include_bytes!("../assets/icons/playlist-add.svg"),
        ),
        Icon::Check => ("check", include_bytes!("../assets/icons/check.svg")),
    };
    // Rasterize at twice the actual device-pixel size, then use a mip level for
    // antialiasing; a fixed large image sampled without mipmaps aliases small strokes.
    let height = (rect.height() * painter.ctx().pixels_per_point())
        .round()
        .clamp(1.0, 256.0) as u32
        * if crisp { 1 } else { 2 };
    let id = egui::Id::unique(("lucide", name, height));
    let texture = painter
        .ctx()
        .data_mut(|data| data.get_temp::<egui::TextureHandle>(id))
        .unwrap_or_else(|| {
            let image = egui_extras::image::load_svg_bytes_with_size(
                bytes,
                egui::load::SizeHint::Height(height),
                &Default::default(),
            )
            .expect("Bundled Lucide SVG");
            let texture = painter.ctx().load_texture(
                name,
                image,
                egui::TextureOptions {
                    mipmap_mode: if crisp {
                        None
                    } else {
                        Some(egui::TextureFilter::Linear)
                    },
                    ..egui::TextureOptions::LINEAR
                },
            );
            painter
                .ctx()
                .data_mut(|data| data.insert_temp(id, texture.clone()));
            texture
        });
    painter.image(
        texture.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        color,
    );
}
#[derive(Clone, Copy)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Quiet,
    Danger,
}

pub fn setting_toggle(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    value: &mut bool,
) -> egui::Response {
    ui.push_id(label, |ui| {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
        ui.painter().text(
            row.left_top(),
            egui::Align2::LEFT_TOP,
            label,
            FontId::proportional(14.0),
            WHITE,
        );
        ui.painter().text(
            row.left_top() + vec2(0.0, 23.0),
            egui::Align2::LEFT_TOP,
            description,
            FontId::proportional(12.0),
            MUTED,
        );
        let mut button = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
            pos2(row.right() - 44.0, row.top() + 4.0),
            vec2(44.0, 26.0),
        )));
        toggle(&mut button, value, label)
    })
    .inner
}

pub fn transport_button(
    ui: &mut egui::Ui,
    symbol: Icon,
    label: &str,
    size: f32,
    glyph_size: f32,
    enabled: bool,
    active: bool,
) -> Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
        let color = if !ui.is_enabled() {
            Color32::from_gray(65)
        } else if response.is_pointer_button_down_on() {
            MUTED
        } else if active || response.hovered() {
            WHITE
        } else {
            Color32::from_gray(166)
        };
        icon(
            ui.painter(),
            symbol,
            Rect::from_center_size(rect.center(), vec2(glyph_size, glyph_size)),
            color,
        );
        if active && matches!(symbol, Icon::Repeat | Icon::RepeatOne | Icon::Shuffle) {
            ui.painter()
                .circle_filled(pos2(rect.center().x, rect.bottom() + 3.0), 2.0, RED);
        }
        focus_ring(ui, rect.shrink(4.0), response.has_focus());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(label)
    })
    .inner
}

pub fn volume_slider(ui: &mut egui::Ui, value: &mut f32, width: f32) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click_and_drag());
    let left = rect.left() + 5.0;
    let right = rect.right() - 5.0;
    let before = *value;
    if let Some(pointer) = response
        .interact_pointer_pos()
        .filter(|_| response.clicked() || response.dragged())
    {
        *value = ((pointer.x - left) / (right - left).max(1.0)).clamp(0.0, 1.0);
        response.request_focus();
    }
    if response.has_focus() {
        ui.input(|input| {
            let step = if input.modifiers.shift { 0.05 } else { 0.01 };
            if input.key_pressed(egui::Key::ArrowRight) || input.key_pressed(egui::Key::ArrowUp) {
                *value += step;
            }
            if input.key_pressed(egui::Key::ArrowLeft) || input.key_pressed(egui::Key::ArrowDown) {
                *value -= step;
            }
            if input.key_pressed(egui::Key::Home) {
                *value = 0.0;
            }
            if input.key_pressed(egui::Key::End) {
                *value = 1.0;
            }
        });
        *value = value.clamp(0.0, 1.0);
    }
    if *value != before {
        response.mark_changed();
    }
    let y = rect.center().y;
    let thumb = left + (right - left) * *value;
    ui.painter().line_segment(
        [pos2(left, y), pos2(right, y)],
        Stroke::new(2.0_f32, Color32::from_gray(74)),
    );
    ui.painter()
        .line_segment([pos2(left, y), pos2(thumb, y)], Stroke::new(2.0_f32, WHITE));
    ui.painter().circle_filled(pos2(thumb, y), 4.5, WHITE);
    response.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), *value as f64, "Volume"));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Volume : {:.0} %", *value * 100.0))
}

pub fn button(
    ui: &mut egui::Ui,
    label: &str,
    symbol: Icon,
    kind: ButtonKind,
    enabled: bool,
) -> Response {
    ui.add_enabled_ui(enabled, |ui| {
        let galley = ui
            .painter()
            .layout_no_wrap(label.into(), FontId::proportional(14.0), WHITE);
        let (rect, response) =
            ui.allocate_exact_size(vec2(galley.size().x + 56.0, 40.0), Sense::click());
        let pressed = response.is_pointer_button_down_on();
        let hovered = response.hovered();
        let (fill, ink, border) = if !ui.is_enabled() {
            (
                Color32::from_gray(28),
                Color32::from_gray(92),
                Stroke::new(1.0_f32, LINE),
            )
        } else {
            match kind {
                ButtonKind::Danger => (
                    Color32::from_rgb(
                        if pressed {
                            184
                        } else if hovered {
                            224
                        } else {
                            207
                        },
                        if pressed { 35 } else { 44 },
                        if pressed { 45 } else { 55 },
                    ),
                    WHITE,
                    Stroke::NONE,
                ),
                ButtonKind::Primary => (
                    Color32::from_gray(if pressed {
                        202
                    } else if hovered {
                        255
                    } else {
                        240
                    }),
                    BLACK,
                    Stroke::NONE,
                ),
                ButtonKind::Secondary => (
                    Color32::from_gray(if pressed {
                        36
                    } else if hovered {
                        28
                    } else {
                        18
                    }),
                    WHITE,
                    Stroke::new(1.0_f32, Color32::from_gray(if hovered { 72 } else { 46 })),
                ),
                ButtonKind::Quiet => (
                    if pressed {
                        Color32::from_gray(34)
                    } else if hovered {
                        Color32::from_gray(26)
                    } else {
                        Color32::TRANSPARENT
                    },
                    if hovered { WHITE } else { MUTED },
                    Stroke::NONE,
                ),
            }
        };
        ui.painter()
            .rect(rect, 10, fill, border, StrokeKind::Inside);
        let text_y = rect.center().y - galley.mesh_bounds.center().y;
        ui.painter()
            .galley_with_override_text_color(pos2(rect.left() + 16.0, text_y), galley, ink);
        icon(
            ui.painter(),
            symbol,
            Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), vec2(17.0, 17.0)),
            ink,
        );
        focus_ring(ui, rect, response.has_focus());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    })
    .inner
}

fn focus_ring(ui: &egui::Ui, rect: Rect, focused: bool) {
    if focused {
        ui.painter().rect_stroke(
            rect.expand(3.0),
            12,
            Stroke::new(1.5_f32, Color32::from_gray(150)),
            StrokeKind::Outside,
        );
    }
}

pub fn account_button(
    ui: &mut egui::Ui,
    connected: bool,
    name: Option<&str>,
    avatar: Option<&egui::TextureHandle>,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(144.0, 40.0), Sense::click());
    let hovered = response.hovered();
    let fill = Color32::from_gray(if response.is_pointer_button_down_on() {
        34
    } else if hovered {
        26
    } else {
        16
    });
    ui.painter().rect(
        rect,
        10,
        fill,
        Stroke::new(1.0_f32, Color32::from_gray(if hovered { 72 } else { 44 })),
        StrokeKind::Inside,
    );
    if let Some(avatar) = avatar {
        crate::artwork::paint_texture(
            ui,
            Rect::from_center_size(pos2(rect.left() + 23.0, rect.center().y), vec2(24.0, 24.0)),
            avatar,
            12,
        );
    } else {
        icon(
            ui.painter(),
            Icon::User,
            Rect::from_center_size(pos2(rect.left() + 23.0, rect.center().y), vec2(17.0, 17.0)),
            MUTED,
        );
    }
    let label = if connected {
        name.unwrap_or("Compte")
    } else {
        "Connexion"
    };
    let label_painter = ui.painter().with_clip_rect(Rect::from_min_max(
        pos2(rect.left() + 42.0, rect.top()),
        pos2(rect.right() - 31.0, rect.bottom()),
    ));
    label_painter.text(
        pos2(rect.left() + 42.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        WHITE,
    );
    icon(
        ui.painter(),
        Icon::ChevronDown,
        Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(13.0, 13.0)),
        MUTED,
    );
    focus_ring(ui, rect, response.has_focus());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, "Ouvrir le menu du compte")
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn seek_bar(ui: &mut egui::Ui, rail: Rect, value: &mut f32, enabled: bool) -> Response {
    let hit = Rect::from_min_max(
        pos2(rail.left(), rail.top() - 4.0),
        pos2(rail.right(), rail.bottom() + 6.0),
    );
    let mut response = ui.interact(
        hit,
        ui.make_persistent_id("playback-progress"),
        if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let before = *value;
    if enabled {
        if let Some(pointer) = response
            .interact_pointer_pos()
            .filter(|_| response.clicked() || response.dragged() || response.drag_stopped())
        {
            *value = ((pointer.x - rail.left()) / rail.width()).clamp(0.0, 1.0);
        }
        if response.has_focus() {
            ui.input(|input| {
                if input.key_pressed(egui::Key::ArrowRight) {
                    *value += 0.01;
                }
                if input.key_pressed(egui::Key::ArrowLeft) {
                    *value -= 0.01;
                }
                if input.key_pressed(egui::Key::Home) {
                    *value = 0.0;
                }
                if input.key_pressed(egui::Key::End) {
                    *value = 1.0;
                }
            });
            *value = value.clamp(0.0, 1.0);
        }
    }
    if *value != before {
        response.mark_changed();
    }
    ui.painter().rect_filled(rail, 0, LINE);
    let x = rail.left() + rail.width() * *value;
    ui.painter()
        .rect_filled(Rect::from_min_max(rail.min, pos2(x, rail.bottom())), 0, RED);
    if enabled && (response.hovered() || response.dragged() || response.has_focus()) {
        ui.painter().circle_filled(
            pos2(
                x.clamp(rail.left() + 4.5, rail.right() - 4.5),
                rail.center().y,
            ),
            4.5,
            RED,
        );
    }
    response
        .widget_info(|| egui::WidgetInfo::slider(enabled, *value as f64, "Position de lecture"));
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

pub fn live_indicator(ui: &egui::Ui, center: egui::Pos2, playing: bool) {
    let mut radius = 3.5;
    if playing {
        let phase = ui.ctx().input(|input| (input.time / 1.6).fract()) as f32;
        let breath = 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos();
        radius = 3.5 + 1.2 * breath;
        ui.painter().circle_stroke(
            center,
            5.0 + phase * 7.0,
            egui::Stroke::new(
                1.4,
                Color32::from_rgba_unmultiplied(255, 68, 68, (190.0 * (1.0 - phase)) as u8),
            ),
        );
        if !ui
            .ctx()
            .input(|input| input.viewport().minimized.unwrap_or(false))
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(66));
        }
    }
    ui.painter().circle_filled(
        center,
        radius,
        if playing {
            RED
        } else {
            RED.gamma_multiply(0.55)
        },
    );
}

pub fn popup_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_gray(18))
        .corner_radius(14)
        .inner_margin(8)
        .stroke(Stroke::new(1.0_f32, Color32::from_gray(46)))
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(180),
        })
}

pub fn menu_item(ui: &mut egui::Ui, label: &str, symbol: Icon, destructive: bool) -> Response {
    let width = ui.available_width().max(220.0);
    menu_item_width(ui, label, symbol, destructive, width)
}
pub fn menu_item_width(
    ui: &mut egui::Ui,
    label: &str,
    symbol: Icon,
    destructive: bool,
    width: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 36.0), Sense::click());
    let active = response.hovered() || response.has_focus();
    if active {
        ui.painter().rect_filled(
            rect,
            7,
            if destructive {
                Color32::from_rgb(49, 22, 26)
            } else {
                Color32::from_gray(31)
            },
        );
    }
    let color = if destructive { RED } else { WHITE };
    icon(
        ui.painter(),
        symbol,
        Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), vec2(18.0, 18.0)),
        color,
    );
    ui.painter().text(
        pos2(rect.left() + 40.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        color,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
pub fn icon_button(
    ui: &mut egui::Ui,
    symbol: Icon,
    label: &str,
    size: f32,
    primary: bool,
    enabled: bool,
) -> Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
        let fill = if primary {
            if ui.is_enabled() {
                WHITE
            } else {
                Color32::from_gray(40)
            }
        } else if response.hovered() {
            Color32::from_gray(28)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().circle_filled(rect.center(), size / 2.0, fill);
        let ink = if !ui.is_enabled() {
            Color32::from_gray(65)
        } else if primary {
            BLACK
        } else {
            MUTED
        };
        let symbol_size = if primary {
            (size * 0.48).clamp(20.0, 30.0)
        } else {
            (size * 0.5).clamp(18.0, 25.0)
        };
        icon(
            ui.painter(),
            symbol,
            Rect::from_center_size(rect.center(), vec2(symbol_size, symbol_size)),
            ink,
        );
        if response.has_focus() {
            ui.painter()
                .circle_stroke(rect.center(), size / 2.0 + 2.0, Stroke::new(1.0_f32, RED));
        }
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(label)
            .on_disabled_hover_text(label)
    })
    .inner
}

pub fn cover(painter: &Painter, rect: Rect, variant: usize) {
    let tones = [
        Color32::from_rgb(35, 20, 24),
        Color32::from_rgb(22, 26, 34),
        Color32::from_rgb(28, 28, 22),
        Color32::from_rgb(28, 22, 32),
    ];
    painter.rect_filled(rect, 8, tones[variant % tones.len()]);
    let center = rect.center();
    for fraction in [0.38, 0.32, 0.25] {
        painter.circle_stroke(
            center,
            rect.width() * fraction,
            Stroke::new(1.0_f32, Color32::from_white_alpha(16)),
        );
    }
    icon(
        painter,
        Icon::Music,
        Rect::from_center_size(center, rect.size() * 0.23),
        Color32::from_gray(130),
    );
}

pub fn content_edge_fades(painter: &Painter, content: Rect, height: f32, top: f32, bottom: f32) {
    for (is_top, amount) in [(true, top), (false, bottom)] {
        if amount <= 0.0 {
            continue;
        }
        let edge = if is_top {
            Rect::from_min_max(
                content.min,
                pos2(
                    content.right(),
                    (content.top() + height).min(content.bottom()),
                ),
            )
        } else {
            Rect::from_min_max(
                pos2(
                    content.left(),
                    (content.bottom() - height).max(content.top()),
                ),
                content.max,
            )
        };
        content_fade(painter, edge, is_top, amount);
    }
}

fn content_fade(painter: &Painter, edge: Rect, is_top: bool, amount: f32) {
    let mut mesh = egui::epaint::Mesh::default();
    let steps = 24_u32;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let y = edge.top() + edge.height() * t;
        let opacity = if is_top { 1.0 - t } else { t };
        let alpha = (255.0 * opacity * opacity * (3.0 - 2.0 * opacity) * amount).round() as u8;
        let color = Color32::from_black_alpha(alpha);
        mesh.colored_vertex(pos2(edge.left(), y), color);
        mesh.colored_vertex(pos2(edge.right(), y), color);
        if step > 0 {
            let index = step * 2;
            mesh.add_triangle(index - 2, index - 1, index);
            mesh.add_triangle(index - 1, index + 1, index);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

pub fn playlist_play_overlay(painter: &Painter, cover: Rect, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    let area = Rect::from_min_max(cover.max - cover.size() * 0.72, cover.max);
    let steps = 12_u32;
    let mut mesh = egui::epaint::Mesh::default();
    for row in 0..=steps {
        for column in 0..=steps {
            let x = column as f32 / steps as f32;
            let y = row as f32 / steps as f32;
            let alpha = ((x * y).powf(0.85) * 170.0 * amount) as u8;
            mesh.colored_vertex(
                area.min + vec2(x * area.width(), y * area.height()),
                Color32::from_black_alpha(alpha),
            );
        }
    }
    for row in 0..steps {
        for column in 0..steps {
            let a = row * (steps + 1) + column;
            let b = a + steps + 1;
            mesh.add_triangle(a, a + 1, b);
            mesh.add_triangle(a + 1, b + 1, b);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    icon(
        painter,
        Icon::Play,
        Rect::from_center_size(cover.max - vec2(25.0, 25.0), vec2(30.0, 30.0)),
        Color32::from_white_alpha((255.0 * amount) as u8),
    );
}

pub fn configure(ctx: &egui::Context) -> crate::light_text::LightText {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BLACK;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = BLACK;
    visuals.faint_bg_color = SURFACE;
    visuals.override_text_color = Some(WHITE);
    visuals.selection.bg_fill = Color32::from_rgb(65, 25, 29);
    visuals.selection.stroke = Stroke::new(1.0_f32, WHITE);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
    ] {
        widget.bg_fill = SURFACE;
        widget.weak_bg_fill = SURFACE;
        widget.bg_stroke = Stroke::NONE;
        widget.corner_radius = 7.into();
    }
    visuals.widgets.hovered.bg_fill = Color32::from_gray(30);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_gray(30);
    visuals.widgets.active.bg_fill = Color32::from_gray(42);
    visuals.widgets.active.weak_bg_fill = Color32::from_gray(42);
    ctx.set_visuals(visuals);
    ctx.global_style_mut(|style| {
        style.interaction.selectable_labels = false;
        #[cfg(feature = "ui-capture")]
        {
            style.animation_time = 0.0;
        }
        style.spacing.item_spacing = vec2(10.0, 10.0);
        style.spacing.button_padding = vec2(16.0, 10.0);
        style.spacing.interact_size.y = 36.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(12.0));
    });
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/segoeui.ttf") {
        fonts
            .font_data
            .insert("segoe".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe".into());
    }
    let mut display = fonts.families[&egui::FontFamily::Proportional].clone();
    if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/seguisb.ttf") {
        fonts.font_data.insert(
            "segoe-semibold".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        display.insert(0, "segoe-semibold".into());
    }
    let mut bold = display.clone();
    if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/segoeuib.ttf") {
        fonts.font_data.insert(
            "segoe-bold".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        bold.insert(0, "segoe-bold".into());
    }
    fonts
        .families
        .insert(egui::FontFamily::Name("bold".into()), bold);
    fonts
        .families
        .insert(egui::FontFamily::Name("display".into()), display);
    let light_text = crate::light_text::LightText::new(fonts.clone());
    ctx.set_fonts(fonts);
    light_text
}

pub const QUALITY_BADGE_SIZE: f32 = 14.0;

pub fn quality_badge(
    ui: &mut egui::Ui,
    tier: crate::youtube::QualityTier,
    baseline_y: Option<f32>,
    light_text: &mut crate::light_text::LightText,
) -> Response {
    use crate::youtube::QualityTier;
    use egui::emath::GuiRounding;
    let (id, allocated) = ui.allocate_space(vec2(QUALITY_BADGE_SIZE, QUALITY_BADGE_SIZE));
    let rect = Rect::from_center_size(
        pos2(
            allocated.center().x,
            baseline_y
                .map(|baseline| baseline - QUALITY_BADGE_SIZE * 0.5)
                .unwrap_or(allocated.center().y),
        ),
        vec2(QUALITY_BADGE_SIZE, QUALITY_BADGE_SIZE),
    )
    .round_to_pixels(ui.painter().pixels_per_point());
    let response = ui.interact(rect, id, Sense::hover());
    let fill = match tier {
        QualityTier::Standard => WHITE,
        QualityTier::High => Color32::from_rgb(105, 221, 153),
        QualityTier::Master => Color32::from_rgb(231, 193, 104),
    };
    ui.painter().rect_filled(rect, 2, fill);
    let glyph = light_text.layout(
        ui.ctx(),
        tier.letter(),
        FontId::new(12.0, egui::FontFamily::Name("bold".into())),
        f32::INFINITY,
    );
    let position = (rect.center() - glyph.mesh_bounds.center().to_vec2())
        .round_to_pixels(ui.painter().pixels_per_point());
    light_text.paint(ui.painter(), position, &glyph);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Label,
            true,
            format!("{} · {}", tier.letter(), tier.name()),
        )
    });
    response
}
pub fn toggle(ui: &mut egui::Ui, value: &mut bool, label: &str) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(44.0, 26.0), Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let amount = ui.ctx().animate_bool_with_time(response.id, *value, 0.14);
    let off = Color32::from_gray(44);
    let on = Color32::from_gray(235);
    let fill =
        egui::Color32::from(egui::Rgba::from(off) * (1.0 - amount) + egui::Rgba::from(on) * amount);
    ui.painter().rect_filled(rect, 13, fill);
    ui.painter().circle_filled(
        pos2(rect.left() + 13.0 + 18.0 * amount, rect.center().y),
        9.0,
        if *value { BLACK } else { WHITE },
    );
    focus_ring(ui, rect.expand(3.0), response.has_focus());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::CheckBox, true, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod seek_tests {
    use super::*;
    fn frame(ctx: &egui::Context, value: &mut f32, events: Vec<egui::Event>) -> Response {
        let mut response = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 150.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    response = Some(seek_bar(
                        ui,
                        Rect::from_min_size(pos2(10.0, 50.0), vec2(100.0, 3.0)),
                        value,
                        true,
                    ));
                });
            },
        );
        // This headless input test intentionally has no renderer to upload textures.
        output.textures_delta.clear();
        response.unwrap()
    }
    fn pointer(x: f32, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: pos2(x, 51.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    #[test]
    fn click_and_drag_change_seek_position_and_clamp_to_the_track() {
        let ctx = egui::Context::default();
        let mut value = 0.2;
        frame(&ctx, &mut value, vec![]);
        frame(
            &ctx,
            &mut value,
            vec![
                egui::Event::PointerMoved(pos2(85.0, 51.0)),
                pointer(85.0, true),
            ],
        );
        assert!(frame(&ctx, &mut value, vec![pointer(85.0, false)]).clicked());
        assert!((value - 0.75).abs() < 0.001);
        frame(&ctx, &mut value, vec![pointer(85.0, true)]);
        assert!(frame(
            &ctx,
            &mut value,
            vec![egui::Event::PointerMoved(pos2(200.0, 51.0))]
        )
        .dragged());
        assert_eq!(value, 1.0);
        assert!(frame(&ctx, &mut value, vec![pointer(200.0, false)]).drag_stopped());
        assert_eq!(value, 1.0);
    }
}
