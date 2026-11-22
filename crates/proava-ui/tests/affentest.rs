//! Affentest: zufällige, aber gültige Bedienung der ganzen Oberfläche ohne
//! Bildschirm (egui_kittest) – Tasten, Tippen mit Umlauten, Klicks,
//! Doppelklicks und Rechtsklicks auf zufällige Bedienelemente (über die
//! AccessKit-Rollen, nicht über Pixel), Klicks an zufällige Stellen, Menüs.
//!
//! Nach jedem Schritt: kein Panic (auch nicht in Hintergrund-Threads), die
//! Projekt-Invarianten aus `common::invariants` gelten. Am Ende einer Folge
//! im Arbeitsbereich: Rückgängig bis zum Anfang ergibt das Ausgangsprojekt.
//!
//! Alles läuft in einer Testablage: Ordner, Projekte, Einstellungen, Exporte;
//! keine Zwischenablage des Systems, kein PDF-Programm, kein Browser, kein
//! zweites proAVA (Sperre in `jobs.rs`, nachgewiesen durch
//! `nothing_is_started_from_the_test`). Feste Saat; längere Läufe `lang_…`
//! (`#[ignore]`) und `scripts/eigenschaften.sh`, siehe docs/BUILD.md.

mod common;

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, Once};

use common::invariants::check_project;
use common::{Key, Modifiers, TempDir};
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use proava_application::{Folders, Launcher, ProjectRepository};
use proava_domain::{Project, ProjectName};
use proava_exchange::GaebPdfExchange;
use proava_infrastructure::{DiskFolders, JsonSettingsFile, XmlProjectFile};
use proava_ui::{App, StartFile};
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};

type UiHarness = Harness<'static, App>;

fn config(default_cases: u32) -> Config {
    let cases = std::env::var("PROAVA_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default_cases);
    let seed = std::env::var("PROAVA_PROPTEST_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0x0A7A_2026);
    Config {
        cases,
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        // A case is a whole session: shrinking stays affordable.
        max_shrink_iters: 300,
        max_shrink_time: 120_000,
        ..Config::default()
    }
}

// ---------- Sandbox ----------

/// Only `root` and below: the file dialogs never leave the test folder.
struct ConfinedFolders {
    root: PathBuf,
    disk: DiskFolders,
}

/// `path` with `.` and `..` resolved without touching the disk.
fn lexical(path: &Path) -> PathBuf {
    let mut parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}

impl ConfinedFolders {
    fn inside(&self, path: &Path) -> bool {
        lexical(path).starts_with(&self.root)
    }
}

impl Folders for ConfinedFolders {
    fn home(&self) -> PathBuf {
        self.root.clone()
    }
    fn subfolders(&self, folder: &Path) -> Vec<PathBuf> {
        if self.inside(folder) {
            self.disk.subfolders(folder)
        } else {
            Vec::new()
        }
    }
    fn is_folder(&self, folder: &Path) -> bool {
        self.inside(folder) && self.disk.is_folder(folder)
    }
    fn is_file(&self, file: &Path) -> bool {
        self.inside(file) && file.is_file()
    }
    fn create_folder(&self, parent: &Path, name: &str) -> Result<PathBuf, String> {
        if self.inside(parent) {
            self.disk.create_folder(parent, name)
        } else {
            Err("außerhalb der Testablage".to_string())
        }
    }
    fn files(&self, folder: &Path) -> Vec<PathBuf> {
        if self.inside(folder) {
            self.disk.files(folder)
        } else {
            Vec::new()
        }
    }
    fn read_file(&self, file: &Path) -> Result<Vec<u8>, String> {
        if self.inside(file) {
            self.disk.read_file(file)
        } else {
            Err("Testablage: außerhalb gesperrt".to_string())
        }
    }
}

/// Project files: writing only inside `root`.
struct ConfinedProjects {
    root: PathBuf,
    files: XmlProjectFile,
}

impl ConfinedProjects {
    fn check(&self, path: &Path) -> Result<(), String> {
        if lexical(path).starts_with(&self.root) {
            Ok(())
        } else {
            Err(format!("Testablage: {} liegt außerhalb", path.display()))
        }
    }
}

impl ProjectRepository for ConfinedProjects {
    fn create_project(&self, folder: &Path, name: &ProjectName) -> Result<PathBuf, String> {
        self.check(folder)?;
        self.files.create_project(folder, name)
    }
    fn load_project(&self, file: &Path) -> Result<Project, String> {
        self.files.load_project(file)
    }
    fn save_project(&self, file: &Path, project: &Project) -> Result<(), String> {
        self.check(file)?;
        self.files.save_project(file, project)
    }
    fn backup(&self, project_file: &Path, target_folder: &Path) -> Result<PathBuf, String> {
        self.check(target_folder)?;
        self.files.backup(project_file, target_folder)
    }
    fn restore(&self, backup_file: &Path, target_folder: &Path) -> Result<PathBuf, String> {
        self.check(target_folder)?;
        self.files.restore(backup_file, target_folder)
    }
    fn modified_iso(&self, file: &Path) -> Option<String> {
        self.files.modified_iso(file)
    }
    fn remove_file(&self, file: &Path) -> Result<(), String> {
        self.check(file)?;
        self.files.remove_file(file)
    }
}

struct Sandbox {
    // Removed at the end (after the harness, see field order in `Session`).
    folder: TempDir,
    root: PathBuf,
    project_file: PathBuf,
}

/// A small, valid PNG for „Bild einfügen“.
fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    let image = image::RgbImage::from_fn(4, 3, |x, y| {
        image::Rgb([(x * 60) as u8, (y * 80) as u8, 128])
    });
    image::DynamicImage::ImageRgb8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

/// The PDF for clips and filed letters (compiled once per test program).
fn sample_pdf() -> Vec<u8> {
    static PDF: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    PDF.get_or_init(proava_print::sample_vector_pdf).clone()
}

/// Everything the newer parts of the program work on: the full demo
/// (`demo::full_demo_project`: Kostenschätzung and Kostenberechnung,
/// STLB-Bau positions with write-protected Langtext and unit, lists with
/// Unterpunkten, a second Ausschreibungs-LV with Pauschalsumme,
/// Zuschlagsposition and Schlussbemerkung, a Kosten-LV), an Abstandshalter
/// and an empty Hinweis in the LV, a filed letter (Schriftverkehr im
/// Projekt) and a PDF clip in the long text of the first position.
fn rich_project() -> Project {
    use proava_domain::{
        ClipRect, CorrespondenceEntry, CorrespondenceKind, LetterKind, LvRow, Note, PdfClip,
        Spacer, TextBlock,
    };
    let mut project = proava_ui::demo::full_demo_project();
    let lv = project.lv_mut();
    lv.insert_row(2, LvRow::Spacer(Spacer::new()));
    lv.insert_row(6, LvRow::Note(Note::new("")));
    // Sent to the bidders: OZ changes ask first, the LV shows „Seit dem
    // Versand geändert“ (qa3).
    lv.mark_sent("2026-11-18T09:00:00".into(), Some("Affentest.X83".into()));
    let pdf = sample_pdf();
    let mut letter = CorrespondenceEntry::new(
        CorrespondenceKind::Letter(LetterKind::Invitation),
        "18.11.2026",
        "Aufforderung zur Angebotsabgabe",
        pdf.clone(),
    );
    letter.recipient_name = "Müller GmbH".into();
    project.add_correspondence(letter);
    let file = project.add_pdf_clip(
        "plan.pdf",
        pdf,
        PdfClip::new(
            1,
            (566.93, 283.46),
            ClipRect::new(0.0, 0.0, 283.46, 283.46),
            "plan.pdf",
        ),
    );
    let first = project
        .lv()
        .rows()
        .iter()
        .position(LvRow::is_position)
        .unwrap();
    if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(first) {
        let mut text = position.long_text().clone();
        text.push(TextBlock::Image {
            file,
            width_mm: Some(120.0),
        });
        position.set_long_text(text);
    }
    project
}

impl Sandbox {
    /// The demo project (the regression tests below count on its rows).
    fn new() -> Self {
        Self::with(proava_ui::demo::demo_project())
    }

    /// The rich project for the tours (see [`rich_project`]) and the files
    /// the newer dialogs pick: a PDF for „Ausschnitt aus PDF einfügen“ and an
    /// LV as CSV.
    fn rich() -> Self {
        let sandbox = Self::with(rich_project());
        std::fs::write(sandbox.root.join("Plan.pdf"), sample_pdf()).unwrap();
        let lv = proava_ui::demo::demo_project().lv().clone();
        let csv =
            proava_exchange::csv_export::lv_to_csv(&lv, proava_application::CsvEncoding::Utf8Bom);
        std::fs::write(sandbox.root.join("LV Fremd.csv"), csv.bytes).unwrap();
        sandbox
    }

    /// Folder of the copies for the PDF viewer in this sandbox.
    fn copies(&self) -> PathBuf {
        self.root.join("pdf-anzeige")
    }

    fn with(project: Project) -> Self {
        let folder = TempDir::new("affe");
        let root = std::fs::canonicalize(&folder.0).unwrap();
        let project_file = root.join("Affentest.proava");
        XmlProjectFile::new()
            .save_project(&project_file, &project)
            .unwrap();
        // Files the dialogs can find: an image, a bid, a foreign LV.
        std::fs::write(root.join("Schrank.png"), png()).unwrap();
        let (mut bid, _) = proava_exchange::lv_to_gaeb(&project, proava_gaeb::Phase::Dp84);
        bid.phase = proava_gaeb::Phase::Dp84;
        std::fs::write(
            root.join("Angebot Müller.X84"),
            proava_gaeb::xml::write(&bid, proava_gaeb::Phase::Dp84).unwrap(),
        )
        .unwrap();
        let (lv, _) = proava_exchange::lv_to_gaeb(&project, proava_gaeb::Phase::Dp83);
        std::fs::write(
            root.join("Fremd-LV.X83"),
            proava_gaeb::xml::write(&lv, proava_gaeb::Phase::Dp83).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("Unterordner")).unwrap();
        Self {
            folder,
            root,
            project_file,
        }
    }

    // The UI takes an `Arc<Launcher>` like `main.rs`; tests stay on one thread.
    #[allow(clippy::arc_with_non_send_sync)]
    fn launcher(&self) -> Arc<Launcher> {
        Arc::new(
            Launcher::new(
                Box::new(ConfinedProjects {
                    root: self.root.clone(),
                    files: XmlProjectFile::new(),
                }),
                Box::new(JsonSettingsFile::at(self.root.join("settings.json"))),
                Box::new(ConfinedFolders {
                    root: self.root.clone(),
                    disk: DiskFolders::new(),
                }),
            )
            .with_exchange(Box::new(GaebPdfExchange)),
        )
    }

    fn harness(&self, start: Option<StartFile>) -> UiHarness {
        // Copies for the PDF viewer (Schriftverkehr ▸ Öffnen) in this
        // sandbox, never in the folder of the process: the tests run side by
        // side in one process (flaky test 18.11., see viewer_copies).
        proava_ui::viewer_copies::use_folder_on_this_thread(Some(self.copies()));
        let launcher = self.launcher();
        if start.is_none() {
            // The start screen lists the test project under „Zuletzt verwendet“.
            launcher.open_project(&self.project_file).unwrap();
        }
        let app = App::new(launcher, start);
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .with_context_setup(App::setup)
            .with_accessibility_check(false)
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
        harness.run_steps(4);
        harness
    }
}

// ---------- Panics in any thread ----------

static PANICS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Records panics of worker threads (unnamed, `jobs::Job`), which the UI
/// only shows as a lost job, and lets the standard hook print every panic.
/// Panics of a test thread (named after the test) fail its case anyway.
fn watch_panics() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let standard = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().name().is_none() {
                if let Ok(mut panics) = PANICS.lock() {
                    panics.push(info.to_string());
                }
            }
            standard(info);
        }));
    });
}

