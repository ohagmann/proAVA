//! „Ausschnitt aus PDF einfügen“: what the program and the MCP server share –
//! reading the PDF, keeping only the chosen page, building the clip.

use proava_domain::{ClipRect, PdfClip};

use crate::ExchangeFiles;

/// Dots per inch of the PNG a PDF clip becomes in GAEB.
pub const GAEB_DPI: f32 = 300.0;
/// A GAEB picture larger than this gets a warning in the export check.
pub const LARGE_GAEB_PICTURE: usize = 5 * 1024 * 1024;
/// Width a clip gets in the long text when nothing else is asked (mm).
pub const DEFAULT_CLIP_WIDTH_MM: f32 = 150.0;
/// Widest clip in the long text (mm), as for pictures.
pub const MAX_CLIP_WIDTH_MM: f32 = 180.0;

/// Rendered pixels: RGBA, 8 bit, not premultiplied (opaque anyway).
#[derive(Debug, Clone, PartialEq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A PDF clip as PNG for GAEB.
#[derive(Debug, Clone, PartialEq)]
pub struct GaebPicture {
    pub png: std::sync::Arc<Vec<u8>>,
    pub width_px: u32,
    pub height_px: u32,
}

/// One picture of the long texts as GAEB DA XML carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct GaebPictureInfo {
    pub file: String,
    /// Bytes in the file (before Base64).
    pub bytes: usize,
    /// PDF clips: pixels of the PNG.
    pub clip_pixels: Option<(u32, u32)>,
    /// A PDF clip that could not be turned into a PNG.
    pub error: Option<String>,
}

/// Pixels across for `width_mm` at [`GAEB_DPI`].
pub fn gaeb_width_px(width_mm: f32) -> u32 {
    (width_mm.clamp(5.0, 1000.0) / 25.4 * GAEB_DPI).round() as u32
}

/// A PDF read for choosing a page and rectangle.
#[derive(Debug, Clone)]
pub struct PdfSource {
    pub name: String,
    pub bytes: Vec<u8>,
    /// Size of every page in points.
    pub pages: Vec<(f32, f32)>,
}

impl PdfSource {
    pub fn read(exchange: &dyn ExchangeFiles, name: &str, bytes: Vec<u8>) -> Result<Self, String> {
        if !proava_domain::is_pdf(&bytes) {
            return Err(format!("„{name}“ ist keine PDF-Datei."));
        }
        let pages = exchange.pdf_pages(&bytes)?;
        Ok(Self {
            name: name.to_string(),
            bytes,
            pages,
        })
    }

    pub fn page_size(&self, page: u32) -> Option<(f32, f32)> {
        self.pages.get(page.checked_sub(1)? as usize).copied()
    }
}

/// What goes into the project for one clip.
#[derive(Debug, Clone)]
pub struct PreparedClip {
    /// File name in the project (`<stem>_S<page>.pdf`).
    pub name: String,
    /// The stored PDF: the one page if it could be cut out, else the file.
    pub bytes: Vec<u8>,
    pub clip: PdfClip,
    /// Size of the chosen file (to tell the user what was saved).
    pub original_bytes: usize,
    /// Only the page was kept (the file had more than one page).
    pub page_only: bool,
}

impl PreparedClip {
    /// Like [`PreparedClip::summary`], before the clip goes into `project`:
    /// a PDF that is already there (another part of the same page) adds
    /// nothing to the project file.
    pub fn summary_for(&self, project: &proava_domain::Project) -> String {
        let known = project.images().iter().any(|image| {
            image.clip.is_some()
                && image.bytes.len() == self.bytes.len()
                && image.bytes == self.bytes
        });
        if known {
            format!(
                "Ausschnitt {} aus {} ({}) – die PDF ist schon im Projekt, die Datei wächst nicht.",
                self.clip.rect.size_text(),
                self.clip.source,
                self.clip.page_text(),
            )
        } else {
            self.summary()
        }
    }

    /// „Ausschnitt 297 × 210 mm aus 40849.pdf, Seite 1 (A0 quer) – im Projekt 1,7 MB (nur diese Seite; Datei 3,2 MB).“
    pub fn summary(&self) -> String {
        let mut text = format!(
            "Ausschnitt {} aus {} ({}) – im Projekt {}",
            self.clip.rect.size_text(),
            self.clip.source,
            self.clip.page_text(),
            proava_domain::size_text(self.bytes.len()),
        );
        if self.page_only {
            text.push_str(&format!(
                " (nur diese Seite; Datei {})",
                proava_domain::size_text(self.original_bytes)
            ));
        }
        text.push('.');
        text
    }
}

/// Builds the clip of `rect` on page `page` (from 1) of `source`: keeps only
/// that page when the file has more (and the page alone is smaller), else
/// the whole file.
pub fn prepare_clip(
    exchange: &dyn ExchangeFiles,
    source: &PdfSource,
    page: u32,
    rect: ClipRect,
) -> Result<PreparedClip, String> {
    let page_size = source.page_size(page).ok_or_else(|| {
        format!(
            "Die PDF hat nur {}.",
            proava_domain::wording::count(source.pages.len(), "Seite", "Seiten")
        )
    })?;
    let rect = rect.clamped(page_size.0, page_size.1);
    if !rect.is_valid() {
        return Err("Der Ausschnitt ist zu klein (mindestens 5 × 5 mm).".to_string());
    }
    let stem = source
        .name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&source.name)
        .trim_end_matches(".pdf")
        .trim_end_matches(".PDF")
        .to_string();
    let name = format!("{stem}_S{page}.pdf");
    let label = if source.pages.len() > 1 {
        format!("{}, Seite {page}", source.name)
    } else {
        source.name.clone()
    };
    let (bytes, stored_page, page_only) = if source.pages.len() > 1 {
        match exchange.pdf_single_page(&source.bytes, page) {
            Ok(single) if single.len() < source.bytes.len() => (single, 1, true),
            _ => (source.bytes.clone(), page, false),
        }
    } else {
        (source.bytes.clone(), 1, false)
    };
    Ok(PreparedClip {
        name,
        bytes,
        clip: PdfClip::new(stored_page, page_size, rect, label),
        original_bytes: source.bytes.len(),
        page_only,
    })
}
