//! Speicher und Zeit beim PDF-Druck großer LVs (synthetisch, keine Kundendaten).
//!
//! ```sh
//! systemd-run --user --scope -q -p MemoryMax=16G -p TasksMax=256 nice -n 10 \
//!     cargo run --release -p proava-exchange --example druck_speicher -- <variante> <dokument> [pdf]
//! ```
//!
//! Ein Druck je Aufruf (frischer Prozess: Typst merkt sich Satzergebnisse im
//! Prozess). Nie mehrere große Drucke gleichzeitig.
//!
//! Varianten (je 2000 Positionen, PROAVA_DRUCK_POSITIONEN ändert die Zahl):
//! - `titel`: 8 Titel mit je 10 Untertiteln à 25 Positionen,
//! - `viele`: 99 Titel mit je 10 Untertiteln à 2 Positionen,
//! - `gross`: ein Untertitel mit 1000 Positionen, dann wie `titel`,
//! - `umbruch`: wie `titel`, aber jeder Langtext ein Absatz mit harten
//!   Zeilenumbrüchen (zwei Drittel Leerzeilen; 30, jede 10. Position 105,
//!   jede 100. 720 Umbrüche).
//!
//! `PROAVA_DRUCK_PROJEKT=<datei.proava>` misst stattdessen ein vorhandenes
//! Projekt (nur lokal; solche Dateien nie ins Repo).
//!
//! Jede Position hat einen Langtext (3–5 Absätze, Aufzählung), jede 10. eine
//! Tabelle, jede 200. ein Bild (10 Bilder à rund 300 KB), jede 7. einen
//! Bezug auf eine von 50 Ausführungsbeschreibungen, jede 5. eine
//! Bieterangabe („Fabrikat“) im Langtext. 5 Angebote, eine
//! Kostenberechnung mit 400 Anlagenzeilen.
//!
//! Dokumente: `aufforderung` (LV ohne Preise), `preise` (LV mit Preisen),
//! `kurztext` (Kurztext-LV), `preisspiegel`, `kosten` – mit den
//! Vorgaben des Druckdialogs (Inhaltsverzeichnis, Übertrag). Mit `pdf` als
//! drittem Argument wird das PDF nach `$PROAVA_DRUCK_ZIEL` (Standard /tmp)
//! geschrieben, für den Bildvergleich vorher/nachher.
//!
//! Umgebung: `PROAVA_DRUCK_OHNE=uebertrag,inhalt,langtext` schaltet
//! Druckoptionen ab, `PROAVA_DRUCK_MIT=titelseite,titelkopf` „Neuer Titel
//! auf neuer Seite“ bzw. `{titel}` in der Kopfzeile an;
//! `PROAVA_DRUCK_SCHRIFT=8…12` die Schriftgröße,
//! `PROAVA_DRUCK_BILD_JEDE=n` setzt dasselbe Bild in jede n-te Position,
//! `PROAVA_DRUCK_WIEDERHOLEN=n` druckt n-mal im selben Prozess.
//!
//! Ausgabe: die Schätzung vor dem Druck (LV mit Langtexten), eine Zeile je
//! Stufe (Daten, Satz, PDF) mit Zeit und Spitzenspeicher in dieser Stufe
//! (zählender Allokator), dazu Seiten, PDF-Größe, die Höchstmarke des
//! Prozesses (VmHWM) und was nach dem Druck belegt bleibt.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use proava_application::{PrintJob, PrintKind};
use proava_domain::{
    Address, Bid, BidPrice, ComplementKind, CostEstimate, CostSection, CostStage, Decimal, Lv,
    LvGroup, LvRow, OzMask, PerformanceDescription, PointRow, Position, Project, ProjectName,
    RichText, Run, Table, TextAnchor, TextBlock, TextComplement,
};

// ---------------------------------------------------------------- Speicher

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

fn mb(bytes: isize) -> f64 {
    bytes as f64 / 1_048_576.0
}

/// Höchstmarke des Prozesses in MB (VmHWM aus /proc/self/status).
fn high_water_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .and_then(|line| line.split_whitespace().nth(1)?.parse::<f64>().ok())
        })
        .map_or(0.0, |kb| kb / 1024.0)
}

// ---------------------------------------------------------------- Projekt

/// Small deterministic random numbers (no extra crate).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const WORDS: &[&str] = &[
    "Lieferung",
    "Montage",
    "Inbetriebnahme",
    "Regler",
    "Fühler",
    "Ventil",
    "Stellantrieb",
    "Schaltschrank",
    "Kabel",
    "Leitung",
    "Datenpunkt",
    "Software",
    "Parametrierung",
    "Prüfung",
    "Dokumentation",
    "gemäß",
    "Planung",
    "Anlage",
    "Lüftung",
    "Heizung",
    "Kälte",
    "BACnet",
    "Modbus",
    "inklusive",
    "Zubehör",
    "Befestigung",
    "Kennzeichnung",
    "Funktionsprüfung",
];