/// The step running now and the steps before it, for the watchdog.
static RUNNING: Mutex<Option<(std::time::Instant, String)>> = Mutex::new(None);

/// A step that does not end (endless loop in the UI) would only show as a
/// timeout: after two minutes the watchdog prints the steps and stops.
fn watch_hangs() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::thread::Builder::new()
            .name("affe-wache".into())
            .spawn(|| loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let stuck = RUNNING
                    .lock()
                    .ok()
                    .and_then(|running| running.clone())
                    .filter(|(started, _)| started.elapsed().as_secs() > 120);
                if let Some((_, trace)) = stuck {
                    eprintln!(
                        "AFFENTEST HÄNGT – ein Schritt läuft seit über zwei Minuten:\n{trace}"
                    );
                    std::process::exit(3);
                }
            })
            .unwrap();
    });
}

fn note_running(trace: Option<String>) {
    if let Ok(mut running) = RUNNING.lock() {
        *running = trace.map(|trace| (std::time::Instant::now(), trace));
    }
}

fn worker_panic() -> Option<String> {
    let mut panics = PANICS.lock().ok()?;
    let first = panics.first().cloned();
    panics.clear();
    first
}

// ---------- Steps ----------

/// Keys the monkey presses (with weights in [`step`]).
const KEYS: &[(Key, Modifiers)] = &[
    (Key::ArrowUp, Modifiers::NONE),
    (Key::ArrowDown, Modifiers::NONE),
    (Key::ArrowLeft, Modifiers::NONE),
    (Key::ArrowRight, Modifiers::NONE),
    (Key::Enter, Modifiers::NONE),
    (Key::Escape, Modifiers::NONE),
    (Key::Tab, Modifiers::NONE),
    (Key::Tab, Modifiers::SHIFT),
    (Key::F2, Modifiers::NONE),
    (Key::F2, Modifiers::SHIFT),
    (Key::F4, Modifiers::NONE),
    (Key::F7, Modifiers::NONE),
    (Key::Delete, Modifiers::NONE),
    (Key::Backspace, Modifiers::NONE),
    (Key::PageUp, Modifiers::NONE),
    (Key::PageDown, Modifiers::NONE),
    (Key::Home, Modifiers::NONE),
    (Key::End, Modifiers::NONE),
    (Key::Space, Modifiers::NONE),
    (Key::Z, Modifiers::COMMAND),
    (Key::Y, Modifiers::COMMAND),
    (Key::C, Modifiers::COMMAND),
    (Key::X, Modifiers::COMMAND),
    (Key::V, Modifiers::COMMAND),
    (Key::S, Modifiers::COMMAND),
    (Key::K, Modifiers::COMMAND),
    (Key::E, Modifiers::COMMAND),
    (Key::A, Modifiers::COMMAND),
    (Key::F, Modifiers::COMMAND),
    (Key::R, Modifiers::COMMAND),
    (Key::H, Modifiers::COMMAND),
    (Key::P, Modifiers::COMMAND),
    (Key::B, Modifiers::COMMAND),
    (Key::I, Modifiers::COMMAND),
    (Key::U, Modifiers::COMMAND),
    (Key::Num1, Modifiers::COMMAND),
    (Key::Num2, Modifiers::COMMAND),
    (Key::Num3, Modifiers::COMMAND),
    (Key::Enter, Modifiers::COMMAND),
    (Key::Enter, Modifiers::ALT),
    (
        Key::Enter,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    (
        Key::T,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    (
        Key::A,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    (
        Key::Z,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    (Key::ArrowUp, Modifiers::ALT),
    (Key::ArrowDown, Modifiers::ALT),
    (Key::ArrowLeft, Modifiers::ALT),
    (Key::ArrowRight, Modifiers::ALT),
    (Key::ArrowUp, Modifiers::SHIFT),
    (Key::ArrowDown, Modifiers::SHIFT),
    (Key::F1, Modifiers::NONE),
    // Newer parts (18.11.): Abstandshalter, Reiter Abrechnung/Nachträge/Kosten,
    // Bibliothek, Duplizieren, Weitersuchen, Einfg, Aufmaß, Langtextfenster.
    (Key::L, Modifiers::COMMAND),
    (Key::Num4, Modifiers::COMMAND),
    (Key::Num5, Modifiers::COMMAND),
    (Key::Num6, Modifiers::COMMAND),
    (Key::D, Modifiers::COMMAND),
    (Key::T, Modifiers::COMMAND),
    (Key::M, Modifiers::COMMAND),
    (Key::F3, Modifiers::NONE),
    (Key::F8, Modifiers::NONE),
    (Key::Insert, Modifiers::NONE),
    (Key::Insert, Modifiers::SHIFT),
    (Key::Enter, Modifiers::SHIFT),
    (Key::F2, Modifiers::ALT),
    (
        Key::E,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    (
        Key::F,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    (
        Key::P,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    (
        Key::H,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    (
        Key::O,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    // Funktionstasten für neue Zeilen (18.11.): F5 STLB-Bau, F6
    // AUSSCHREIBEN.DE (only the take-over dialog – starting a program is
    // refused in jobs.rs, see `nothing_is_started_from_the_test`), F9
    // Position (Umschalt: davor), F10 Hinweis (Strg: davor; Umschalt: Menü),
    // F11 Ausführungsbeschreibung, F12 Titel (Umschalt: Untertitel).
    (Key::F5, Modifiers::NONE),
    (Key::F6, Modifiers::NONE),
    (Key::F9, Modifiers::NONE),
    (Key::F9, Modifiers::SHIFT),
    (Key::F10, Modifiers::NONE),
    (Key::F10, Modifiers::COMMAND),
    (Key::F10, Modifiers::SHIFT),
    (Key::F11, Modifiers::NONE),
    (Key::F12, Modifiers::NONE),
    (Key::F12, Modifiers::SHIFT),
    // Bieterangabe / Angabe des Auftraggebers als Feld im Langtext.
    (
        Key::B,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
    (
        Key::G,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
    ),
];

/// What the monkey types: words with umlauts, numbers, special characters –
/// no `/`, `\`, `.` or `~`, so typed file names stay in the test folder.
const WORDS: &[&str] = &[
    "Wärme",
    "Lüftung",
    "Öl",
    "Straße",
    "ÄÖÜ",
    "ß",
    "12",
    "3,5",
    "0",
    "-7",
    " ",
    "x",
    "–",
    "„Typ A“",
    "€",
    "m²",
    "(1)",
    "%",
    ":",
    ";",
    "&",
    "<b>",
    "Fühler",
    "DDC",
    "a",
    "Ü",
    "1000",
    "99,99",
    "\n",
    // From Word or PDF: control characters that must not reach the project.
    "\u{b}",
    "\u{c}",
    "\r\n",
    "\u{1b}",
    "\t",
];

/// Labels that leave the open project (only on the free tour).
const LEAVING: &[&str] = &["Start", "Projekt schließen"];

#[derive(Debug, Clone)]
enum Step {
    Key(usize),
    /// Any key with any modifier combination.
    AnyKey(Index, u8),
    Type(Vec<usize>),
    /// Pasting text (Strg+V from the system clipboard).
    Paste(Vec<usize>),
    Click(Index),
    DoubleClick(Index),
    RightClick(Index),
    /// Somewhere in the window (per mille of width and height).
    ClickAt(u16, u16),
    /// An entry of the menu bar, then one of the entries that appeared.
    Menu(Index, Index),
    Scroll(Index, bool),
    /// The window's X (Rückfrage beim Schließen with unsaved changes).
    CloseWindow,
}

fn step() -> impl Strategy<Value = Step> {
    let i = || any::<Index>();
    prop_oneof![
        10 => (0..KEYS.len()).prop_map(Step::Key),
        1 => (i(), 0u8..6).prop_map(|(key, modifiers)| Step::AnyKey(key, modifiers)),
        4 => prop::collection::vec(0..WORDS.len(), 1..4).prop_map(Step::Type),
        1 => prop::collection::vec(0..WORDS.len(), 1..8).prop_map(Step::Paste),
        10 => i().prop_map(Step::Click),
        3 => i().prop_map(Step::DoubleClick),
        1 => i().prop_map(Step::RightClick),
        2 => (0u16..1000, 0u16..1000).prop_map(|(x, y)| Step::ClickAt(x, y)),
        4 => (i(), i()).prop_map(|(menu, entry)| Step::Menu(menu, entry)),
        1 => (i(), any::<bool>()).prop_map(|(at, down)| Step::Scroll(at, down)),
        1 => Just(Step::CloseWindow),
    ]
}

const ANY_KEYS: &[Key] = &[
    Key::A,
    Key::B,
    Key::C,
    Key::D,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::I,
    Key::J,
    Key::K,
    Key::L,
    Key::M,
    Key::N,
    Key::O,
    Key::P,
    Key::Q,
    Key::R,
    Key::S,
    Key::T,
    Key::U,
    Key::V,
    Key::W,
    Key::X,
    Key::Y,
    Key::Z,
    Key::Num0,
    Key::Num1,
    Key::Num4,
    Key::Num9,
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
    Key::F11,
    Key::F12,
    Key::Insert,
    Key::Delete,
    Key::Plus,
    Key::Minus,
    Key::Comma,
    Key::Period,
    Key::Space,
    Key::Enter,
    Key::Tab,
    Key::Escape,
    Key::Home,
    Key::End,
];

const MODIFIERS: [Modifiers; 6] = [
    Modifiers::NONE,
    Modifiers::SHIFT,
    Modifiers::COMMAND,
    Modifiers {
        shift: true,
        ..Modifiers::COMMAND
    },
    Modifiers::ALT,
    Modifiers {
        shift: true,
        ..Modifiers::ALT
    },
];

fn interactive(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::Tab
            | Role::CheckBox
            | Role::RadioButton
            | Role::ComboBox
            | Role::TextInput
            | Role::MultilineTextInput
            | Role::Row
            | Role::Cell
            | Role::ListItem
            | Role::MenuItem
            | Role::Link
            | Role::Slider
            | Role::SpinButton
    )
}

/// Interactive widgets on screen: (label, centre).
fn targets(harness: &UiHarness, stay: bool) -> Vec<(String, egui::Pos2)> {
    let size = egui::vec2(1400.0, 900.0);
    harness
        .query_all_by(|node| interactive(node.role()))
        .filter_map(|node| {
            let label = node.accesskit_node().label().unwrap_or_default();
            if stay && LEAVING.iter().any(|leaving| label.trim() == *leaving) {
                return None;
            }
            let center = node.rect().center();
            (center.x >= 0.0 && center.y >= 0.0 && center.x < size.x && center.y < size.y).then(
                || {
                    (
                        format!("{:?} „{}“", node.accesskit_node().role(), label.trim()),
                        center,
                    )
                },
            )
        })
        .collect()
}

/// Entries of the menu bar (buttons in the `Menu` container).
fn menu_entries(harness: &UiHarness) -> Vec<(String, egui::Pos2)> {
    harness
        .query_all_by(|node| {
            node.role() == Role::Button
                && node
                    .parent()
                    .is_some_and(|parent| parent.role() == Role::Menu)
        })
        .map(|node| {
            (
                node.accesskit_node()
                    .label()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                node.rect().center(),
            )
        })
        .collect()
}

fn settle(harness: &mut UiHarness) {
    harness.run_steps(4);
}

fn pointer_click(
    harness: &mut UiHarness,
    at: egui::Pos2,
    button: egui::PointerButton,
    times: usize,
) {
    harness.event(egui::Event::PointerMoved(at));
    harness.step();
    for _ in 0..times {
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: at,
                button,
                pressed,
                modifiers: Modifiers::NONE,
            });
            harness.step();
        }
    }
}

/// Runs one step; returns what was done (for the failure trace).
fn apply(harness: &mut UiHarness, step: &Step, stay: bool) -> String {
    let done = match step {
        Step::Key(index) => {
            let (key, modifiers) = KEYS[*index];
            harness.key_press_modifiers(modifiers, key);
            format!("Taste {}", proava_ui::shortcuts::key_text(key, modifiers))
        }
        Step::AnyKey(key, modifiers) => {
            let key = ANY_KEYS[key.index(ANY_KEYS.len())];
            let modifiers = MODIFIERS[*modifiers as usize];
            harness.key_press_modifiers(modifiers, key);
            format!("Taste {}", proava_ui::shortcuts::key_text(key, modifiers))
        }
        Step::Type(words) => {
            let text: String = words.iter().map(|word| WORDS[*word]).collect();
            harness.event(egui::Event::Text(text.clone()));
            format!("Tippen {text:?}")
        }
        Step::Paste(words) => {
            let text: String = words.iter().map(|word| WORDS[*word]).collect();
            harness.event(egui::Event::Paste(text.clone()));
            format!("Einfügen {text:?}")
        }
        Step::Click(at) | Step::DoubleClick(at) | Step::RightClick(at) => {
            let targets = targets(harness, stay);
            if targets.is_empty() {
                return "kein Ziel".to_string();
            }
            let (label, center) = targets[at.index(targets.len())].clone();
            let (button, times, what) = match step {
                Step::Click(_) => (egui::PointerButton::Primary, 1, "Klick"),
                Step::DoubleClick(_) => (egui::PointerButton::Primary, 2, "Doppelklick"),
                _ => (egui::PointerButton::Secondary, 1, "Rechtsklick"),
            };
            pointer_click(harness, center, button, times);
            format!("{what} {label}")
        }
        Step::ClickAt(x, y) => {
            let at = egui::pos2(*x as f32 * 1.4, *y as f32 * 0.9);
            // Not on something that leaves the project.
            if stay
                && harness
                    .query_all_by(|node| {
                        node.label()
                            .is_some_and(|label| LEAVING.contains(&label.trim()))
                    })
                    .any(|node| node.rect().contains(at))
            {
                return "Klick ausgelassen".to_string();
            }
            pointer_click(harness, at, egui::PointerButton::Primary, 1);
            format!("Klick bei {:.0},{:.0}", at.x, at.y)
        }
        Step::Menu(menu, entry) => {
            let menus = menu_entries(harness);
            if menus.is_empty() {
                return "keine Menüleiste".to_string();
            }
            let (name, center) = menus[menu.index(menus.len())].clone();
            let before: Vec<String> = targets(harness, stay)
                .into_iter()
                .map(|(label, _)| label)
                .collect();
            pointer_click(harness, center, egui::PointerButton::Primary, 1);
            settle(harness);
            let entries: Vec<(String, egui::Pos2)> = targets(harness, stay)
                .into_iter()
                .filter(|(label, _)| !before.contains(label))
                .collect();
            if entries.is_empty() {
                return format!("Menü „{name}“ ohne Einträge");
            }
            let (label, center) = entries[entry.index(entries.len())].clone();
            harness.event(egui::Event::PointerMoved(center));
            // Submenus open on hover.
            harness.run_steps(20);
            pointer_click(harness, center, egui::PointerButton::Primary, 1);
            format!("Menü „{name}“ ▸ {label}")
        }
        Step::Scroll(at, down) => {
            let targets = targets(harness, stay);
            if targets.is_empty() {
                return "kein Ziel".to_string();
            }
            let (label, center) = targets[at.index(targets.len())].clone();
            harness.event(egui::Event::PointerMoved(center));
            harness.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, if *down { -3.0 } else { 3.0 }),
                modifiers: Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            });
            format!("Rad {} bei {label}", if *down { "runter" } else { "hoch" })
        }
        Step::CloseWindow => {
            harness
                .input_mut()
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default()
                .events
                .push(egui::ViewportEvent::Close);
            harness.step();
            "Fenster schließen (X)".to_string()
        }
    };
    settle(harness);
    done
}

// ---------- Checks ----------

fn session(harness: &UiHarness) -> Option<u64> {
    harness
        .state()
        .workspace()
        .map(|workspace| workspace.session())
}

/// Waits until background work (export, import, PDF) is done.
fn wait_idle(harness: &mut UiHarness) {
    for _ in 0..600 {
        if !harness.state().busy() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.step();
    }
    settle(harness);
}

/// Closes dialogs, menus, help and cell editing, so Strg+Z reaches the
/// project history.
fn close_everything(harness: &mut UiHarness) {
    for _ in 0..8 {
        harness.key_press(Key::Escape);
        settle(harness);
    }
    for _ in 0..6 {
        let open = harness
            .state()
            .workspace()
            .is_some_and(|workspace| workspace.dialog_open());
        if !open {
            break;
        }
        let button = [
            "Abbrechen",
            "Schließen",
            "Verwerfen",
            "Nicht übernehmen",
            "Fertig",
        ]
        .into_iter()
        .find_map(|label| {
            harness
                .query_all_by(|node| {
                    node.role() == Role::Button
                        && node.label().is_some_and(|text| text.trim() == label)
                })
                .next()
                .map(|node| node.rect().center())
        });
        match button {
            Some(center) => pointer_click(harness, center, egui::PointerButton::Primary, 1),
            None => harness.key_press(Key::Escape),
        }
        settle(harness);
    }
}

/// The project for comparing: which LV is shown and unused images do not
/// count (undo does not switch views, images are tidied on saving), nor
/// the Versandstand (when the request for quotation went out, the state
/// and file the bidders got) – facts, not edits (the export happened; undo
/// does not forget it, `Lv::keep_sent_state_of`).
fn comparable(project: &Project, start: &Project) -> Project {
    let mut project = project.clone();
    project.set_active_lv(0);
    project.remove_unused_images();
    for index in 0..project.lv_count() {
        let Some(sent) = start.lv_at(index).cloned() else {
            continue;
        };
        if let Some(lv) = project.lv_at_mut(index) {
            lv.keep_sent_state_of(&sent);
        }
    }
    project
}

/// Strg+Z until nothing changes any more; then the start project must be back.
fn undo_to_start(harness: &mut UiHarness, start: &Project) -> Result<(), String> {
    wait_idle(harness);
    close_everything(harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Num1);
    settle(harness);
    let target = comparable(start, start);
    let mut unchanged = 0;
    for _ in 0..400 {
        let before = comparable(harness.state().project().ok_or("kein Projekt")?, start);
        if same_content(&before, &target) {
            return Ok(());
        }
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        settle(harness);
        let after = comparable(harness.state().project().ok_or("kein Projekt")?, start);
        if after == before {
            unchanged += 1;
            if unchanged >= 4 {
                break;
            }
        } else {
            unchanged = 0;
        }
    }
    let mut now = comparable(harness.state().project().ok_or("kein Projekt")?, start);
    let mut target = target;
    if same_content(&now, &target) {
        return Ok(());
    }
    align_counters(&mut now, &mut target);
    Err(first_difference(
        &format!("{target:#?}"),
        &format!("{now:#?}"),
    ))
}

/// Ids of bids are never handed out twice: undo brings the bids back, the
/// counter for the next id stays where it was (`Project::restore_offers`).
fn align_counters(left: &mut Project, right: &mut Project) {
    for index in 0..left.lv_count().min(right.lv_count()) {
        let next = left.next_bid_id_of(index).max(right.next_bid_id_of(index));
        left.reserve_bid_ids_of(index, next);
        right.reserve_bid_ids_of(index, next);
    }
}

/// Equal apart from the id counters (see [`align_counters`]).
fn same_content(left: &Project, right: &Project) -> bool {
    let (mut left, mut right) = (left.clone(), right.clone());
    align_counters(&mut left, &mut right);
    left == right
}

/// The first differing line of two debug outputs, with some context.
fn first_difference(expected: &str, got: &str) -> String {
    let content = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|line| !line.trim_start().starts_with("key:"))
            .map(str::to_string)
            .collect()
    };
    let expected = content(expected);
    let got = content(got);
    let at = expected
        .iter()
        .zip(&got)
        .position(|(left, right)| left != right)
        .unwrap_or(expected.len().min(got.len()));
    let from = at.saturating_sub(10);
    format!(
        "Rückgängig bis zum Anfang ergibt nicht das Ausgangsprojekt – erste Abweichung in Zeile {at}:\n{}\n--- Anfang: {:?}\n--- jetzt:  {:?}",
        expected[from..at].join("\n"),
        expected.get(at),
        got.get(at)
    )
}

/// Where a session runs: in the open project (Rückgängig is checked) or
/// from the start screen (everything allowed, also closing and opening).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tour {
    Project,
    StartScreen,
}

