//! Ausschnitte aus PDF-Seiten: pages of a PDF, one page as a PDF of its own,
//! a rectangle of a page as pixels (preview, GAEB). Everything in-process:
//! Typst embeds the page (as the printout does), `hayro` – Typst's own PDF
//! rasterizer – turns the result into pixels.

use std::sync::Arc;

use hayro::hayro_interpret::font::{FontData, FontQuery, StandardFont};
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::{RenderCache, RenderSettings};

use crate::world::PrintWorld;
use crate::PrintError;

/// Most pixels one rendering may have (about 240 MB in RGBA): protects the
/// PC from a tiny rectangle of an A0 page asked for at a huge width.
pub const MAX_PIXELS: u64 = 60_000_000;
/// Longest side in pixels (the rasterizer counts in 16 bit).
pub const MAX_SIDE: u32 = 16_000;

/// A rectangle on a PDF page in points from the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfArea {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl PdfArea {
    fn is_valid(&self) -> bool {
        [self.left, self.top, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width > 0.5
            && self.height > 0.5
    }
}

/// Pixels of a rendered rectangle: RGBA, 8 bit, opaque (white paper).
#[derive(Debug, Clone, PartialEq)]
pub struct ClipPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl ClipPixels {
    /// The pixels as PNG file.
    pub fn to_png(&self) -> Result<Vec<u8>, PrintError> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            let mut writer = encoder
                .write_header()
                .map_err(|error| PrintError::Data(format!("PNG: {error}")))?;
            writer
                .write_image_data(&self.rgba)
                .map_err(|error| PrintError::Data(format!("PNG: {error}")))?;
        }
        Ok(out)
    }
}

fn load(pdf: &[u8]) -> Result<Pdf, PrintError> {
    Pdf::new(Arc::new(pdf.to_vec())).map_err(|error| {
        PrintError::Data(match error {
            hayro::hayro_syntax::LoadPdfError::Decryption(_) => {
                "Die PDF ist verschlüsselt oder kennwortgeschützt.".to_string()
            }
            hayro::hayro_syntax::LoadPdfError::Invalid => {
                "Die Datei ist keine lesbare PDF.".to_string()
            }
        })
    })
}

/// Size of every page in points (as shown: crop box, rotation applied).
pub fn pdf_pages(pdf: &[u8]) -> Result<Vec<(f32, f32)>, PrintError> {
    let document = load(pdf)?;
    let pages: Vec<(f32, f32)> = document
        .pages()
        .iter()
        .map(|page| page.render_dimensions())
        .collect();
    if pages.is_empty() {
        return Err(PrintError::Data("Die PDF hat keine Seiten.".to_string()));
    }
    Ok(pages)
}

fn page_size(pdf: &[u8], page: u32) -> Result<(f32, f32), PrintError> {
    let pages = pdf_pages(pdf)?;
    pages.get(page.max(1) as usize - 1).copied().ok_or_else(|| {
        PrintError::Data(format!(
            "Die PDF hat nur {} {}.",
            pages.len(),
            if pages.len() == 1 { "Seite" } else { "Seiten" }
        ))
    })
}

/// Number in Typst source (points).
fn pt(value: f32) -> String {
    format!("{value:.4}pt")
}

/// Typst document with one page of `width` × `height` points holding page
/// `page` of `/quelle.pdf` (size `image`), moved by (`dx`, `dy`);
/// everything outside the page is cut. Width and height of the image are
/// both given and `fit: "stretch"`: else Typst shrinks a page taller than
/// the frame to the frame and crops it in the middle.
fn typst_page(width: f32, height: f32, dx: f32, dy: f32, page: u32, image: (f32, f32)) -> String {
    format!(
        "#set page(width: {w}, height: {h}, margin: 0pt)\n\
         #block(width: {w}, height: {h}, clip: true, place(top + left, dx: {dx}, dy: {dy}, \
         image(\"/quelle.pdf\", page: {page}, width: {iw}, height: {ih}, fit: \"stretch\")))\n",
        w = pt(width),
        h = pt(height),
        dx = pt(dx),
        dy = pt(dy),
        iw = pt(image.0),
        ih = pt(image.1),
    )
}

fn compile(source: String, pdf: &[u8]) -> Result<Vec<u8>, PrintError> {
    let mut world = PrintWorld::new(&source);
    world.add_file("/quelle.pdf", pdf.to_vec());
    let document = world.layout()?;
    world.pdf(&document)
}

