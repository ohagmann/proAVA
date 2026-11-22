//! Window „Eigenschaften“ of one LV row in its own operating-system window
//! (egui viewport – it can go onto another screen; where a backend cannot
//! open more windows, egui shows it as a movable window inside the program).
//! Opened by a double click on a row, Alt+Enter or the context menu.
//!
//! Reiter „Allgemein“ (OZ, link to the Ausführungsbeschreibung, Kurztext,
//! Menge, Einheit, EP, GP, Langtext), „Weiteres“ (Positionsart, Bezüge,
//! STLB-Bau, Textergänzungen and the rare fields) and „Grafik“ (images of the
//! long text); on the right the Prüfleiste with the findings of the row.
//! Titles, hints and Ausführungsbeschreibungen get fewer Reiter.
//!
//! Changes go into the project at once, on the undo stack of the main
//! window, so the table shows them in the same frame. The window follows the
//! selection of the table (the active row of several; the table keeps the
//! keyboard, a field being typed in is taken over first) unless „Festhalten“
//! is ticked (Anwender 18.11.); „Vorige/Nächste“ (Bild↑/Bild↓) moves both.

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui::{self, FontId, RichText, Sense, Stroke};
use proava_application::{Launcher, WindowPlace};
use proava_domain::{
    normalize_ordinal, ComplementKind, EditStatus, Lv, LvRow, Money, PositionKind, Project,
    RichText as Text, RowId, TextBlock,
};

use super::fields::{self, caption};
use super::{
    kind_group, kind_label, number_field, select_all_on_focus, set_long_text, status_selector,
    EditorOutput, LvEditor, NumberEdit, UNITS,
};
use crate::file_dialog::{DialogResult, FileDialog};
use crate::icons::Icon;
use crate::lv_check::{self, Severity, SHORT_TEXT_LIMIT};
use crate::lv_ops::Command;
use crate::rich_editor::{RichEditor, DEFAULT_IMAGE_WIDTH_MM};
use crate::theme::Palette;
use crate::undo::EditKey;
use crate::{
    complement_display, form_kit, images, numbers, open_points, strings, struct_ops, widgets,
};

const DEFAULT_SIZE: (f32, f32) = (1180.0, 820.0);
/// Small enough for a quarter of a 1920 screen (Super+Pfeil, Anwender 18.11.).
const MIN_SIZE: [f32; 2] = [640.0, 420.0];
/// Width of the Prüfleiste on the right.
const CHECK_WIDTH: f32 = 270.0;
/// Seconds a new size or position must stay before it is remembered.
const SAVE_DELAY: f64 = 1.0;

/// Reiter of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Tab {
    General,
    More,
    Takeoff,
    Graphics,
}

impl Tab {
    pub(super) fn label(self) -> &'static str {
        match self {
            Tab::General => "Allgemein",
            Tab::More => "Weiteres",
            Tab::Takeoff => "Mengenermittlung",
            Tab::Graphics => "Grafik",
        }
    }
}

/// Reiter a row offers: positions all three, hints and
/// Ausführungsbeschreibungen Allgemein and Grafik, titles only Allgemein.
pub(super) fn tabs_for(row: &LvRow) -> &'static [Tab] {
    match row {
        LvRow::Position(_) => &[Tab::General, Tab::More, Tab::Takeoff, Tab::Graphics],
        LvRow::Note(_) | LvRow::Performance(_) => &[Tab::General, Tab::Graphics],
        LvRow::Group(_) | LvRow::Spacer(_) => &[Tab::General],
    }
}

/// What kind of row it is, in words: „Position“, „Titel“ (the level name),
/// „Hinweis“, „Ausführungsbeschreibung“.
pub(super) fn kind_word(lv: &Lv, row: &LvRow) -> String {
    match row {
        LvRow::Position(_) => "Position".to_string(),
        LvRow::Group(group) => lv.level_label(group.level()),
        LvRow::Note(_) => "Hinweis".to_string(),
        LvRow::Performance(_) => "Ausführungsbeschreibung".to_string(),
        LvRow::Spacer(_) => "Abstandshalter".to_string(),
    }
}

/// Title of the window: „Eigenschaften – Position 01.02.0010 Kabeltrasse“.
pub(super) fn window_title(lv: &Lv, row: &LvRow) -> String {
    let number = match row {
        LvRow::Performance(performance) => performance.mark(),
        other => other.ordinal().to_string(),
    };
    let text = row.short_text();
    let parts: Vec<&str> = [number.trim(), text.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    format!("Eigenschaften – {} {}", kind_word(lv, row), parts.join(" "))
        .trim_end()
        .to_string()
}

/// The next (`forward`) or previous row of the same kind as row `index`:
/// positions step through positions, titles through titles and so on.
pub(super) fn neighbour(lv: &Lv, index: usize, forward: bool) -> Option<usize> {
    let rows = lv.rows();
    let kind = std::mem::discriminant(rows.get(index)?);
    let same = |at: &usize| std::mem::discriminant(&rows[*at]) == kind;
    if forward {
        (index + 1..rows.len()).find(same)
    } else {
        (0..index).rev().find(same)
    }
}

/// One line of the Prüfleiste.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RowFinding {
    pub severity: Severity,
    pub text: String,
    /// Reiter where it is fixed.
    pub tab: Tab,
}

/// Findings of one row: the checks of „LV prüfen“ (`lv_check`) and of the
/// Offene Punkte for this row, and what only matters here (Textergänzungen,
/// Bezüge, Bilder, empty texts). Errors first.
pub(super) fn row_findings(
    project: &Project,
    id: RowId,
    speller: Option<&proava_spell::Speller>,
) -> Vec<RowFinding> {
    let lv = project.lv();
    let Some(index) = lv.index_of(id) else {
        return Vec::new();
    };
    let row = &lv.rows()[index];
    let mut findings: Vec<RowFinding> = Vec::new();
    let mut push = |severity: Severity, text: String, tab: Tab| {
        if !findings.iter().any(|finding| finding.text == text) {
            findings.push(RowFinding {
                severity,
                text,
                tab,
            });
        }
    };

    // „LV prüfen“, only what concerns this row (without the OZ in front).
    let prefix = format!("{} {}", kind_word(lv, row), row.ordinal());
    let titled = format!("Titel {}", row.ordinal());
    for finding in lv_check::check(project, None)
        .into_iter()
        .filter(|finding| finding.row == Some(id))
    {
        let text = short_finding(&finding.text, &[prefix.as_str(), titled.as_str()]);
        let tab = if finding.text.contains("Gruppe") {
            Tab::More
        } else {
            Tab::General
        };
        push(finding.severity, text, tab);
    }
    // „LV prüfen“ names a doubled OZ only at its first row.
    if row.has_ordinal()
        && !row.ordinal().trim().is_empty()
        && lv
            .duplicate_ordinals()
            .iter()
            .any(|ordinal| normalize_ordinal(ordinal) == normalize_ordinal(row.ordinal()))
    {
        push(
            Severity::Error,
            format!("OZ {} kommt mehrfach vor.", row.ordinal()),
            Tab::General,
        );
    }

    match row {
        LvRow::Position(position) => {
            // Offene Punkte: missing EP (once others have one), in Bearbeitung.
            let prices = open_points::prices_expected(lv);
            for point in open_points::collect(lv, prices)
                .into_iter()
                .filter(|point| point.row == id)
            {
                match point.kind {
                    open_points::OpenKind::MissingPrice => push(
                        Severity::Warning,
                        "Ohne Einheitspreis – andere Positionen haben einen.".to_string(),
                        Tab::General,
                    ),
                    open_points::OpenKind::InProgress => push(
                        Severity::Hint,
                        "Status „in Bearbeitung“.".to_string(),
                        Tab::General,
                    ),
                    open_points::OpenKind::PriceCheck => {
                        if let Some(check) = position.price_check() {
                            push(
                                Severity::Warning,
                                format!(
                                    "{} – der errechnete GP gilt. Bestätigen über das Kontextmenü der Zeile \
                                     (Status › GP-Abweichung bestätigen) oder Menge/EP ändern.",
                                    check.text(lv.currency())
                                ),
                                Tab::General,
                            );
                        }
                    }
                    _ => {}
                }
            }
            for problem in struct_ops::complement_problems(position.complements()) {
                push(Severity::Warning, problem, Tab::More);
            }
            for complement in position.complements() {
                if complement.kind == ComplementKind::Owner && complement.body.trim().is_empty() {
                    push(
                        Severity::Warning,
                        format!(
                            "Textergänzung {} (vom Auftraggeber) ist leer.",
                            complement.marker.trim()
                        ),
                        Tab::More,
                    );
                }
            }
            if position
                .performance()
                .is_some_and(|performance| lv.performance_by_id(performance).is_none())
            {
                push(
                    Severity::Error,
                    "Verweist auf eine Ausführungsbeschreibung, die es nicht mehr gibt."
                        .to_string(),
                    Tab::General,
                );
            }
            if position
                .repeats()
                .is_some_and(|base| lv.position_by_id(base).is_none())
            {
                push(
                    Severity::Warning,
                    "Wiederholung einer Position, die es nicht mehr gibt.".to_string(),
                    Tab::More,
                );
            }
            // Grund- und Wahlposition brauchen einander („LV prüfen“ nennt
            // nur die erste Position der Gruppe).
            let partner = |wanted: fn(PositionKind, u32) -> bool, group: u32| {
                lv.positions()
                    .any(|other| other.id() != id && wanted(other.kind(), group))
            };
            match position.kind() {
                PositionKind::Base { group }
                    if !partner(
                        |kind, group| matches!(kind, PositionKind::Alternative { group: g, .. } if g == group),
                        group,
                    ) =>
                {
                    push(
                        Severity::Error,
                        format!("Grundposition der Gruppe {group} ohne Wahlposition."),
                        Tab::More,
                    );
                }
                PositionKind::Alternative { group, .. }
                    if !partner(
                        |kind, group| matches!(kind, PositionKind::Base { group: g } if g == group),
                        group,
                    ) =>
                {
                    push(
                        Severity::Error,
                        format!("Wahlposition der Gruppe {group} ohne Grundposition."),
                        Tab::More,
                    );
                }
                _ => {}
            }
            if position.stlb_text_changed() {
                push(
                    Severity::Hint,
                    strings::STLB_TEXT_CHANGED.to_string(),
                    Tab::More,
                );
            }
            // Mengenermittlung: wrong lines, quantity not taken over.
            let (_, errors) = position.takeoff_total();
            for error in errors {
                push(
                    Severity::Error,
                    format!("Mengenermittlung {error}"),
                    Tab::Takeoff,
                );
            }
            if let Some(sum) = position.takeoff_differs() {
                push(
                    Severity::Warning,
                    format!(
                        "Menge {} weicht von der Mengenermittlung ({}) ab.",
                        numbers::quantity_text(position.quantity()),
                        numbers::quantity_text(sum)
                    ),
                    Tab::Takeoff,
                );
            }
            if !position.is_stlb_placeholder() && position.long_text().is_empty() {
                push(Severity::Hint, "Langtext leer.".to_string(), Tab::General);
            }
            spelling_hint(
                speller,
                [
                    ("Kurztext", position.short_text().to_string()),
                    ("Langtext", position.long_text().to_plain()),
                ],
                &mut push,
            );
            images_hint(position.long_text(), &mut push);
        }
        // „Hinweis ohne Text“ comes from „LV prüfen“ above.
        LvRow::Note(note) => {
            spelling_hint(
                speller,
                [
                    ("Überschrift", note.heading().to_string()),
                    ("Text", note.text().to_plain()),
                ],
                &mut push,
            );
            images_hint(note.text(), &mut push);
        }
        LvRow::Performance(performance) => {
            if performance.label().trim().is_empty() {
                push(
                    Severity::Warning,
                    "Ohne Bezeichnung.".to_string(),
                    Tab::General,
                );
            }
            let length = performance.label().chars().count();
            if length > struct_ops::PERFORMANCE_LABEL_LIMIT {
                push(
                    Severity::Warning,
                    format!(
                        "Bezeichnung hat {length} Zeichen – GAEB kürzt auf {}.",
                        struct_ops::PERFORMANCE_LABEL_LIMIT
                    ),
                    Tab::General,
                );
            }
            if performance.long_text().is_empty() {
                push(
                    Severity::Warning,
                    "Langtext leer.".to_string(),
                    Tab::General,
                );
            }
            if lv.positions_using(id).is_empty() {
                push(
                    Severity::Hint,
                    "Noch von keiner Position verwendet.".to_string(),
                    Tab::General,
                );
            }
            spelling_hint(
                speller,
                [
                    ("Bezeichnung", performance.label().to_string()),
                    ("Langtext", performance.long_text().to_plain()),
                ],
                &mut push,
            );
            images_hint(performance.long_text(), &mut push);
        }
        LvRow::Group(group) => {
            spelling_hint(
                speller,
                [("Text", group.short_text().to_string())],
                &mut push,
            );
        }
        LvRow::Spacer(_) => {}
    }
    findings.sort_by_key(|finding| finding.severity);
    findings
}

/// Most unknown words named in the Prüfleiste; the rest as „und n weitere“.
const SPELLING_LIMIT: usize = 8;

/// Unknown words (Rechtschreibung, `crate::spell`) as one hint: the word and
/// where it stands. Only when the check is on and a dictionary is loaded.
fn spelling_hint<const N: usize>(
    speller: Option<&proava_spell::Speller>,
    texts: [(&str, String); N],
    push: &mut impl FnMut(Severity, String, Tab),
) {
    let Some(speller) = speller else {
        return;
    };
    let found = proava_spell::collect_unknown(
        speller,
        texts
            .iter()
            .map(|(place, text)| (place.to_string(), text.as_str())),
    );
    if found.is_empty() {
        return;
    }
    let mut words: Vec<String> = found
        .iter()
        .take(SPELLING_LIMIT)
        .map(|(word, (_, places))| format!("„{word}“ ({})", places.join(", ")))
        .collect();
    if found.len() > SPELLING_LIMIT {
        words.push(format!("und {} weitere", found.len() - SPELLING_LIMIT));
    }
    push(
        Severity::Hint,
        format!(
            "{}: {} – Rechtschreibung prüfen (F7).",
            if found.len() == 1 {
                "Unbekanntes Wort"
            } else {
                "Unbekannte Wörter"
            },
            words.join(", ")
        ),
        Tab::General,
    );
}