/// Runs `steps`; checks after every step and, in the project, undo at the end.
fn run(tour: Tour, warm_up: &[&str], steps: &[Step]) -> Result<(), TestCaseError> {
    watch_panics();
    watch_hangs();
    let _ = worker_panic();
    // Never more processes than at the start (see `nothing_is_started_from_the_test`).
    let children_before = child_processes();
    let sandbox = Sandbox::rich();
    let start_file = match tour {
        Tour::Project => Some(StartFile::Project(sandbox.project_file.clone())),
        Tour::StartScreen => None,
    };
    let mut harness = sandbox.harness(start_file);
    let start = harness.state().project().cloned();
    let first_session = session(&harness);
    let stay = tour == Tour::Project;
    let mut trace: Vec<String> = Vec::new();
    for room in warm_up {
        for label in room.split(" | ") {
            // A renamed menu entry must not leave the room unvisited unnoticed.
            if !open_by_label(&mut harness, label) {
                return Err(TestCaseError::fail(format!(
                    "Rundgang: „{label}“ nicht gefunden"
                )));
            }
            trace.push(format!("öffnen „{label}“"));
        }
    }
    for (number, step) in steps.iter().enumerate() {
        note_running(Some(format!("{}\n{number:3} {step:?}", tail(&trace))));
        let tab = tab_expectation(&harness, step);
        // A panic in the UI ends the case with the steps that led to it.
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            apply(&mut harness, step, stay)
        }));
        let done = match done {
            Ok(done) => done,
            Err(panic) => {
                let message = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
                    .unwrap_or_default();
                trace.push(format!("{number:3} {step:?}"));
                return Err(TestCaseError::fail(format!(
                    "Panic in Schritt {number}: {message}\nVerlauf:\n{}",
                    tail(&trace)
                )));
            }
        };
        if std::env::var_os("PROAVA_PROPTEST_STATS").is_some() {
            eprintln!("STAT {done}");
        }
        trace.push(format!("{number:3} {done}"));
        let children = child_processes();
        let failure = (children.len() > children_before.len())
            .then(|| format!("Kindprozess gestartet: {children:?}"))
            .or_else(|| worker_panic().map(|panic| format!("Panic im Hintergrund: {panic}")))
            .or_else(|| {
                harness
                    .state()
                    .project()
                    .and_then(|project| check_project(project).err())
            })
            .or_else(|| harness.state().project().and_then(stray_control_character))
            .or_else(|| tab.and_then(|expected| tab_failure(&harness, expected)));
        if let Some(failure) = failure {
            return Err(TestCaseError::fail(format!(
                "nach Schritt {number}: {failure}\nVerlauf:\n{}",
                tail(&trace)
            )));
        }
    }
    note_running(Some(format!("{}\nRückgängig bis zum Anfang", tail(&trace))));
    if let (Tour::Project, Some(start)) = (tour, &start) {
        if session(&harness) == first_session {
            undo_to_start(&mut harness, start).map_err(|error| {
                TestCaseError::fail(format!("{error}\nVerlauf:\n{}", tail(&trace)))
            })?;
        }
    }
    wait_idle(&mut harness);
    note_running(None);
    drop(harness);
    drop(sandbox.folder);
    Ok(())
}