fn sentence(rng: &mut Rng, words: usize) -> String {
    let mut text = String::new();
    for index in 0..words {
        if index > 0 {
            text.push(' ');
        }
        text.push_str(WORDS[rng.below(WORDS.len() as u64) as usize]);
    }
    text.push('.');
    text
}

fn long_text(rng: &mut Rng, number: usize, image: Option<&str>) -> RichText {
    let mut blocks = Vec::new();
    // About a quarter page per position (2000 positions ≈ 500 pages).
    for _ in 0..1 + rng.below(2) {
        let words = 15 + rng.below(25) as usize;
        blocks.push(TextBlock::paragraph(vec![
            Run::bold(sentence(rng, 3)),
            Run::plain(format!(" {}", sentence(rng, words))),
        ]));
    }
    for _ in 0..rng.below(3) {
        blocks.push(TextBlock::list_item(
            false,
            vec![Run::plain(sentence(rng, 6))],
        ));
    }
    if number.is_multiple_of(10) {
        let rows = (0..5)
            .map(|row| {
                (0..4)
                    .map(|column| {
                        if row == 0 {
                            format!("Spalte {}", column + 1)
                        } else {
                            sentence(rng, 2)
                        }
                    })
                    .collect()
            })
            .collect();
        let mut table = Table::from_texts(rows);
        table.header_rows = 1;
        blocks.push(TextBlock::Table(table));
    }
    if let Some(file) = image {
        blocks.push(TextBlock::Image {
            file: file.to_string(),
            width_mm: Some(60.0),
        });
    }
    RichText::new(blocks)
}

/// Variant `umbruch`: the long text as one paragraph whose lines are hard line
/// breaks, two thirds of them empty (as long texts from other programs come
/// in): 30 breaks, every 10th position 105, every 100th 720.
fn broken_text(rng: &mut Rng, number: usize) -> RichText {
    let breaks = if number % 100 == 50 {
        720
    } else if number.is_multiple_of(10) {
        105
    } else {
        30
    };
    let lines: Vec<String> = (0..=breaks)
        .map(|_| {
            if rng.below(3) == 0 {
                let words = 2 + rng.below(7) as usize;
                sentence(rng, words)
            } else {
                String::new()
            }
        })
        .collect();
    RichText::new(vec![TextBlock::paragraph(vec![Run::plain(
        lines.join("\n"),
    )])])
}

/// Noise PNG of about 300 KB.
fn image_bytes(seed: u64) -> Vec<u8> {
    let mut rng = Rng(seed | 1);
    let size = 330u32;
    let pixels: Vec<u8> = (0..size * size * 3).map(|_| rng.next() as u8).collect();
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, size, size);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
    }
    out
}

/// Titles: (number of top titles, subtitles each, positions each); `gross`
/// puts 1000 positions under the first subtitle.
fn build_lv(variant: &str, positions: usize, images: &[String], rng: &mut Rng) -> Lv {
    let mut lv = Lv::new("LV Gebäudeautomation", OzMask::parse("11.22.PPPP").unwrap());
    let mut performance_ids = Vec::new();
    for number in 0..50 {
        let id = lv.push_row(LvRow::Performance(PerformanceDescription::new(
            (number + 1).to_string(),
            format!("Ausführung {}", number + 1),
            long_text(rng, number, None),
        )));
        performance_ids.push(id);
    }
    let (per_title, per_sub) = match variant {
        "viele" => (10, 2),
        _ => (10, 25),
    };
    let mut count = 0;
    'outer: for title_number in 0..99 {
        let mut title = LvGroup::new(
            0,
            "",
            format!("Titel {} – {}", title_number + 1, sentence(rng, 2)),
        );
        title.set_cost_group(Some(format!("48{}", 1 + title_number % 9)));
        lv.push_group(title);
        for sub in 0..per_title {
            lv.push_group(LvGroup::new(
                1,
                "",
                format!("Untertitel {} – {}", sub + 1, sentence(rng, 2)),
            ));
            let here = if variant == "gross" && title_number == 0 && sub == 0 {
                1000
            } else {
                per_sub
            };
            for _ in 0..here {
                let mut position = Position::new(
                    "",
                    sentence(rng, 5),
                    Decimal::from(1 + rng.below(200)),
                    ["Stk", "m", "psch", "h"][rng.below(4) as usize],
                    Some(Decimal::new(100 + rng.below(500_000) as i64, 2)),
                );
                // PROAVA_DRUCK_BILD_JEDE=n: the same image in every n-th position.
                let image = match std::env::var("PROAVA_DRUCK_BILD_JEDE")
                    .ok()
                    .and_then(|n| n.parse::<usize>().ok())
                {
                    Some(every) => (count % every == 3).then(|| images[0].as_str()),
                    None => (count % 200 == 7 && !images.is_empty())
                        .then(|| images[(count / 200) % images.len()].as_str()),
                };
                let text = if variant == "umbruch" {
                    broken_text(rng, count)
                } else {
                    long_text(rng, count, image)
                };
                let first = text.blocks().len();
                position.set_long_text(text);
                if count % 7 == 0 {
                    position.set_performance(Some(performance_ids[count % performance_ids.len()]));
                }
                if count % 5 == 0 {
                    // Bieterangabe at the end of the long text, on a line of its own.
                    position.set_complements(vec![TextComplement {
                        marker: "01".into(),
                        kind: ComplementKind::Bidder,
                        caption: "Fabrikat:".into(),
                        body: String::new(),
                        tail: String::new(),
                        anchor: Some(TextAnchor {
                            block: first,
                            offset: 0,
                            own_line: true,
                        }),
                    }]);
                }
                lv.push_position(position);
                count += 1;
                if count == positions {
                    break 'outer;
                }
            }
        }
    }
    lv.renumber();
    lv
}