fn images_hint(text: &Text, push: &mut impl FnMut(Severity, String, Tab)) {
    let count = text.images().count();
    if count > 0 {
        push(
            Severity::Hint,
            match count {
                1 => "1 Bild im Langtext – wird gedruckt; in GAEB DA XML nur mit „Bilder mitgeben“ \
                      (PDF-Ausschnitte als PNG), in GAEB 90 nicht."
                    .to_string(),
                count => format!(
                    "{count} Bilder im Langtext – werden gedruckt; in GAEB DA XML nur mit „Bilder mitgeben“ \
                     (PDF-Ausschnitte als PNG), in GAEB 90 nicht."
                ),
            },
            Tab::Graphics,
        );
    }
}

/// „Position 01.02.0010 ohne Menge.“ → „Ohne Menge.“: the window shows the
/// row already.
fn short_finding(text: &str, prefixes: &[&str]) -> String {
    let mut rest = text;
    for prefix in prefixes {
        if let Some(after) = text.strip_prefix(prefix) {
            rest = after.trim_start_matches(':').trim_start();
            break;
        }
    }
    let mut characters = rest.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => text.to_string(),
    }
}

/// Images of a long text in order: (block index, file name, width in mm).
pub(super) fn text_images(text: &Text) -> Vec<(usize, String, Option<f32>)> {
    text.blocks()
        .iter()
        .enumerate()
        .filter_map(|(at, block)| match block {
            TextBlock::Image { file, width_mm } => Some((at, file.clone(), *width_mm)),
            _ => None,
        })
        .collect()
}

/// The text without the image block `block` (the project keeps the file).
pub(super) fn without_image(text: &Text, block: usize) -> Text {
    let mut text = text.clone();
    if matches!(text.blocks().get(block), Some(TextBlock::Image { .. })) {
        text.blocks_mut().remove(block);
    }
    text
}

/// The text with a new print width (mm) for the image block `block`.
pub(super) fn with_image_width(text: &Text, block: usize, width: f32) -> Text {
    let mut text = text.clone();
    if let Some(TextBlock::Image { width_mm, .. }) = text.blocks_mut().get_mut(block) {
        *width_mm = Some(width.clamp(10.0, 180.0));
    }
    text
}

/// The text with an image at the end (Reiter Grafik).
pub(super) fn with_image_at_end(text: &Text, file: String) -> Text {
    let mut text = text.clone();
    text.push(TextBlock::Image {
        file,
        width_mm: Some(DEFAULT_IMAGE_WIDTH_MM),
    });
    text
}

/// Aufmaß of the Abrechnung for one position, by invoice in the order the
/// invoices came in, lines without invoice last: (heading, lines, sum of the
/// valid lines).
pub(super) fn billing_takeoff(
    billing: &proava_domain::Billing,
    position: RowId,
) -> Vec<(
    String,
    Vec<proava_domain::TakeoffLine>,
    proava_domain::Decimal,
)> {
    let lines = billing.takeoff_of(position);
    let mut groups = Vec::new();
    let mut push = |heading: String, invoice: Option<proava_domain::InvoiceId>| {
        let own: Vec<proava_domain::TakeoffLine> = lines
            .iter()
            .filter(|line| line.invoice == invoice)
            .cloned()
            .collect();
        if !own.is_empty() {
            let (sum, _) = billing.takeoff_sum(position, invoice);
            groups.push((heading, own, sum.normalize()));
        }
    };
    for invoice in billing.invoices() {
        let number = invoice.number.trim();
        push(
            if number.is_empty() {
                invoice.kind.label().to_string()
            } else {
                format!("{} {number}", invoice.kind.label())
            },
            Some(invoice.id),
        );
    }
    push("Keiner Rechnung zugeordnet".to_string(), None);
    groups
}

/// Where the Reiter chosen last is kept for the next window.
fn last_tab_id() -> egui::Id {
    egui::Id::unique("position_window_last_tab")
}

fn long_text_of(row: &LvRow) -> Option<&Text> {
    match row {
        LvRow::Position(position) => Some(position.long_text()),
        LvRow::Note(note) => Some(note.text()),
        LvRow::Performance(performance) => Some(performance.long_text()),
        LvRow::Group(_) | LvRow::Spacer(_) => None,
    }
}

type FindingsKey = (RowId, u64, u64, bool);

/// State of the window „Eigenschaften“.
pub(super) struct PositionWindow {
    pub(super) row: RowId,
    /// Reiter shown (the chosen one when the row has it, else Allgemein).
    tab: Tab,
    /// Reiter the user chose last: kept across rows that lack it (a title
    /// has no Grafik), so the next position shows it again.
    chosen_tab: Tab,
    /// „Festhalten“: the window keeps its row when the table selects another
    /// (to compare two positions). Not remembered; following is the default.
    pub(super) held: bool,
    editor: RichEditor,
    /// The editor showed the text of this row (images go to the caret).
    editor_row: Option<RowId>,
    /// Bring the window to the front on the next frame.
    raise: bool,
    /// Keyboard focus for the window after opening/raising (Esc, Tab work
    /// without a click first, Anwender 18.11.).
    focus: crate::window_focus::FocusRequest,
    /// Another row, taken once the focused field is left (its value first).
    pending: Option<RowId>,
    /// The window manager asked to close the window (cross, Alt+F4, window
    /// list): it closes once the focused field has taken its value.
    closing: bool,
    quantity: String,
    price: String,
    quantity_error: Option<String>,
    price_error: Option<String>,
    /// Row and editor revision the number fields were loaded for.
    numbers_for: Option<(RowId, u64)>,
    unit_chips: bool,
    /// „Bild einfügen“ inside this window.
    dialog: Option<FileDialog>,
    image_folder: Option<PathBuf>,
    /// „Ausschnitt aus PDF einfügen …“ inside this window.
    clip_dialog: Option<crate::pdf_clip_dialog::PdfClipDialog>,
    /// Image shown large on the Reiter Grafik.
    preview: Option<String>,
    /// Line of the Mengenermittlung whose first field gets the caret.
    takeoff_focus: Option<usize>,
    /// Caret into the Kurztext (titles: Text) on the next frame – after an
    /// explicit open (double click, Alt+Enter), not when following the table.
    focus_short: bool,
    textures: HashMap<String, Result<egui::TextureHandle, String>>,
    /// Findings and the state they were computed for (row, edits, revision,
    /// dictionary loaded).
    findings: Option<(FindingsKey, Vec<RowFinding>)>,
    /// Inner size and outer position of the window: last seen, since when,
    /// last remembered.
    place: WindowPlace,
    place_since: f64,
    saved: WindowPlace,
    /// Size and position were read from the settings.
    started: bool,
    /// Bezug of a Zuschlagsposition: search, range, Teilmengen being typed.
    surcharge: super::surcharge_refs::SurchargeInput,
}

impl PositionWindow {
    fn new(row: RowId) -> Self {
        let mut editor = RichEditor::new("position_window");
        editor.pdf_clips = true;
        editor.min_height = 320.0;
        editor.zoom = 1.05;
        Self {
            row,
            tab: Tab::General,
            chosen_tab: Tab::General,
            held: false,
            editor,
            editor_row: None,
            raise: true,
            focus: crate::window_focus::FocusRequest::default(),
            pending: None,
            closing: false,
            quantity: String::new(),
            price: String::new(),
            quantity_error: None,
            price_error: None,
            numbers_for: None,
            unit_chips: false,
            dialog: None,
            image_folder: None,
            clip_dialog: None,
            preview: None,
            takeoff_focus: None,
            focus_short: true,
            textures: HashMap::new(),
            findings: None,
            place: WindowPlace::default(),
            place_since: 0.0,
            saved: WindowPlace::default(),
            started: false,
            surcharge: Default::default(),
        }
    }

    /// Shows another row (same Reiter when it has it).
    fn show_row(&mut self, row: RowId) {
        if self.row != row {
            self.row = row;
            self.editor.reset();
            self.editor_row = None;
            self.numbers_for = None;
            self.preview = None;
            self.unit_chips = false;
            self.surcharge = Default::default();
        }
    }

    /// Another LV became the active one: show its row `row` right away and
    /// load every field fresh (row ids are only unique within one LV), so
    /// nothing of the row before can reach the row with the same id there.
    pub(super) fn other_lv(&mut self, row: RowId) {
        self.row = row;
        self.held = false;
        self.pending = None;
        self.editor.reset();
        self.editor_row = None;
        self.numbers_for = None;
        self.quantity_error = None;
        self.price_error = None;
        self.preview = None;
        self.unit_chips = false;
        self.findings = None;
    }

    /// The LV changed from outside (undo, another LV): load the fields again.
    pub(super) fn reload(&mut self) {
        self.editor.reset();
        self.numbers_for = None;
        self.surcharge = Default::default();
    }

    pub(super) fn viewport_id() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("proava-position-window")
    }

    fn open_clip_dialog(&mut self, launcher: &Launcher) {
        if self.clip_dialog.is_some() || self.dialog.is_some() {
            return;
        }
        let start = self
            .image_folder
            .clone()
            .or_else(|| launcher.last_folder())
            .unwrap_or_else(|| launcher.home_folder());
        self.clip_dialog = Some(crate::pdf_clip_dialog::PdfClipDialog::open(launcher, start));
    }

    fn open_image_dialog(&mut self, launcher: &Launcher) {
        if self.dialog.is_some() {
            return;
        }
        let start = self
            .image_folder
            .clone()
            .or_else(|| launcher.last_folder())
            .unwrap_or_else(|| launcher.home_folder());
        self.dialog = Some(FileDialog::open(
            launcher,
            "Bild einfügen",
            "PNG oder JPEG – mit Strg+Klick mehrere wählen.",
            start,
            &strings::IMAGE_EXTENSIONS,
            true,
        ));
    }
}

/// What happened in the window in one frame (applied after drawing).
#[derive(Default)]
struct Frame {
    close: bool,
    step: Option<bool>,
    undo: bool,
    redo: bool,
    /// Strg+S: save the project (the main window does it).
    save: bool,
    /// Strg+Umschalt+F / Strg+Umschalt+E: status of the window's position.
    status: Option<StatusKey>,
    files: Vec<(String, Vec<u8>)>,
    /// A PDF dropped on the window: choose the part of a page.
    pdf: Option<(String, Vec<u8>)>,
    /// The PDF clip chosen in the window and its width in mm.
    clip: Option<(proava_application::PreparedClip, f32)>,
    toast: Option<String>,
    /// F9, F10 … in the window: a new row after the window's row.
    new_row: Option<super::context_menu::MenuChoice>,
}

#[derive(Clone, Copy)]
enum StatusKey {
    ToggleDone,
    InProgress,
}

impl LvEditor {
    /// Opens the window „Eigenschaften“ for `row` or brings it to the front.
    pub(super) fn open_position_window(&mut self, row: RowId) {
        match &mut self.position_window {
            Some(window) => {
                window.show_row(row);
                window.pending = None;
                window.raise = true;
                window.focus_short = true;
            }
            None => self.position_window = Some(PositionWindow::new(row)),
        }
    }