/// Tab or Umschalt+Tab while a table cell is edited, with an editable cell
/// beside it in that direction: (edited cell, the cell Tab must open).
/// (Anwender 18.11. 23:16: on an STLB-Bau position Tab from the Menge ended
/// the editing instead of going to the EP – no panic, no broken invariant.)
fn tab_expectation(
    harness: &UiHarness,
    step: &Step,
) -> Option<(proava_ui::cell_edit::Cell, proava_ui::cell_edit::Cell)> {
    let Step::Key(index) = step else {
        return None;
    };
    let (key, modifiers) = KEYS[*index];
    if key != Key::Tab {
        return None;
    }
    let workspace = harness.state().workspace()?;
    if workspace.any_dialog_open() {
        return None;
    }
    let cell = workspace.editing_cell()?;
    let row = workspace.editing_index()?;
    let shown = workspace
        .table_layout()
        .map(|columns| proava_ui::cell_edit::Shown::of(&columns))
        .unwrap_or(proava_ui::cell_edit::Shown::ALL);
    let lv = harness.state().project()?.lv();
    let next = proava_ui::cell_edit::step_editable(lv, row, shown, cell, !modifiers.shift)?;
    Some((cell, next))
}

/// After such a Tab a cell is still edited: the next one, or the same when
/// its value was refused; a question (title KG over finished positions …)
/// may take over.
fn tab_failure(
    harness: &UiHarness,
    (cell, next): (proava_ui::cell_edit::Cell, proava_ui::cell_edit::Cell),
) -> Option<String> {
    let workspace = harness.state().workspace()?;
    (workspace.editing_cell().is_none() && !workspace.any_dialog_open())
        .then(|| format!("Tab in der Zelle {cell:?}: danach keine Zelle offen, erwartet {next:?}"))
}

