//! Ausschnitt aus einer PDF-Seite im Langtext (e.g. part of a system
//! topology): the project keeps the PDF (usually only the one page), the
//! page and the rectangle. The printout embeds the page as vector graphics
//! and clips it to the rectangle; GAEB gets a PNG (GAEB only knows pixels).
//!
//! The asset of the long-text image block ([`crate::ImageAsset`]) carries a
//! [`PdfClip`]: `bytes` are then the PDF, not a picture. Assets with a clip
//! are never changed in place (another rectangle = another asset), only the
//! preview choices (frozen, hidden) are.

use std::sync::Arc;

/// Points (1/72 inch) per millimetre.
pub const PT_PER_MM: f32 = 72.0 / 25.4;

/// Rectangle on a PDF page in points, measured from the top-left corner of
/// the page as shown (crop box, rotation applied).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl ClipRect {
    /// Smallest side in points (about 5 mm) – smaller rectangles are refused.
    pub const MIN_SIDE: f32 = 14.0;

    pub fn new(left: f32, top: f32, width: f32, height: f32) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }

    /// The whole page.
    pub fn page(page_width: f32, page_height: f32) -> Self {
        Self::new(0.0, 0.0, page_width, page_height)
    }

    /// Rectangle from shares 0…1 of the page (left, top, right, bottom).
    pub fn from_shares(
        page_width: f32,
        page_height: f32,
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    ) -> Self {
        let (x0, x1) = (left.min(right), left.max(right));
        let (y0, y1) = (top.min(bottom), top.max(bottom));
        Self::new(
            x0 * page_width,
            y0 * page_height,
            (x1 - x0) * page_width,
            (y1 - y0) * page_height,
        )
    }

    /// The rectangle inside the page (cut at the edges, never negative).
    pub fn clamped(self, page_width: f32, page_height: f32) -> Self {
        let left = self.left.clamp(0.0, page_width);
        let top = self.top.clamp(0.0, page_height);
        let right = (self.left + self.width).clamp(left, page_width);
        let bottom = (self.top + self.height).clamp(top, page_height);
        Self::new(left, top, right - left, bottom - top)
    }

    pub fn width_mm(&self) -> f32 {
        self.width / PT_PER_MM
    }

    pub fn height_mm(&self) -> f32 {
        self.height / PT_PER_MM
    }

    /// Whether the rectangle is usable: finite, at least [`ClipRect::MIN_SIDE`]
    /// on both sides.
    pub fn is_valid(&self) -> bool {
        [self.left, self.top, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width >= Self::MIN_SIDE
            && self.height >= Self::MIN_SIDE
    }

    /// Height / width.
    pub fn aspect(&self) -> f32 {
        if self.width > 0.0 {
            self.height / self.width
        } else {
            1.0
        }
    }

    /// „594 × 420 mm“.
    pub fn size_text(&self) -> String {
        format!("{:.0} × {:.0} mm", self.width_mm(), self.height_mm())
    }
}

/// The PDF behind a long-text image and which part of which page it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfClip {
    /// Page in the stored PDF, counted from 1.
    pub page: u32,
    /// Size of that page in points (as shown: crop box, rotation applied).
    pub page_width: f32,
    pub page_height: f32,
    pub rect: ClipRect,
    /// Where the PDF came from, shown in the placeholder (file name, page).
    pub source: String,
    /// The preview stays as it is (no new rendering, also after opening the
    /// project); the picture is [`PdfClip::frozen_png`].
    pub preview_frozen: bool,
    /// Only a placeholder with name and size instead of the preview.
    pub preview_hidden: bool,
    /// The frozen preview as PNG (stored in the project file).
    pub frozen_png: Option<Arc<Vec<u8>>>,
}

impl PdfClip {
    pub fn new(
        page: u32,
        page_size: (f32, f32),
        rect: ClipRect,
        source: impl Into<String>,
    ) -> Self {
        Self {
            page: page.max(1),
            page_width: page_size.0,
            page_height: page_size.1,
            rect: rect.clamped(page_size.0, page_size.1),
            source: source.into(),
            preview_frozen: false,
            preview_hidden: false,
            frozen_png: None,
        }
    }

    /// Same PDF part (page and rectangle); the preview choices do not count.
    pub fn same_part(&self, other: &PdfClip) -> bool {
        self.page == other.page && self.rect == other.rect
    }

    /// Paper size name of the page, e.g. „A0 quer“, or „841 × 1189 mm“.
    pub fn page_text(&self) -> String {
        paper_name(self.page_width, self.page_height)
    }
}

