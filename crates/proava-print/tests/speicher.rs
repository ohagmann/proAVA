//! Speicher beim Druck großer LVs (eigenes Testprogramm: der zählende
//! Allokator misst nur diesen Test).
//!
//! Bis 18.11.2026 wuchs der Speicher beim Satz mit Übertrag quadratisch –
//! jede Kopf- und Fußzeile ging alle Übertrags-Marken mit ihren Seiten durch;
//! 2000 Positionen brauchten über 16 GB. Der Normallauf prüft, dass der
//! Speicher linear wächst und die Schätzung vor dem Druck nicht weit daneben
//! liegt; `lang_2000_positionen_unter_drei_gb` (`#[ignore]`) misst ein LV von
//! 2000 Positionen (Release: rund 2 GB, 7 s).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

use proava_print::{
    estimate_lv_print, render_lv_pdf_counted, LvPrintMode, PrintHeader, PrintItem, PrintLv,
    PrintOptions, PrintPosition, PrintTitle, TextBlock, TextRun,
};
use rust_decimal::Decimal;

struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let now =
            LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed) + layout.size() as isize;
        PEAK.fetch_max(now, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let delta = new_size as isize - layout.size() as isize;
        let now = LIVE.fetch_add(delta, Ordering::Relaxed) + delta;
        PEAK.fetch_max(now, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const WORDS: [&str; 12] = [
    "Lieferung",
    "Montage",
    "Regler",
    "Fühler",
    "Ventil",
    "Schaltschrank",
    "Datenpunkt",
    "Parametrierung",
    "Prüfung",
    "Dokumentation",
    "inklusive",
    "Funktionsprüfung",
];

fn words(seed: usize, count: usize) -> String {
    (0..count)
        .map(|index| WORDS[(seed * 7 + index * 5) % WORDS.len()])
        .collect::<Vec<_>>()
        .join(" ")
        + "."
}

/// Titles with subtitles of 25 positions; long texts of one or two paragraphs,
/// a list and every tenth a table (about four positions per page).
fn big_lv(positions: usize) -> PrintLv {
    let mut titles = Vec::new();
    let mut number = 0;
    'outer: for title in 1..=99 {
        let mut subtitles = Vec::new();
        for sub in 1..=10 {
            let mut items = Vec::new();
            for index in 1..=25 {
                let mut long_text = vec![TextBlock::paragraph(vec![
                    TextRun::bold(words(number, 3)),
                    TextRun::plain(format!(" {}", words(number + 1, 15 + number % 25))),
                ])];
                if number % 2 == 0 {
                    long_text.push(TextBlock::text(words(number + 2, 20)));
                }
                long_text.push(TextBlock::bullet(words(number + 3, 6)));
                if number % 10 == 0 {
                    long_text.push(TextBlock::table(
                        true,
                        (0..5)
                            .map(|row| (0..4).map(|column| words(row + column, 2)).collect())
                            .collect(),
                    ));
                }
                items.push(PrintItem::Position(PrintPosition {
                    oz: format!("{title:02}.{sub:02}.{index:04}"),
                    short_text: words(number, 5),
                    long_text,
                    quantity: Some(Decimal::from(1 + number % 200)),
                    unit: "St".into(),
                    unit_price: Some(Decimal::new(1000 + number as i64 * 37, 2)),
                    in_total: true,
                    ..PrintPosition::default()
                }));
                number += 1;
                if number == positions {
                    subtitles.push(PrintItem::Title(PrintTitle {
                        lump: false,
                        oz: format!("{title:02}.{sub:02}"),
                        label: String::new(),
                        text: words(sub, 2),
                        total: None,
                        items,
                    }));
                    titles.push(PrintItem::Title(PrintTitle {
                        lump: false,
                        oz: format!("{title:02}"),
                        label: String::new(),
                        text: words(title, 2),
                        total: None,
                        items: subtitles,
                    }));
                    break 'outer;
                }
            }
            subtitles.push(PrintItem::Title(PrintTitle {
                lump: false,
                oz: format!("{title:02}.{sub:02}"),
                label: String::new(),
                text: words(sub, 2),
                total: None,
                items,
            }));
        }
        titles.push(PrintItem::Title(PrintTitle {
            lump: false,
            oz: format!("{title:02}"),
            label: String::new(),
            text: words(title, 2),
            total: None,
            items: subtitles,
        }));
    }
    PrintLv {
        header: PrintHeader {
            project_name: "Speichertest".into(),
            lv_name: "LV 480".into(),
            date: "18.11.2026".into(),
            ..PrintHeader::default()
        },
        items: titles,
        vat_percent: Decimal::from(19),
        ..PrintLv::default()
    }
}

/// The options the print dialog starts with: Inhaltsverzeichnis and Übertrag.
fn dialog_options() -> PrintOptions {
    PrintOptions {
        contents: true,
        carry: true,
        ..PrintOptions::default()
    }
}

/// Pages and peak memory (MB) of the printout.
fn measure(positions: usize) -> (usize, f64) {
    let lv = big_lv(positions);
    // The cache of the printout before goes on a helper thread: wait for it.
    proava_print::release_cache_now();
    let start = LIVE.load(Ordering::Relaxed);
    PEAK.store(start, Ordering::Relaxed);
    let (pdf, pages) = render_lv_pdf_counted(&lv, LvPrintMode::Priced, &dialog_options()).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let peak = PEAK.load(Ordering::Relaxed) - start;
    (pages, peak as f64 / 1_048_576.0)
}

/// Linear, not quadratic: twice the positions take about twice the memory
/// (300 → 600 positions: 1,6 times; the quadratic layout took 3,4 times).
/// And the estimate before the printout is near the measured pages and peak.
#[test]
fn memory_grows_linearly_with_the_positions_and_the_estimate_is_near() {
    // Fonts and the Typst library load once; not part of the comparison.
    measure(5);
    let (small_pages, small) = measure(300);
    let (big_pages, big) = measure(600);
    let ratio = big / small;
    assert!(
        ratio < 2.5,
        "600 statt 300 Positionen: {big:.0} MB statt {small:.0} MB (Faktor {ratio:.2}), {small_pages} → {big_pages} Seiten"
    );
    let estimate = estimate_lv_print(&big_lv(600), &dialog_options());
    let pages = estimate.pages as f64 / big_pages as f64;
    assert!(
        (0.6..1.7).contains(&pages),
        "geschätzt {} Seiten, gedruckt {big_pages}",
        estimate.pages
    );
    let peak = estimate.peak_bytes as f64 / 1_048_576.0;
    assert!(
        peak > big,
        "Schätzung {peak:.0} MB unter der gemessenen Spitze {big:.0} MB"
    );
}

/// Teurer Lauf (Release): 2000 Positionen mit Übertrag und Inhaltsverzeichnis.
#[test]
#[ignore]
fn lang_2000_positionen_unter_drei_gb() {
    let started = std::time::Instant::now();
    let (pages, peak) = measure(2000);
    let estimate = estimate_lv_print(&big_lv(2000), &dialog_options());
    eprintln!(
        "2000 Positionen: {pages} Seiten, Spitze {peak:.0} MB, {:.1} s; geschätzt {} Seiten, {:.0} MB",
        started.elapsed().as_secs_f64(),
        estimate.pages,
        estimate.peak_bytes as f64 / 1_048_576.0
    );
    assert!(peak < 3072.0, "Spitze {peak:.0} MB");
}
