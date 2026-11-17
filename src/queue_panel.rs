//! Temporary queue editor above the player. Never edits the YouTube playlist.
use crate::{
    artwork::ArtworkCache,
    playback_queue::PlaybackQueue,
    provider::Track,
    ui::{self, ButtonKind, Icon, LINE, MUTED, RED, SURFACE, WHITE},
};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, FontId, Layout, Rect, RichText, Sense, Stroke,
};

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Toggle,
    Play(usize),
    Remove(usize),
    Move(usize, usize),
}
pub struct Output {
    pub close: bool,
    pub action: Option<Action>,
}

pub fn show(
    ctx: &egui::Context,
    queue: &PlaybackQueue,
    artwork: &mut ArtworkCache,
    dragging: &mut Option<usize>,
    paused: bool,
    busy: bool,
) -> Output {
    let mut close = false;
    let mut action = None;
    let area = egui::Modal::default_area(egui::Id::unique("playback-queue"))
        .default_size(vec2(
            640.0,
            (ctx.content_rect().height() - 200.0).min(520.0),
        ))
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -128.0));
    let modal = egui::Modal::new(egui::Id::unique("playback-queue"))
        .area(area)
        .frame(
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(16)
                .stroke(Stroke::new(1.0, LINE))
                .inner_margin(22),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().scroll.dormant_background_opacity = 0.0;
            ui.spacing_mut().scroll.active_background_opacity = 0.0;
            ui.spacing_mut().scroll.interact_background_opacity = 0.0;
            ui.set_width((ctx.content_rect().width() - 88.0).min(596.0));
            ui.horizontal(|ui| {
                ui.label(ui::heading("À suivre", 24.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close_response = ui::transport_button(
                        ui,
                        Icon::Close,
                        "Fermer la file",
                        32.0,
                        18.0,
                        true,
                        false,
                    );
                    #[cfg(feature = "ui-capture")]
                    ctx.data_mut(|data| {
                        data.insert_temp(egui::Id::unique("qa_queue_close"), close_response.rect)
                    });
                    if close_response.clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(12.0);
            let mut rows = queue.entries();
            let mut view = View {
                artwork,
                dragging,
                paused,
                editable: !busy,
            };
            if let Some((position, track)) = rows.next() {
                ui.label(RichText::new("EN COURS").size(10.0).color(MUTED));
                if let Some(selected) = view.row(ui, position, track, true) {
                    action = Some(selected);
                }
            }
            let next: Vec<_> = rows.collect();
            ui.add_space(12.0);
            ui.label(RichText::new("ENSUITE").size(10.0).color(MUTED));
            if next.is_empty() {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(if busy {
                        "Préparation de la suite…"
                    } else {
                        "Fin de la file"
                    })
                    .size(13.0)
                    .color(MUTED),
                );
            } else {
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("upcoming-queue")
                    .max_height((ctx.content_rect().height() - 390.0).clamp(100.0, 280.0))
                    .show_rows(ui, 66.0, next.len(), |ui, range| {
                        for index in range {
                            let (position, track) = next[index];
                            if let Some(selected) = view.row(ui, position, track, false) {
                                action = Some(selected);
                            }
                        }
                    });
                #[cfg(feature = "ui-capture")]
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::unique("qa_queue_scrollbar"), scroll.inner_rect)
                });
                #[cfg(not(feature = "ui-capture"))]
                let _ = scroll;
            }
        });
    if ctx.input(|input| input.pointer.any_released()) {
        *dragging = None;
    }
    Output {
        close: close || modal.should_close(),
        action,
    }
}