/// Page `page` (from 1) as a PDF of its own – what a long text needs of a
/// many-page file. The page is embedded as Typst embeds it in the printout
/// (vector, fonts along); a one-page PDF comes back unchanged.
pub fn pdf_single_page(pdf: &[u8], page: u32) -> Result<Vec<u8>, PrintError> {
    let pages = pdf_pages(pdf)?;
    if pages.len() == 1 && page <= 1 {
        return Ok(pdf.to_vec());
    }
    let (width, height) = page_size(pdf, page)?;
    let single = compile(
        typst_page(width, height, 0.0, 0.0, page, (width, height)),
        pdf,
    )?;
    // The new file must hold exactly that page.
    match pdf_pages(&single)?.as_slice() {
        [(w, h)] if (w - width).abs() < 1.0 && (h - height).abs() < 1.0 => Ok(single),
        _ => Err(PrintError::Pdf(
            "Die Seite ließ sich nicht herauslösen.".to_string(),
        )),
    }
}

/// The rectangle `area` of page `page` as a one-page PDF (vector).
pub fn pdf_area(pdf: &[u8], page: u32, area: PdfArea) -> Result<Vec<u8>, PrintError> {
    if !area.is_valid() {
        return Err(PrintError::Data("Der Ausschnitt ist leer.".to_string()));
    }
    let size = page_size(pdf, page)?;
    compile(
        typst_page(area.width, area.height, -area.left, -area.top, page, size),
        pdf,
    )
}

/// Pixel size for `width_px` pixels across `area` (height by the aspect),
/// shrunk to the limits.
pub fn pixel_size(area: PdfArea, width_px: u32) -> (u32, u32) {
    let mut width = width_px.clamp(1, MAX_SIDE) as f64;
    let aspect = f64::from(area.height / area.width.max(0.01));
    let mut height = (width * aspect).round().max(1.0);
    if height > f64::from(MAX_SIDE) {
        width *= f64::from(MAX_SIDE) / height;
        height = f64::from(MAX_SIDE);
    }
    let pixels = width * height;
    if pixels > MAX_PIXELS as f64 {
        let k = (MAX_PIXELS as f64 / pixels).sqrt();
        width *= k;
        height *= k;
    }
    (
        width.floor().max(1.0) as u32,
        height.floor().max(1.0) as u32,
    )
}

/// Renders `area` of page `page` with `width_px` pixels across (height by
/// the aspect, both within [`MAX_SIDE`]/[`MAX_PIXELS`]) on white paper.
pub fn render_pdf_area(
    pdf: &[u8],
    page: u32,
    area: PdfArea,
    width_px: u32,
) -> Result<ClipPixels, PrintError> {
    let cropped = pdf_area(pdf, page, area)?;
    let document = load(&cropped)?;
    let Some(first) = document.pages().first() else {
        return Err(PrintError::Pdf(
            "Der Ausschnitt hat keine Seite.".to_string(),
        ));
    };
    let (page_width, page_height) = first.render_dimensions();
    let (width, height) = pixel_size(
        PdfArea {
            left: 0.0,
            top: 0.0,
            width: page_width,
            height: page_height,
        },
        width_px,
    );
    let settings = RenderSettings {
        x_scale: width as f32 / page_width.max(0.01),
        y_scale: height as f32 / page_height.max(0.01),
        width: Some(width as u16),
        height: Some(height as u16),
        bg_color: hayro::vello_cpu::color::palette::css::WHITE,
    };
    let pixmap = hayro::render(
        first,
        &RenderCache::new(),
        &interpreter_settings(),
        &settings,
    );
    Ok(ClipPixels {
        width: u32::from(pixmap.width()),
        height: u32::from(pixmap.height()),
        // Opaque: premultiplied = straight RGBA.
        rgba: pixmap.data_as_u8_slice().to_vec(),
    })
}