    /// The edit panel while its row is in the window: a short note instead
    /// of the same fields twice.
    pub(super) fn popped_out(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        index: usize,
    ) {
        let row = &project.lv().rows()[index];
        ui.label(
            RichText::new(format!(
                "{} {}",
                kind_word(project.lv(), row),
                match row {
                    LvRow::Performance(performance) => performance.mark(),
                    other => other.ordinal().to_string(),
                }
            ))
            .size(15.0)
            .strong()
            .color(palette.text),
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new("Steht im Fenster „Eigenschaften“ – dort bearbeiten.")
                .size(13.0)
                .color(palette.text_dim),
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Fenster nach vorn").clicked() {
                if let Some(window) = &mut self.position_window {
                    window.raise = true;
                }
            }
            if ui
                .button("Hier bearbeiten")
                .on_hover_text("Schließt das Fenster „Eigenschaften“")
                .clicked()
            {
                self.close_position_window = true;
            }
        });
    }

    /// Row shown in the window „Langtext“ (Umschalt+F2), if it is open.
    pub fn long_text_window_row(&self) -> Option<RowId> {
        self.big_text.as_ref().map(|big| big.row)
    }

    /// Whether the window „Eigenschaften“ is open (and for which row).
    pub fn position_window_row(&self) -> Option<RowId> {
        self.position_window.as_ref().map(|window| window.row)
    }

    /// Draws the window „Eigenschaften“ (every frame while it is open).
    pub(super) fn position_window_ui(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        output: &mut EditorOutput,
    ) {
        let Some(mut window) = self.position_window.take() else {
            return;
        };
        if std::mem::take(&mut self.close_position_window) {
            launcher.set_position_window(window.place);
            // Typed values the fields refused go with the window.
            self.revision += 1;
            return;
        }
        if !window.started {
            window.started = true;
            window.saved = launcher.position_window();
            window.place = window.saved;
            // The Reiter chosen last (this session).
            if let Some(tab) = ctx.data(|data| data.get_temp::<Tab>(last_tab_id())) {
                window.tab = tab;
                window.chosen_tab = tab;
            }
        }
        // The window follows the selection of the table – click, keys, search
        // hits, Prüfliste, the bridge; with several rows the active one –
        // unless it is held. The table keeps the keyboard.
        if !window.held {
            if let Some(selected) = self.selected {
                if selected != window.row && project.lv().index_of(selected).is_some() {
                    window.pending = Some(selected);
                }
            }
        }
        let Some(index) = project.lv().index_of(window.row) else {
            // The row is gone (deleted, another LV): the window closes.
            launcher.set_position_window(window.place);
            return;
        };
        let row = &project.lv().rows()[index];
        let title = window_title(project.lv(), row);
        let size = window.saved.size.unwrap_or(DEFAULT_SIZE);
        let mut builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([size.0, size.1])
            .with_min_inner_size(MIN_SIZE)
            .with_window_level(self.window_level);
        if let Some((x, y)) = window.saved.at {
            builder = builder.with_position([x, y]);
        }
        if std::mem::take(&mut window.raise) {
            builder = builder.with_active(true);
            ctx.send_viewport_cmd_to(PositionWindow::viewport_id(), egui::ViewportCommand::Focus);
            // That request comes before the window is mapped and may be lost;
            // the window asks again from inside until it has the focus.
            window.focus.start();
        }

        let mut frame = Frame::default();
        ctx.show_viewport_immediate(PositionWindow::viewport_id(), builder, |ui, class| {
            let ctx = ui.ctx().clone();
            if ctx.input(|input| input.viewport().close_requested()) {
                window.closing = true;
            }
            let embedded = class == egui::ViewportClass::EmbeddedWindow;
            if !embedded {
                remember_place(&ctx, &mut window, launcher);
                window.focus.keep(&ctx);
            }
            window_input(&ctx, &mut window, &mut frame, embedded);
            // The cross (and Alt+F4) close like the button: a field being
            // typed in first takes its value (it is left), the window closes
            // in the next frame – Esc and the button leave the field the same
            // way before they close.
            if window.closing {
                if ctx.text_edit_focused() {
                    ctx.memory_mut(|memory| {
                        if let Some(focused) = memory.focused() {
                            memory.surrender_focus(focused);
                        }
                    });
                    ctx.request_repaint();
                } else {
                    frame.close = true;
                }
            }
            // Another row: first let the focused field go (it takes its value).
            if let Some(next) = window.pending {
                if ctx.text_edit_focused() {
                    ctx.memory_mut(|memory| {
                        if let Some(focused) = memory.focused() {
                            memory.surrender_focus(focused);
                        }
                    });
                    ctx.request_repaint();
                } else {
                    window.pending = None;
                    window.show_row(next);
                }
            }
            if let Some((name, bytes)) = frame.pdf.take() {
                if window.clip_dialog.is_none() {
                    match crate::pdf_clip_dialog::PdfClipDialog::with_pdf(launcher, &name, bytes) {
                        Ok(dialog) => window.clip_dialog = Some(dialog),
                        Err(error) => frame.toast = Some(error),
                    }
                }
            }
            if let Some(dialog) = &mut window.clip_dialog {
                match dialog.show(ui, palette, launcher) {
                    crate::pdf_clip_dialog::ClipDialogResult::Open => {}
                    crate::pdf_clip_dialog::ClipDialogResult::Cancelled => {
                        window.image_folder = dialog.folder.clone().or(window.image_folder.take());
                        window.clip_dialog = None;
                    }
                    crate::pdf_clip_dialog::ClipDialogResult::Chosen(prepared, width) => {
                        window.image_folder = dialog.folder.clone().or(window.image_folder.take());
                        frame.clip = Some((*prepared, width));
                        window.clip_dialog = None;
                    }
                }
            }
            if let Some(dialog) = &mut window.dialog {
                match dialog.show(ui, palette, launcher) {
                    DialogResult::Open => {}
                    DialogResult::Cancelled => window.dialog = None,
                    DialogResult::Chosen(paths) => {
                        for path in &paths {
                            if let Ok(bytes) = launcher.read_file(path) {
                                frame
                                    .files
                                    .push((crate::file_dialog::file_name(path), bytes));
                            }
                        }
                        window.image_folder = paths
                            .first()
                            .and_then(|path| path.parent())
                            .map(std::path::Path::to_path_buf);
                        window.dialog = None;
                    }
                }
            }
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(palette.background)
                        .inner_margin(egui::Margin::same(12)),
                )
                .show(ui, |ui| {
                    if self.panel_row == Some(window.row) {
                        // The edit panel showed this row in this frame (just
                        // opened or switched): the fields come next frame.
                        ui.ctx().request_repaint();
                    } else if let Some(index) = project.lv().index_of(window.row) {
                        // (The row may have changed above.)
                        self.window_body(
                            ui,
                            palette,
                            project,
                            launcher,
                            &mut window,
                            index,
                            &mut frame,
                            output,
                        );
                    }
                });
        });

        if !frame.files.is_empty() {
            output.merge(self.window_insert_images(project, &mut window, frame.files));
        }
        if let Some(message) = frame.toast.take() {
            output.merge(EditorOutput::toast(message));
        }
        if let Some((prepared, width)) = frame.clip.take() {
            output.merge(self.window_insert_pdf_clip(project, &mut window, prepared, width));
        }
        if frame.save {
            output.save = true;
        }
        if let Some(key) = frame.status {
            let row = window.row;
            let locked = project
                .lv()
                .position_by_id(row)
                .map(|position| position.is_locked());
            if let Some(locked) = locked {
                let status = match key {
                    StatusKey::InProgress => EditStatus::InProgress,
                    StatusKey::ToggleDone if locked => EditStatus::Open,
                    StatusKey::ToggleDone => EditStatus::Done,
                };
                output.merge(self.set_status(project, &[row], status));
            }
        }
        if frame.undo || frame.redo {
            let command = if frame.undo {
                Command::Undo
            } else {
                Command::Redo
            };
            output.merge(self.run(command, project, ctx));
        }
        if let Some(choice) = frame.new_row {
            // After the window's row; the window follows the selection and
            // shows the new row. The take-over dialogs open in the main window.
            self.select(project.lv(), window.row);
            output.merge(self.run_choice(choice, project, ctx));
            if matches!(
                choice,
                super::context_menu::MenuChoice::Editor(
                    Command::FromStlb | Command::FromAusschreiben
                )
            ) {
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
            }
        }
        if let Some(forward) = frame.step {
            if let Some(next) = project
                .lv()
                .index_of(window.row)
                .and_then(|index| neighbour(project.lv(), index, forward))
            {
                let id = project.lv().rows()[next].id();
                self.select(project.lv(), id);
                window.show_row(id);
            } else {
                output.toast = Some(if forward {
                    "Das ist die letzte.".to_string()
                } else {
                    "Das ist die erste.".to_string()
                });
            }
        }
        if frame.close {
            launcher.set_position_window(window.place);
            // Typed values the fields refused go with the window.
            self.revision += 1;
        } else {
            self.position_window = Some(window);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn window_body(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        window: &mut PositionWindow,
        index: usize,
        frame: &mut Frame,
        output: &mut EditorOutput,
    ) {
        let row = project.lv().rows()[index].clone();
        let id = row.id();
        let tabs = tabs_for(&row);
        window.tab = if tabs.contains(&window.chosen_tab) {
            window.chosen_tab
        } else if tabs.contains(&window.tab) {
            window.tab
        } else {
            Tab::General
        };
        let locked = row
            .as_position()
            .is_some_and(|position| position.is_locked());

        // Kopf: Art und OZ, Schließen. Positionen, Hinweise und
        // Ausführungsbeschreibungen wählen ihre Zeilenart hier (Umwandeln).
        let mut convert_to = None;
        crate::widgets::row(ui, |ui| {
            match proava_domain::RowKind::of(&row) {
                Some(kind) => {
                    convert_to = super::convert::kind_choice(ui, palette, kind, id, !locked);
                }
                None => {
                    ui.label(
                        RichText::new(kind_word(project.lv(), &row))
                            .size(13.0)
                            .color(palette.text_dim),
                    );
                }
            }
            let number = match &row {
                LvRow::Performance(performance) => performance.mark(),
                other => other.ordinal().to_string(),
            };
            ui.label(
                RichText::new(number)
                    .size(16.0)
                    .strong()
                    .monospace()
                    .color(palette.text),
            );
            if let Some(position) = row.as_position() {
                for mark in position.marks() {
                    ui.label(
                        RichText::new(mark)
                            .size(11.5)
                            .color(palette.accent)
                            .background_color(palette.accent_soft),
                    );
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_label_button(ui, palette, Icon::Close, "Schließen (Esc)")
                    .on_hover_text("Änderungen sind schon übernommen")
                    .clicked()
                {
                    frame.close = true;
                }
                ui.add_space(8.0);
                ui.checkbox(&mut window.held, strings::HOLD_WINDOW)
                    .on_hover_text(strings::HOLD_WINDOW_HINT);
            });
        });
        if let Some(kind) = convert_to {
            output.merge(self.request_conversion(project, index, kind));
            if self.dialog.is_some() {
                // The question shows in the main window: bring it forward.
                ui.ctx()
                    .send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
            }
            // Tabs and fields belong to the old kind: next frame.
            return;
        }
        ui.add_space(4.0);
        // Blättern und Status.
        let (previous, next) = match &row {
            LvRow::Position(_) => ("Vorige Position", "Nächste Position"),
            LvRow::Group(_) => ("Voriger Titel", "Nächster Titel"),
            LvRow::Note(_) => ("Voriger Hinweis", "Nächster Hinweis"),
            LvRow::Performance(_) => ("Vorige Beschreibung", "Nächste Beschreibung"),
            LvRow::Spacer(_) => ("Voriger Abstandshalter", "Nächster Abstandshalter"),
        };
        ui.horizontal(|ui| {
            let lv = project.lv();
            for (forward, icon, label, key) in [
                (false, Icon::Up, previous, "Bild↑"),
                (true, Icon::Down, next, "Bild↓"),
            ] {
                let enabled = neighbour(lv, index, forward).is_some();
                let clicked = ui
                    .add_enabled_ui(enabled, |ui| {
                        widgets::icon_label_button(ui, palette, icon, label)
                            .on_hover_text(key)
                            .clicked()
                    })
                    .inner;
                if clicked && enabled {
                    frame.step = Some(forward);
                }
            }
            if let Some(position) = row.as_position() {
                ui.add_space(16.0);
                if let Some(status) = status_selector(ui, palette, position.status(), id) {
                    output.merge(self.set_status(project, &[id], status));
                }
            }
            // The note only where it has room (narrow window: the buttons).
            let note = RichText::new("Änderungen wirken sofort – Strg+Z macht sie rückgängig")
                .size(11.5)
                .color(palette.text_dim);
            let width = egui::WidgetText::from(note.clone())
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Body,
                )
                .size()
                .x;
            if ui.available_width() >= width + 16.0 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(note);
                });
            }
        });
        ui.add_space(6.0);
        let labelled: Vec<(Tab, &str)> = tabs.iter().map(|tab| (*tab, tab.label())).collect();
        if form_kit::tab_bar(ui, palette, &labelled, &mut window.tab) {
            let tab = window.tab;
            window.chosen_tab = tab;
            ui.ctx()
                .data_mut(|data| data.insert_temp(last_tab_id(), tab));
        }

        // Inhalt links, Prüfleiste rechts.
        let area = ui.available_rect_before_wrap();
        let check_width = CHECK_WIDTH.min(area.width() * 0.34);
        let content = egui::Rect::from_min_max(
            area.min,
            egui::pos2(area.right() - check_width - 14.0, area.bottom()),
        );
        let check =
            egui::Rect::from_min_max(egui::pos2(area.right() - check_width, area.top()), area.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("position_window_scroll", window.tab))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width() - 8.0);
                    if locked {
                        locked_banner(ui, palette);
                        ui.disable();
                    }
                    match (window.tab, &row) {
                        (Tab::General, LvRow::Position(_)) => self.window_position_general(
                            ui, palette, project, launcher, window, index, output,
                        ),
                        (Tab::General, LvRow::Group(_)) => {
                            self.window_group_general(ui, palette, project, window, index, output)
                        }
                        (Tab::General, LvRow::Note(note)) => {
                            self.note_extra(ui, palette, project, index, note, output);
                            self.window_long_text(
                                ui, palette, project, launcher, window, index, output,
                            );
                        }
                        (Tab::General, LvRow::Performance(_)) => self.window_performance_general(
                            ui, palette, project, launcher, window, index, output,
                        ),
                        (Tab::General, LvRow::Spacer(_)) => {
                            ui.label(
                                RichText::new(
                                    "Leerzeile nur für die Bearbeitung: keine OZ, kein Text, kein Preis. Erscheint \
                                     weder im Druck noch in GAEB oder CSV.",
                                )
                                .size(13.0)
                                .color(palette.text_dim),
                            );
                        }
                        (Tab::More, _) => {
                            self.window_position_more(ui, palette, project, index, output)
                        }
                        (Tab::Takeoff, _) => {
                            self.window_takeoff(ui, palette, project, window, index, output)
                        }
                        (Tab::Graphics, _) => self
                            .window_graphics(ui, palette, project, launcher, window, index, output),
                    }
                });
        });
        ui.scope_builder(egui::UiBuilder::new().max_rect(check), |ui| {
            self.check_bar(ui, palette, project, window, id);
        });
        // Only right after opening: a Kurztext not shown now gets no caret later.
        window.focus_short = false;
        ui.allocate_rect(area, Sense::hover());
    }

    #[allow(clippy::too_many_arguments)]
    fn window_position_general(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        window: &mut PositionWindow,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let Some(position) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        let id = position.id();
        let ctx = ui.ctx().clone();
        if position.is_stlb_placeholder() {
            ui.label(
                RichText::new(strings::WS_STLB_DETAIL)
                    .size(12.5)
                    .color(palette.accent),
            );
            ui.add_space(4.0);
            if ui
                .button(strings::STLB_TAKE_BUTTON)
                .on_hover_text(strings::STLB_TAKE_HINT)
                .clicked()
            {
                output.open_stlb = true;
            }
            ui.add_space(8.0);
        }
        self.ordinal_field(ui, palette, project, index, output);
        // The link to an Ausführungsbeschreibung, seen at once (not only on „Weiteres“).
        self.performance_link_line(ui, palette, project, id, output);

        // Kurztext (the caption names the field for screen readers and tests)
        let label = ui.label(RichText::new("Kurztext").size(12.0).color(palette.text_dim));
        ui.add_space(3.0);
        let mut short = position.short_text().to_string();
        let edit = egui::TextEdit::singleline(&mut short)
            .id(egui::Id::unique(("position_window_short", id)))
            .desired_width(f32::INFINITY)
            .font(FontId::proportional(15.0))
            .margin(egui::Margin::symmetric(6, 5))
            .show(ui);
        let mut response = edit.response.response.clone().labelled_by(label.id);
        if std::mem::take(&mut window.focus_short) {
            response.request_focus();
        }
        // Wavy lines and suggestions like the Kurztext of the edit panel.
        if crate::spell::text_edit(ui, &edit, &mut short) {
            response.mark_changed();
        }
        let length = short.chars().count();
        ui.label(
            RichText::new(format!("{length}/{SHORT_TEXT_LIMIT}"))
                .size(11.5)
                .color(if length > SHORT_TEXT_LIMIT {
                    palette.error
                } else {
                    palette.text_dim
                }),
        );
        if response.changed() {
            self.undo
                .record(project.lv(), Some(EditKey(format!("short:{}", id.0))));
            if let Some(position) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                position.set_short_text(short);
            }
            output.dirty = true;
        }
        ui.add_space(8.0);

        // Menge | Einheit | EP | GP
        if window.numbers_for != Some((id, self.revision)) {
            window.numbers_for = Some((id, self.revision));
            window.quantity = numbers::quantity_entry_shown(&position);
            window.price = numbers::price_text(position.unit_price());
            window.quantity_error = None;
            window.price_error = None;
        }
        let mut unit = position.unit().to_string();
        let mut unit_value: Option<String> = None;
        let mut unit_focused = false;
        let mut quantity_edit = NumberEdit::default();
        let mut price_edit = NumberEdit::default();
        // The unit belongs to the STLB-Bau text (write-protected).
        let unit_locked = position.is_text_locked();
        let mut unit_toast = None;
        // A Zuschlagsposition: Bezugssumme instead of Menge, „%“ as unit,
        // the percentage as EP, GP = Bezugssumme × Prozentsatz.
        let surcharge_value = position.is_surcharge().then(|| {
            project
                .lv()
                .surcharge_values()
                .get(&id)
                .copied()
                .unwrap_or_default()
        });
        ui.columns(4, |columns| {
            if let Some(value) = surcharge_value {
                read_only_box(
                    &mut columns[0],
                    palette,
                    "Bezugssumme",
                    value.base.plain(),
                    "Summe der Positionen, auf die der Zuschlag gerechnet wird",
                );
            } else {
                quantity_edit = number_field(
                    &mut columns[0],
                    palette,
                    "Menge",
                    &mut window.quantity,
                    window.quantity_error.is_some(),
                    ("position_window_qty", id),
                );
            }
            if surcharge_value.is_some() {
                read_only_box(
                    &mut columns[1],
                    palette,
                    "Einheit",
                    "%".to_string(),
                    "Eine Zuschlagsposition hat keine Menge und Einheit",
                );
            } else {
                let unit_too_long = unit.trim().chars().count() > lv_check::UNIT_LIMIT;
                columns[1].label(RichText::new("Einheit").size(12.0).color(if unit_too_long {
                    palette.warn
                } else {
                    palette.text_dim
                }));
                columns[1].add_space(3.0);
                let (response, toast) = super::stlb_lock::locked_or_free(
                    &mut columns[1],
                    egui::Id::unique(("position_window_unit_lock", id)),
                    unit_locked,
                    |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut unit)
                                .id(egui::Id::unique(("position_window_unit", id)))
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::symmetric(6, 5)),
                        )
                    },
                );
                unit_toast = toast;
                select_all_on_focus(&response, &unit);
                if !unit_locked {
                    unit_value = crate::widgets::live_text(&response, &unit, position.unit());
                }
                unit_focused = response.has_focus() && !unit_locked;
            }
            price_edit = number_field(
                &mut columns[2],
                palette,
                &if surcharge_value.is_some() {
                    "Prozentsatz (%)".to_string()
                } else {
                    format!(
                        "EP ({})",
                        proava_domain::price_check::currency_symbol(project.lv().currency())
                    )
                },
                &mut window.price,
                window.price_error.is_some(),
                ("position_window_price", id),
            );
            columns[3].label(
                RichText::new(format!(
                    "GP ({})",
                    proava_domain::price_check::currency_symbol(project.lv().currency())
                ))
                .size(12.0)
                .color(palette.text_dim),
            );
            columns[3].add_space(3.0);
            let total = match surcharge_value {
                Some(value) => value.amount.plain(),
                None => position.total_price().plain(),
            };
            let counted = position.kind().counts_in_total();
            let column = &mut columns[3];
            let (rect, response) = column
                .allocate_exact_size(egui::vec2(column.available_width(), 26.0), Sense::hover());
            column.painter().rect(
                rect,
                egui::CornerRadius::same(4),
                palette.panel_alt,
                Stroke::new(1.0, palette.line),
                egui::StrokeKind::Inside,
            );
            column.painter().text(
                rect.right_center() - egui::vec2(7.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                if counted { total } else { format!("({total})") },
                FontId::monospace(13.0),
                if counted {
                    palette.text
                } else {
                    palette.text_dim
                },
            );
            response.on_hover_text(if surcharge_value.is_some() {
                "Bezugssumme × Prozentsatz – wird gerechnet"
            } else {
                "Menge × EP – wird gerechnet"
            });
        });
        if surcharge_value.is_some() {
            self.surcharge_references(ui, palette, project, index, &mut window.surcharge, output);
        }
        if unit.trim().chars().count() > lv_check::UNIT_LIMIT {
            ui.label(
                RichText::new(format!(
                    "Einheit: GAEB überträgt höchstens {} Zeichen.",
                    lv_check::UNIT_LIMIT
                ))
                .size(12.0)
                .color(palette.warn),
            );
        }
        for error in [&window.quantity_error, &window.price_error]
            .into_iter()
            .flatten()
        {
            ui.label(
                RichText::new(error.as_str())
                    .size(12.0)
                    .color(palette.error),
            );
        }
        // Einheiten zur Auswahl, solange das Feld den Fokus hat.
        let (pointer_down, pointer_released) =
            ctx.input(|input| (input.pointer.any_down(), input.pointer.any_released()));
        window.unit_chips =
            unit_focused || (window.unit_chips && (pointer_down || pointer_released));
        if window.unit_chips {
            let needle = unit.trim().to_lowercase();
            ui.horizontal_wrapped(|ui| {
                for suggestion in UNITS {
                    if !needle.is_empty()
                        && !suggestion.to_lowercase().starts_with(&needle)
                        && suggestion.to_lowercase() != needle
                    {
                        continue;
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new(suggestion).size(12.5))
                                .min_size(egui::vec2(34.0, 22.0)),
                        )
                        .clicked()
                    {
                        unit = suggestion.to_string();
                        unit_value = Some(unit.clone());
                        ctx.memory_mut(|memory| {
                            memory.request_focus(egui::Id::unique(("position_window_price", id)))
                        });
                    }
                }
            });
        }
        if let Some(toast) = unit_toast {
            output.toast = Some(toast);
        }
        if let Some(unit) = unit_value {
            self.undo
                .record(project.lv(), Some(EditKey(format!("unit:{}", id.0))));
            if let Some(position) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                position.set_unit(&unit);
            }
            output.dirty = true;
        }
        if let Some((value, formula)) = quantity_edit.apply(
            &window.quantity,
            &mut window.quantity_error,
            numbers::parse_quantity_entry,
            "Menge",
        ) {
            if value != position.quantity() || formula.as_deref() != position.quantity_formula() {
                self.undo
                    .record(project.lv(), Some(EditKey(format!("qty:{}", id.0))));
                if let Some(position) = project
                    .lv_mut()
                    .row_mut(index)
                    .and_then(LvRow::as_position_mut)
                {
                    position.set_quantity(value);
                    position.set_quantity_formula(formula.clone());
                }
                output.dirty = true;
            }
            if quantity_edit.left && formula.is_none() {
                window.quantity = numbers::quantity_text(value);
            }
        }
        // Tab from the EP goes on into the Langtext (past the toolbar) – not
        // into a write-protected STLB-Bau text (the free fields follow).
        if price_edit.left
            && !unit_locked
            && ui.input(|input| input.key_pressed(egui::Key::Tab) && !input.modifiers.shift)
        {
            window.editor.request_focus();
        }
        if let Some(value) = price_edit.apply(
            &window.price,
            &mut window.price_error,
            numbers::parse_price,
            "EP",
        ) {
            if value != position.unit_price() {
                self.undo
                    .record(project.lv(), Some(EditKey(format!("price:{}", id.0))));
                if let Some(position) = project
                    .lv_mut()
                    .row_mut(index)
                    .and_then(LvRow::as_position_mut)
                {
                    position.set_unit_price(value);
                }
                output.dirty = true;
            }
            if price_edit.left {
                window.price = numbers::price_text(value);
            }
        }
        // Rechenansatz and a GP outside the sum, from the stored values.
        if let Some(current) = project.lv().rows()[index].as_position() {
            ui.horizontal_wrapped(|ui| {
                if !current.kind().counts_in_total() {
                    ui.label(
                        RichText::new(format!(
                            "{} – der GP zählt nicht in die Summe.",
                            kind_label(current.kind())
                        ))
                        .size(12.0)
                        .color(palette.text_dim),
                    );
                    ui.add_space(16.0);
                }
                if let Some(formula) = current.quantity_formula() {
                    ui.label(
                        RichText::new(format!(
                            "Rechenansatz {formula} = {}",
                            numbers::quantity_text(current.quantity())
                        ))
                        .size(12.0)
                        .color(palette.text_dim),
                    );
                }
                if !current.takeoff().is_empty() {
                    let (sum, _) = current.takeoff_total();
                    let differs = current.takeoff_differs().is_some();
                    let text = format!(
                        "Mengenermittlung: {}, Summe {}{}",
                        crate::format::count(current.takeoff().len(), "Zeile", "Zeilen"),
                        numbers::quantity_text(sum),
                        if differs { " – weicht ab" } else { "" }
                    );
                    if ui
                        .add(
                            egui::Label::new(RichText::new(text).size(12.0).color(if differs {
                                palette.warn
                            } else {
                                palette.text_dim
                            }))
                            .sense(Sense::click()),
                        )
                        .on_hover_text("Zum Reiter Mengenermittlung")
                        .clicked()
                    {
                        window.tab = Tab::Takeoff;
                        window.chosen_tab = Tab::Takeoff;
                    }
                }
            });
        }
        ui.add_space(10.0);
        self.window_long_text(ui, palette, project, launcher, window, index, output);
    }

    fn window_group_general(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        window: &mut PositionWindow,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let Some(group) = project.lv().rows()[index].as_group().cloned() else {
            return;
        };
        let id = group.id();
        self.ordinal_field(ui, palette, project, index, output);
        caption(ui, palette, "Text");
        let mut text = group.short_text().to_string();
        let response = ui.add(
            egui::TextEdit::singleline(&mut text)
                .id(egui::Id::unique(("position_window_title", id)))
                .desired_width(f32::INFINITY)
                .font(FontId::proportional(15.0))
                .margin(egui::Margin::symmetric(6, 5)),
        );
        if std::mem::take(&mut window.focus_short) {
            response.request_focus();
        }
        if response.changed() {
            self.undo
                .record(project.lv(), Some(EditKey(format!("short:{}", id.0))));
            if let Some(LvRow::Group(group)) = project.lv_mut().row_mut(index) {
                group.set_short_text(text);
            }
            output.dirty = true;
        }
        ui.add_space(10.0);
        let levels = project.lv().level_count();
        let mut shift = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Stufe {} von {}", group.level() + 1, levels.max(1)))
                    .size(13.0)
                    .color(palette.text),
            );
            ui.add_space(8.0);
            if ui
                .add_enabled(group.level() > 0, egui::Button::new("Höher"))
                .clicked()
            {
                shift = Some(Command::Higher);
            }
            if ui
                .add_enabled(group.level() + 1 < levels, egui::Button::new("Tiefer"))
                .clicked()
            {
                shift = Some(Command::Deeper);
            }
        });
        if let Some(command) = shift {
            self.selected = Some(id);
            self.single(id);
            output.merge(self.run(command, project, &ui.ctx().clone()));
        }
        self.cost_group_field(ui, palette, project, &[id], output);
        self.point_section_summary(ui, palette, project, index);
        ui.add_space(10.0);
        self.work_category_field(ui, palette, project, index, output);
        ui.add_space(10.0);
        let total = project
            .lv()
            .group_totals()
            .into_iter()
            .find(|total| total.group_row == index)
            .map(|total| total.total)
            .unwrap_or(Money::ZERO);
        let range = project.lv().block_range(index);
        let positions = project.lv().rows()[range.clone()]
            .iter()
            .filter(|row| row.is_position())
            .count();
        ui.label(
            RichText::new(format!(
                "Summe {} · {}",
                proava_domain::price_check::amount(total, project.lv().currency()),
                crate::format::count(positions, "Position", "Positionen")
            ))
            .size(13.0)
            .color(palette.text),
        );
        if positions > 0 {
            ui.add_space(10.0);
            let counts = project.lv().status_counts_in(range);
            ui.label(
                RichText::new(format!(
                    "Bearbeitungsstand: {} von {} fertig, {} in Bearbeitung",
                    counts.done,
                    counts.total(),
                    counts.in_progress
                ))
                .size(12.5)
                .color(palette.text_dim),
            );
            ui.add_space(4.0);
            let mut chosen = None;
            ui.horizontal_wrapped(|ui| {
                for status in [EditStatus::Done, EditStatus::InProgress, EditStatus::Open] {
                    if ui
                        .button(format!("Alle {}", status.label()))
                        .on_hover_text("Setzt den Status aller Positionen unter diesem Titel")
                        .clicked()
                    {
                        chosen = Some(status);
                    }
                }
            });
            if let Some(status) = chosen {
                output.merge(self.set_status(project, &[id], status));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn window_performance_general(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        window: &mut PositionWindow,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let Some(performance) = project.lv().rows()[index].as_performance().cloned() else {
            return;
        };
        let id = performance.id();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(140.0);
                self.performance_number(ui, palette, project, &performance, output, "window");
            });
            ui.vertical(|ui| {
                caption(ui, palette, "Bezeichnung");
                let mut label = performance.label().to_string();
                if fields::text_field(
                    ui,
                    &mut label,
                    egui::Id::unique(("position_window_performance_label", id)),
                    "z. B. Kabeltrassen",
                ) {
                    self.edit_window_performance(project, index, "label", |p| p.set_label(label));
                    output.dirty = true;
                }
            });
        });
        ui.add_space(8.0);
        caption(ui, palette, "Kurztext (optional)");
        let mut short = performance.short_text().to_string();
        if fields::text_field(
            ui,
            &mut short,
            egui::Id::unique(("position_window_performance_short", id)),
            "",
        ) {
            self.edit_window_performance(project, index, "short", |p| p.set_short_text(short));
            output.dirty = true;
        }
        ui.add_space(8.0);
        let users = project.lv().positions_using(id);
        caption(
            ui,
            palette,
            &match users.len() {
                0 => "Noch von keiner Position verwendet".to_string(),
                1 => "Verwendet von 1 Position".to_string(),
                count => format!("Verwendet von {count} Positionen"),
            },
        );
        let mut jump = None;
        egui::ScrollArea::vertical()
            .id_salt(("position_window_users", id))
            .max_height(120.0)
            .show(ui, |ui| {
                for user in &users {
                    if let Some(position) = project.lv().position_by_id(*user) {
                        let text = format!("{}  {}", position.ordinal(), position.short_text());
                        if ui
                            .add(egui::Button::new(RichText::new(text).size(12.5)).frame(false))
                            .on_hover_text("Diese Position zeigen")
                            .clicked()
                        {
                            jump = Some(*user);
                        }
                    }
                }
            });
        if let Some(target) = jump {
            self.select(project.lv(), target);
            window.pending = Some(target);
        }
        ui.add_space(10.0);
        self.window_long_text(ui, palette, project, launcher, window, index, output);
    }

    fn edit_window_performance(
        &mut self,
        project: &mut Project,
        index: usize,
        field: &str,
        change: impl FnOnce(&mut proava_domain::PerformanceDescription),
    ) {
        let id = project.lv().rows()[index].id();
        self.undo
            .record(project.lv(), Some(EditKey(format!("{field}:{}", id.0))));
        if let Some(LvRow::Performance(performance)) = project.lv_mut().row_mut(index) {
            change(performance);
        }
    }

    /// Reiter „Weiteres“ of a position.
    fn window_position_more(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let Some(position) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        let id = position.id();

        form_kit::section(ui, palette, "Positionsart");
        let mut kind = position.kind();
        let mut lump = position.is_lump_sum();
        let mut hourly = position.is_hourly();
        // STLB-Platzhalter: no switch (Anwender 18.11.: by hand straight to STLB-Bau);
        // a placeholder from GAEB/MCP stays, removable in the context menu.
        let stlb = position.is_stlb_placeholder();
        egui::ComboBox::from_id_salt(("position_window_kind", id))
            .width(ui.available_width().min(320.0))
            .selected_text(kind_label(kind))
            .show_ui(ui, |ui| {
                let group = kind_group(kind);
                for option in [
                    PositionKind::Normal,
                    PositionKind::Provisional { with_total: false },
                    PositionKind::Provisional { with_total: true },
                    PositionKind::Base { group },
                    PositionKind::Alternative { group, serial: 1 },
                    PositionKind::Supplement,
                ] {
                    let same = std::mem::discriminant(&option) == std::mem::discriminant(&kind)
                        && kind_label(option) == kind_label(kind);
                    if ui.selectable_label(same, kind_label(option)).clicked() && !same {
                        kind = option;
                    }
                }
            });
        match &mut kind {
            PositionKind::Base { group } => {
                crate::widgets::row(ui, |ui| {
                    ui.label("Wahlgruppe");
                    ui.add(egui::DragValue::new(group).range(1..=999));
                });
            }
            PositionKind::Alternative { group, serial } => {
                crate::widgets::row(ui, |ui| {
                    ui.label("Wahlgruppe");
                    ui.add(egui::DragValue::new(group).range(1..=999));
                    ui.add_space(8.0);
                    ui.label("Wahl-Nr.");
                    ui.add(egui::DragValue::new(serial).range(1..=9));
                });
            }
            _ => {}
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut lump, "Pauschal");
            ui.checkbox(&mut hourly, "Stundenlohn");
        });
        // Zuschlagsposition / „zu bezuschlagen“ (Bezug on Reiter Allgemein).
        self.surcharge_fields(ui, palette, project, index, output);
        if kind != position.kind()
            || lump != position.is_lump_sum()
            || hourly != position.is_hourly()
            || stlb != position.is_stlb_placeholder()
        {
            self.undo
                .record(project.lv(), Some(EditKey(format!("kind:{}", id.0))));
            if let Some(position) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                position.set_kind(kind);
                if lump && !position.is_lump_sum() && position.unit().is_empty() {
                    position.set_unit("psch");
                }
                position.set_lump_sum(lump);
                position.set_hourly(hourly);
                position.set_stlb_placeholder(stlb);
            }
            output.dirty = true;
        }
        // Grund- und Wahlpositionen derselben Gruppe.
        if let PositionKind::Base { group } | PositionKind::Alternative { group, .. } = kind {
            let partners: Vec<(RowId, String)> = project
                .lv()
                .positions()
                .filter(|other| other.id() != id)
                .filter(|other| {
                    matches!(other.kind(), PositionKind::Base { group: g } | PositionKind::Alternative { group: g, .. } if g == group)
                })
                .map(|other| {
                    (
                        other.id(),
                        format!(
                            "{}  {}  ({})",
                            other.ordinal(),
                            other.short_text(),
                            kind_label(other.kind())
                        ),
                    )
                })
                .collect();
            ui.add_space(4.0);
            caption(ui, palette, &format!("Wahlgruppe {group}"));
            if partners.is_empty() {
                form_kit::hint(ui, palette, "Keine weitere Position in dieser Gruppe.");
            }
            let mut jump = None;
            for (other, text) in &partners {
                if ui
                    .add(egui::Button::new(RichText::new(text).size(12.5)).frame(false))
                    .on_hover_text("Diese Position zeigen")
                    .clicked()
                {
                    jump = Some(*other);
                }
            }
            if let Some(target) = jump {
                self.select(project.lv(), target);
            }
        }
        ui.add_space(10.0);

        // Kostengruppe (DIN 276), with its own caption.
        self.cost_group_field(ui, palette, project, &[id], output);
        ui.add_space(10.0);
        self.work_category_field(ui, palette, project, index, output);
        ui.add_space(10.0);

        // The Ausführungsbeschreibung is chosen on „Allgemein“ under the OZ
        // (performance_link_line) – one place per window (Anwender 18.11.).

        if position.stlb_reference().is_some() || position.catalog_line().is_some() {
            form_kit::section(ui, palette, "STLB-Bau");
            if let Some(stlb) = position.stlb_reference() {
                ui.label(RichText::new(stlb.summary()).size(12.5).color(palette.text));
            }
            if let Some(line) = position.catalog_line() {
                ui.label(RichText::new(line).size(12.0).color(palette.text_dim));
            }
            if position.stlb_text_changed() {
                ui.label(
                    RichText::new(strings::STLB_TEXT_CHANGED)
                        .size(12.0)
                        .color(palette.warn),
                );
            }
            ui.add_space(10.0);
        }

        form_kit::section(ui, palette, "Weitere Angaben");
        self.position_extra_fields(ui, palette, project, index, output);

        if let Some(current) = project.lv().rows()[index].as_position() {
            if !current.complements().is_empty() {
                ui.add_space(8.0);
                caption(ui, palette, "Langtext mit Textergänzungen (Ansicht)");
                egui::Frame::new()
                    .stroke(Stroke::new(1.0, palette.line))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let blocks =
                            complement_display::display(current.long_text(), current.complements());
                        complement_display::show(ui, palette, &blocks);
                    });
            }
        }
    }

    /// Field „Leistungsbereich (Gewerk)“: free input (taken when the field is
    /// left) or a choice from the Leistungsbereiche of STLB-Bau and those in
    /// use in the LV.
    fn work_category_field(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let (id, current, is_title) = match &project.lv().rows()[index] {
            LvRow::Position(position) => (
                position.id(),
                position.work_category().map(str::to_string),
                false,
            ),
            LvRow::Group(group) => (group.id(), group.work_category().map(str::to_string), true),
            _ => return,
        };
        let ctx = ui.ctx().clone();
        caption(
            ui,
            palette,
            if is_title {
                "Leistungsbereich (Gewerk) – Vorgabe für alles darunter"
            } else {
                "Leistungsbereich (Gewerk)"
            },
        );
        let field = egui::Id::unique(("position_window_work_category", id, self.revision));
        let pending_id = egui::Id::unique(("position_window_work_pending", id));
        let mut chosen: Option<Option<String>> = None;
        ui.horizontal(|ui| {
            let checked = fields::checked_field(
                ui,
                palette,
                field,
                current.as_deref().unwrap_or(""),
                160.0,
                FontId::proportional(13.0),
            );
            checked
                .response
                .on_hover_text("STLB-Bau-Nummer wie 070 oder ein eigener Name; leer = keiner");
            if let Some(text) = checked.commit {
                match proava_domain::parse_work_category(&text) {
                    Ok(code) => {
                        chosen = Some(code);
                        fields::accept(&ctx, field);
                    }
                    Err(error) => fields::refuse(&ctx, field, error),
                }
            }
            let in_use: Vec<String> = project
                .lv()
                .work_categories_in_use()
                .into_iter()
                .filter(|code| proava_domain::work_category_label(code).is_none())
                .collect();
            egui::ComboBox::from_id_salt(("position_window_work_list", id))
                .width(ui.available_width().min(360.0))
                .selected_text("Liste …")
                .height(360.0)
                .show_ui(ui, |ui| {
                    let none = if is_title {
                        "– keiner –"
                    } else {
                        "– keiner eigener –"
                    };
                    if ui.selectable_label(current.is_none(), none).clicked() {
                        chosen = Some(None);
                    }
                    for code in &in_use {
                        let text = format!("{code}  (im LV)");
                        if ui
                            .selectable_label(current.as_deref() == Some(code.as_str()), text)
                            .clicked()
                        {
                            chosen = Some(Some(code.clone()));
                        }
                    }
                    for (code, name) in proava_domain::STLB_WORK_CATEGORIES {
                        if ui
                            .selectable_label(
                                current.as_deref() == Some(*code),
                                format!("{code} {name}"),
                            )
                            .clicked()
                        {
                            chosen = Some(Some(code.to_string()));
                        }
                    }
                });
        });
        let inherited = project
            .lv()
            .effective_work_category(index)
            .filter(|(_, inherited)| *inherited)
            .map(|(code, _)| code);
        if let Some(name) = current
            .as_deref()
            .and_then(proava_domain::work_category_label)
        {
            ui.label(RichText::new(name).size(12.0).color(palette.text_dim));
        } else if let Some(code) = inherited.filter(|_| current.is_none()) {
            // Shown as in the KG field: what applies and where it comes from.
            let lv = project.lv();
            let from = crate::lv_ops::ancestors(lv, index)
                .into_iter()
                .rev()
                .filter_map(|title| lv.row_by_id(title))
                .find(|row| {
                    row.as_group()
                        .and_then(|group| group.work_category())
                        .is_some()
                })
                .map(|row| format!(" vom Titel {}", row.ordinal()))
                .unwrap_or_default();
            ui.label(
                RichText::new(format!(
                    "Gilt{from}: {}",
                    proava_domain::work_category_text(&code)
                ))
                .size(12.0)
                .color(palette.text_dim),
            );
        } else if let Some(lb) = project.lv().rows()[index]
            .as_position()
            .and_then(|position| position.stlb_reference())
            .map(|stlb| stlb.work_category.trim().to_string())
            .filter(|lb| current.is_none() && !lb.is_empty())
        {
            // A text from STLB-Bau knows its Leistungsbereich.
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Der Text stammt aus STLB-Bau, LB {}.",
                        proava_domain::work_category_text(&lb)
                    ))
                    .size(12.0)
                    .color(palette.text_dim),
                );
                if ui.small_button("Übernehmen").clicked() {
                    chosen = Some(Some(lb.clone()));
                }
            });
        }

        // A title's new value would change finished positions: ask first
        // (like the Kostengruppe).
        let mut apply: Option<Option<String>> = None;
        if let Some(code) = chosen.filter(|code| *code != current) {
            let heirs = if is_title {
                project
                    .lv()
                    .locked_work_category_heirs(&[index], code.as_deref())
            } else {
                Vec::new()
            };
            if heirs.is_empty() {
                apply = Some(code);
            } else {
                ctx.data_mut(|data| data.insert_temp(pending_id, (code, heirs.len())));
            }
        }
        if let Some((code, count)) =
            ctx.data(|data| data.get_temp::<(Option<String>, usize)>(pending_id))
        {
            egui::Frame::new()
                .stroke(Stroke::new(1.0, palette.warn))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(proava_domain::work_category_heirs_question(
                            count,
                            code.as_deref(),
                        ))
                        .size(12.5)
                        .color(palette.text),
                    );
                    ui.label(
                        RichText::new("Ihr Status bleibt „fertig“.")
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Trotzdem ändern").clicked() {
                            apply = Some(code.clone());
                            ctx.data_mut(|data| data.remove::<(Option<String>, usize)>(pending_id));
                        }
                        if ui.button("Abbrechen").clicked() {
                            ctx.data_mut(|data| data.remove::<(Option<String>, usize)>(pending_id));
                        }
                    });
                });
        }
        if let Some(code) = apply {
            self.undo.record(project.lv(), None);
            match project.lv_mut().row_mut(index) {
                Some(LvRow::Position(position)) => position.set_work_category(code),
                Some(LvRow::Group(group)) => group.set_work_category(code),
                _ => {}
            }
            self.revision += 1;
            output.dirty = true;
        }
    }

    /// Reiter „Mengenermittlung“: lines with place and Rechenansatz – the
    /// same kind of lines as the Aufmaß of the Abrechnung – their sum and
    /// the takeover as the quantity of the position.
    fn window_takeoff(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        window: &mut PositionWindow,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let Some(position) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        let id = position.id();
        form_kit::hint(
            ui,
            palette,
            "Je Zeile Ort oder Bezeichnung und Rechenansatz (Grundrechenarten, Klammern, deutsches Komma, z. B. 2*12,5+3). Die Summe ist die Planungsmenge der Position.",
        );
        ui.add_space(8.0);
        let mut lines = position.takeoff().to_vec();
        // Some(key): the lines changed (key groups typing into one field).
        let mut changed: Option<Option<EditKey>> = None;
        let mut remove = None;
        let mut swap = None;
        let field_id = |number: usize, part: &str| {
            egui::Id::unique(("position_window_takeoff", id, number, part.to_string()))
        };
        if !lines.is_empty() {
            egui::Grid::new(("position_window_takeoff_grid", id))
                .num_columns(5)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    for title in ["Nr.", "Ort / Bezeichnung", "Rechenansatz", "Ergebnis", ""] {
                        ui.label(RichText::new(title).size(12.0).color(palette.text_dim));
                    }
                    ui.end_row();
                    let count = lines.len();
                    for (number, line) in lines.iter_mut().enumerate() {
                        ui.label(
                            RichText::new(format!("{}", number + 1))
                                .monospace()
                                .color(palette.text_dim),
                        );
                        // Fixed widths: grid cells would shrink the fields.
                        let label = ui.add_sized(
                            [300.0, 24.0],
                            egui::TextEdit::singleline(&mut line.label)
                                .id(field_id(number, "label"))
                                .hint_text("z. B. EG Strang 1"),
                        );
                        // A new line: the caret goes into its first field
                        // (it exists only from this frame on).
                        if window.takeoff_focus == Some(number) {
                            label.request_focus();
                            window.takeoff_focus = None;
                        }
                        if label.changed() {
                            changed =
                                Some(Some(EditKey(format!("takeoff:{}:{number}:label", id.0))));
                        }
                        if ui
                            .add_sized(
                                [240.0, 24.0],
                                egui::TextEdit::singleline(&mut line.formula)
                                    .id(field_id(number, "formula"))
                                    .hint_text("z. B. 2*12,5+3")
                                    .font(FontId::monospace(13.0)),
                            )
                            .changed()
                        {
                            changed =
                                Some(Some(EditKey(format!("takeoff:{}:{number}:formula", id.0))));
                        }
                        match line.result() {
                            Ok(value) => {
                                ui.add_sized(
                                    [90.0, 24.0],
                                    egui::Label::new(
                                        RichText::new(numbers::quantity_text(value))
                                            .font(FontId::monospace(13.0))
                                            .color(palette.text),
                                    ),
                                );
                            }
                            Err(error) => {
                                ui.label(RichText::new("Fehler").size(12.5).color(palette.error))
                                    .on_hover_text(error);
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled_ui(number > 0, |ui| {
                                    widgets::tool_button(
                                        ui,
                                        palette,
                                        Icon::Up,
                                        false,
                                        "Zeile nach oben",
                                    )
                                    .clicked()
                                })
                                .inner
                            {
                                swap = Some((number, number - 1));
                            }
                            if ui
                                .add_enabled_ui(number + 1 < count, |ui| {
                                    widgets::tool_button(
                                        ui,
                                        palette,
                                        Icon::Down,
                                        false,
                                        "Zeile nach unten",
                                    )
                                    .clicked()
                                })
                                .inner
                            {
                                swap = Some((number, number + 1));
                            }
                            if widgets::tool_button(
                                ui,
                                palette,
                                Icon::Trash,
                                false,
                                "Zeile entfernen",
                            )
                            .clicked()
                            {
                                remove = Some(number);
                            }
                        });
                        ui.end_row();
                    }
                });
            ui.add_space(6.0);
        }
        if widgets::icon_label_button(ui, palette, Icon::Plus, "Zeile hinzufügen")
            .on_hover_text("Neue Zeile am Ende der Mengenermittlung")
            .clicked()
        {
            lines.push(proava_domain::TakeoffLine::default());
            changed = Some(None);
            window.takeoff_focus = Some(lines.len() - 1);
        }
        if let Some(number) = remove {
            lines.remove(number);
            changed = Some(None);
        }
        if let Some((a, b)) = swap {
            lines.swap(a, b);
            changed = Some(None);
        }
        if let Some(key) = changed {
            self.undo.record(project.lv(), key);
            if let Some(position) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                position.set_takeoff(lines.clone());
            }
            output.dirty = true;
        }

        // Summe und Übernahme als Menge.
        let Some(current) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        let (sum, errors) = current.takeoff_total();
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "Summe {} {}",
                    numbers::quantity_text(sum),
                    current.unit()
                ))
                .size(14.0)
                .strong()
                .color(palette.text),
            );
            ui.add_space(16.0);
            ui.label(
                RichText::new(format!(
                    "Menge der Position {} {}",
                    numbers::quantity_text(current.quantity()),
                    current.unit()
                ))
                .size(12.5)
                .color(if current.takeoff_differs().is_some() {
                    palette.warn
                } else {
                    palette.text_dim
                }),
            );
            ui.add_space(16.0);
            let can_take = current.takeoff_differs().is_some();
            if ui
                .add_enabled(can_take, egui::Button::new("Summe als Menge übernehmen"))
                .on_hover_text("Die Menge der Position wird die Summe der Mengenermittlung")
                .on_disabled_hover_text(if current.takeoff().is_empty() {
                    "Noch keine Zeilen"
                } else if !errors.is_empty() {
                    "Erst die Zeilen mit Fehler berichtigen"
                } else {
                    "Menge und Summe sind gleich"
                })
                .clicked()
            {
                self.undo.record(project.lv(), None);
                if let Some(position) = project
                    .lv_mut()
                    .row_mut(index)
                    .and_then(LvRow::as_position_mut)
                {
                    position.set_quantity(sum);
                    position.set_quantity_formula(None);
                }
                // The number fields of Allgemein load the new value.
                self.revision += 1;
                output.dirty = true;
                output.toast = Some(format!(
                    "Menge {} {} aus der Mengenermittlung.",
                    numbers::quantity_text(sum),
                    current.unit()
                ));
            }
        });
        for error in &errors {
            ui.label(
                RichText::new(error.as_str())
                    .size(12.0)
                    .color(palette.error),
            );
        }
        self.window_billing_takeoff(ui, palette, project, id, &current);
    }

    /// The Aufmaß of the Abrechnung for this position (read only): the same
    /// kind of lines, by invoice.
    fn window_billing_takeoff(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        id: RowId,
        position: &proava_domain::Position,
    ) {
        let billing = project.billing();
        if billing.is_empty() && project.contract().is_none() {
            return;
        }
        ui.add_space(14.0);
        form_kit::section(ui, palette, "Aufmaß (Abrechnung)");
        let groups = billing_takeoff(billing, id);
        if groups.is_empty() {
            form_kit::hint(ui, palette, "Zu dieser Position gibt es noch kein Aufmaß.");
        }
        let mut total = proava_domain::Decimal::ZERO;
        for (heading, lines, sum) in &groups {
            total += *sum;
            ui.label(
                RichText::new(format!(
                    "{heading} – {} {}",
                    numbers::quantity_text(*sum),
                    position.unit()
                ))
                .size(12.5)
                .strong()
                .color(palette.text),
            );
            for line in lines {
                let result = line
                    .result()
                    .map(numbers::quantity_text)
                    .unwrap_or_else(|_| "Fehler".to_string());
                ui.label(
                    RichText::new(format!(
                        "{}  ·  {} = {result}",
                        line.label.trim(),
                        line.formula.trim()
                    ))
                    .size(12.0)
                    .color(palette.text_dim),
                );
            }
            ui.add_space(4.0);
        }
        if !groups.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Aufmaß gesamt {} {} · Planungsmenge {} {}",
                    numbers::quantity_text(total.normalize()),
                    position.unit(),
                    numbers::quantity_text(position.quantity()),
                    position.unit()
                ))
                .size(12.5)
                .color(palette.text),
            );
        }
        // MERGE(abrechnung2): Knopf „Aufmaß bearbeiten …“ (Symbol + Text), der das
        // Aufmaßfenster des Reiters Abrechnung für diese Position öffnet (Strg+M).
    }

    /// The long text of the row in the window's own editor, filling the rest.
    #[allow(clippy::too_many_arguments)]
    fn window_long_text(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        window: &mut PositionWindow,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let row = &project.lv().rows()[index];
        let id = row.id();
        let Some(text) = long_text_of(row).cloned() else {
            return;
        };
        let locked = row
            .as_position()
            .is_some_and(|position| position.is_locked());
        // STLB-Bau-Text: key with version, write-protected, the way out.
        let stlb = row
            .as_position()
            .filter(|position| position.is_text_locked())
            .cloned();
        if let Some(position) = &stlb {
            if super::stlb_lock::banner(ui, palette, position) {
                output.merge(self.request_free(project, &[id]));
                ui.ctx()
                    .send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
                return;
            }
            if let Some(edited) =
                super::stlb_lock::complement_values(ui, palette, position, "window")
            {
                self.store_complement_values(project, index, position, &edited, output);
                window.editor.reset();
            }
        }
        caption(ui, palette, "Langtext");
        window.editor.min_height = (ui.available_height() - 70.0).max(260.0);
        let mut edited = text;
        let complements = super::row_complements(project, index);
        let mut list = complements.clone().unwrap_or_default();
        window.editor.field_presets = crate::rich_editor::FieldPresets::of(launcher);
        let images = project.images();
        let editor = &mut window.editor;
        let (result, toast) = super::stlb_lock::locked_or_free(
            ui,
            egui::Id::unique(("position_window_long_lock", id)),
            stlb.is_some(),
            |ui| {
                if complements.is_some() {
                    editor.show_with_complements(ui, palette, &mut edited, &mut list, images)
                } else {
                    editor.show(ui, palette, &mut edited, images)
                }
            },
        );
        window.editor_row = Some(id);
        if let Some(toast) = toast {
            output.toast = Some(toast);
        }
        if locked || stlb.is_some() {
            if result.changed {
                window.editor.reset();
            }
            if locked {
                window.editor.reset();
            }
            return;
        }
        if result.pick_image {
            window.open_image_dialog(launcher);
        }
        if result.pick_pdf_clip {
            window.open_clip_dialog(launcher);
        }
        if result.changed {
            let key = result
                .edit_key
                .map(|key| EditKey(format!("{}:window:{key}", id.0)));
            self.undo.record(project.lv(), key);
            set_long_text(project, index, edited);
            super::set_complements(project, index, complements.map(|_| list));
            output.dirty = true;
        }
    }

    /// Reiter „Grafik“: the images of the long text.
    #[allow(clippy::too_many_arguments)]
    fn window_graphics(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        window: &mut PositionWindow,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let row = &project.lv().rows()[index];
        let id = row.id();
        let Some(text) = long_text_of(row).cloned() else {
            return;
        };
        let list = text_images(&text);
        if row
            .as_position()
            .is_some_and(|position| position.is_text_locked())
        {
            // Images belong to the STLB-Bau text: shown, not changed.
            form_kit::hint(ui, palette, proava_domain::stlb::STLB_LOCKED);
            ui.disable();
        }
        form_kit::hint(
            ui,
            palette,
            "Bilder stehen im Langtext an ihrer Stelle; sie werden gedruckt und gehen mit „Bilder mitgeben“ in GAEB DA XML (PDF-Ausschnitte als PNG mit 300 dpi).",
        );
        form_kit::hint(
            ui,
            palette,
            "Ein neues Bild kommt ans Ende des Langtexts – auch per Ziehen ins Fenster oder Strg+V.",
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if widgets::icon_label_button(ui, palette, Icon::Image, "Bild einfügen …")
                .on_hover_text("PNG oder JPEG ans Ende des Langtexts")
                .clicked()
            {
                window.open_image_dialog(launcher);
            }
            if widgets::icon_label_button(
                ui,
                palette,
                Icon::Document,
                crate::rich_editor::CLIP_MENU,
            )
            .on_hover_text(
                "Teil einer PDF-Seite ans Ende des Langtexts – im Druck als Vektorgrafik",
            )
            .clicked()
            {
                window.open_clip_dialog(launcher);
            }
            ui.label(
                RichText::new(match list.len() {
                    0 => "Keine Bilder".to_string(),
                    1 => "1 Bild".to_string(),
                    count => format!("{count} Bilder"),
                })
                .size(12.5)
                .color(palette.text_dim),
            );
        });
        ui.add_space(8.0);
        let ctx = ui.ctx().clone();
        let mut remove = None;
        let mut width_change = None;
        for (block, file, width) in &list {
            let is_clip = project.image(file).is_some_and(|asset| asset.is_pdf_clip());
            let texture = match project.image(file).filter(|asset| asset.is_pdf_clip()) {
                Some(asset) => {
                    // PDF clip: the preview of the editor (rendered once).
                    let texture = clip_thumbnail(&ctx, asset);
                    window.textures.insert(file.clone(), texture.clone());
                    texture
                }
                None => window
                    .textures
                    .entry(file.clone())
                    .or_insert_with(|| load_texture(&ctx, project, file))
                    .clone(),
            };
            egui::Frame::new()
                .stroke(Stroke::new(1.0, palette.line))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    crate::widgets::row(ui, |ui| {
                        match &texture {
                            Ok(handle) => {
                                let size = handle.size_vec2();
                                let scale = (160.0 / size.x).min(110.0 / size.y).min(1.0);
                                let response = ui
                                    .add(
                                        egui::Image::new((handle.id(), size * scale))
                                            .sense(Sense::click()),
                                    )
                                    .on_hover_text("Groß ansehen");
                                if response.clicked() {
                                    window.preview = Some(file.clone());
                                }
                            }
                            Err(error) => {
                                // A clip without picture (hidden, being rendered) is no error.
                                let failed = !is_clip || error.starts_with("Vorschau nicht");
                                let color = if failed {
                                    palette.error
                                } else {
                                    palette.text_dim
                                };
                                ui.label(RichText::new(error).size(12.0).color(color));
                            }
                        }
                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(file).size(13.0).color(palette.text));
                            crate::widgets::row(ui, |ui| {
                                ui.label(
                                    RichText::new("Breite im Druck (mm)")
                                        .size(12.0)
                                        .color(palette.text_dim),
                                );
                                let mut value = width.unwrap_or(DEFAULT_IMAGE_WIDTH_MM);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut value)
                                            .range(10.0..=180.0)
                                            .speed(1.0),
                                    )
                                    .changed()
                                {
                                    width_change = Some((*block, value));
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Ansehen").clicked() {
                                    window.preview = Some(file.clone());
                                }
                                if widgets::icon_label_button(ui, palette, Icon::Trash, "Entfernen")
                                    .on_hover_text(
                                        "Bild aus dem Langtext nehmen (bleibt in der Projektdatei)",
                                    )
                                    .clicked()
                                {
                                    remove = Some(*block);
                                }
                            });
                        });
                    });
                });
            ui.add_space(6.0);
        }
        if let Some((block, value)) = width_change {
            self.undo.record(
                project.lv(),
                Some(EditKey(format!("image_width:{}:{block}", id.0))),
            );
            set_long_text(project, index, with_image_width(&text, block, value));
            output.dirty = true;
        }
        if let Some(block) = remove {
            self.undo.record(project.lv(), None);
            let complements = super::complements_after_block(project, index, block, true);
            set_long_text(project, index, without_image(&text, block));
            super::set_complements(project, index, complements);
            output.dirty = true;
            output.toast = Some("Bild aus dem Langtext entfernt.".into());
        }
        // Vorschau
        if let Some(file) = window.preview.clone() {
            if !list.iter().any(|(_, other, _)| *other == file) {
                window.preview = None;
            } else if let Some(Ok(handle)) = window.textures.get(&file) {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&file).size(13.0).strong().color(palette.text));
                    if ui.button("Vorschau schließen").clicked() {
                        window.preview = None;
                    }
                });
                let size = handle.size_vec2();
                let scale = (ui.available_width() / size.x).min(1.0);
                ui.add(egui::Image::new((handle.id(), size * scale)));
            }
        }
    }

    /// Prüfleiste: findings of the row, a click goes to the Reiter.
    fn check_bar(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        window: &mut PositionWindow,
        id: RowId,
    ) {
        let speller = crate::spell::speller(ui.ctx());
        let key = (
            id,
            self.undo.record_count(),
            self.revision,
            speller.is_some(),
        );
        if window.findings.as_ref().map(|(at, _)| *at) != Some(key) {
            window.findings = Some((key, row_findings(project, id, speller.as_deref())));
        }
        let findings = window
            .findings
            .as_ref()
            .map(|(_, findings)| findings.clone())
            .unwrap_or_default();
        let tabs = project
            .lv()
            .row_by_id(id)
            .map(tabs_for)
            .unwrap_or(&[Tab::General]);
        egui::Frame::new()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.line))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                ui.label(
                    RichText::new("Prüfung")
                        .size(14.0)
                        .strong()
                        .color(palette.text),
                );
                ui.label(
                    RichText::new(match findings.len() {
                        0 => "Keine Befunde.".to_string(),
                        1 => "1 Befund".to_string(),
                        count => format!("{count} Befunde"),
                    })
                    .size(12.0)
                    .color(palette.text_dim),
                );
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt("position_window_check")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (number, finding) in findings.iter().enumerate() {
                            let color = match finding.severity {
                                Severity::Error => palette.error,
                                Severity::Warning => palette.warn,
                                Severity::Hint => palette.text_dim,
                            };
                            let response = egui::Frame::new()
                                .stroke(Stroke::new(1.0, color.gamma_multiply(0.7)))
                                .corner_radius(egui::CornerRadius::same(5))
                                .inner_margin(egui::Margin::symmetric(8, 6))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        RichText::new(finding.severity.label())
                                            .size(11.5)
                                            .strong()
                                            .color(color),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&finding.text)
                                                .size(12.5)
                                                .color(palette.text),
                                        )
                                        .wrap(),
                                    );
                                })
                                .response;
                            if tabs.contains(&finding.tab) {
                                let response = ui
                                    .interact(
                                        response.rect,
                                        egui::Id::unique(("position_window_finding", id, number)),
                                        Sense::click(),
                                    )
                                    .on_hover_text(format!("Zum Reiter „{}“", finding.tab.label()));
                                if response.clicked() {
                                    window.tab = finding.tab;
                                    window.chosen_tab = finding.tab;
                                }
                            }
                            ui.add_space(4.0);
                        }
                    });
            });
    }

    /// A PDF clip chosen in the window: into the project and the long text
    /// of the window's row (at the caret on Allgemein, else at the end).
    fn window_insert_pdf_clip(
        &mut self,
        project: &mut Project,
        window: &mut PositionWindow,
        prepared: proava_application::PreparedClip,
        width_mm: f32,
    ) -> EditorOutput {
        let at_caret = window.tab == Tab::General && window.editor_row == Some(window.row);
        if !at_caret {
            return self.insert_pdf_clip(
                project,
                window.row,
                prepared,
                width_mm,
                super::pdf_clip::ClipPlace::End,
            );
        }
        let Some(index) = project.lv().index_of(window.row) else {
            return EditorOutput::default();
        };
        if project.lv().rows()[index].is_group() {
            return EditorOutput::toast(
                "PDF-Ausschnitte gehören in den Langtext einer Position oder eines Hinweises.",
            );
        }
        if let Err(message) = project.lv().ensure_editable_rows(&[index]) {
            return EditorOutput::toast(message);
        }
        // Images belong to the text: not into an STLB-Bau text.
        if let Err(message) = project
            .lv()
            .ensure_text_editable(&[project.lv().rows()[index].id()])
        {
            return EditorOutput::toast(message);
        }
        self.undo.record(project.lv(), None);
        let summary = prepared.summary_for(project);
        let file = project.add_pdf_clip(&prepared.name, prepared.bytes, prepared.clip);
        let text = window.editor.insert_image_sized(file, width_mm);
        let complements = window.editor.complements();
        set_long_text(project, index, text);
        super::set_complements(project, index, complements);
        EditorOutput {
            dirty: true,
            toast: Some(summary),
            ..EditorOutput::default()
        }
    }

    /// Images dropped, pasted or chosen in the window: into the project and
    /// the long text of the window's row (at the caret on Allgemein, else at
    /// the end).
    fn window_insert_images(
        &mut self,
        project: &mut Project,
        window: &mut PositionWindow,
        files: Vec<(String, Vec<u8>)>,
    ) -> EditorOutput {
        let Some(index) = project.lv().index_of(window.row) else {
            return EditorOutput::default();
        };
        if project.lv().rows()[index].is_group() {
            return EditorOutput::toast(
                "Bilder gehören in den Langtext einer Position oder eines Hinweises.",
            );
        }
        if let Err(message) = project.lv().ensure_editable_rows(&[index]) {
            return EditorOutput::toast(message);
        }
        // Images belong to the text: not into an STLB-Bau text.
        if let Err(message) = project
            .lv()
            .ensure_text_editable(&[project.lv().rows()[index].id()])
        {
            return EditorOutput::toast(message);
        }
        let at_caret = window.tab == Tab::General && window.editor_row == Some(window.row);
        let mut added = 0;
        let mut refused = Vec::new();
        for (name, bytes) in files {
            if !images::is_supported_image(&bytes) {
                refused.push(name);
                continue;
            }
            if added == 0 {
                self.undo.record(project.lv(), None);
            }
            let file = project.add_image(&name, bytes);
            let text = if at_caret {
                let text = window.editor.insert_image(file);
                super::set_complements(project, index, window.editor.complements());
                text
            } else {
                let current = long_text_of(&project.lv().rows()[index])
                    .cloned()
                    .unwrap_or_default();
                // Fields at the end of the text stay at the end.
                let complements =
                    super::complements_after_block(project, index, current.blocks().len(), false);
                super::set_complements(project, index, complements);
                with_image_at_end(&current, file)
            };
            set_long_text(project, index, text);
            added += 1;
        }
        EditorOutput {
            dirty: added > 0,
            toast: Some(match (added, refused.is_empty()) {
                (0, _) => format!("Kein PNG/JPEG: {}", refused.join(", ")),
                (count, true) => format!(
                    "{} eingefügt.",
                    crate::format::count(count, "Bild", "Bilder")
                ),
                (count, false) => format!(
                    "{} eingefügt; übergangen: {}",
                    crate::format::count(count, "Bild", "Bilder"),
                    refused.join(", ")
                ),
            }),
            ..EditorOutput::default()
        }
    }
}

