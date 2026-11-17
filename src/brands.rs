//! DPI-aware rasterization of the bundled official SVGs, cached at each displayed size.
use eframe::egui::{self, pos2, vec2, Color32, Rect, TextureHandle};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Brand {
    YouTubeMusic,
    Spotify,
}
impl Brand {
    fn source(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::YouTubeMusic => (
                "youtube-music",
                include_bytes!("../assets/brands/youtube-music.svg"),
            ),
            Self::Spotify => ("spotify", include_bytes!("../assets/brands/spotify.svg")),
        }
    }
}
fn texture(ctx: &egui::Context, brand: Brand, height: f32) -> TextureHandle {
    // An exact 2:1 reduction uses a properly averaged mip level instead of
    // sampling a few isolated texels from a fixed, much larger image.
    let pixels = (height * ctx.pixels_per_point()).round().clamp(1.0, 512.0) as u32 * 2;
    let id = egui::Id::unique(("brand-dpi", brand, pixels));
    ctx.data_mut(|data| data.get_temp::<TextureHandle>(id))
        .unwrap_or_else(|| {
            let (name, bytes) = brand.source();
            let image = egui_extras::image::load_svg_bytes_with_size(
                bytes,
                egui::load::SizeHint::Height(pixels),
                &Default::default(),
            )
            .expect("Bundled official SVG");
            let texture = ctx.load_texture(
                name,
                image,
                egui::TextureOptions {
                    mipmap_mode: Some(egui::TextureFilter::Linear),
                    ..egui::TextureOptions::LINEAR
                },
            );
            ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
            texture
        })
}
pub fn paint(ui: &egui::Ui, brand: Brand, rect: Rect, tint: Color32) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let texture = texture(ui.ctx(), brand, rect.height());
    let size = texture.size_vec2();
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let fitted = Rect::from_center_size(rect.center(), size * scale);
    ui.painter().image(
        texture.id(),
        fitted,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        tint,
    );
}
pub fn image(ui: &mut egui::Ui, brand: Brand, side: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), egui::Sense::hover());
    paint(ui, brand, rect, Color32::WHITE);
}