/// Fonts a PDF names without embedding them: the standard fonts Typst ships.
fn interpreter_settings() -> InterpreterSettings {
    fn standard(font: StandardFont) -> Option<(FontData, u32)> {
        let bytes: &'static [u8] = match font {
            StandardFont::Helvetica => typst_assets::pdf::SANS,
            StandardFont::HelveticaBold => typst_assets::pdf::SANS_BOLD,
            StandardFont::HelveticaOblique => typst_assets::pdf::SANS_ITALIC,
            StandardFont::HelveticaBoldOblique => typst_assets::pdf::SANS_BOLD_ITALIC,
            StandardFont::Courier => typst_assets::pdf::FIXED,
            StandardFont::CourierBold => typst_assets::pdf::FIXED_BOLD,
            StandardFont::CourierOblique => typst_assets::pdf::FIXED_ITALIC,
            StandardFont::CourierBoldOblique => typst_assets::pdf::FIXED_BOLD_ITALIC,
            StandardFont::TimesRoman => typst_assets::pdf::SERIF,
            StandardFont::TimesBold => typst_assets::pdf::SERIF_BOLD,
            StandardFont::TimesItalic => typst_assets::pdf::SERIF_ITALIC,
            StandardFont::TimesBoldItalic => typst_assets::pdf::SERIF_BOLD_ITALIC,
            StandardFont::ZapfDingBats => typst_assets::pdf::DING_BATS,
            StandardFont::Symbol => typst_assets::pdf::SYMBOL,
        };
        Some((Arc::new(bytes), 0))
    }
    InterpreterSettings {
        font_resolver: Arc::new(|query| match query {
            FontQuery::Standard(font) => standard(*font),
            FontQuery::Fallback(fallback) => standard(fallback.pick_standard_font()),
        }),
        cmap_resolver: Arc::new(|_| None),
        warning_sink: Arc::new(|_| {}),
        render_annotations: false,
    }
}

/// What a PDF draws, as far as tests and checks need it: the text of all
/// pages (from the glyphs, so only real text counts – not text turned into
/// pixels) and the number of raster images drawn.
pub fn pdf_content(pdf: &[u8]) -> Result<(String, usize), PrintError> {
    use hayro::hayro_interpret::{
        interpret_page, BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image,
        InterpreterCache, Paint, PathDrawMode, SoftMask,
    };
    use kurbo::{Affine, BezPath, Rect};

    #[derive(Default)]
    struct Collect {
        text: String,
        images: usize,
    }
    impl<'a> Device<'a> for Collect {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'a>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'a>, _: &PathDrawMode) {}
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
        fn draw_glyph(
            &mut self,
            glyph: &hayro::hayro_interpret::font::Glyph<'a>,
            _: Affine,
            _: Affine,
            _: &Paint<'a>,
            _: &GlyphDrawMode,
        ) {
            match glyph.as_unicode() {
                Some(hayro::hayro_interpret::hayro_cmap::BfString::Char(ch)) => self.text.push(ch),
                Some(hayro::hayro_interpret::hayro_cmap::BfString::String(text)) => {
                    self.text.push_str(&text)
                }
                None => {}
            }
        }
        fn draw_image(&mut self, image: Image<'a, '_>, _: Affine) {
            if matches!(image, Image::Raster(_)) {
                self.images += 1;
            }
        }
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }

    let document = load(pdf)?;
    let cache = InterpreterCache::new();
    let mut device = Collect::default();
    for page in document.pages().iter() {
        let (width, height) = page.render_dimensions();
        let mut context = Context::new(
            Affine::IDENTITY,
            Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            &cache,
            page.xref(),
            interpreter_settings(),
        );
        interpret_page(page, &mut context, &mut device);
        device.text.push('\n');
    }
    Ok((device.text, device.images))
}

