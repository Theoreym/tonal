//! Black text needs raw glyph coverage, not the gamma ramp used by the dark UI.
//! Keep a small independent atlas so both treatments can coexist in one frame.
use eframe::egui::{
    self,
    emath::GuiRounding,
    epaint::{
        text::{FontDefinitions, Fonts, ViewportKey},
        FontColorTransferFunction, Galley, TextOptions,
    },
    Color32, FontId, Pos2, Sense, TextureHandle, TextureOptions,
};
use std::sync::Arc;

const SIDE: usize = 1024;

pub struct LightText {
    fonts: Fonts,
    texture: Option<TextureHandle>,
    pass: Option<u64>,
}

impl LightText {
    pub fn new(definitions: FontDefinitions) -> Self {
        Self {
            fonts: Fonts::new(
                TextOptions {
                    max_texture_side: SIDE,
                    color_transfer_function: FontColorTransferFunction::LIGHT_MODE_DEFAULT,
                    ..Default::default()
                },
                definitions,
            ),
            texture: None,
            pass: None,
        }
    }

    pub fn layout(
        &mut self,
        ctx: &egui::Context,
        text: &str,
        font: FontId,
        width: f32,
    ) -> Arc<Galley> {
        let pass = ctx.cumulative_pass_nr();
        if self.pass != Some(pass) {
            self.fonts
                .begin_pass(*self.fonts.options(), ViewportKey::default());
            self.pass = Some(pass);
        }
        let galley = self
            .fonts
            .with_pixels_per_point(ctx.pixels_per_point())
            .layout(text.into(), font, Color32::BLACK, width);
        // A fixed GPU size keeps earlier meshes valid when more glyphs are laid
        // out later in the same pass. Upload only the changed atlas rectangle.
        let texture = self.texture.get_or_insert_with(|| {
            ctx.load_texture(
                "light-text-atlas",
                egui::ColorImage::filled([SIDE, SIDE], Color32::TRANSPARENT),
                TextureOptions::LINEAR,
            )
        });
        if let Some(delta) = self.fonts.font_image_delta() {
            texture.set_partial(
                delta.pos.unwrap_or([0, 0]),
                delta.image,
                TextureOptions::LINEAR,
            );
        }
        galley
    }

    pub fn paint(&self, painter: &egui::Painter, position: Pos2, galley: &Galley) {
        let Some(texture) = &self.texture else { return };
        let position = position.round_to_pixels(painter.pixels_per_point());
        for row in &galley.rows {
            let mut mesh = row.visuals.mesh.clone();
            mesh.texture_id = texture.id();
            for vertex in &mut mesh.vertices {
                vertex.pos += position.to_vec2() + row.pos.to_vec2();
                vertex.uv.x /= SIDE as f32;
                vertex.uv.y /= SIDE as f32;
            }
            painter.add(egui::Shape::mesh(mesh));
        }
    }

    pub fn label(&mut self, ui: &mut egui::Ui, text: &str, font: FontId) {
        let galley = self.layout(ui.ctx(), text, font, ui.available_width());
        let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
        self.paint(ui.painter(), rect.min, &galley);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_coverage_and_texture_coordinates_survive_later_glyph_allocations() {
        let ctx = egui::Context::default();
        let mut text = LightText::new(FontDefinitions::default());
        let badge = text.layout(&ctx, "M", FontId::proportional(12.0), 100.0);
        assert_eq!(
            text.fonts.options().color_transfer_function,
            FontColorTransferFunction::Off
        );
        let texture = text.texture.as_ref().unwrap().id();
        let _notice = text.layout(
            &ctx,
            "Écouter une nouvelle chanson — ajouté à ma playlist",
            FontId::proportional(16.0),
            240.0,
        );
        assert_eq!(text.texture.as_ref().unwrap().id(), texture);
        let image = text.fonts.image();
        assert!(image
            .pixels
            .iter()
            .any(|pixel| pixel.a() > 0 && pixel.a() < 255));
        for row in &badge.rows {
            assert!(!row.visuals.mesh.is_empty());
            assert!(row
                .visuals
                .mesh
                .vertices
                .iter()
                .all(|vertex| vertex.uv.x >= 0.0
                    && vertex.uv.x < SIDE as f32
                    && vertex.uv.y >= 0.0
                    && vertex.uv.y < SIDE as f32));
        }
    }
}