fn locked_banner(ui: &mut egui::Ui, palette: &Palette) {
    egui::Frame::new()
        .fill(palette.done.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, palette.done))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(format!(
                    "Position ist als fertig markiert – {}.",
                    proava_domain::LOCKED_HINT
                ))
                .size(12.5)
                .color(palette.text),
            );
        });
    ui.add_space(8.0);
}

/// Keys and dropped or pasted files of the window: Esc, Strg+Z/Y, Bild↑/↓,
/// Strg+S, Strg+Umschalt+F and Strg+Umschalt+E work as in the main window. Embedded in
/// the main window (backends without more windows, UI tests) the main window
/// gets the same input and handles all but Esc itself.
/// The first new-row command among the key presses and every such key (to
/// take them from the text fields).
pub(crate) fn new_row_keys(
    events: &[egui::Event],
) -> Option<(
    super::context_menu::MenuChoice,
    Vec<(egui::Key, egui::Modifiers)>,
)> {
    let mut first = None;
    let mut keys = Vec::new();
    for event in events {
        if let egui::Event::Key {
            key,
            pressed,
            modifiers,
            ..
        } = event
        {
            if let Some(choice) = super::context_menu::new_row_for_key(*key, *modifiers) {
                keys.push((*key, *modifiers));
                if *pressed && first.is_none() {
                    first = Some(choice);
                }
            }
        }
    }
    first.map(|choice| (choice, keys))
}

