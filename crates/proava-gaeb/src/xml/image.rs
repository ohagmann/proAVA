//! Images in long texts (`<image Encoding="base64" Type="image/jpeg" width="…">Base64</image>`).

use super::base64;
use super::dom::Element;
use crate::model::TextBlock;

/// MIME type and file extension from the first bytes.
pub(crate) fn format_of(data: &[u8]) -> Option<(&'static str, &'static str)> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if data.starts_with(b"\x89PNG") {
        Some(("image/png", "png"))
    } else if data.starts_with(b"GIF8") {
        Some(("image/gif", "gif"))
    } else if data.starts_with(b"BM") {
        Some(("image/bmp", "bmp"))
    } else {
        None
    }
}

fn extension_of_type(mime: &str) -> Option<&'static str> {
    match mime.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" | "image/pjpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

/// File name made up from the content, stable across reading and writing.
pub(crate) fn name_for(data: &[u8], extension: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("bild_{hash:08x}.{extension}")
}

/// Width as written by the writer (whole pixels at 96 dpi).
pub(crate) fn px_from_mm(mm: f32) -> u32 {
    (mm * 96.0 / 25.4).round().max(1.0) as u32
}

pub(crate) fn mm_from_px(px: f32) -> f32 {
    px * 25.4 / 96.0
}

/// HTML-like width ("320", "320px", "8cm", "50mm", "120pt") in mm.
fn width_mm(text: &str) -> Option<f32> {
    let text = text.trim().to_ascii_lowercase();
    let digits_end = text
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == ','))
        .unwrap_or(text.len());
    let value: f32 = text[..digits_end].replace(',', ".").parse().ok()?;
    let mm = match text[digits_end..].trim() {
        "" | "px" => mm_from_px(value),
        "mm" => value,
        "cm" => value * 10.0,
        "pt" => value * 25.4 / 72.0,
        "in" => value * 25.4,
        _ => return None,
    };
    (mm > 0.0).then_some(mm)
}

fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    base.chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// The image block of an `<image>` element; None when its content is no
/// Base64 picture.
pub(crate) fn read_image(element: &Element) -> Option<TextBlock> {
    let data = base64::decode(&element.text())?;
    let extension = format_of(&data)
        .map(|(_, extension)| extension)
        .or_else(|| element.attr("Type").and_then(extension_of_type))?;
    let file_name = element
        .attr("Name")
        .map(clean_name)
        .filter(|name| !name.trim_matches(['.', '_']).is_empty())
        .unwrap_or_else(|| name_for(&data, extension));
    Some(TextBlock::Image {
        file_name,
        width_mm: element.attr("width").and_then(width_mm),
        data,
    })
}

/// Writes `<image>` when the format is one GAEB 3.3 allows (JPEG, PNG, GIF).
pub(crate) fn write_image(data: &[u8], width_mm: Option<f32>, out: &mut String) -> bool {
    let Some((mime, _)) = format_of(data).filter(|(mime, _)| *mime != "image/bmp") else {
        return false;
    };
    // Encoding is base64 by default; written out as AVA programs do.
    out.push_str(&format!("<image Encoding=\"base64\" Type=\"{mime}\""));
    if let Some(width) = width_mm {
        out.push_str(&format!(" width=\"{}\"", px_from_mm(width)));
    }
    out.push('>');
    out.push_str(&base64::encode(data));
    out.push_str("</image>");
    true
}

/// How an image block reads back after [`write_image`]: None if dropped.
pub(crate) fn as_written(block: &TextBlock) -> Option<TextBlock> {
    let TextBlock::Image { data, width_mm, .. } = block else {
        return Some(block.clone());
    };
    let (mime, extension) = format_of(data)?;
    (mime != "image/bmp").then(|| TextBlock::Image {
        file_name: name_for(data, extension),
        data: data.clone(),
        width_mm: width_mm.map(|mm| mm_from_px(px_from_mm(mm) as f32)),
    })
}