/// A small vector PDF for tests (made with Typst, nothing from outside):
/// pages of 200 × 100 mm; page 1 has „TOPOLOGIEKERN“ top left with a thick
/// line below and „PLANKOPFRAND“ bottom right, page 2 „ZWEITESEITE“.
#[doc(hidden)]
pub fn sample_vector_pdf() -> Vec<u8> {
    let source = "#set page(width: 200mm, height: 100mm, margin: 0mm)\n\
        #set text(font: \"Liberation Sans\", size: 14pt)\n\
        #place(top + left, dx: 10mm, dy: 10mm)[TOPOLOGIEKERN]\n\
        #place(top + left, dx: 150mm, dy: 80mm)[PLANKOPFRAND]\n\
        #place(top + left, dx: 10mm, dy: 30mm, line(length: 80mm, stroke: 2pt))\n\
        #pagebreak()\n\
        #place(top + left, dx: 20mm, dy: 20mm)[ZWEITESEITE]\n";
    let world = PrintWorld::new(source);
    world
        .layout()
        .and_then(|document| world.pdf(&document))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pdf() -> Vec<u8> {
        sample_vector_pdf()
    }

    #[test]
    fn pages_single_page_and_content() {
        let pdf = sample_pdf();
        let pages = pdf_pages(&pdf).unwrap();
        assert_eq!(pages.len(), 2);
        assert!(
            (pages[0].0 - 566.93).abs() < 0.5 && (pages[0].1 - 283.46).abs() < 0.5,
            "{pages:?}"
        );
        let (text, images) = pdf_content(&pdf).unwrap();
        assert!(
            text.contains("TOPOLOGIEKERN") && text.contains("ZWEITESEITE"),
            "{text}"
        );
        assert_eq!(images, 0);

        let second = pdf_single_page(&pdf, 2).unwrap();
        assert_eq!(pdf_pages(&second).unwrap().len(), 1);
        let (text, images) = pdf_content(&second).unwrap();
        assert!(
            text.contains("ZWEITESEITE") && !text.contains("TOPOLOGIEKERN"),
            "{text}"
        );
        assert_eq!(images, 0, "Seite bleibt Vektor");
        // A one-page file comes back unchanged.
        assert_eq!(pdf_single_page(&second, 1).unwrap(), second);
        assert!(pdf_single_page(&pdf, 3).is_err());
        assert!(pdf_pages(b"%PDF-1.7 kaputt").is_err());
    }

    #[test]
    fn area_renders_only_the_rectangle() {
        let pdf = sample_pdf();
        // Left half of page 1 (the word TOPOLOGIEKERN and the line).
        let area = PdfArea {
            left: 0.0,
            top: 0.0,
            width: 283.46,
            height: 283.46,
        };
        let cut = pdf_area(&pdf, 1, area).unwrap();
        let (text, images) = pdf_content(&cut).unwrap();
        assert!(text.contains("TOPOLOGIEKERN"), "{text}");
        assert_eq!(images, 0);
        let pixels = render_pdf_area(&pdf, 1, area, 300).unwrap();
        assert_eq!((pixels.width, pixels.height), (300, 300));
        assert_eq!(pixels.rgba.len(), 300 * 300 * 4);
        let dark = pixels.rgba.chunks(4).filter(|px| px[0] < 100).count();
        assert!(dark > 300, "Linie und Text sind zu sehen: {dark}");
        // The right half holds no line: far fewer dark pixels.
        let right = PdfArea {
            left: 300.0,
            ..area
        };
        let other = render_pdf_area(&pdf, 1, right, 300).unwrap();
        let dark_right = other.rgba.chunks(4).filter(|px| px[0] < 100).count();
        assert!(dark_right < dark, "{dark_right} < {dark}");
        let png = pixels.to_png().unwrap();
        assert!(png.starts_with(b"\x89PNG"));
    }

    /// Dark pixels in columns `xs` and rows `ys`.
    fn dark_in(pixels: &ClipPixels, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> usize {
        let mut count = 0;
        for y in ys {
            for x in xs.clone() {
                let at = ((y * pixels.width + x) * 4) as usize;
                if pixels.rgba[at] < 128 {
                    count += 1;
                }
            }
        }
        count
    }

    /// The rectangle is taken from the top-left corner of the page, also when
    /// it is lower than the page (Typst must not fit the page into the frame).
    #[test]
    fn upper_and_lower_half_are_where_they_belong() {
        let pdf = sample_pdf();
        let upper = PdfArea {
            left: 0.0,
            top: 0.0,
            width: 566.93,
            height: 141.73,
        };
        let lower = PdfArea {
            top: 141.73,
            ..upper
        };
        // 2 px per mm: 400 × 100 pixels each.
        let top = render_pdf_area(&pdf, 1, upper, 400).unwrap();
        let bottom = render_pdf_area(&pdf, 1, lower, 400).unwrap();
        assert_eq!((top.width, top.height, bottom.height), (400, 100, 100));
        assert!(dark_in(&top, 30..170, 58..63) > 200, "Linie bei 30 mm");
        assert!(
            dark_in(&top, 20..110, 18..32) > 20,
            "TOPOLOGIEKERN bei 10 mm"
        );
        assert_eq!(dark_in(&top, 290..400, 0..100), 0, "rechts oben ist leer");
        assert!(
            dark_in(&bottom, 295..395, 56..74) > 20,
            "PLANKOPFRAND bei 80 mm"
        );
        assert_eq!(dark_in(&bottom, 0..200, 0..100), 0, "links unten ist leer");
    }

    #[test]
    fn pixel_limits() {
        let area = PdfArea {
            left: 0.0,
            top: 0.0,
            width: 10.0,
            height: 1000.0,
        };
        let (w, h) = pixel_size(area, 2000);
        assert!(
            h <= MAX_SIDE && u64::from(w) * u64::from(h) <= MAX_PIXELS,
            "{w} × {h}"
        );
        let wide = PdfArea {
            width: 1000.0,
            height: 1000.0,
            ..area
        };
        let (w, h) = pixel_size(wide, 16_000);
        assert!(u64::from(w) * u64::from(h) <= MAX_PIXELS, "{w} × {h}");
    }
}