fn window_input(
    ctx: &egui::Context,
    window: &mut PositionWindow,
    frame: &mut Frame,
    embedded: bool,
) {
    if window.dialog.is_some() || window.clip_dialog.is_some() {
        return;
    }
    if embedded {
        if !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            frame.close = true;
        }
        return;
    }
    let (dropped, events) =
        ctx.input(|input| (input.raw.dropped_files.clone(), input.events.clone()));
    for file in dropped {
        if let Ok(bytes) = file.bytes() {
            let name = crate::file_dialog::file_name(file.path());
            if proava_domain::is_pdf(&bytes) {
                frame.pdf.get_or_insert((name, bytes));
            } else {
                frame.files.push((name, bytes));
            }
        }
    }
    for event in &events {
        if let egui::Event::PasteImage(image) = event {
            if let Ok(bytes) = images::encode_png(image) {
                frame.files.push(("Zwischenablage.png".into(), bytes));
            }
        }
    }
    // New rows (F9 …) work here as in the table, after the window's row. As
    // a window of its own it gets these keys itself; embedded (one window
    // only, the tests) the table's keyboard has them – the window follows the
    // selection, so the new row comes after the same row there.
    if let Some(keys) = new_row_keys(&events) {
        frame.new_row = Some(keys.0);
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                !matches!(event, egui::Event::Key { key, modifiers, .. }
                    if keys.1.contains(&(*key, *modifiers)))
            })
        });
    }
    // Not inside `input_mut`: the context is locked there.
    let popup = egui::Popup::is_any_open(ctx);
    ctx.input_mut(|input| {
        if !popup && input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            frame.close = true;
        }
        let shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::S) {
            frame.save = true;
        }
        if input.consume_key(shift, egui::Key::E) {
            frame.status = Some(StatusKey::InProgress);
        } else if input.consume_key(shift, egui::Key::F) {
            frame.status = Some(StatusKey::ToggleDone);
        }
        let shift_z = shift;
        if input.consume_key(shift_z, egui::Key::Z)
            || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
        {
            frame.redo = true;
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z) {
            frame.undo = true;
        }
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::COMMAND] {
            if input.consume_key(modifiers, egui::Key::PageUp) {
                frame.step = Some(false);
            }
            if input.consume_key(modifiers, egui::Key::PageDown) {
                frame.step = Some(true);
            }
        }
    });
}

