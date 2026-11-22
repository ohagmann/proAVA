//! PDF-Ausschnitte for print and GAEB: rectangle conversion, PNG for GAEB
//! (300 dpi, cached so that the export check and the export render once).

use std::sync::{Arc, Mutex};

use proava_application::pdf_clip::{gaeb_width_px, GaebPicture, Pixels};
use proava_domain::{ClipRect, ImageAsset};
use proava_print::PdfArea;

pub(crate) fn area_of(rect: &ClipRect) -> PdfArea {
    PdfArea {
        left: rect.left,
        top: rect.top,
        width: rect.width,
        height: rect.height,
    }
}

pub(crate) fn render(
    pdf: &[u8],
    page: u32,
    rect: ClipRect,
    width_px: u32,
) -> Result<Pixels, String> {
    let pixels = proava_print::render_pdf_area(pdf, page, area_of(&rect), width_px)
        .map_err(|error| error.to_string())?;
    Ok(Pixels {
        width: pixels.width,
        height: pixels.height,
        rgba: pixels.rgba,
    })
}

/// Key of a rendered GAEB picture: content, part and width.
#[derive(Clone, PartialEq)]
struct Key {
    fingerprint: u64,
    length: usize,
    page: u32,
    rect: [u32; 4],
    width_px: u32,
}

/// Last GAEB pictures (a handful; an export renders each clip once).
static CACHE: Mutex<Vec<(Key, GaebPicture)>> = Mutex::new(Vec::new());
const CACHE_ENTRIES: usize = 12;

/// How often a GAEB picture was rendered (tests: the cache works).
pub static GAEB_RENDERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The pictures of the active LV's long texts as GAEB carries them.
pub(crate) fn gaeb_pictures(
    project: &proava_domain::Project,
) -> Vec<proava_application::GaebPictureInfo> {
    project
        .lv_image_uses()
        .into_iter()
        .filter_map(|(asset, width_mm)| {
            let asset = asset?;
            Some(match &asset.clip {
                None => proava_application::GaebPictureInfo {
                    file: asset.file.clone(),
                    bytes: asset.bytes.len(),
                    clip_pixels: None,
                    error: None,
                },
                Some(_) => {
                    let width =
                        width_mm.unwrap_or(proava_application::pdf_clip::DEFAULT_CLIP_WIDTH_MM);
                    match gaeb_picture(asset, width) {
                        Ok(picture) => proava_application::GaebPictureInfo {
                            file: asset.file.clone(),
                            bytes: picture.png.len(),
                            clip_pixels: Some((picture.width_px, picture.height_px)),
                            error: None,
                        },
                        Err(error) => proava_application::GaebPictureInfo {
                            file: asset.file.clone(),
                            bytes: 0,
                            clip_pixels: None,
                            error: Some(error),
                        },
                    }
                }
            })
        })
        .collect()
}

/// The PNG a PDF clip becomes in GAEB DA XML: 300 dpi at `width_mm`.
pub(crate) fn gaeb_picture(asset: &ImageAsset, width_mm: f32) -> Result<GaebPicture, String> {
    let clip = asset
        .clip
        .as_ref()
        .ok_or_else(|| format!("„{}“ ist kein PDF-Ausschnitt.", asset.file))?;
    let width_px = gaeb_width_px(width_mm);
    let key = Key {
        fingerprint: proava_domain::pdf_clip::fingerprint(&asset.bytes),
        length: asset.bytes.len(),
        page: clip.page,
        rect: [
            clip.rect.left.to_bits(),
            clip.rect.top.to_bits(),
            clip.rect.width.to_bits(),
            clip.rect.height.to_bits(),
        ],
        width_px,
    };
    if let Some((_, picture)) = CACHE
        .lock()
        .map_err(|_| "Zwischenspeicher gesperrt".to_string())?
        .iter()
        .find(|(cached, _)| *cached == key)
    {
        return Ok(picture.clone());
    }
    GAEB_RENDERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pixels =
        proava_print::render_pdf_area(&asset.bytes, clip.page, area_of(&clip.rect), width_px)
            .map_err(|error| format!("PDF-Ausschnitt „{}“: {error}", asset.file))?;
    let png = pixels
        .to_png()
        .map_err(|error| format!("PDF-Ausschnitt „{}“: {error}", asset.file))?;
    let picture = GaebPicture {
        png: Arc::new(png),
        width_px: pixels.width,
        height_px: pixels.height,
    };
    if let Ok(mut cache) = CACHE.lock() {
        if cache.len() >= CACHE_ENTRIES {
            cache.remove(0);
        }
        cache.push((key, picture.clone()));
    }
    Ok(picture)
}
