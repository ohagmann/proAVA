//! Eingebaute Hilfe: „Hilfe ▸ Tastenkürzel …“ / F1 (searchable overview from
//! [`crate::shortcuts`], filtered by the view F1 was pressed in) and
//! „Hilfe ▸ Handbuch (PDF)“ (the PDF next to the program, else the copy built
//! into the program). Any screen asks with [`request`] / [`request_handbook`];
//! the host shows the window above everything.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};

use crate::shortcuts::{self, Area};
use crate::theme::Palette;
use crate::widgets;

/// Handbuch, built from `docs/handbuch/handbuch.typ` (see docs/handbuch/README.md).
pub const HANDBOOK_PDF: &[u8] = include_bytes!("../../../docs/handbuch/proava-handbuch.pdf");
/// File name next to the program (release archives) and in the temp folder.
pub const HANDBOOK_FILE: &str = "proava-handbuch.pdf";

const REQUEST_ID: &str = "proava_help_request";
const HANDBOOK_ID: &str = "proava_handbook_request";

/// Which part of the program the help is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Topic {
    /// Everything (menu „Tastenkürzel …“).
    #[default]
    All,
    Start,
    Lv,
    Bids,
    Comparison,
    Billing,
    ChangeOrders,
    Costs,
}

impl Topic {
    pub const ALL: [Topic; 8] = [
        Topic::All,
        Topic::Start,
        Topic::Lv,
        Topic::Bids,
        Topic::Comparison,
        Topic::Billing,
        Topic::ChangeOrders,
        Topic::Costs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Topic::All => "Alle",
            Topic::Start => "Startseite",
            Topic::Lv => "LV",
            Topic::Bids => "Angebote",
            Topic::Comparison => "Preisspiegel",
            Topic::Billing => "Abrechnung",
            Topic::ChangeOrders => "Nachträge",
            Topic::Costs => "Kosten",
        }
    }

    /// Areas of the shortcut overview that belong to the topic.
    pub fn areas(self) -> &'static [Area] {
        match self {
            Topic::All => &Area::ALL,
            Topic::Lv => &[
                Area::General,
                Area::NewRows,
                Area::Lv,
                Area::Structure,
                Area::Cell,
                Area::Text,
            ],
            // The start screen has no keys of its own (Strg+S … need a project).
            Topic::Start => &[],
            Topic::Bids | Topic::Comparison | Topic::ChangeOrders => &[Area::General],
            Topic::Billing => &[Area::General, Area::Billing],
            Topic::Costs => &[Area::General, Area::Costs],
        }
    }

    /// Short orientation and the chapter of the Handbuch.
    pub fn intro(self) -> (&'static str, &'static str) {
        match self {
            Topic::All => (
                "Alle Tastenkürzel von proAVA. Die Liste entsteht aus denselben Befehlen wie Menüs \
                 und Befehlssuche (Strg+K) – sie stimmt also immer mit dem Programm überein.",
                "Kapitel „Tastenkürzel“",
            ),
            Topic::Start => (
                "Startseite: neues Projekt anlegen (leer, aus einer Vorlage, aus einer GAEB-Datei \
                 X81–X86, D81–D86 oder einer CSV-Tabelle), Sicherung anlegen oder wiederherstellen \
                 oder ein zuletzt verwendetes Projekt öffnen.",
                "Kapitel „Projekt anlegen und öffnen“",
            ),
            Topic::Lv => (
                "LV-Tabelle: Zeile wählen, Zelle mit F2/Enter oder einfach durch Tippen bearbeiten. \
                 Langtext, Bilder und Ausführungsbeschreibungen im Eigenschaftenfenster oder mit \
                 Umschalt+F2 im eigenen Fenster. Rechtsklick zeigt alle Befehle zur Zeile. \
                 „Importieren …“ im Menü GAEB holt ein LV oder eine Zulieferung (X81–X86, D81–D86) \
                 als neues LV, neues Projekt, ins aktive LV oder in die Bibliothek. \
                 F8 blendet die Positionsbibliothek mit Volltextsuche ein.",
                "Kapitel „LV schreiben“",
            ),
            Topic::ChangeOrders => (
                "Nachträge: Liste mit Status, Summen und Prognose; Nachtrag bearbeiten mit Kopf, \
                 Nachtrags-LV (neue Positionen im Nachtragsbereich, Mehr-/Mindermengen), angebotenen \
                 und geprüften Preisen, Prüfhilfe und Statusverlauf. PDF „Prüfung Nachtragsangebot“ \
                 und „Nachtragsübersicht“, Nachtrags-LV als X83/X86, Nachtragsangebot einlesen.",
                "Kapitel „Nachträge“",
            ),
            Topic::Bids => (
                "Angebote: oben das Bieterverzeichnis (angefragte Bieter, Eingang, Niederschrift, \
                 Schriftverkehr), darunter die GAEB-Angebote (X84, D84); proAVA prüft sie gegen das \
                 LV (fehlende Preise, geänderte Mengen, Textergänzungen der Bieter). Nachlässe und \
                 Wertung je Bieter, Auftrag erteilen.",
                "Kapitel „Angebote einlesen und prüfen“",
            ),
            Topic::Comparison => (
                "Preisspiegel: alle Bieter nebeneinander je Position und Titel, günstigster Preis \
                 markiert, Abweichung zum Mittel. Von hier aus Auftrag erteilen und drucken.",
                "Kapitel „Preisspiegel“ und „Vergabe abschließen“",
            ),
            Topic::Costs => (
                "Kosten: Kostenschätzung nach DIN 276 (2. Ebene) und Kostenberechnung (3. Ebene). Normalfall \
                 ist das Kosten-LV – ein Kurztext-LV mit Menge × Kostenkennwert je Position, Titel nach \
                 Kostengruppen, bearbeitet im Reiter LV, nie für Bieter. Sonderfall Gebäudeautomation: \
                 Datenpunkte je Bauteil × Kennwert bzw. EP je DP-Art, aufgeteilt auf KG 481–489 mit \
                 Prüfsumme. Kostenkontrolle gegen die LVs, PDF mit Strg+P.",
                "Kapitel „Kosten nach DIN 276“",
            ),
            Topic::Billing => (
                "Abrechnung: Auftrag aus dem beauftragten Angebot übernehmen, Rechnungen der Firma \
                 erfassen, je Position Rechnungsmenge, geprüfte Menge, Aufmaß (Strg+M) oder \
                 Leistungsfortschritt eintragen; der Rechenweg zeigt die Freigabe, die Befunde \
                 führen zur Position.",
                "Kapitel „Abrechnung“",
            ),
        }
    }
}

