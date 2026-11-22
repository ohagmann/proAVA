//! Leistungsmessung mit einem großen synthetischen Projekt (keine Kundendaten).
//!
//! `cargo run --release -p proava-ui --example perf`
//!
//! Projekt: 3 LVs (2000 + 500 + 500 Positionen) mit formatierten Langtexten
//! (Absätze, Aufzählungen, Tabellen), 50 Ausführungsbeschreibungen, 10 Bilder
//! à ~300 KB, 10 Angebote auf dem ersten LV. Gemessen werden Dateien, GAEB,
//! Preisspiegel, PDFs, die Bildzeit der Oberfläche (egui_kittest, ohne
//! Bildschirm, nur CPU: Aufbau, Layout und Tessellierung wie eframe, ohne
//! AccessKit und ohne GPU-Zeichnen) und der Speicher
//! je Rückgängig-Schritt (zählender Allokator).
//!
//! Alle LVs stehen untereinander in der LV-Tabelle (mit Kopfzeile je LV).
//!
//! Umgebung: PROAVA_PERF_POSITIONS (Standard 2000) für das erste LV,
//! PROAVA_PERF_POSITIONS_LV2 (Standard 500) für das zweite,
//! PROAVA_PERF_FRAMES (Standard 30) Bilder je Ablauf, PROAVA_PERF_ONLY=pdf
//! endet nach den PDFs, PROAVA_PERF_ONLY=pdfstufen misst nur die Stufen des
//! PDF-Drucks, PROAVA_PERF_SKIP=pdf lässt die PDFs aus,
//! PROAVA_PERF_REPORT=<datei> schreibt die Tabelle.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use proava_application::{
    ComparisonPrint, ExchangeFiles, GaebExport, Launcher, PdfDocument, ProjectRepository,
};
use proava_domain::{
    Address, Bid, BidPrice, ComparisonOptions, Decimal, InvitedBidder, Lv, LvGroup, LvRow, OzMask,
    PerformanceDescription, Position, PriceComparison, Project, ProjectName, RichText, Run, Table,
    TextBlock,
};
use proava_exchange::GaebPdfExchange;
use proava_infrastructure::{DiskFolders, JsonSettingsFile, XmlProjectFile};
use proava_ui::{App, StartFile};

// ---------------------------------------------------------------- Speicher

struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_add(
            new_size as isize - layout.size() as isize,
            Ordering::Relaxed,
        );
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn live_bytes() -> isize {
    LIVE.load(Ordering::Relaxed)
}

