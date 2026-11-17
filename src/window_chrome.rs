//! Native window commands, with an egui title bar and resize borders.
use crate::ui::{self, Icon, BLACK, MUTED, WHITE};
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, Response, Sense, Stroke, StrokeKind};

/// Let the Windows 11 compositor round the real window, without a transparent surface.
/// Windows removes rounding itself when the window is maximized or snapped.
pub fn round_corners(cc: &eframe::CreationContext<'_>) -> Result<(), &'static str> {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        };
        let handle = cc
            .window_handle()
            .map_err(|_| "Window handle unavailable")?;
        let RawWindowHandle::Win32(window) = handle.as_raw() else {
            return Err("Not a Win32 window");
        };
        let preference = DWMWCP_ROUND;
        // SAFETY: the handle belongs to the live eframe window; the typed value remains
        // valid for this synchronous call. Unsupported Windows versions return an error.
        let result = unsafe {
            DwmSetWindowAttribute(
                window.hwnd.get() as _,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                (&preference as *const i32).cast(),
                std::mem::size_of_val(&preference) as u32,
            )
        };
        if result < 0 {
            Err("Native rounded corners unavailable")
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = cc;
        Ok(())
    }
}

pub fn app_icon() -> egui::IconData {
    let image = egui_extras::image::load_svg_bytes_with_size(
        include_bytes!("../assets/tonal.svg"),
        egui::load::SizeHint::Height(256),
        &Default::default(),
    )
    .expect("Valid application icon");
    egui::IconData {
        rgba: image
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_array())
            .collect(),
        width: image.size[0] as u32,
        height: image.size[1] as u32,
    }
}

pub fn caption_button(
    ui: &mut egui::Ui,
    rect: Rect,
    symbol: Icon,
    label: &str,
    close: bool,
) -> Response {
    let response = ui.interact(rect, ui.make_persistent_id(label), Sense::click());
    let fill = if response.hovered() || response.has_focus() {
        if close {
            Color32::from_rgb(210, 35, 50)
        } else {
            Color32::from_gray(30)
        }
    } else {
        BLACK
    };
    ui.painter().rect_filled(rect, 6, fill);
    ui::icon(
        ui.painter(),
        symbol,
        Rect::from_center_size(rect.center(), vec2(17.0, 17.0)),
        if response.hovered() { WHITE } else { MUTED },
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(2.0),
            4,
            Stroke::new(1.0_f32, MUTED),
            StrokeKind::Inside,
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response.on_hover_text(label)
}

pub fn title_drag(ui: &mut egui::Ui, rect: Rect, salt: &str) {
    if rect.width() < 2.0 {
        return;
    }
    let response = ui.interact(rect, ui.make_persistent_id(salt), Sense::click_and_drag());
    if response.double_clicked() {
        toggle_maximize(ui.ctx());
    } else if response.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

pub fn toggle_maximize(ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
}

pub fn resize_borders(ctx: &egui::Context) {
    use egui::{CursorIcon as Cursor, ResizeDirection as Edge};
    if ctx.input(|input| {
        input.viewport().maximized.unwrap_or(false) || input.viewport().fullscreen.unwrap_or(false)
    }) {
        return;
    }
    let rect = ctx.content_rect();
    let side = 5.0;
    let corner = 12.0;
    let edges = [
        (
            Edge::NorthWest,
            Rect::from_min_size(rect.min, vec2(corner, corner)),
            Cursor::ResizeNwSe,
        ),
        (
            Edge::NorthEast,
            Rect::from_min_size(
                pos2(rect.right() - corner, rect.top()),
                vec2(corner, corner),
            ),
            Cursor::ResizeNeSw,
        ),
        (
            Edge::SouthWest,
            Rect::from_min_size(
                pos2(rect.left(), rect.bottom() - corner),
                vec2(corner, corner),
            ),
            Cursor::ResizeNeSw,
        ),
        (
            Edge::SouthEast,
            Rect::from_min_size(rect.max - vec2(corner, corner), vec2(corner, corner)),
            Cursor::ResizeNwSe,
        ),
        (
            Edge::North,
            Rect::from_min_max(
                rect.min + vec2(corner, 0.0),
                pos2(rect.right() - corner, rect.top() + side),
            ),
            Cursor::ResizeVertical,
        ),
        (
            Edge::South,
            Rect::from_min_max(
                pos2(rect.left() + corner, rect.bottom() - side),
                rect.max - vec2(corner, 0.0),
            ),
            Cursor::ResizeVertical,
        ),
        (
            Edge::West,
            Rect::from_min_max(
                rect.min + vec2(0.0, corner),
                pos2(rect.left() + side, rect.bottom() - corner),
            ),
            Cursor::ResizeHorizontal,
        ),
        (
            Edge::East,
            Rect::from_min_max(
                pos2(rect.right() - side, rect.top() + corner),
                rect.max - vec2(0.0, corner),
            ),
            Cursor::ResizeHorizontal,
        ),
    ];
    for (index, (edge, bounds, cursor)) in edges.into_iter().enumerate() {
        egui::Area::new(egui::Id::unique(("window-resize", index)))
            .order(egui::Order::Foreground)
            .fixed_pos(bounds.min)
            .pivot(Align2::LEFT_TOP)
            .movable(false)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let (_, response) = ui.allocate_exact_size(bounds.size(), Sense::click_and_drag());
                if response.hovered() {
                    ctx.set_cursor_icon(cursor);
                }
                if response.is_pointer_button_down_on()
                    && ctx.input(|input| input.pointer.primary_pressed())
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(edge));
                }
            });
    }
}