fn build_project(variant: &str, positions: usize) -> Project {
    let mut rng = Rng(0x5eed_1234);
    let mut project = Project::new(
        ProjectName::parse("Druckspeicher").unwrap(),
        "2026-11-18T18:00:00",
        Lv::new("leer", OzMask::parse("11.22.PPPP").unwrap()),
    );
    let images: Vec<String> = (0..10)
        .map(|number| {
            project.add_image(&format!("bild{number}.png"), image_bytes(number as u64 + 7))
        })
        .collect();
    *project.lv_mut() = build_lv(variant, positions, &images, &mut rng);
    let ids: Vec<_> = project
        .lv()
        .positions()
        .map(|position| (position.id(), position.unit_price(), position.quantity()))
        .collect();
    for bidder in 0..5 {
        let mut bid = Bid::new(
            Address::named(format!("Bieter {}", bidder + 1)),
            format!("bieter{bidder}.X84"),
        );
        for (index, (id, price, quantity)) in ids.iter().enumerate() {
            if (index + bidder) % 97 == 0 {
                continue;
            }
            let factor = Decimal::new(85 + rng.below(40) as i64, 2);
            let unit = (price.unwrap_or_default() * factor).round_dp(2);
            bid.set_price(
                *id,
                BidPrice {
                    unit_price: Some(unit),
                    stated_total: Some((unit * quantity).round_dp(2)),
                    not_offered: false,
                },
            );
        }
        project.add_bid(bid);
    }
    let mut estimate = CostEstimate::template(CostStage::Calculation, "18.11.2026", "Planer");
    estimate.sections.clear();
    for section in 0..4 {
        let mut part = CostSection::new(&format!("Bauteil {}", section + 1));
        for row in 0..100 {
            let mut line = PointRow::new("RLT", &format!("Anlage {}-{}", section + 1, row + 1));
            line.location = format!("ASP{:02}", row % 20 + 1);
            line.counts = (0..6).map(|_| rng.below(40) as u32).collect();
            part.rows.push(line);
        }
        estimate.sections.push(part);
    }
    project.add_cost_estimate(estimate);
    project
}

