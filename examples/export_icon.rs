//! Regenerate the Windows icon from the original vector mark.
//! cargo run --example export_icon --features ui-capture
use image::ImageEncoder;
use std::{fs, io::Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sizes = [16_u32, 20, 24, 32, 48, 64, 128, 256];
    let mut entries = Vec::new();
    for size in sizes {
        let image = egui_extras::image::load_svg_bytes_with_size(
            include_bytes!("../assets/tonal.svg"),
            eframe::egui::load::SizeHint::Height(size),
            &Default::default(),
        )?;
        let pixels: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(
            &pixels,
            size,
            size,
            image::ExtendedColorType::Rgba8,
        )?;
        if size == 256 {
            fs::write("assets/tonal.png", &png)?;
        }
        entries.push((size, png));
    }
    let mut icon = fs::File::create("assets/tonal.ico")?;
    icon.write_all(&[0, 0, 1, 0])?;
    icon.write_all(&(entries.len() as u16).to_le_bytes())?;
    let mut offset = (6 + 16 * entries.len()) as u32;
    for (size, png) in &entries {
        icon.write_all(&[*size as u8, *size as u8, 0, 0])?;
        icon.write_all(&1_u16.to_le_bytes())?;
        icon.write_all(&32_u16.to_le_bytes())?;
        icon.write_all(&(png.len() as u32).to_le_bytes())?;
        icon.write_all(&offset.to_le_bytes())?;
        offset += png.len() as u32;
    }
    for (_, png) in entries {
        icon.write_all(&png)?;
    }
    Ok(())
}