/// Texts of the project with a control character that came in by typing or
/// pasting (the input filter of every frame removes them).
fn stray_control_character(project: &Project) -> Option<String> {
    for lv in project.lvs() {
        for row in lv.rows() {
            let mut texts = vec![row.short_text().to_string()];
            if let Some(position) = row.as_position() {
                texts.push(position.long_text().to_plain());
            }
            for text in texts {
                if text.chars().any(proava_domain::is_stray_control) {
                    return Some(format!("Steuerzeichen im Projekt: {text:?}"));
                }
            }
        }
    }
    None
}

fn tail(trace: &[String]) -> String {
    // PROAVA_AFFE_TRACE=alle: the whole session (long runs, bug hunting).
    let keep = if std::env::var("PROAVA_AFFE_TRACE").is_ok_and(|value| value == "alle") {
        trace.len()
    } else {
        60
    };
    trace[trace.len().saturating_sub(keep)..].join("\n")
}

/// Opens a view or dialog by its menu path „Menü ▸ Eintrag“ or clicks the
/// first element whose label starts with `path`; false when not found.
fn open_by_label(harness: &mut UiHarness, path: &str) -> bool {
    let found = match path.split_once(" ▸ ") {
        Some((menu, entry)) => {
            let menu_center = menu_entries(harness)
                .into_iter()
                .find(|(name, _)| name == menu)
                .map(|(_, center)| center);
            let Some(menu_center) = menu_center else {
                return false;
            };
            pointer_click(harness, menu_center, egui::PointerButton::Primary, 1);
            settle(harness);
            let entry_center = harness
                .query_all_by(|node| {
                    node.label().is_some_and(|label| {
                        label.trim() == entry || label.starts_with(&format!("{entry} "))
                    })
                })
                .next()
                .map(|node| node.rect().center());
            if let Some(center) = entry_center {
                harness.event(egui::Event::PointerMoved(center));
                harness.run_steps(20);
                pointer_click(harness, center, egui::PointerButton::Primary, 1);
            }
            entry_center.is_some()
        }
        None => {
            let center = harness
                .query_all_by(|node| {
                    node.label()
                        .is_some_and(|label| label.trim() == path || label.starts_with(path))
                })
                .next()
                .map(|node| node.rect().center());
            if let Some(center) = center {
                pointer_click(harness, center, egui::PointerButton::Primary, 1);
            }
            center.is_some()
        }
    };
    settle(harness);
    found
}

/// Every view and dialog the tour visits, by its menu path.
const ROOMS: &[&str] = &[
    "Projekt ▸ Projektstammdaten …",
    "LV ▸ LV-Eigenschaften …",
    "LV ▸ Gliederung und OZ-Maske …",
    "Bearbeiten ▸ Suchen und Ersetzen",
    "Prüfen ▸ Rechtschreibung prüfen …",
    "LV ▸ Neu nummerieren …",
    "Prüfen ▸ Kosten nach Kostengruppen …",
    "Prüfen ▸ Offene Punkte …",
    "Prüfen ▸ LV prüfen …",
    "Drucken ▸ Drucken …",
    "Angebote ▸ Bieterverzeichnis",
    "Angebote ▸ Schriftverkehr (Anschreiben an Bieter) …",
    "Angebote ▸ Angebote einlesen (X84/D84) …",
    "Angebote ▸ Auftrag erteilen …",
    "Drucken ▸ Niederschrift über die Angebotsöffnung …",
    "Ansicht ▸ Ansicht Angebote",
    "Ansicht ▸ Ansicht Preisspiegel",
    "Extras ▸ Einstellungen …",
    "Hilfe ▸ Tastenkürzel …",
    // Newer parts (18.11.).
    "Ansicht ▸ Ansicht Abrechnung",
    "Ansicht ▸ Ansicht Nachträge",
    "Ansicht ▸ Ansicht Kosten",
    "Kosten ▸ Neue Kostenberechnung nach Datenpunkten (GA)",
    "Kosten ▸ Kostenberechnung aus Kostenschätzung (Datenpunkte)",
    // Kosten-LV (Kostenermittlung als Kurztext-LV, 18.11.).
    "Kosten ▸ Neue Kostenschätzung als Kurztext-LV …",
    "Kosten ▸ Kostenberechnung aus Kostenschätzung (Kurztext-LV)",
    "Drucken ▸ Kostenschätzung / Kostenberechnung (DIN 276) …",
    "Angebote ▸ Schriftverkehr im Projekt (abgelegte Schreiben) …",
    "GAEB ▸ CSV importieren …",
    "GAEB ▸ Als CSV exportieren …",
    "Einfügen ▸ Neuer Abstandshalter darunter (Leerzeile)",
    "Prüfen ▸ Leere Hinweise in Abstandshalter umwandeln",
    // The first position (with the PDF clip), then the button of its long text.
    "01.01.0010 | Ausschnitt aus PDF einfügen …",
];

/// Every text field on screen (a typed space must show at once, not only
/// with the next character): click into it,
/// End, „ “ – the field shows the space at once; then Rücktaste (the value
/// stays as it was). Fields that may not hold a space by design are left out
/// (see [`SPACE_FREE`]). Returns what went wrong.
fn trailing_space_problems(
    harness: &mut UiHarness,
    place: &str,
    checked: &mut usize,
) -> Vec<String> {
    let focused = |harness: &UiHarness| {
        harness
            .query_all_by(|node| node.role() == Role::TextInput && node.is_focused())
            .next()
            .map(|node| (node.rect(), node.value().unwrap_or_default()))
    };
    let mut problems = Vec::new();
    let mut fields: Vec<(egui::Rect, String)> = harness
        .query_all_by(|node| node.role() == Role::TextInput && !node.is_disabled())
        .map(|node| {
            (
                node.rect(),
                node.accesskit_node().label().unwrap_or_default(),
            )
        })
        .collect();
    fields.sort_by(|a, b| {
        (a.0.top(), a.0.left())
            .partial_cmp(&(b.0.top(), b.0.left()))
            .unwrap()
    });
    for (rect, label) in fields {
        if SPACE_FREE.iter().any(|free| label.starts_with(free)) {
            continue;
        }
        pointer_click(harness, rect.center(), egui::PointerButton::Primary, 1);
        settle(harness);
        // Covered by a popup or not editable: nothing to check.
        let Some((at, before)) = focused(harness).filter(|(at, _)| at.contains(rect.center()))
        else {
            continue;
        };
        harness.key_press(Key::End);
        settle(harness);
        let Some((_, before)) = focused(harness)
            .filter(|(now, _)| *now == at)
            .or(Some((at, before)))
        else {
            continue;
        };
        harness.event(egui::Event::Text(" ".into()));
        settle(harness);
        *checked += 1;
        let after = focused(harness).map(|(_, value)| value);
        if after.as_deref() == Some(format!("{before} ").as_str()) {
            harness.key_press(Key::Backspace);
            settle(harness);
        } else {
            let name = if label.is_empty() {
                format!("{before:?}")
            } else {
                format!("„{label}“")
            };
            problems.push(format!(
                "{place}: Feld {name}: {before:?} + Leerzeichen → {after:?}"
            ));
        }
    }
    problems
}

/// Fields that take no space by design (labels as AccessKit names).
const SPACE_FREE: &[&str] = &[];