// ---------------------------------------------------------------- Messen

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let variant = args.first().map(String::as_str).unwrap_or("titel");
    let document = args.get(1).map(String::as_str).unwrap_or("aufforderung");
    let write = args.get(2).is_some_and(|arg| arg == "pdf");
    let positions: usize = std::env::var("PROAVA_DRUCK_POSITIONEN")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2000);
    let kind = match document {
        "aufforderung" => PrintKind::RequestForQuotation,
        "preise" => PrintKind::PricedLv,
        "kurztext" => PrintKind::ShortTextLv,
        "preisspiegel" => PrintKind::PriceComparison,
        "kosten" => PrintKind::CostEstimate,
        other => panic!("unbekanntes Dokument „{other}“"),
    };

    // PROAVA_DRUCK_PROJEKT=<datei.proava>: an existing project instead of the
    // generated one (local measurements only; such files never go into the repo).
    let project = match std::env::var("PROAVA_DRUCK_PROJEKT") {
        Ok(file) => proava_infrastructure::project_from_bytes(
            &std::fs::read(&file).expect("Projektdatei lesen"),
        )
        .expect("Projektdatei"),
        Err(_) => build_project(variant, positions),
    };
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    println!(
        "{variant}/{document}: {positions} Positionen, Projekt {:.0} MB",
        mb(base)
    );

    let mut job = PrintJob::new(kind);
    // PROAVA_DRUCK_OHNE=uebertrag,inhalt / PROAVA_DRUCK_MIT=titelseite: options of the dialog.
    let off = std::env::var("PROAVA_DRUCK_OHNE").unwrap_or_default();
    job.settings.carry &= !off.contains("uebertrag");
    job.settings.contents &= !off.contains("inhalt");
    job.settings.long_texts &= !off.contains("langtext");
    // PROAVA_DRUCK_SCHRIFT=8…12: font size of the printout.
    if let Some(size) = std::env::var("PROAVA_DRUCK_SCHRIFT")
        .ok()
        .and_then(|size| size.parse().ok())
    {
        job.settings.font_size_pt = size;
    }
    let on = std::env::var("PROAVA_DRUCK_MIT").unwrap_or_default();
    job.settings.title_page_break |= on.contains("titelseite");
    if on.contains("titelkopf") {
        // {titel} in the page header (the current title on every page).
        job.frame.header[0] = "{projekt}\n{titel}".into();
    }
    println!(
        "  Übertrag {}, Inhaltsverzeichnis {}, Langtexte {}, Titel auf neuer Seite {}",
        job.settings.carry,
        job.settings.contents,
        job.settings.long_texts,
        job.settings.title_page_break
    );
    if matches!(kind, PrintKind::RequestForQuotation | PrintKind::PricedLv)
        && job.settings.long_texts
    {
        // The estimate the printout is checked with (same options as the dialog).
        let mode = if kind == PrintKind::RequestForQuotation {
            proava_print::LvPrintMode::Blank
        } else {
            proava_print::LvPrintMode::Priced
        };
        let mut model = proava_exchange::lv_to_print(&project, mode, "18.11.2026");
        model.header.frame.header = job.frame.header.clone();
        let options = proava_print::PrintOptions {
            carry: job.settings.carry,
            contents: job.settings.contents,
            title_page_break: job.settings.title_page_break,
            ..proava_print::PrintOptions::default()
        };
        let estimate = proava_print::estimate_lv_print(&model, &options);
        println!(
            "  Schätzung: {} Seiten, Spitze {:.0} MB; frei {:.0} MB",
            estimate.pages,
            estimate.peak_bytes as f64 / 1_048_576.0,
            proava_exchange::memory::available_memory().unwrap_or(0) as f64 / 1_048_576.0
        );
    }
    // The estimate is not part of the measurement.
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
    let stage_start = std::cell::Cell::new(Instant::now());
    let total = Instant::now();
    proava_print::observe_stages(Some(Box::new(move |stage| {
        let peak = PEAK.swap(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
        let name = match stage {
            proava_print::Stage::Data => "Daten",
            proava_print::Stage::Layout => "Satz",
            proava_print::Stage::Pdf => "PDF",
        };
        println!(
            "  {name:<6} {:>7.1} s   Spitze {:>7.0} MB   danach {:>6.0} MB",
            stage_start.get().elapsed().as_secs_f64(),
            mb(peak - base),
            mb(LIVE.load(Ordering::Relaxed) - base)
        );
        stage_start.set(Instant::now());
    })));
    // PROAVA_DRUCK_WIEDERHOLEN=n: print n times in this process (what stays after each).
    let repeat: usize = std::env::var("PROAVA_DRUCK_WIEDERHOLEN")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    for _ in 1..repeat {
        let (bytes, _) =
            proava_exchange::render_print(&project, &job, "18.11.2026").expect("Druck");
        proava_print::release_cache_now();
        println!(
            "  nach einem Druck gehalten {:.0} MB",
            mb(LIVE.load(Ordering::Relaxed) - base - bytes.len() as isize)
        );
    }
    let (bytes, outcome) =
        proava_exchange::render_print(&project, &job, "18.11.2026").expect("Druck");
    let elapsed = total.elapsed();
    // Typst's cache is freed on a helper thread: wait for it.
    proava_print::release_cache_now();
    println!(
        "  gesamt {:>7.1} s   {} Seiten, PDF {:.1} MB, Höchstmarke Prozess {:.0} MB, danach gehalten {:.0} MB",
        elapsed.as_secs_f64(),
        outcome.pages,
        bytes.len() as f64 / 1_048_576.0,
        high_water_mb(),
        mb(LIVE.load(Ordering::Relaxed) - base - bytes.len() as isize)
    );
    if write {
        let folder = std::env::var("PROAVA_DRUCK_ZIEL").unwrap_or_else(|_| "/tmp".into());
        let file = std::path::Path::new(&folder).join(format!("{variant}_{document}.pdf"));
        std::fs::write(&file, &bytes).unwrap();
        println!("  geschrieben: {}", file.display());
    }
}