struct View<'a> {
    artwork: &'a mut ArtworkCache,
    dragging: &'a mut Option<usize>,
    paused: bool,
    editable: bool,
}
impl View<'_> {
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        position: usize,
        track: &Track,
        active: bool,
    ) -> Option<Action> {
        ui.push_id(position, |ui| {
            let (row, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::click());
            if active || response.hovered() {
                ui.painter().rect_filled(
                    row,
                    8,
                    egui::Color32::from_gray(if active { 30 } else { 26 }),
                );
            }
            let cover = Rect::from_min_size(row.min + vec2(10.0, 9.0), vec2(48.0, 48.0));
            self.artwork.paint(ui, cover, track.cover_url.as_deref(), 4);
            if active || response.hovered() {
                ui.painter()
                    .rect_filled(cover, 4, egui::Color32::from_black_alpha(140));
                ui::icon(
                    ui.painter(),
                    if active && !self.paused {
                        Icon::Pause
                    } else {
                        Icon::Play
                    },
                    cover.shrink(12.0),
                    WHITE,
                );
            }
            let mut text = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(
                cover.right_top() + vec2(14.0, 4.0),
                pos2(row.right() - 88.0, row.bottom()),
            )));
            text.spacing_mut().item_spacing.y = 4.0;
            text.add(
                egui::Label::new(
                    RichText::new(&track.title)
                        .font(FontId::new(14.0, egui::FontFamily::Name("bold".into()))),
                )
                .truncate()
                .show_tooltip_when_elided(false),
            );
            text.add(
                egui::Label::new(RichText::new(&track.artist).size(12.0).color(MUTED))
                    .truncate()
                    .show_tooltip_when_elided(false),
            );
            let handle =
                Rect::from_center_size(pos2(row.right() - 61.0, row.center().y), vec2(24.0, 36.0));
            let menu =
                Rect::from_center_size(pos2(row.right() - 29.0, row.center().y), vec2(28.0, 36.0));
            #[cfg(feature = "ui-capture")]
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::unique(("qa_queue_handle", position)), handle);
                data.insert_temp(egui::Id::unique(("qa_queue_row", position)), row);
            });
            if !active {
                let drag = ui
                    .interact(
                        handle,
                        ui.scope_id().with("queue-drag"),
                        if self.editable {
                            Sense::drag()
                        } else {
                            Sense::hover()
                        },
                    )
                    .on_hover_cursor(egui::CursorIcon::Grab);
                // Capture the visible handle directly: the row can otherwise win the initial press.
                let pressed_pos = ui.input(|input| {
                    input
                        .pointer
                        .primary_pressed()
                        .then(|| input.pointer.interact_pos())
                        .flatten()
                });
                let pressed_on_handle = self.editable
                    && pressed_pos.is_some_and(|pos| {
                        handle.contains(pos)
                            && ui.clip_rect().contains(pos)
                            && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
                    });
                if drag.drag_started() || pressed_on_handle {
                    *self.dragging = Some(position);
                }
                if drag.dragged()
                    || (*self.dragging == Some(position)
                        && ui.input(|input| input.pointer.primary_down()))
                {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                for x in [-2.5, 2.5] {
                    for y in [-5.0, 0.0, 5.0] {
                        ui.painter()
                            .circle_filled(handle.center() + vec2(x, y), 1.3, MUTED);
                    }
                }
                if let Some(from) = *self.dragging {
                    if from != position
                        && ui
                            .input(|input| input.pointer.interact_pos())
                            .is_some_and(|pos| row.contains(pos) && ui.clip_rect().contains(pos))
                    {
                        let y = if from < position {
                            row.bottom()
                        } else {
                            row.top()
                        };
                        ui.painter().line_segment(
                            [pos2(row.left() + 10.0, y), pos2(row.right() - 10.0, y)],
                            Stroke::new(2.0, RED),
                        );
                        if ui.input(|input| input.pointer.any_released()) {
                            return Some(Action::Move(from, position));
                        }
                    }
                }
            }
            let mut selected = None;
            if !active {
                let response = ui
                    .interact(menu, ui.scope_id().with("queue-menu"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                for y in [-5.0, 0.0, 5.0] {
                    ui.painter()
                        .circle_filled(menu.center() + vec2(0.0, y), 1.4, MUTED);
                }
                egui::Popup::menu(&response).show(|ui| {
                    if ui::button(
                        ui,
                        "Lire maintenant",
                        Icon::Play,
                        ButtonKind::Quiet,
                        self.editable,
                    )
                    .clicked()
                    {
                        selected = Some(Action::Play(position));
                        ui.close();
                    }
                    if ui::button(
                        ui,
                        "Retirer de la file",
                        Icon::Close,
                        ButtonKind::Quiet,
                        self.editable,
                    )
                    .clicked()
                    {
                        selected = Some(Action::Remove(position));
                        ui.close();
                    }
                });
            }
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                && ui
                    .input(|input| input.pointer.interact_pos())
                    .is_some_and(|pos| !menu.contains(pos) && !handle.contains(pos))
            {
                selected = Some(if active {
                    Action::Toggle
                } else {
                    Action::Play(position)
                });
            }
            selected
        })
        .inner
    }
}