/// Remembers size and position once they stayed for a moment.
fn remember_place(ctx: &egui::Context, window: &mut PositionWindow, launcher: &Launcher) {
    let (inner, outer, now, whole_screen) = ctx.input(|input| {
        let viewport = input.viewport();
        (
            viewport.inner_rect,
            viewport.outer_rect,
            input.time,
            viewport.maximized == Some(true)
                || viewport.minimized == Some(true)
                || viewport.fullscreen == Some(true),
        )
    });
    // Maximised or minimised the window keeps the place it had before: that
    // is where it comes back (and opens next time), not the whole screen.
    if whole_screen {
        return;
    }
    let place = WindowPlace {
        size: inner
            .filter(|rect| rect.width() > 100.0 && rect.height() > 100.0)
            .map(|rect| (rect.width(), rect.height()))
            .or(window.place.size),
        at: outer
            .map(|rect| (rect.min.x, rect.min.y))
            .or(window.place.at),
    };
    if place != window.place {
        window.place = place;
        window.place_since = now;
    }
    if window.place != window.saved {
        if now - window.place_since >= SAVE_DELAY {
            launcher.set_position_window(window.place);
            window.saved = window.place;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(SAVE_DELAY));
        }
    }
}

/// Small picture of a PDF clip for the Reiter Grafik (none while hidden).
fn clip_thumbnail(
    ctx: &egui::Context,
    asset: &proava_domain::ImageAsset,
) -> Result<egui::TextureHandle, String> {
    let previews = crate::pdf_preview::ClipPreviews::of(ctx);
    if asset.clip.as_ref().is_some_and(|clip| clip.preview_hidden) || previews.all_hidden() {
        return Err("PDF-Ausschnitt (Vorschau ausgeblendet)".to_string());
    }
    match previews.picture(asset, 400) {
        crate::pdf_preview::Shown::Picture(texture)
        | crate::pdf_preview::Shown::Waiting(Some(texture)) => Ok(texture),
        crate::pdf_preview::Shown::Waiting(None) => Err("Vorschau wird gerechnet …".to_string()),
        crate::pdf_preview::Shown::Failed(error) => Err(format!("Vorschau nicht möglich: {error}")),
        crate::pdf_preview::Shown::Unavailable => Err("PDF-Ausschnitt".to_string()),
    }
}