/// A space typed at the end of a text field shows at once – in every room of the tour, in the edit panel of each kind of row,
/// in the position window and in a library („Bibliothek bearbeiten …“).
#[test]
fn every_text_field_shows_a_trailing_space_at_once() {
    let mut problems = Vec::new();
    let mut checked = 0;
    // The rows of the LV: edit panel and position window.
    for library in [false, true] {
        for (row, window) in [
            ("01.01.0010", false),
            ("01.01.0020", false),
            ("01", false),
            ("01.01.0010", true),
        ] {
            let sandbox = Sandbox::rich();
            if library {
                let launcher = sandbox.launcher();
                let mut preferences = launcher.preferences();
                preferences.library_file = Some(sandbox.project_file.clone());
                launcher.set_preferences(preferences).unwrap();
            }
            let mut harness =
                sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
            let place = format!(
                "{} {row}{}",
                if library { "Bibliothek" } else { "LV" },
                if window { " Positionsfenster" } else { "" }
            );
            let center = harness
                .query_all_by(|node| {
                    node.role() == Role::Row
                        && node
                            .label()
                            .is_some_and(|label| label.starts_with(&format!("{row} ")))
                })
                .next()
                .map(|node| node.rect().center())
                .unwrap_or_else(|| panic!("Zeile {row}"));
            pointer_click(&mut harness, center, egui::PointerButton::Primary, 1);
            settle(&mut harness);
            if window {
                harness.key_press_modifiers(Modifiers::ALT, Key::Enter);
                settle(&mut harness);
            }
            if window {
                let open = harness
                    .query_all_by(|node| node.label().as_deref() == Some("Schließen (Esc)"))
                    .next()
                    .is_some();
                if !open {
                    problems.push(format!("{place}: Fenster nicht offen"));
                    continue;
                }
                // Positions have tabs, titles one page.
                let mut tabs = 0;
                for tab in ["Allgemein", "Weiteres", "Mengenermittlung"] {
                    if click_button(&mut harness, tab) {
                        tabs += 1;
                        problems.extend(trailing_space_problems(
                            &mut harness,
                            &format!("{place} {tab}"),
                            &mut checked,
                        ));
                    }
                }
                if tabs == 0 {
                    problems.extend(trailing_space_problems(&mut harness, &place, &mut checked));
                }
            } else {
                problems.extend(trailing_space_problems(&mut harness, &place, &mut checked));
            }
        }
    }
    // Every room of the tour.
    for room in ROOMS.iter().chain(
        [
            "Ansicht ▸ Ansicht Kostenverfolgung",
            "Angebote ▸ Bieterverzeichnis | Bieter hinzufügen",
        ]
        .iter(),
    ) {
        let sandbox = Sandbox::rich();
        let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
        let mut found = true;
        for label in room.split(" | ") {
            found &= open_by_label(&mut harness, label);
        }
        if !found {
            problems.push(format!("{room}: nicht gefunden"));
            continue;
        }
        problems.extend(trailing_space_problems(&mut harness, room, &mut checked));
    }
    // A renamed role or a changed layout must not leave the test empty.
    assert!(checked >= 150, "nur {checked} Felder geprüft");
    assert!(
        problems.is_empty(),
        "{} Felder:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// Child processes of this test process (all threads), from
/// `/proc/self/task/*/children` – started programs stay listed (also as
/// zombies) until they are reaped, which nothing here does.
fn child_processes() -> Vec<String> {
    let mut children = Vec::new();
    if let Ok(tasks) = std::fs::read_dir("/proc/self/task") {
        for task in tasks.flatten() {
            if let Ok(list) = std::fs::read_to_string(task.path().join("children")) {
                children.extend(list.split_whitespace().map(str::to_string));
            }
        }
    }
    children
}

/// Clicks the last button with exactly this label (a dialog is drawn after
/// the window, so „Speichern“ there before the one in the title bar);
/// false when there is none.
fn click_button(harness: &mut UiHarness, label: &str) -> bool {
    let center = harness
        .query_all_by(|node| {
            node.role() == Role::Button && node.label().is_some_and(|text| text.trim() == label)
        })
        .last()
        .map(|node| node.rect().center());
    match center {
        Some(center) => {
            pointer_click(harness, center, egui::PointerButton::Primary, 1);
            settle(harness);
            true
        }
        None => false,
    }
}

/// Schutz des PCs (Nacht 17./18.11.: der Affentest startete über „Neues
/// Fenster“ das Testprogramm immer wieder selbst): jeder Weg, der in der
/// echten App ein anderes Programm startet, wird im Affentest-Harness
/// ausgelöst – „Neues Fenster“, „Projekt schreibgeschützt öffnen“ (zweites
/// Fenster), „Handbuch (PDF)“, „PDF speichern“ mit „Öffnen“ in der
/// Statuszeile und mit „nach dem Speichern öffnen“ (Kostenermittlung),
/// „Öffnen“ im Schriftverkehr im Projekt, „STLB-Bau Online öffnen“,
/// „AUSSCHREIBEN.DE öffnen“ und im Startfenster „Im Dateimanager zeigen“.
/// Jeder Weg muss die Sperre in `jobs.rs` erreichen (`refused_starts` zählt
/// je Thread – ein umbenannter Menüeintrag fällt so auf, statt den Test
/// still leer laufen zu lassen); danach gibt es keinen einzigen Kindprozess.
#[test]
#[cfg(target_os = "linux")]
fn nothing_is_started_from_the_test() {
    assert!(
        std::path::Path::new("/proc/self/task").is_dir(),
        "/proc fehlt – Zählung unmöglich"
    );
    let before = child_processes();
    let sandbox = Sandbox::rich();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    let mut refused = proava_ui::jobs::refused_starts();
    let mut reached = |what: &str| {
        let now = proava_ui::jobs::refused_starts();
        assert!(
            now > refused,
            "{what}: der Weg hat die Sperre nicht erreicht (Menüeintrag oder Knopf umbenannt?)"
        );
        refused = now;
    };

    assert!(must_open(&mut harness, "Projekt ▸ Neues Fenster"));
    reached("Neues Fenster");

    assert!(must_open(
        &mut harness,
        "Projekt ▸ Projekt schreibgeschützt öffnen …"
    ));
    let file = harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.contains("Affentest.proava"))
        })
        .next()
        .map(|node| node.rect().center())
        .expect("Projektdatei im Dateidialog");
    pointer_click(&mut harness, file, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    assert!(
        click_button(&mut harness, proava_ui::strings::OPEN),
        "Öffnen im Dateidialog"
    );
    wait_idle(&mut harness);
    reached("Projekt schreibgeschützt öffnen");

    assert!(must_open(&mut harness, "Hilfe ▸ Handbuch (PDF)"));
    wait_idle(&mut harness);
    reached("Hilfe ▸ Handbuch (PDF)");

    // The menu entry opens the print dialog with the kind chosen.
    assert!(must_open(
        &mut harness,
        "Drucken ▸ LV ohne Preise (Angebotsaufforderung) …"
    ));
    assert!(
        click_button(&mut harness, "PDF speichern …"),
        "PDF speichern im Druckdialog"
    );
    assert!(
        click_button(&mut harness, proava_ui::strings::FD_SAVE),
        "Speichern im Dateidialog"
    );
    wait_idle(&mut harness);
    let pdf = std::fs::read_dir(&sandbox.root)
        .unwrap()
        .flatten()
        .any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "pdf" && !entry.path().ends_with("Plan.pdf"))
        });
    assert!(pdf, "PDF in der Testablage geschrieben");
    assert!(
        click_button(&mut harness, proava_ui::strings::OPEN),
        "„Öffnen“ in der Statuszeile"
    );
    wait_idle(&mut harness);
    reached("PDF speichern ▸ Öffnen");

    // Kostenermittlung (Druckdialog für alle PDFs) with „nach dem Speichern öffnen“.
    assert!(must_open(
        &mut harness,
        "Drucken ▸ Kostenschätzung / Kostenberechnung (DIN 276) …"
    ));
    let open_after = harness
        .query_all_by(|node| {
            node.role() == Role::CheckBox
                && node
                    .label()
                    .is_some_and(|label| label.contains("nach dem Speichern öffnen"))
        })
        .next()
        .expect("„nach dem Speichern öffnen“ im Druckdialog");
    if open_after.accesskit_node().toggled() != Some(egui::accesskit::Toggled::True) {
        let at = open_after.rect().center();
        pointer_click(&mut harness, at, egui::PointerButton::Primary, 1);
        settle(&mut harness);
    }
    assert!(
        click_button(&mut harness, "PDF speichern …"),
        "PDF speichern (Kostenermittlung)"
    );
    assert!(
        click_button(&mut harness, proava_ui::strings::FD_SAVE),
        "Speichern im Dateidialog"
    );
    wait_idle(&mut harness);
    reached("Kostenermittlung drucken, nach dem Speichern öffnen");

    // Schriftverkehr im Projekt: „Öffnen“ legt eine Kopie für die PDF-Anzeige an.
    assert!(must_open(
        &mut harness,
        "Angebote ▸ Schriftverkehr im Projekt (abgelegte Schreiben) …"
    ));
    assert!(
        must_open(&mut harness, "Aufforderung an Müller GmbH vom 18.11.2026"),
        "abgelegtes Schreiben in der Liste"
    );
    assert!(
        click_button(&mut harness, "Öffnen"),
        "„Öffnen“ im Schriftverkehr"
    );
    reached("Schriftverkehr im Projekt ▸ Öffnen");
    let stored = harness.state().project().unwrap().correspondence()[0]
        .file
        .clone();
    let copy = sandbox
        .copies()
        .join(proava_ui::viewer_copies::copy_name(&stored));
    assert!(
        copy.is_file(),
        "Kopie für die PDF-Anzeige in der Testablage: {}",
        copy.display()
    );
    // Never in the folder of the process, which all tests of this program
    // share (they run side by side and would tidy each other's copies away).
    let shared = proava_ui::viewer_copies::folder_in(&std::env::temp_dir(), std::process::id());
    assert!(
        !shared.exists(),
        "Kopie im gemeinsamen Ordner des Prozesses: {}",
        shared.display()
    );
    close_everything(&mut harness);

    // Web pages of the text sources.
    assert!(must_open(
        &mut harness,
        "Einfügen ▸ Neue Position aus STLB-Bau …"
    ));
    assert!(
        click_button(&mut harness, "STLB-Bau Online öffnen"),
        "Knopf STLB-Bau Online"
    );
    reached("STLB-Bau Online öffnen");
    close_everything(&mut harness);
    assert!(must_open(
        &mut harness,
        "Einfügen ▸ Neue Position aus AUSSCHREIBEN.DE …"
    ));
    assert!(
        click_button(&mut harness, "AUSSCHREIBEN.DE öffnen"),
        "Knopf AUSSCHREIBEN.DE"
    );
    reached("AUSSCHREIBEN.DE öffnen");
    close_everything(&mut harness);
    harness.run_steps(30);
    drop(harness);

    // Startfenster: Rechtsklick auf das Projekt ▸ „Im Dateimanager zeigen“.
    let mut start = sandbox.harness(None);
    let row = start
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.contains("Affentest"))
        })
        .next()
        .map(|node| node.rect().center())
        .expect("Testprojekt unter „Zuletzt verwendet“");
    pointer_click(&mut start, row, egui::PointerButton::Secondary, 1);
    settle(&mut start);
    assert!(
        click_button(&mut start, "Im Dateimanager zeigen"),
        "„Im Dateimanager zeigen“ im Kontextmenü"
    );
    reached("Startfenster ▸ Im Dateimanager zeigen");
    start.run_steps(30);

    let after = child_processes();
    assert_eq!(after, before, "Kindprozesse gestartet");
}

