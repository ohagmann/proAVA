//! Picture helpers for long texts: preview decoding, clipboard images as PNG,
//! which files count as pictures.

use eframe::egui;

/// Decodes PNG/JPEG and shrinks it to at most 900 px width for the preview.
pub fn decode_preview(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    decode_preview_width(bytes, 900)
}

/// Decodes PNG/JPEG and shrinks it to at most `max_width` pixels across.
pub fn decode_preview_width(bytes: &[u8], max_width: u32) -> Result<egui::ColorImage, String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    let image = if image.width() > max_width {
        image.resize(
            max_width,
            max_width * image.height() / image.width().max(1),
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    let rgba = image.to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

/// Encodes a pasted clipboard image as PNG.
pub fn encode_png(image: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let raw: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|pixel| pixel.to_srgba_unmultiplied())
        .collect();
    let buffer = image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, raw)
        .ok_or_else(|| "Bild aus der Zwischenablage ist beschädigt.".to_string())?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    buffer
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(bytes.into_inner())
}

/// Whether bytes look like PNG or JPEG.
pub fn is_supported_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Color32, ColorImage};

    use super::*;

    #[test]
    fn png_encode_and_decode() {
        let mut picture = ColorImage::filled([3, 2], Color32::from_rgb(10, 200, 30));
        picture.pixels[0] = Color32::RED;
        let bytes = encode_png(&picture).unwrap();
        assert!(is_supported_image(&bytes));
        let decoded = decode_preview(&bytes).unwrap();
        assert_eq!(decoded.size, [3, 2]);
        assert_eq!(decoded.pixels[0], Color32::RED);
        assert!(decode_preview(b"kein Bild").is_err());
    }
}