fn load_texture(
    ctx: &egui::Context,
    project: &Project,
    file: &str,
) -> Result<egui::TextureHandle, String> {
    let asset = project
        .image(file)
        .ok_or_else(|| format!("Bild „{file}“ fehlt in der Projektdatei."))?;
    let image = images::decode_preview(&asset.bytes)?;
    Ok(ctx.load_texture(format!("pw-{file}"), image, egui::TextureOptions::LINEAR))
}

/// A labelled read-only number box (like the GP field).
fn read_only_box(ui: &mut egui::Ui, palette: &Palette, label: &str, text: String, hover: &str) {
    ui.label(RichText::new(label).size(12.0).color(palette.text_dim));
    ui.add_space(3.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), Sense::hover());
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(4),
        palette.panel_alt,
        Stroke::new(1.0, palette.line),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.right_center() - egui::vec2(7.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        text,
        FontId::monospace(13.0),
        palette.text_dim,
    );
    response.on_hover_text(hover);
}

#[cfg(test)]
mod tests {
    use proava_domain::{
        Decimal, Lv, LvGroup, Note, OzMask, PerformanceDescription, Position, PositionKind,
        Project, ProjectName, TextComplement,
    };

    use super::*;

    fn project() -> Project {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.push_group(LvGroup::new(0, "", "Titel"));
        lv.push_group(LvGroup::new(1, "", "Untertitel"));
        lv.push_position(Position::new(
            "",
            "Station",
            Decimal::ONE,
            "St",
            Some(Decimal::TEN),
        ));
        lv.push_note(Note::new("Hinweis"));
        lv.push_position(Position::new("", "", Decimal::ZERO, "", None));
        let mut base = Position::new("", "Grund", Decimal::ONE, "St", None);
        base.set_kind(PositionKind::Base { group: 1 });
        lv.push_position(base);
        lv.push_group(LvGroup::new(1, "", "Zweiter"));
        lv.push_position(Position::new("", "Kabel", Decimal::TEN, "m", None));
        lv.renumber();
        Project::new(ProjectName::parse("P").unwrap(), "2026-11-17", lv)
    }