/// Like [`open_by_label`], but says whether the menu entry or label was there.
fn must_open(harness: &mut UiHarness, path: &str) -> bool {
    open_by_label(harness, path)
}

/// Gefunden mit dem Affentest: Positionsfenster und Eigenschaftenfenster
/// zeigen dieselbe Position – das KG-Feld hatte in beiden dieselbe globale
/// Id (egui: „changed layer_id during the frame“, Debug-Assert).
#[test]
fn position_window_and_detail_panel_show_the_same_row() {
    let sandbox = Sandbox::new();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    let row = harness
        .query_all_by(|node| {
            node.role() == Role::Row
                && node
                    .label()
                    .is_some_and(|label| label.starts_with("01.01.0010 "))
        })
        .next()
        .map(|node| node.rect().center())
        .expect("Zeile 01.01.0010");
    pointer_click(&mut harness, row, egui::PointerButton::Primary, 2);
    settle(&mut harness);
    assert!(
        click_button(&mut harness, "Hier bearbeiten"),
        "Positionsfenster offen"
    );
    assert!(click_button(&mut harness, "Weiteres"), "Reiter Weiteres");
    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    settle(&mut harness);
    // Selection grows by one row: both panels show the KG field of the same rows.
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowUp);
    harness.run_steps(10);
}

/// Tab-Folgen (19.11., nach dem Rückschritt vom 18.11. 23:16, den keine
/// Zufallsfolge fand): in jeder Positionszeile beider Ausschreibungs-LVs des
/// vollen Demos von der ersten Zelle mit Tab bis zum Zeilenende und mit
/// Umschalt+Tab zurück – nach jedem Druck ist genau die nächste bearbeitbare
/// Zelle offen (gesperrte wie die STLB-Einheit übersprungen), am Ende keine.
#[test]
fn tab_walks_every_row_of_the_full_demo() {
    use proava_ui::cell_edit::{step_editable, Cell, Shown};
    let sandbox = Sandbox::rich();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    let press = |harness: &mut UiHarness, modifiers: Modifiers, key: Key| {
        harness.key_press_modifiers(modifiers, key);
        settle(harness);
    };
    // (editing cell, row) and what Tab in `forward` must open next.
    let expected = |harness: &UiHarness, forward: bool| -> (Cell, Option<Cell>) {
        let workspace = harness.state().workspace().unwrap();
        let cell = workspace.editing_cell().expect("eine Zelle offen");
        let row = workspace.editing_index().expect("Zeile der Zelle");
        let shown = workspace
            .table_layout()
            .map(|columns| Shown::of(&columns))
            .unwrap_or(Shown::ALL);
        let lv = harness.state().project().unwrap().lv();
        (cell, step_editable(lv, row, shown, cell, forward))
    };
    press(&mut harness, Modifiers::NONE, Key::Home);
    let mut walked = 0;
    let mut locked = 0;
    let mut last = None;
    let mut same = 0;
    for _ in 0..400 {
        let workspace = harness.state().workspace().unwrap();
        let project = harness.state().project().unwrap();
        let place = (project.active_lv(), workspace.selected_index());
        let position = place
            .1
            .and_then(|index| project.lv().rows()[index].as_position())
            .cloned();
        if let Some(position) = position {
            press(&mut harness, Modifiers::NONE, Key::F2);
            if harness
                .state()
                .workspace()
                .unwrap()
                .editing_cell()
                .is_some()
            {
                // To the first editable cell …
                while let (_, Some(previous)) = expected(&harness, false) {
                    press(&mut harness, Modifiers::SHIFT, Key::Tab);
                    assert_eq!(
                        harness.state().workspace().unwrap().editing_cell(),
                        Some(previous),
                        "Umschalt+Tab in „{}“",
                        position.short_text()
                    );
                }
                // … and with Tab to the end of the row.
                loop {
                    let (cell, next) = expected(&harness, true);
                    press(&mut harness, Modifiers::NONE, Key::Tab);
                    assert_eq!(
                        harness.state().workspace().unwrap().editing_cell(),
                        next,
                        "Tab von {cell:?} in „{}“",
                        position.short_text()
                    );
                    if next.is_none() {
                        break;
                    }
                }
                walked += 1;
                locked += usize::from(position.is_text_locked());
            }
        }
        press(&mut harness, Modifiers::NONE, Key::ArrowDown);
        if last == Some(place) {
            same += 1;
            if same > 2 {
                break;
            }
        } else {
            same = 0;
        }
        last = Some(place);
    }
    assert!(walked >= 40, "{walked} Positionen durchlaufen");
    assert_eq!(locked, 2, "beide STLB-Positionen dabei");
}

/// Gefunden mit dem Affentest: der Dateidialog im Dialog „Aus STLB-Bau
/// übernehmen“ schließt (Klick daneben) – im selben Frame malte der
/// STLB-Dialog sein Modal mit derselben Id („🔥“, kittest bricht ab).
#[test]
fn file_picker_inside_the_stlb_dialog_closes_cleanly() {
    let sandbox = Sandbox::new();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    open_by_label(&mut harness, "Einfügen ▸ Neue Position aus STLB-Bau …");
    assert!(
        click_button(&mut harness, "GAEB-Datei wählen …"),
        "Dateidialog geöffnet"
    );
    pointer_click(
        &mut harness,
        egui::pos2(8.0, 8.0),
        egui::PointerButton::Primary,
        1,
    );
    harness.run_steps(6);
    assert!(
        harness.state().workspace().unwrap().dialog_open(),
        "STLB-Dialog bleibt"
    );
}

/// Gefunden mit dem Affentest (Saat 14): eine Bieter-PLZ geändert, danach
/// steht der Cursor in einem Textfeld des LV-Editors (hier: neuer Hinweis
/// bei ausgeblendetem Eigenschaftenfenster, das später wieder erscheint).
/// Strg+Z im Feld nahm nur LV-Änderungen zurück – die neuere PLZ-Änderung
/// blieb für immer.
#[test]
fn undo_in_an_lv_text_field_reaches_newer_offer_changes() {
    let sandbox = Sandbox::new();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    let start = harness.state().project().unwrap().clone();
    open_by_label(&mut harness, "Ansicht ▸ Eigenschaftenfenster anzeigen");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::H);
    settle(&mut harness);
    let row = harness
        .query_all_by(|node| {
            node.role() == Role::Row
                && node
                    .label()
                    .is_some_and(|label| label.starts_with("01.02.0050 "))
        })
        .next()
        .map(|node| node.rect().center())
        .expect("Zeile 01.02.0050");
    pointer_click(&mut harness, row, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    open_by_label(&mut harness, "Angebote ▸ Bieterverzeichnis");
    let field = harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.value().as_deref() == Some("12345")
        })
        .next()
        .map(|node| node.rect().center())
        .expect("PLZ 12345 im Angebote-Reiter");
    pointer_click(&mut harness, field, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    harness.event(egui::Event::Text("ÄÖÜ".into()));
    settle(&mut harness);
    harness.event(egui::Event::Text("0Fühler\n".into()));
    settle(&mut harness);
    harness.key_press(Key::Backspace);
    settle(&mut harness);
    assert_ne!(
        harness.state().project().unwrap().bids()[0]
            .bidder()
            .postal_code,
        "12345"
    );
    open_by_label(&mut harness, "Ansicht ▸ Eigenschaftenfenster anzeigen");
    undo_to_start(&mut harness, &start).unwrap_or_else(|error| panic!("{error}"));
}