/// „A3 quer“, „A4 hoch“ … for the usual DIN sizes (± 3 mm), else „W × H mm“.
pub fn paper_name(width_pt: f32, height_pt: f32) -> String {
    let (w, h) = (width_pt / PT_PER_MM, height_pt / PT_PER_MM);
    let (short, long) = (w.min(h), w.max(h));
    let sizes = [
        ("A0", 841.0, 1189.0),
        ("A1", 594.0, 841.0),
        ("A2", 420.0, 594.0),
        ("A3", 297.0, 420.0),
        ("A4", 210.0, 297.0),
        ("A5", 148.0, 210.0),
    ];
    for (name, s, l) in sizes {
        if (short - s).abs() <= 3.0 && (long - l).abs() <= 3.0 {
            let orientation = if w > h { "quer" } else { "hoch" };
            return format!("{name} {orientation}");
        }
    }
    format!("{w:.0} × {h:.0} mm")
}

/// Whether bytes look like a PDF file.
pub fn is_pdf(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    head.windows(5).any(|window| window == b"%PDF-")
}

/// Cheap fingerprint of large content (length and a sample of the bytes) –
/// for preview caches, not for security.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ bytes.len() as u64;
    let mut eat = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    let edge = bytes.len().min(4096);
    bytes[..edge].iter().copied().for_each(&mut eat);
    bytes[bytes.len() - edge..]
        .iter()
        .copied()
        .for_each(&mut eat);
    let mut index = edge;
    while index < bytes.len() {
        eat(bytes[index]);
        index += 509;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_from_shares_and_clamping() {
        let rect = ClipRect::from_shares(3370.0, 2384.0, 0.5, 0.0, 0.0, 0.5);
        assert_eq!(rect, ClipRect::new(0.0, 0.0, 1685.0, 1192.0));
        let cut = ClipRect::new(-10.0, 100.0, 50.0, 5000.0).clamped(595.0, 842.0);
        assert_eq!(cut, ClipRect::new(0.0, 100.0, 40.0, 742.0));
        assert!(cut.is_valid());
        assert!(!ClipRect::new(0.0, 0.0, 5.0, 100.0).is_valid());
        assert!(!ClipRect::new(f32::NAN, 0.0, 50.0, 100.0).is_valid());
        assert_eq!(ClipRect::page(841.89, 595.28).size_text(), "297 × 210 mm");
    }

    #[test]
    fn paper_names() {
        assert_eq!(paper_name(3370.0, 2384.0), "A0 quer");
        assert_eq!(paper_name(595.28, 841.89), "A4 hoch");
        assert_eq!(paper_name(500.0, 500.0), "176 × 176 mm");
    }

    #[test]
    fn same_content_tells_parts_apart() {
        use crate::{ImageAsset, Lv, OzMask, Project, ProjectName};
        let mut project = Project::new(
            ProjectName::parse("P").unwrap(),
            "",
            Lv::new("LV", OzMask::parse("11.PPPP").unwrap()),
        );
        let pdf = b"%PDF-1.7".to_vec();
        let left = PdfClip::new(
            1,
            (600.0, 400.0),
            ClipRect::new(0.0, 0.0, 300.0, 400.0),
            "a.pdf",
        );
        let right = PdfClip::new(
            1,
            (600.0, 400.0),
            ClipRect::new(300.0, 0.0, 300.0, 400.0),
            "a.pdf",
        );
        let a = project.add_pdf_clip("a.pdf", pdf.clone(), left.clone());
        let b = project.add_pdf_clip("a.pdf", pdf.clone(), right);
        assert_eq!(b, "a_2.pdf");
        let (first, second) = (project.image(&a).unwrap(), project.image(&b).unwrap());
        assert!(
            !first.same_content(second),
            "anderes Rechteck = anderer Inhalt"
        );
        let mut hidden = first.clone();
        hidden.clip.as_mut().unwrap().preview_hidden = true;
        assert!(
            first.same_content(&hidden),
            "Vorschau-Schalter zählen nicht"
        );
        let picture = ImageAsset {
            file: "a.pdf".into(),
            bytes: pdf,
            clip: None,
        };
        assert!(!first.same_content(&picture));
        // A copy into another project keeps the clip.
        let mut other = project.clone();
        let copied = other.add_asset_copy(first);
        assert_eq!(
            other.image(&copied).unwrap().clip.as_ref().unwrap().rect,
            left.rect
        );
        assert!(!project.set_clip_preview("fehlt.pdf", true, true, None));
        assert!(project.set_clip_preview(&a, false, true, None));
        assert!(
            !project.set_clip_preview(&a, true, true, None),
            "einfrieren braucht ein Bild"
        );
    }

    #[test]
    fn pdf_detection_and_fingerprint() {
        assert!(is_pdf(b"%PDF-1.7\n..."));
        assert!(!is_pdf(b"\x89PNG"));
        let a = vec![7u8; 20_000];
        let mut b = a.clone();
        assert_eq!(fingerprint(&a), fingerprint(&b));
        b[4096 + 509 * 3] ^= 1;
        assert_ne!(fingerprint(&a), fingerprint(&b), "Stichprobe in der Mitte");
        let mut c = a.clone();
        c[19_999] ^= 1;
        assert_ne!(fingerprint(&a), fingerprint(&c), "Ende");
        assert_ne!(fingerprint(&a[..19_000]), fingerprint(&a));
    }
}