    fn id_of(project: &Project, short: &str) -> RowId {
        project
            .lv()
            .rows()
            .iter()
            .find(|row| row.short_text() == short)
            .unwrap()
            .id()
    }

    #[test]
    fn tabs_depend_on_the_row() {
        let project = project();
        let lv = project.lv();
        let names = |index: usize| -> Vec<&str> {
            tabs_for(&lv.rows()[index])
                .iter()
                .map(|tab| tab.label())
                .collect()
        };
        assert_eq!(names(0), ["Allgemein"], "Titel");
        assert_eq!(
            names(2),
            ["Allgemein", "Weiteres", "Mengenermittlung", "Grafik"],
            "Position"
        );
        assert_eq!(names(3), ["Allgemein", "Grafik"], "Hinweis");
        assert_eq!(kind_word(lv, &lv.rows()[2]), "Position");
        assert_eq!(kind_word(lv, &lv.rows()[3]), "Hinweis");
        let title = window_title(lv, &lv.rows()[2]);
        assert!(
            title.starts_with("Eigenschaften – Position 01.01.0010 Station"),
            "{title}"
        );
    }

    #[test]
    fn neighbours_are_rows_of_the_same_kind() {
        let project = project();
        let lv = project.lv();
        // 0 Titel, 1 Untertitel, 2 Station, 3 Hinweis, 4 leer, 5 Grund, 6 Zweiter, 7 Kabel
        assert_eq!(neighbour(lv, 2, true), Some(4), "über den Hinweis weg");
        assert_eq!(neighbour(lv, 5, true), Some(7), "über den Titel weg");
        assert_eq!(neighbour(lv, 7, true), None);
        assert_eq!(neighbour(lv, 2, false), None);
        assert_eq!(neighbour(lv, 1, true), Some(6), "Titel zu Titel");
        assert_eq!(neighbour(lv, 3, true), None, "einziger Hinweis");
    }

    #[test]
    fn findings_of_a_position_come_from_the_lv_check_and_the_open_points() {
        let project = project();
        let empty = project.lv().rows()[4].id();
        let findings = row_findings(&project, empty, None);
        let texts: Vec<&str> = findings
            .iter()
            .map(|finding| finding.text.as_str())
            .collect();
        assert!(texts.contains(&"Ohne Menge."), "{texts:?}");
        assert!(texts.contains(&"Ohne Einheit."), "{texts:?}");
        assert!(texts.contains(&"Ohne Kurztext."), "{texts:?}");
        assert!(
            texts
                .iter()
                .any(|text| text.starts_with("Ohne Einheitspreis")),
            "eine andere Position hat einen EP: {texts:?}"
        );
        assert_eq!(findings[0].severity, Severity::Error, "Fehler zuerst");
        // Only this row – the base position's finding belongs to it.
        assert!(!texts.iter().any(|text| text.contains("Gruppe")));
        let base = row_findings(&project, id_of(&project, "Grund"), None);
        let finding = base
            .iter()
            .find(|finding| finding.text.contains("ohne Wahlposition"))
            .expect("Grundposition ohne Wahlposition");
        assert_eq!(finding.tab, Tab::More);
        // A good position: only the hint about the empty long text.
        let station = row_findings(&project, id_of(&project, "Station"), None);
        assert_eq!(
            station
                .iter()
                .map(|finding| finding.text.as_str())
                .collect::<Vec<_>>(),
            ["Langtext leer."]
        );
    }

    #[test]
    fn findings_name_empty_owner_complements_doubled_ordinals_and_images() {
        let mut project = project();
        let id = id_of(&project, "Kabel");
        let index = project.lv().index_of(id).unwrap();
        if let Some(position) = project
            .lv_mut()
            .row_mut(index)
            .and_then(LvRow::as_position_mut)
        {
            position.set_complements(vec![
                TextComplement {
                    marker: "01".into(),
                    kind: ComplementKind::Owner,
                    ..TextComplement::default()
                },
                TextComplement {
                    marker: "02".into(),
                    kind: ComplementKind::Bidder,
                    ..TextComplement::default()
                },
            ]);
            position.set_long_text(with_image_at_end(&Text::plain("Text"), "a.png".into()));
            position.set_ordinal("01.01.0010");
        }
        let findings = row_findings(&project, id, None);
        let texts: Vec<&str> = findings
            .iter()
            .map(|finding| finding.text.as_str())
            .collect();
        assert!(
            texts.contains(&"Textergänzung 01 (vom Auftraggeber) ist leer."),
            "{texts:?}"
        );
        assert!(
            !texts.iter().any(|text| text.contains("Textergänzung 02")),
            "Bieter füllt selbst"
        );
        assert!(
            texts.contains(&"OZ 01.01.0010 kommt mehrfach vor."),
            "{texts:?}"
        );
        let image = findings
            .iter()
            .find(|finding| finding.text.starts_with("1 Bild"))
            .unwrap();
        assert_eq!(image.tab, Tab::Graphics);
        assert_eq!(image.severity, Severity::Hint);
    }

    #[test]
    fn findings_of_titles_hints_and_descriptions() {
        let mut project = project();
        let lv = project.lv_mut();
        let description = lv.push_row(LvRow::Performance(PerformanceDescription::new(
            "1",
            "",
            Text::default(),
        )));
        let note = lv.push_note(Note::new(""));
        let title = lv.push_group(LvGroup::new(0, "", "Leer"));
        lv.renumber();
        let texts = |id| -> Vec<String> {
            row_findings(&project, id, None)
                .into_iter()
                .map(|finding| finding.text)
                .collect()
        };
        let description_texts = texts(description);
        assert!(description_texts.contains(&"Ohne Bezeichnung.".to_string()));
        assert!(description_texts.contains(&"Noch von keiner Position verwendet.".to_string()));
        assert_eq!(
            texts(note),
            ["Ohne Text nach 01.02.0010 – für eine Leerzeile in der Bearbeitung den Abstandshalter verwenden."]
        );
        assert!(
            texts(title).contains(&"Enthält keine Positionen.".to_string()),
            "{:?}",
            texts(title)
        );
    }

    #[test]
    fn unknown_words_are_one_hint_with_their_places() {
        const AFF: &str = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwz\n";
        const DIC: &str = "3\nKabel\nliefern\nmontieren\n";
        let speller = proava_spell::Speller::from_text(AFF, DIC, &[]).unwrap();
        let mut project = project();
        let id = id_of(&project, "Kabel");
        let index = project.lv().index_of(id).unwrap();
        if let Some(position) = project
            .lv_mut()
            .row_mut(index)
            .and_then(LvRow::as_position_mut)
        {
            position.set_long_text(Text::plain("Kabel liefern und montiren"));
        }
        let findings = row_findings(&project, id, Some(&speller));
        let spelling: Vec<&RowFinding> = findings
            .iter()
            .filter(|finding| finding.text.contains("Rechtschreibung"))
            .collect();
        assert_eq!(spelling.len(), 1, "{findings:?}");
        assert_eq!(spelling[0].severity, Severity::Hint);
        assert_eq!(spelling[0].tab, Tab::General);
        assert!(
            spelling[0].text.contains("„montiren“ (Langtext)"),
            "{}",
            spelling[0].text
        );
        assert!(
            spelling[0].text.contains("„und“ (Langtext)"),
            "nicht im Testwörterbuch"
        );
        assert!(!spelling[0].text.contains("„Kabel“"), "bekannt");
        // Without a dictionary (check off or still loading): nothing.
        assert!(!row_findings(&project, id, None)
            .iter()
            .any(|finding| finding.text.contains("Rechtschreibung")));
    }

    #[test]
    fn billing_takeoff_is_grouped_by_invoice() {
        use proava_domain::{Billing, Invoice, InvoiceKind, TakeoffLine};
        let mut billing = Billing::default();
        let first = billing.add_invoice(Invoice::new(InvoiceKind::Partial, "1", "01.10.2026"));
        let second = billing.add_invoice(Invoice::new(InvoiceKind::Final, "", "01.11.2026"));
        let row = RowId(7);
        let line = |label: &str, formula: &str, invoice| TakeoffLine {
            label: label.into(),
            formula: formula.into(),
            invoice,
        };
        billing.takeoff_mut(row).extend([
            line("EG", "10", Some(first)),
            line("ohne", "1,5", None),
            line("OG", "2*3", Some(second)),
            line("EG Nachtrag", "2", Some(first)),
        ]);
        billing
            .takeoff_mut(RowId(8))
            .push(line("andere", "99", Some(first)));
        let groups = billing_takeoff(&billing, row);
        let summary: Vec<(String, usize, String)> = groups
            .iter()
            .map(|(heading, lines, sum)| (heading.clone(), lines.len(), sum.to_string()))
            .collect();
        assert_eq!(
            summary,
            [
                ("Abschlagsrechnung 1".to_string(), 2, "12".to_string()),
                ("Schlussrechnung".to_string(), 1, "6".to_string()),
                (
                    "Keiner Rechnung zugeordnet".to_string(),
                    1,
                    "1.5".to_string()
                ),
            ]
        );
        assert!(billing_takeoff(&billing, RowId(9)).is_empty());
    }

    #[test]
    fn images_are_listed_resized_and_removed() {
        let text = with_image_at_end(
            &with_image_at_end(&Text::plain("A\nB"), "a.png".into()),
            "b.png".into(),
        );
        let list = text_images(&text);
        assert_eq!(
            list,
            [
                (2, "a.png".to_string(), Some(DEFAULT_IMAGE_WIDTH_MM)),
                (3, "b.png".to_string(), Some(DEFAULT_IMAGE_WIDTH_MM))
            ]
        );
        let wider = with_image_width(&text, 3, 120.0);
        assert_eq!(text_images(&wider)[1].2, Some(120.0));
        assert_eq!(
            text_images(&with_image_width(&text, 3, 900.0))[1].2,
            Some(180.0)
        );
        let fewer = without_image(&wider, 2);
        assert_eq!(text_images(&fewer), [(2, "b.png".to_string(), Some(120.0))]);
        assert_eq!(fewer.to_plain(), "A\nB", "Text bleibt");
        assert_eq!(
            without_image(&fewer, 0),
            fewer,
            "kein Bild an der Stelle: nichts"
        );
    }

    #[test]
    fn short_findings_drop_the_row_name() {
        assert_eq!(
            short_finding("Position 01.0010 ohne Menge.", &["Position 01.0010"]),
            "Ohne Menge."
        );
        assert_eq!(
            short_finding(
                "Position 01.0010: Kurztext hat 80 Zeichen.",
                &["Position 01.0010"]
            ),
            "Kurztext hat 80 Zeichen."
        );
        assert_eq!(
            short_finding("OZ 1 kommt mehrfach vor.", &["Position 1"]),
            "OZ 1 kommt mehrfach vor."
        );
    }
}