/// Gefunden mit dem Affentest (Saaten 9 und 13): eine Angebotsänderung,
/// danach eine LV-Änderung in einem Textfeld des Eigenschaftenfensters.
/// Strg+Z im Feld nahm die LV-Änderung zurück, die Reihenfolge des Projekts
/// erfuhr davon nichts – jedes weitere Strg+Z blieb beim Editor, die
/// Angebotsänderung kam nie zurück.
#[test]
fn undo_in_a_field_keeps_the_project_order_in_step() {
    let sandbox = Sandbox::new();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    let start = harness.state().project().unwrap().clone();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Num2);
    settle(&mut harness);
    let excluded = harness
        .query_all_by(|node| {
            node.role() == Role::CheckBox
                && node
                    .label()
                    .is_some_and(|label| label.contains("ausschließen"))
        })
        .next()
        .map(|node| node.rect().center())
        .expect("„Vom Preisspiegel ausschließen“");
    pointer_click(&mut harness, excluded, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    assert!(harness
        .state()
        .project()
        .unwrap()
        .bids()
        .iter()
        .any(|bid| bid.is_excluded()));

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Num1);
    settle(&mut harness);
    let row = harness
        .query_all_by(|node| {
            node.role() == Role::Row
                && node
                    .label()
                    .is_some_and(|label| label.starts_with("01.01.0010 "))
        })
        .next()
        .map(|node| node.rect().center())
        .expect("Zeile 01.01.0010");
    pointer_click(&mut harness, row, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    let short = harness
        .state()
        .project()
        .unwrap()
        .lv()
        .rows()
        .iter()
        .find(|row| row.ordinal() == "01.01.0010")
        .unwrap()
        .short_text()
        .to_string();
    let field = harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.value().as_deref() == Some(short.as_str())
        })
        .next()
        .map(|node| node.rect().center())
        .expect("Kurztext im Eigenschaftenfenster");
    pointer_click(&mut harness, field, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    harness.key_press(Key::End);
    harness.event(egui::Event::Text(" neu".into()));
    settle(&mut harness);
    // Some time passes: the field's own steps do not join the change above.
    harness.run_steps(120);
    assert_ne!(
        harness.state().project().unwrap().lv(),
        start.lv(),
        "LV geändert"
    );
    // Strg+Z only, the cursor stays in the field.
    for _ in 0..12 {
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        settle(&mut harness);
    }
    let now = harness.state().project().unwrap();
    assert!(
        same_content(&comparable(now, &start), &comparable(&start, &start)),
        "nicht alles zurückgenommen: ausgeschlossen = {:?}",
        now.bids()
            .iter()
            .map(|bid| bid.is_excluded())
            .collect::<Vec<_>>()
    );
}

/// Gefunden mit dem Affentest (Saat 9, zweiter Lauf): den Titel zweimal
/// geändert – im Eigenschaftenfenster und in der Tabelle –, dann Strg+Z im
/// Kurztext-Feld. Das Feld nahm sein Getipptes zurück, das nächste Strg+Z
/// holte es wieder: Strg+Z pendelte für immer zwischen zwei Texten.
#[test]
fn undo_in_a_field_does_not_swing_back_and_forth() {
    let sandbox = Sandbox::new();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    let title = |harness: &UiHarness| {
        harness.state().project().unwrap().lv().rows()[0]
            .short_text()
            .to_string()
    };
    let original = title(&harness);
    let field = |harness: &UiHarness, value: &str| {
        harness
            .query_all_by(|node| {
                node.role() == Role::TextInput && node.value().as_deref() == Some(value)
            })
            .next()
            .map(|node| node.rect().center())
            .unwrap_or_else(|| panic!("Feld mit „{value}“"))
    };
    // 1. In the Eigenschaftenfenster.
    let at = field(&harness, &original);
    pointer_click(&mut harness, at, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.event(egui::Event::Text("Fühlerm²".into()));
    settle(&mut harness);
    harness.key_press(Key::Escape);
    settle(&mut harness);
    // 2. In the table: typing over the selected title.
    let row = harness
        .query_all_by(|node| {
            node.role() == Role::Row && node.label().is_some_and(|label| label.starts_with("01 "))
        })
        .next()
        .map(|node| node.rect().center())
        .expect("Titelzeile 01");
    pointer_click(&mut harness, row, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    harness.event(egui::Event::Text("Fühler".into()));
    settle(&mut harness);
    harness.key_press(Key::Enter);
    settle(&mut harness);
    assert_eq!(title(&harness), "Fühler");
    // 3. Strg+Z in the Kurztext field (Enter moved on: the title again).
    pointer_click(&mut harness, row, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    let at = field(&harness, "Fühler");
    pointer_click(&mut harness, at, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    let mut seen = Vec::new();
    for _ in 0..8 {
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        settle(&mut harness);
        seen.push(title(&harness));
    }
    let back = seen
        .iter()
        .position(|text| *text == original)
        .unwrap_or_else(|| panic!("Titel nie zurück: {seen:?}"));
    assert!(
        seen[back..].iter().all(|text| *text == original),
        "Strg+Z pendelt: {seen:?}"
    );
}

/// Gefunden mit dem Affentest (Saat 16): „Angebot entfernen …“ offen
/// lassen, Reiter wechseln, „Angebote ▸ Auftrag erteilen …“ – beide
/// Dialoge des Angebote-Reiters malten ihr Modal mit derselben Id („🔥“).
#[test]
fn award_dialog_replaces_a_pending_question_of_the_bids_view() {
    let sandbox = Sandbox::new();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Num2);
    settle(&mut harness);
    assert!(
        click_button(&mut harness, "Angebot entfernen …"),
        "Angebot entfernen"
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Num3);
    settle(&mut harness);
    open_by_label(&mut harness, "Angebote ▸ Auftrag erteilen …");
    harness.run_steps(6);
    assert!(
        click_button(&mut harness, "Abbrechen"),
        "Auftragsdialog offen"
    );
    let bids = harness.state().project().unwrap().bids().len();
    assert_eq!(bids, 4, "nichts entfernt");
}

/// Gefunden mit dem Affentest: Hilfe (Tastenkürzel) und Befehlspalette
/// gleichzeitig offen – beide Dialoge hatten dieselbe Modal-Id (egui malt
/// dann „🔥 …“ und kittest bricht ab).
#[test]
fn help_window_and_command_palette_open_together() {
    let sandbox = Sandbox::new();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    open_by_label(&mut harness, "Hilfe ▸ Tastenkürzel …");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::K);
    harness.run_steps(6);
    assert!(
        harness.state().workspace().unwrap().dialog_open(),
        "Palette offen"
    );
}

/// Steuerzeichen aus eingefügtem Text (Word, PDF) kommen nicht ins Projekt;
/// Zeilenumbruch und Tab bleiben (zentral beim Eingang jedes Frames).
#[test]
fn pasted_control_characters_do_not_reach_the_project() {
    let sandbox = Sandbox::new();
    let mut harness = sandbox.harness(Some(StartFile::Project(sandbox.project_file.clone())));
    let first = harness.state().project().unwrap().lv().rows()[0]
        .short_text()
        .to_string();
    let field = harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.value().as_deref() == Some(first.as_str())
        })
        .next()
        .expect("Kurztext des ersten Titels im Bearbeitungsfenster")
        .rect()
        .center();
    pointer_click(&mut harness, field, egui::PointerButton::Primary, 1);
    settle(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.event(egui::Event::Paste("Wärme\u{b}zentrale\u{c}\u{1b}".into()));
    settle(&mut harness);
    harness.key_press(Key::Enter);
    settle(&mut harness);
    let text = harness.state().project().unwrap().lv().rows()[0]
        .short_text()
        .to_string();
    assert_eq!(text, "Wärmezentrale");
}

proptest! {
    #![proptest_config(config(3))]

    /// Frei im Projekt: Tasten, Tippen, Klicks, Menüs; am Ende Rückgängig bis zum Anfang.
    #[test]
    fn monkey_in_the_project_keeps_it_consistent_and_undoable(
        steps in prop::collection::vec(step(), 20..60),
    ) {
        run(Tour::Project, &[], &steps)?;
    }

    /// Vom Startfenster aus (Testprojekt unter „Zuletzt verwendet“): alles erlaubt.
    #[test]
    fn monkey_from_the_start_screen_never_crashes(
        steps in prop::collection::vec(step(), 20..50),
    ) {
        run(Tour::StartScreen, &[], &steps)?;
    }
}

proptest! {
    #![proptest_config(config(1))]

    /// Rundgang: jedes Fenster öffnen, darin zufällig bedienen, Rückgängig bis zum Anfang.
    #[test]
    fn monkey_visits_every_window(
        steps in prop::collection::vec(prop::collection::vec(step(), 6..12), ROOMS.len()),
    ) {
        for (room, steps) in ROOMS.iter().zip(&steps) {
            run(Tour::Project, &[room], steps)?;
        }
    }
}

proptest! {
    #![proptest_config(config(40))]

    /// Teurer Lauf (`cargo test -- --ignored lang_`): lange Sitzungen.
    #[test]
    #[ignore]
    fn lang_monkey_in_the_project(steps in prop::collection::vec(step(), 100..250)) {
        run(Tour::Project, &[], &steps)?;
    }

    /// Teurer Lauf: lange Sitzungen vom Startfenster aus.
    #[test]
    #[ignore]
    fn lang_monkey_from_the_start_screen(steps in prop::collection::vec(step(), 100..250)) {
        run(Tour::StartScreen, &[], &steps)?;
    }

    /// Teurer Lauf: jedes Fenster mit längeren Folgen.
    #[test]
    #[ignore]
    fn lang_monkey_visits_every_window(
        room in 0..ROOMS.len(),
        steps in prop::collection::vec(step(), 30..80),
    ) {
        run(Tour::Project, &[ROOMS[room]], &steps)?;
    }
}