/// Ask the host to open the help window on `topic`.
pub fn request(ctx: &egui::Context, topic: Topic) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique(REQUEST_ID), topic));
}

/// The topic someone asked for since the last call (clears the request).
pub fn take_request(ctx: &egui::Context) -> Option<Topic> {
    ctx.data_mut(|data| data.remove_temp::<Topic>(egui::Id::unique(REQUEST_ID)))
}

/// Ask the host to open the Handbuch (PDF).
pub fn request_handbook(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique(HANDBOOK_ID), true));
}

pub fn take_handbook_request(ctx: &egui::Context) -> bool {
    ctx.data_mut(|data| data.remove_temp::<bool>(egui::Id::unique(HANDBOOK_ID)))
        .unwrap_or(false)
}

/// Where the Handbuch is read from: next to the program (release archive,
/// `~/.local/share/doc/proava` after install_linux.sh), else `None`.
fn shipped_handbook(exe: &Path) -> Option<PathBuf> {
    let folder = exe.parent()?;
    [
        folder.join(HANDBOOK_FILE),
        folder.join("../share/doc/proava").join(HANDBOOK_FILE),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

/// Writes the built-in copy to `folder` unless the same file is already there.
fn write_builtin(folder: &Path) -> Result<PathBuf, String> {
    let path = folder.join(format!("proava-handbuch-{}.pdf", crate::about::VERSION));
    let same = std::fs::metadata(&path)
        .map(|meta| meta.len() == HANDBOOK_PDF.len() as u64)
        .unwrap_or(false);
    if !same {
        std::fs::write(&path, HANDBOOK_PDF).map_err(|error| {
            format!(
                "Handbuch lässt sich nicht ablegen ({}): {error}",
                path.display()
            )
        })?;
    }
    Ok(path)
}

/// The Handbuch as a file: shipped next to the program, else the built-in copy
/// in the temp folder.
pub fn handbook_file() -> Result<PathBuf, String> {
    if let Some(path) = std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(shipped_handbook)
    {
        return Ok(path);
    }
    write_builtin(&std::env::temp_dir())
}

/// Opens the Handbuch in the system's PDF viewer.
pub fn open_handbook() -> Result<PathBuf, String> {
    let path = handbook_file()?;
    crate::jobs::open_with_system(&path)?;
    Ok(path)
}

pub struct HelpWindow {
    topic: Topic,
    search: String,
    focus: bool,
    notice: Option<String>,
}

impl HelpWindow {
    pub fn new(topic: Topic) -> Self {
        Self {
            topic,
            search: String::new(),
            focus: true,
            notice: None,
        }
    }

    /// Message shown above the list (e.g. the Handbuch could not be opened).
    pub fn with_notice(mut self, notice: impl Into<String>) -> Self {
        self.notice = Some(notice.into());
        self
    }

    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Lines shown for the current topic and search, grouped by area.
    pub fn lines(&self) -> Vec<shortcuts::Shortcut> {
        let areas = self.topic.areas();
        shortcuts::filtered(&self.search, None)
            .into_iter()
            .filter(|entry| areas.contains(&entry.area))
            .collect()
    }

    /// Draws the window; false when it was closed.
    pub fn show(&mut self, ui: &mut egui::Ui, palette: &Palette) -> bool {
        let mut open = true;
        let height = (ui.ctx().content_rect().height() - 330.0).clamp(160.0, 560.0);
        let (_, close) = widgets::modal(ui, palette, 760.0, |ui| {
            ui.label(
                RichText::new("Hilfe – Tastenkürzel")
                    .size(20.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(8.0);
            crate::widgets::row(ui, |ui| {
                for topic in Topic::ALL {
                    if ui
                        .selectable_label(self.topic == topic, topic.label())
                        .clicked()
                    {
                        self.topic = topic;
                    }
                }
                ui.add_space(16.0);
                ui.label(RichText::new("Suchen").size(12.5).color(palette.text_dim));
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Befehl oder Taste, z. B. „titel“ oder „strg+t“")
                        .desired_width(260.0),
                );
                if self.focus {
                    search.request_focus();
                    self.focus = false;
                }
            });
            ui.add_space(6.0);
            if let Some(notice) = &self.notice {
                ui.label(RichText::new(notice).size(12.5).color(palette.error));
                ui.add_space(4.0);
            }
            let (intro, chapter) = self.topic.intro();
            ui.label(RichText::new(intro).size(12.5).color(palette.text));
            ui.add_space(8.0);

            let lines = self.lines();
            egui::Frame::new()
                .stroke(egui::Stroke::new(1.0, palette.line))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if lines.is_empty() {
                                let text = if self.topic.areas().is_empty() {
                                    "Die Startseite hat keine eigenen Tastenkürzel. Im geöffneten \
                                     Projekt gelten die unter „Alle“."
                                } else {
                                    "Kein Tastenkürzel passt zur Suche."
                                };
                                ui.label(RichText::new(text).color(palette.text_dim));
                            }
                            for area in Area::ALL {
                                let group: Vec<&shortcuts::Shortcut> =
                                    lines.iter().filter(|entry| entry.area == area).collect();
                                if group.is_empty() {
                                    continue;
                                }
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(area.label())
                                        .size(12.5)
                                        .strong()
                                        .color(palette.accent),
                                );
                                egui::Grid::new(("help_shortcuts", area))
                                    .num_columns(2)
                                    .min_col_width(360.0)
                                    .spacing([16.0, 3.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        for entry in group {
                                            ui.label(
                                                RichText::new(&entry.action)
                                                    .size(12.5)
                                                    .color(palette.text),
                                            );
                                            ui.label(
                                                RichText::new(entry.keys.join("  ·  "))
                                                    .size(12.5)
                                                    .monospace()
                                                    .color(palette.text),
                                            );
                                            ui.end_row();
                                        }
                                    });
                            }
                        });
                });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .button(format!("Handbuch (PDF) öffnen – {chapter}"))
                    .clicked()
                {
                    match open_handbook() {
                        Ok(_) => self.notice = None,
                        Err(error) => self.notice = Some(error),
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::accent_button(ui, palette, "Schließen") {
                        open = false;
                    }
                });
            });
        });
        open && !close
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handbook_is_built_in_and_a_pdf() {
        assert!(HANDBOOK_PDF.starts_with(b"%PDF-"));
        assert!(HANDBOOK_PDF.len() > 50_000, "Handbuch fehlt oder ist leer");
    }

    #[test]
    fn builtin_copy_is_written_once() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-help-test-")
            .tempdir()
            .unwrap();
        let folder = scratch.path().to_path_buf();
        std::fs::create_dir_all(&folder).unwrap();
        let first = write_builtin(&folder).unwrap();
        assert_eq!(std::fs::read(&first).unwrap(), HANDBOOK_PDF);
        let second = write_builtin(&folder).unwrap();
        assert_eq!(first, second);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn shipped_copy_next_to_the_program_wins() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-help-ship-")
            .tempdir()
            .unwrap();
        let folder = scratch.path().to_path_buf();
        std::fs::create_dir_all(&folder).unwrap();
        let exe = folder.join("proava");
        assert_eq!(shipped_handbook(&exe), None);
        std::fs::write(folder.join(HANDBOOK_FILE), b"%PDF-1.7").unwrap();
        assert_eq!(shipped_handbook(&exe), Some(folder.join(HANDBOOK_FILE)));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn topics_filter_the_overview() {
        let mut window = HelpWindow::new(Topic::Lv);
        let lv = window.lines();
        assert!(lv.iter().any(|entry| entry.area == Area::Lv));
        assert!(lv.iter().any(|entry| entry.area == Area::Cell));
        window.topic = Topic::Start;
        assert!(window.lines().is_empty(), "Startseite ohne Projekt-Kürzel");
        window.topic = Topic::Comparison;
        assert!(window
            .lines()
            .iter()
            .all(|entry| entry.area == Area::General));
        window.topic = Topic::All;
        window.search = "strg+t".into();
        let found = window.lines();
        assert!(
            found.iter().any(|entry| entry.action == "Neuer Titel"),
            "{found:?}"
        );
        assert!(found.iter().all(|entry| entry
            .keys
            .iter()
            .any(|key| key.to_lowercase().contains("strg+t"))));
    }
}
