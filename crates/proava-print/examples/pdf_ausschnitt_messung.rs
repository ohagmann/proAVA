//! Messung „Ausschnitt aus PDF einfügen“ mit einer echten PDF (nur lesen):
//! Seitengrößen, Rasterzeit der Vorschau bei verschiedenen Breiten, PNG für
//! GAEB (300 dpi), Herauslösen einer Seite, Ausschnitt als Vektor-PDF.
//!
//! `cargo run --release -p proava-print --example pdf_ausschnitt_messung -- <datei.pdf> [ausgabeordner] [links oben rechts unten]`
//!
//! Gemessen wird ohne Anteile die linke Hälfte der ersten Seite ohne die
//! unteren 15 % (typisch: Zeichnung ohne Plankopf). Mit Ausgabeordner werden die Bilder
//! dort abgelegt (nie in das Repo legen). Startet keine Prozesse.

use std::time::Instant;

use proava_print::{pdf_area, pdf_pages, pdf_single_page, render_pdf_area, PdfArea};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(file) = args.next() else {
        eprintln!("Aufruf: pdf_ausschnitt_messung <datei.pdf> [ausgabeordner]");
        return;
    };
    let out = args.next().map(std::path::PathBuf::from);
    let shares: Vec<f32> = args.filter_map(|value| value.parse().ok()).collect();
    let pdf = std::fs::read(&file).expect("PDF lesbar");
    println!("Datei: {file} ({} Bytes)", pdf.len());

    let start = Instant::now();
    let pages = pdf_pages(&pdf).expect("Seiten");
    println!(
        "Seiten: {} – Seite 1 {:.0} × {:.0} pt ({:?})",
        pages.len(),
        pages[0].0,
        pages[0].1,
        start.elapsed()
    );

    let start = Instant::now();
    let single = pdf_single_page(&pdf, 1).expect("Seite 1");
    println!(
        "Seite 1 allein: {} Bytes ({:?})",
        single.len(),
        start.elapsed()
    );

    let (w, h) = pages[0];
    let area = match shares.as_slice() {
        [left, top, right, bottom] => PdfArea {
            left: left * w,
            top: top * h,
            width: (right - left) * w,
            height: (bottom - top) * h,
        },
        _ => PdfArea {
            left: 0.0,
            top: 0.0,
            width: w / 2.0,
            height: h * 0.85,
        },
    };
    let start = Instant::now();
    let cut = pdf_area(&pdf, 1, area).expect("Ausschnitt");
    println!(
        "Ausschnitt als Vektor-PDF: {} Bytes ({:?})",
        cut.len(),
        start.elapsed()
    );
    if let Some(out) = &out {
        std::fs::write(out.join("ausschnitt.pdf"), &cut).expect("schreiben");
    }

    for width in [400u32, 800, 1600, 2400] {
        let start = Instant::now();
        let pixels = render_pdf_area(&pdf, 1, area, width).expect("rastern");
        let took = start.elapsed();
        println!(
            "Vorschau {width} px: {} × {} in {took:?}",
            pixels.width, pixels.height
        );
        if let Some(out) = &out {
            if width == 1600 {
                std::fs::write(out.join("vorschau_1600.png"), pixels.to_png().expect("png"))
                    .expect("schreiben");
            }
        }
    }
    // GAEB: 150 mm im Langtext bei 300 dpi.
    let px = (150.0 / 25.4 * 300.0_f32).round() as u32;
    let start = Instant::now();
    let pixels = render_pdf_area(&pdf, 1, area, px).expect("rastern");
    let png = pixels.to_png().expect("png");
    println!(
        "GAEB-PNG 150 mm / 300 dpi: {} × {}, {} Bytes in {:?}",
        pixels.width,
        pixels.height,
        png.len(),
        start.elapsed()
    );
    if let Some(out) = &out {
        std::fs::write(out.join("gaeb_300dpi.png"), &png).expect("schreiben");
    }
}