fn mb(bytes: isize) -> String {
    format!("{:.2} MB", bytes as f64 / 1_048_576.0)
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
    for _ in 0..3 + rng.below(3) {
        let words = 25 + rng.below(30) as usize;
        blocks.push(TextBlock::paragraph(vec![
            Run::bold(sentence(rng, 3)),
            Run::plain(format!(" {}", sentence(rng, words))),
        ]));
    }
    for _ in 0..3 {
        blocks.push(TextBlock::list_item(
            false,
            vec![Run::plain(sentence(rng, 8))],
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

/// A very long text (about 20 pages): 200 paragraphs, lists and 5 tables.
fn very_long_text(rng: &mut Rng) -> RichText {
    let mut blocks = Vec::new();
    for number in 0..200 {
        blocks.extend(
            long_text(rng, if number % 40 == 0 { 0 } else { 1 }, None)
                .blocks()
                .iter()
                .take(2)
                .cloned(),
        );
    }
    RichText::new(blocks)
}

/// Noise PNG of about 300 KB.
fn image_bytes(seed: u64) -> Vec<u8> {
    let mut rng = Rng(seed | 1);
    let size = 330;
    let pixels: Vec<u8> = (0..size * size * 3).map(|_| rng.next() as u8).collect();
    let image = image::RgbImage::from_raw(size, size, pixels).unwrap();
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn build_lv(
    title: &str,
    positions: usize,
    performances: usize,
    images: &[String],
    rng: &mut Rng,
) -> Lv {
    let mut lv = Lv::new(title, OzMask::parse("11.22.PPPP").unwrap());
    let mut performance_ids = Vec::new();
    for number in 0..performances {
        let id = lv.push_row(LvRow::Performance(PerformanceDescription::new(
            (number + 1).to_string(),
            format!("Ausführung {}", number + 1),
            long_text(rng, number, None),
        )));
        performance_ids.push(id);
    }
    let per_title = 10;
    let per_sub = 25;
    let mut count = 0;
    'outer: for title_number in 0.. {
        let mut title = LvGroup::new(
            0,
            "",
            format!("Titel {} – {}", title_number + 1, sentence(rng, 2)),
        );
        // Kostengruppen 481 … 489 an den Titeln.
        title.set_cost_group(Some(format!("48{}", 1 + title_number % 9)));
        lv.push_group(title);
        for sub in 0..per_title {
            lv.push_group(LvGroup::new(
                1,
                "",
                format!("Untertitel {} – {}", sub + 1, sentence(rng, 2)),
            ));
            for _ in 0..per_sub {
                let mut position = Position::new(
                    "",
                    sentence(rng, 5),
                    Decimal::from(1 + rng.below(200)),
                    ["Stk", "m", "psch", "h"][rng.below(4) as usize],
                    Some(Decimal::new(100 + rng.below(500_000) as i64, 2)),
                );
                let image = (count % 200 == 7 && !images.is_empty())
                    .then(|| images[(count / 200) % images.len()].as_str());
                position.set_long_text(long_text(rng, count, image));
                if !performance_ids.is_empty() && count % 7 == 0 {
                    position.set_performance(Some(performance_ids[count % performance_ids.len()]));
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

fn build_project(positions: usize) -> Project {
    let mut rng = Rng(0x5eed_1234);
    let mut project = Project::new(
        ProjectName::parse("Leistungstest").unwrap(),
        "2026-11-17T18:00:00",
        Lv::new("leer", OzMask::parse("11.22.PPPP").unwrap()),
    );
    let images: Vec<String> = (0..10)
        .map(|number| {
            project.add_image(&format!("bild{number}.png"), image_bytes(number as u64 + 7))
        })
        .collect();
    *project.lv_mut() = build_lv("LV 1 Gebäudeautomation", positions, 50, &images, &mut rng);
    // The first position carries a very long text (Langtext-Editor).
    let first = project
        .lv()
        .rows()
        .iter()
        .position(LvRow::is_position)
        .unwrap();
    let text = very_long_text(&mut rng);
    if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(first) {
        position.set_long_text(text);
    }
    let second: usize = std::env::var("PROAVA_PERF_POSITIONS_LV2")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(500);
    project.add_lv(build_lv("LV 2 Heizung", second, 0, &[], &mut rng));
    project.add_lv(build_lv("LV 3 Lüftung", 500, 0, &[], &mut rng));
    project.set_active_lv(0);

    let ids: Vec<_> = project
        .lv()
        .positions()
        .map(|position| (position.id(), position.unit_price(), position.quantity()))
        .collect();
    for bidder in 0..10 {
        let mut bid = Bid::new(
            Address::named(format!("Bieter {}", bidder + 1)),
            format!("bieter{bidder}.X84"),
        );
        for (index, (id, price, quantity)) in ids.iter().enumerate() {
            if (index + bidder) % 97 == 0 {
                continue;
            }
            let factor = Decimal::new(85 + rng.below(40) as i64, 2);
            let unit = price.unwrap_or_default() * factor;
            let unit = unit.round_dp(2);
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
        // Bieterverzeichnis für den Schriftverkehr.
        project.add_invitee(InvitedBidder::new(Address::named(format!(
            "Bieter {}",
            bidder + 1
        ))));
    }
    project
}

// ---------------------------------------------------------------- Messen

struct Report {
    rows: Vec<(String, String, String)>,
}

impl Report {
    fn add(&mut self, what: &str, value: String, note: impl Into<String>) {
        let note = note.into();
        println!("{what:<48} {value:>14}  {note}");
        self.rows.push((what.to_string(), value, note));
    }
}

fn ms(duration: Duration) -> String {
    format!("{:.1} ms", duration.as_secs_f64() * 1000.0)
}

/// CPU time of the whole process (user + system, all threads) from
/// /proc/self/stat; less disturbed by other work on the machine than the
/// wall time. Zero where /proc is missing.
fn cpu_time() -> Duration {
    let ticks = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|stat| {
            let rest = stat.rsplit_once(')')?.1.to_string();
            let fields: Vec<&str> = rest.split_whitespace().collect();
            Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
        })
        .unwrap_or(0);
    // CLK_TCK is 100 on Linux.
    Duration::from_millis(ticks * 10)
}

/// Median of `runs` runs.
fn time<T>(runs: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut times = Vec::new();
    let mut last = None;
    for _ in 0..runs {
        let start = Instant::now();
        last = Some(f());
        times.push(start.elapsed());
    }
    times.sort();
    (times[times.len() / 2], last.unwrap())
}

struct Frames {
    median: Duration,
    p95: Duration,
    max: Duration,
}

impl Frames {
    fn show(&self) -> String {
        format!(
            "{} (p95 {}, max {})",
            ms(self.median),
            ms(self.p95),
            ms(self.max)
        )
    }
}

type UiHarness = Harness<'static, App>;

/// Runs `frames` frames with the events from `input` (all events of one
/// call in one frame) and times them like eframe: `Context::run_ui` plus
/// tessellation, without AccessKit (the program builds that tree only for
/// a screen reader). PROAVA_PERF_ACCESSKIT=1 times kittest steps instead
/// (with AccessKit, one step per event).
fn frames(
    harness: &mut UiHarness,
    frames: usize,
    input: impl FnMut(usize) -> Vec<egui::Event>,
) -> Frames {
    frames_with(harness, frames, |_| {}, input)
}

/// Like [`frames`], with `prepare` (untimed kittest steps) before each frame.
fn frames_with(
    harness: &mut UiHarness,
    frames: usize,
    mut prepare: impl FnMut(&mut UiHarness),
    mut input: impl FnMut(usize) -> Vec<egui::Event>,
) -> Frames {
    let with_accesskit = std::env::var("PROAVA_PERF_ACCESSKIT").is_ok_and(|value| value == "1");
    let mut times = Vec::new();
    for index in 0..frames {
        prepare(harness);
        let events = input(index);
        if with_accesskit {
            for event in events {
                harness.event(event);
            }
            let start = Instant::now();
            harness.step();
            times.push(start.elapsed());
            continue;
        }
        let ctx = harness.ctx.clone();
        let mut raw = harness.input_mut().take();
        raw.events = events;
        raw.predicted_dt = 1.0 / 60.0;
        ctx.disable_accesskit();
        let start = Instant::now();
        // Same wrapping as the kittest steps (scope + central panel frame), so
        // widget ids and focus stay the same between both kinds of frames.
        let output = ctx.run_ui(raw, |ui| {
            ui.scope_builder(egui::UiBuilder::new(), |ui| {
                egui::Frame::central_panel(ui.style())
                    .outer_margin(8.0)
                    .inner_margin(0.0)
                    .show(ui, |ui| harness.state_mut().ui(ui));
            });
        });
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        times.push(start.elapsed());
        drop(primitives);
        ctx.enable_accesskit();
    }
    // Bring the AccessKit tree of the harness up to date.
    harness.step();
    times.sort();
    Frames {
        median: times[times.len() / 2],
        p95: times[(times.len() * 95 / 100).min(times.len() - 1)],
        max: *times.last().unwrap(),
    }
}

fn key_events(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    [true, false]
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .to_vec()
}

fn key(harness: &mut UiHarness, key: egui::Key, modifiers: egui::Modifiers) {
    for event in key_events(key, modifiers) {
        harness.event(event);
    }
}

fn wheel_events(at: egui::Pos2, dy: f32) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(at),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, dy),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

#[allow(clippy::arc_with_non_send_sync)]
fn launcher(folder: &Path) -> Arc<Launcher> {
    Arc::new(
        Launcher::new(
            Box::new(XmlProjectFile::new()),
            Box::new(JsonSettingsFile::at(folder.join("settings.json"))),
            Box::new(DiskFolders::new()),
        )
        .with_exchange(Box::new(GaebPdfExchange)),
    )
}

/// Opens a menu of the menu bar (exact label) and settles.
fn open_menu(harness: &mut UiHarness, label: &str) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Button && node.label().as_deref() == Some(label)
        })
        .next()
        .unwrap_or_else(|| panic!("Menü „{label}“ fehlt"))
        .click();
    harness.run_steps(3);
}

fn click_label(harness: &mut UiHarness, starts: &str) {
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.starts_with(starts)))
        .next()
        .unwrap_or_else(|| panic!("„{starts}“ fehlt"))
        .click();
    harness.run_steps(3);
}

/// Home, then down to the first position of the LV.
fn select_first_position(harness: &mut UiHarness) {
    key(harness, egui::Key::Home, egui::Modifiers::NONE);
    harness.run_steps(2);
    for _ in 0..200 {
        let workspace = harness.state().workspace().unwrap();
        let index = workspace.selected_index().unwrap_or(0);
        if workspace.project().lv().rows()[index].is_position() {
            break;
        }
        key(harness, egui::Key::ArrowDown, egui::Modifiers::NONE);
        harness.step();
    }
    harness.run_steps(2);
}

fn main() {
    let positions: usize = std::env::var("PROAVA_PERF_POSITIONS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(2000);
    let count: usize = std::env::var("PROAVA_PERF_FRAMES")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(30);
    let folder = std::env::temp_dir().join(format!("proava-perf-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let mut report = Report { rows: Vec::new() };
    let release = !cfg!(debug_assertions);
    println!(
        "proAVA Leistungsmessung ({} Build), Ordner {}",
        if release { "Release" } else { "Debug" },
        folder.display()
    );

    let (took, project) = time(1, || build_project(positions));
    let total: usize = project.lvs().map(Lv::position_count).sum();
    report.add(
        "Projekt erzeugen (synthetisch)",
        ms(took),
        format!(
            "{} LVs, {total} Positionen, {} Bilder, 10 Angebote",
            project.lvs().count(),
            project.images().len()
        ),
    );

    // Dateien
    let file = folder.join("Leistungstest.proava");
    let repository = XmlProjectFile::new();
    let (took, _) = time(3, || repository.save_project(&file, &project).unwrap());
    let size = std::fs::metadata(&file).map(|meta| meta.len()).unwrap_or(0);
    report.add(
        "Projektdatei speichern",
        ms(took),
        format!("{:.1} MB", size as f64 / 1_048_576.0),
    );
    let (took, loaded) = time(3, || repository.load_project(&file).unwrap());
    report.add("Projektdatei laden", ms(took), "");
    assert_eq!(loaded.lv().position_count(), project.lv().position_count());
    drop(loaded);

    // GAEB
    let exchange = GaebPdfExchange;
    let x83 = folder.join("LV.X83");
    let (took, _) = time(3, || {
        exchange
            .export_gaeb(&project, GaebExport::RequestForQuotation, &x83)
            .unwrap()
    });
    report.add("GAEB-Export X83 (LV 1)", ms(took), "");
    let x82 = folder.join("LV.X82");
    let (took, _) = time(3, || {
        exchange
            .export_gaeb(&project, GaebExport::PricedLv, &x82)
            .unwrap()
    });
    report.add("GAEB-Export X82 (LV 1)", ms(took), "");
    let x84 = folder.join("Bieter.X84");
    {
        let (mut doc, _) = proava_exchange::lv_to_gaeb(&project, proava_gaeb::Phase::Dp82);
        doc.phase = proava_gaeb::Phase::Dp84;
        std::fs::write(
            &x84,
            proava_gaeb::xml::write(&doc, proava_gaeb::Phase::Dp84).unwrap(),
        )
        .unwrap();
    }
    let (took, import) = time(3, || exchange.import_bid(&project, &x84).unwrap());
    report.add(
        "Angebot X84 einlesen und zuordnen",
        ms(took),
        format!("{} Preise", import.bid.prices().len()),
    );

    // Preisspiegel
    let (took, comparison) = time(5, || {
        PriceComparison::build(project.lv(), project.bids(), ComparisonOptions::default())
    });
    report.add(
        "PriceComparison::build (10 Angebote)",
        ms(took),
        format!("{} Zeilen", comparison.lines.len()),
    );

    // Kostengruppen, Rechtschreibung (Wörterbuch des Rechners, falls vorhanden).
    let (took, kg) = time(5, || {
        proava_domain::cost_group_report(project.lv(), project.bids())
    });
    report.add(
        "Kosten nach Kostengruppen berechnen",
        ms(took),
        format!("{} Zeilen", kg.lines.len()),
    );
    let (took, speller) = time(1, || proava_spell::Speller::find_and_load(None, &[]));
    match speller {
        Ok(speller) => {
            report.add(
                "Wörterbuch laden (beim Öffnen, im Hintergrund)",
                ms(took),
                "",
            );
            let (took, _) = time(1, || {
                proava_ui::spell_dialog::SpellDialog::new(&project, &speller)
            });
            report.add("F7 Rechtschreibung alle LVs, erstes Mal", ms(took), "");
            let (took, _) = time(3, || {
                proava_ui::spell_dialog::SpellDialog::new(&project, &speller)
            });
            report.add(
                "F7 Rechtschreibung alle LVs, danach",
                ms(took),
                "Wörter schon geprüft",
            );
        }
        Err(error) => report.add("Wörterbuch laden", "–".into(), format!("{error}")),
    }

    let only = std::env::var("PROAVA_PERF_ONLY").unwrap_or_default();
    // PDF (PROAVA_PERF_SKIP=pdf lässt sie aus, für schnelle Vergleichsläufe)
    if std::env::var("PROAVA_PERF_SKIP").unwrap_or_default() != "pdf" && only != "pdfstufen" {
        let pdf = folder.join("LV.pdf");
        let (took, _) = time(1, || {
            exchange
                .export_pdf(&project, PdfDocument::PricedLv, &pdf)
                .unwrap()
        });
        let pages = std::fs::metadata(&pdf).map(|meta| meta.len()).unwrap_or(0);
        report.add(
            "PDF bepreistes LV",
            ms(took),
            format!("{:.1} MB", pages as f64 / 1_048_576.0),
        );
        let pdf = folder.join("Preisspiegel.pdf");
        let (took, _) = time(1, || {
            exchange
                .export_price_comparison_pdf(&project, &ComparisonPrint::default(), &pdf)
                .unwrap()
        });
        report.add("PDF Preisspiegel", ms(took), "");
    }
    // PROAVA_PERF_ONLY=pdfstufen: die Stufen einzeln, in frischem Prozess
    // (Typst merkt sich Satzergebnisse im Prozess; danach ist der Satz fast gratis).
    if only == "pdfstufen" {
        // PROAVA_PERF_DOC=lv oder ps: nur eines der beiden Dokumente.
        use proava_print::{LvPrintMode, PrintOptions};
        let which = std::env::var("PROAVA_PERF_DOC").unwrap_or_default();
        let stage =
            |report: &mut Report, what: &str, cpu: Duration, wall: Duration, note: String| {
                report.add(what, ms(wall), format!("CPU {}{note}", ms(cpu)));
            };
        if which != "ps" {
            let cpu = cpu_time();
            let (took, print) = time(1, || {
                proava_exchange::lv_to_print(&project, LvPrintMode::Priced, "17.11.2026")
            });
            stage(
                &mut report,
                "bepreistes LV: Druckdaten",
                cpu_time() - cpu,
                took,
                String::new(),
            );
            let cpu = cpu_time();
            let (took, pages) = time(1, || {
                proava_print::lv_page_count(&print, LvPrintMode::Priced, &PrintOptions::default())
                    .unwrap()
            });
            stage(
                &mut report,
                "bepreistes LV: Typst-Satz",
                cpu_time() - cpu,
                took,
                format!(", {pages} Seiten"),
            );
            let cpu = cpu_time();
            let (took, _) = time(1, || {
                proava_print::render_lv_pdf_with(
                    &print,
                    LvPrintMode::Priced,
                    &PrintOptions::default(),
                )
                .unwrap()
            });
            stage(
                &mut report,
                "bepreistes LV: PDF schreiben (Satz gemerkt)",
                cpu_time() - cpu,
                took,
                String::new(),
            );
        }
        if which != "lv" {
            let cpu = cpu_time();
            let (took, print) = time(1, || {
                proava_exchange::price_comparison_to_print(&project, "17.11.2026")
            });
            stage(
                &mut report,
                "Preisspiegel: Druckdaten",
                cpu_time() - cpu,
                took,
                String::new(),
            );
            let cpu = cpu_time();
            let (took, pages) = time(1, || {
                proava_print::price_comparison_page_count(&print).unwrap()
            });
            stage(
                &mut report,
                "Preisspiegel: Typst-Satz",
                cpu_time() - cpu,
                took,
                format!(", {pages} Seiten"),
            );
            let cpu = cpu_time();
            let (took, _) = time(1, || {
                proava_print::render_price_comparison_pdf(&print).unwrap()
            });
            stage(
                &mut report,
                "Preisspiegel: PDF schreiben (Satz gemerkt)",
                cpu_time() - cpu,
                took,
                String::new(),
            );
        }
        let _ = std::fs::remove_dir_all(&folder);
        return;
    }

    if only == "pdf" {
        let _ = std::fs::remove_dir_all(&folder);
        return;
    }

    // Rückgängig: Schnappschuss eines LV
    let before = live_bytes();
    let copies: Vec<Lv> = (0..5).map(|_| project.lv().clone()).collect();
    let per_copy = (live_bytes() - before) / 5;
    drop(copies);
    report.add(
        "Speicher je LV-Kopie (Rückgängig-Schnappschuss)",
        mb(per_copy),
        format!("{positions} Positionen"),
    );

    // Oberfläche
    let mut harness: UiHarness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .with_context_setup(App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, app: &mut App| app.ui(ui),
            App::new(launcher(&folder), Some(StartFile::Project(file.clone()))),
        );
    let start = Instant::now();
    harness.run_steps(5);
    report.add(
        "Oberfläche: Projekt öffnen (5 Bilder)",
        ms(start.elapsed()),
        "",
    );
    assert!(
        harness.state().project().is_some(),
        "Projekt nicht geöffnet"
    );
    let table = egui::pos2(500.0, 500.0);

    let idle = frames(&mut harness, count, |_| Vec::new());
    report.add("Bild LV-Tabelle, ruhig", idle.show(), "");
    let scroll = frames(&mut harness, count, |_| wheel_events(table, -300.0));
    report.add("Bild LV-Tabelle, scrollen", scroll.show(), "300 pt je Bild");
    // Ende des ersten LV, dann über die Kopfzeile ins zweite LV scrollen
    // (Zeilen zweier LVs im Bild, Summen des zweiten aus dem Zwischenspeicher).
    key(&mut harness, egui::Key::End, egui::Modifiers::NONE);
    harness.run_steps(3);
    let across = frames(&mut harness, count, |_| wheel_events(table, -60.0));
    report.add(
        "Bild LV-Tabelle, scrollen über die LV-Grenze",
        across.show(),
        "60 pt je Bild, Kopfzeile LV 2",
    );
    // ↓ am Ende des LV: das nächste LV wird das aktive (Undo-Stapel,
    // Ansicht, Fenster wechseln mit).
    let change = frames(&mut harness, count, |frame| {
        let key = if frame % 2 == 0 {
            egui::Key::ArrowDown
        } else {
            egui::Key::ArrowUp
        };
        key_events(key, egui::Modifiers::NONE)
    });
    report.add(
        "Bild LV-Tabelle, Wechsel des aktiven LV (↓/↑ an der Grenze)",
        change.show(),
        "",
    );
    key(&mut harness, egui::Key::Home, egui::Modifiers::NONE);
    harness.run_steps(3);
    let select = frames(&mut harness, count, |_| {
        key_events(egui::Key::ArrowDown, egui::Modifiers::NONE)
    });
    report.add(
        "Bild LV-Tabelle, Auswahl ändern (↓)",
        select.show(),
        "mit Langtext im Eigenschaftenfenster",
    );
    // Erste Position (sehr langer Langtext) wählen.
    select_first_position(&mut harness);
    let long_idle = frames(&mut harness, count, |_| Vec::new());
    report.add(
        "Bild LV-Tabelle, Position mit 20 Seiten Langtext",
        long_idle.show(),
        "Langtext im Eigenschaftenfenster",
    );
    // Zweite Position wählen und in den Kurztext tippen.
    key(&mut harness, egui::Key::ArrowDown, egui::Modifiers::NONE);
    harness.run_steps(2);
    key(&mut harness, egui::Key::F2, egui::Modifiers::NONE);
    harness.run_steps(3);
    let selected = harness
        .state()
        .workspace()
        .unwrap()
        .selected_index()
        .expect("Auswahl");
    let row_id = harness.state().project().unwrap().lv().rows()[selected].id();
    let short = |h: &UiHarness| {
        h.state()
            .project()
            .unwrap()
            .lv()
            .rows()
            .iter()
            .find(|row| row.id() == row_id)
            .unwrap()
            .short_text()
    };
    let typing = frames(&mut harness, count, |_| vec![egui::Event::Text("x".into())]);
    report.add("Bild LV-Tabelle, tippen in eine Zelle", typing.show(), "");
    key(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    harness.run_steps(3);
    assert!(
        short(&harness).ends_with(&"x".repeat(count)),
        "Tippen kam nicht an: {}",
        short(&harness)
    );

    // Rückgängig-Speicher über die Oberfläche: Kurztext n-mal ändern.
    let steps = 10;
    let before = live_bytes();
    for step in 0..steps {
        key(&mut harness, egui::Key::F2, egui::Modifiers::NONE);
        harness.run_steps(3);
        harness.event(egui::Event::Text(format!("{step}")));
        harness.run_steps(3);
        key(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
        harness.run_steps(3);
    }
    let per_step = (live_bytes() - before) / steps as isize;
    let changed = harness
        .state()
        .project()
        .unwrap()
        .lv()
        .rows()
        .iter()
        .filter(|row| {
            row.short_text().len() == 1 && row.short_text().chars().all(|c| c.is_ascii_digit())
        })
        .count();
    assert_eq!(changed, steps, "Änderungen kamen nicht an");
    report.add(
        "Rückgängig: Speicher je Schritt (Oberfläche)",
        mb(per_step),
        format!("{steps} Kurztext-Änderungen"),
    );

    // Automatische Sicherung: Bild, in dem sie fällig wird (das LV hat
    // ungesicherte Änderungen). Zwischen den Bildern Zeit vorspulen und dem
    // Schreiben Zeit lassen.
    let autosave = frames_with(
        &mut harness,
        5,
        |h| {
            std::thread::sleep(Duration::from_millis(600));
            h.step();
            let now = h.ctx.input(|input| input.time);
            h.input_mut().time = Some(now + 600.0);
        },
        |_| Vec::new(),
    );
    harness.input_mut().time = None;
    std::thread::sleep(Duration::from_millis(600));
    harness.run_steps(3);
    report.add(
        "Bild mit fälliger automatischer Sicherung",
        autosave.show(),
        "Projekt 4,3 MB",
    );

    // Neue Position (Strg+Enter) und Rückgängig (Strg+Z) im großen LV.
    let row_count = |h: &UiHarness| h.state().project().unwrap().lv().rows().len();
    let rows_before = row_count(&harness);
    let escape = |h: &mut UiHarness| {
        key(h, egui::Key::Escape, egui::Modifiers::NONE);
        h.run_steps(3);
    };
    let inserts = 10;
    let insert = frames_with(&mut harness, inserts, escape, |_| {
        key_events(egui::Key::Enter, egui::Modifiers::COMMAND)
    });
    assert_eq!(
        row_count(&harness),
        rows_before + inserts,
        "Einfügen kam nicht an"
    );
    report.add(
        "Bild neue Position (Strg+Enter)",
        insert.show(),
        "LV fortlaufend nummeriert",
    );
    let undo = frames_with(&mut harness, inserts, escape, |_| {
        key_events(egui::Key::Z, egui::Modifiers::COMMAND)
    });
    assert_eq!(row_count(&harness), rows_before, "Rückgängig kam nicht an");
    report.add("Bild Rückgängig (Strg+Z)", undo.show(), "");

    // Langtext-Fenster (Umschalt+F2) mit dem sehr langen Text: ruhig und tippen.
    select_first_position(&mut harness);
    let long_row = {
        let workspace = harness.state().workspace().unwrap();
        workspace.project().lv().rows()[workspace.selected_index().unwrap()].id()
    };
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F2);
    harness.run_steps(5);
    let long = frames(&mut harness, count, |_| Vec::new());
    report.add("Bild Langtext-Fenster (20 Seiten), ruhig", long.show(), "");
    let long_typing = frames(&mut harness, count, |_| vec![egui::Event::Text("y".into())]);
    report.add(
        "Bild Langtext-Fenster (20 Seiten), tippen",
        long_typing.show(),
        "",
    );
    let long_plain = |h: &UiHarness| match h.state().project().unwrap().lv().row_by_id(long_row) {
        Some(LvRow::Position(position)) => position.long_text().to_plain(),
        _ => String::new(),
    };
    if !long_plain(&harness).contains(&"y".repeat(count)) {
        let lv = harness.state().project().unwrap().lv();
        let hits: Vec<String> = lv
            .rows()
            .iter()
            .filter(|row| row.short_text().contains("yy"))
            .map(|row| row.short_text().to_string())
            .collect();
        panic!("Tippen im Langtext kam nicht an; Kurztexte mit yy: {hits:?}");
    }
    key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    harness.run_steps(3);

    // Fenster „Kosten nach Kostengruppen“ (Menü Prüfen).
    open_menu(&mut harness, "LV");
    click_label(&mut harness, "Kosten nach Kostengruppen");
    let kg_window = frames(&mut harness, count, |_| Vec::new());
    report.add(
        "Bild mit Fenster Kosten nach Kostengruppen",
        kg_window.show(),
        "",
    );
    click_label(&mut harness, "Schließen");

    // Angebote, Preisspiegel
    click_label(&mut harness, "Angebote (");
    let bids = frames(&mut harness, count, |_| Vec::new());
    report.add("Bild Angebote, ruhig", bids.show(), "");
    click_label(&mut harness, "Preisspiegel");
    let comparison_idle = frames(&mut harness, count, |_| Vec::new());
    report.add("Bild Preisspiegel, ruhig", comparison_idle.show(), "");
    let comparison_scroll = frames(&mut harness, count, |_| wheel_events(table, -300.0));
    report.add("Bild Preisspiegel, scrollen", comparison_scroll.show(), "");

    // Schriftverkehr (Menü Angebote), 10 angefragte Bieter.
    open_menu(&mut harness, "Angebote");
    click_label(&mut harness, "Schriftverkehr");
    let letters = frames(&mut harness, count, |_| Vec::new());
    report.add(
        "Bild Schriftverkehr",
        letters.show(),
        "10 angefragte Bieter",
    );

    let _ = std::fs::remove_dir_all(&folder);
    if let Ok(target) = std::env::var("PROAVA_PERF_REPORT") {
        let mut text = String::from("| Messung | Ergebnis | Hinweis |\n|---|---:|---|\n");
        for (what, value, note) in &report.rows {
            text.push_str(&format!("| {what} | {value} | {note} |\n"));
        }
        std::fs::write(PathBuf::from(target), text).unwrap();
    }
}
