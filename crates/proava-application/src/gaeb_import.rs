//! GAEB-Import: an LV file (X81 … X86, D81 … D86, also inside a zip, or a
//! CSV table in the proAVA CSV format) is read into memory, shown with what is in
//! it, and then put where the user wants it – as a new LV of the open
//! project, as a new project under a free name, into the own library or as
//! the source of a takeover. Every import ends with a short protocol: what
//! was read, what was taken, what was left out or changed and why.

use std::path::{Path, PathBuf};

use proava_domain::price_check::{self, amount, currency_name};
use proava_domain::transfer::{self, TransferOptions};
use proava_domain::wording::count;
use proava_domain::{Lv, LvRow, Money, Project, ProjectName, RowId};

use crate::exchange::{ExchangeFiles, LvImport};
use crate::protocol::{Protocol, ProtocolEntry};

/// A GAEB file read into memory, ready to be put somewhere.
#[derive(Debug, Clone)]
pub struct GaebImport {
    pub file: PathBuf,
    pub read: LvImport,
}

impl GaebImport {
    /// A CSV table (proAVA CSV format) rather than a GAEB file.
    pub fn is_csv(&self) -> bool {
        self.read.format.starts_with("CSV")
    }

    /// Währung of the read LV (CSV: chosen in the dialog; the code, e.g.
    /// „EUR“, „USD“, „CHF“). The texts of the GP-Prüfung follow it.
    pub fn set_currency(&mut self, code: &str) {
        let code = code.trim().to_uppercase();
        if !code.is_empty() {
            self.read.project.lv_mut().set_currency(code);
        }
    }
}

/// Reads a GAEB file for the import (on a worker thread if it is large).
/// The project in memory is named after the file.
pub fn read(
    exchange: &dyn ExchangeFiles,
    file: &Path,
    now_iso: &str,
) -> Result<GaebImport, String> {
    let stem = file
        .file_stem()
        .map(|stem| stem.to_string_lossy().trim().to_string())
        .unwrap_or_default();
    let name = ProjectName::parse(&stem)
        .or_else(|_| ProjectName::parse("GAEB-Import"))
        .map_err(|error| error.to_string())?;
    let read = exchange.read_lv(file, &name, now_iso)?;
    Ok(GaebImport {
        file: file.to_path_buf(),
        read,
    })
}

/// Name of an exchange phase as the user knows it.
pub fn phase_label(phase: u8) -> &'static str {
    match phase {
        81 => "Leistungsbeschreibung",
        // DIN 276 calls the priced LV Kostenvoranschlag; the GAEB name stays visible.
        82 => "Bepreistes LV (GAEB: Kostenanschlag)",
        83 => "Angebotsaufforderung",
        84 => "Angebotsabgabe",
        85 => "Nebenangebot",
        86 => "Auftragserteilung",
        _ => "unbekannte Phase",
    }
}

/// What the dialog shows before anything is taken.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPreview {
    pub file_name: String,
    /// „X83 – Angebotsaufforderung“, „D81 – Leistungsbeschreibung“.
    pub kind: String,
    pub phase: Option<u8>,
    /// „GAEB DA XML 3.2“, „GAEB 90“.
    pub format: String,
    pub program: String,
    pub lv_number: String,
    pub lv_title: String,
    /// Project label of the file (LblPrj), if any.
    pub project_label: String,
    pub oz_mask: String,
    pub titles: usize,
    pub positions: usize,
    /// Hinweise inside the LV (without the Vorbemerkungen).
    pub notes: usize,
    pub preliminary: usize,
    /// Schlussbemerkungen (texts after the LV).
    pub closing: usize,
    pub performances: usize,
    pub images: usize,
    pub without_quantity: usize,
    pub without_price: usize,
    pub net_total: Money,
    pub warnings: Vec<String>,
    /// CSV: columns as recognised in the order of the file („–“ = not used);
    /// empty for GAEB.
    pub columns: Vec<String>,
    /// Währung of the LV (code, „EUR“ …).
    pub currency: String,
    /// GP-Prüfung: positions whose GP in the file is not Menge × EP …
    pub price_checks: usize,
    /// … and the sum of the differences (GP laut Datei minus errechnet).
    pub price_difference: Money,
    /// „OZ: GP laut Datei 12,00 €, errechnet 10,00 € – Abweichung 2,00 €“
    pub price_check_lines: Vec<String>,
    /// Further facts of the file (CSV: Trenner, Zeichensatz).
    pub facts: Vec<(String, String)>,
}

impl ImportPreview {
    pub fn of(import: &GaebImport) -> Self {
        let project = &import.read.project;
        let lv = project.lv();
        let phase = import.read.phase;
        let letter = if import.read.format.contains("90") {
            'D'
        } else {
            'X'
        };
        let kind = match phase {
            Some(phase) => format!("{letter}{phase} – {}", phase_label(phase)),
            None if import.is_csv() => "CSV-Tabelle".to_string(),
            None => "unbekannt".to_string(),
        };
        let file_name = import
            .file
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let notes: Vec<&proava_domain::Note> = lv
            .rows()
            .iter()
            .filter_map(|row| match row {
                LvRow::Note(note) => Some(note),
                _ => None,
            })
            .collect();
        let preliminary = notes.iter().filter(|note| note.is_preliminary()).count();
        let closing = notes.iter().filter(|note| note.is_closing()).count();
        let (price_checks, price_difference) = price_check::open_checks(lv);
        let price_check_lines = lv
            .positions()
            .filter_map(|position| {
                position
                    .price_check()
                    .map(|check| format!("{}: {}", position.ordinal(), check.text(lv.currency())))
            })
            .collect();
        Self {
            file_name,
            kind,
            phase,
            // A CSV table names Trenner and Zeichensatz in `facts`.
            format: if import.is_csv() {
                String::new()
            } else {
                import.read.format.clone()
            },
            program: import.read.program.clone(),
            lv_number: lv.number().trim().to_string(),
            lv_title: lv.title().trim().to_string(),
            project_label: project.info().label.trim().to_string(),
            oz_mask: lv.oz_mask().pattern().to_string(),
            titles: lv.group_count(),
            positions: lv.position_count(),
            notes: notes.len() - preliminary - closing,
            preliminary,
            closing,
            performances: lv.performance_count(),
            images: project.images().len(),
            without_quantity: lv
                .positions()
                .filter(|position| position.quantity().is_zero() && !position.is_lump_sum())
                .count(),
            without_price: lv
                .positions()
                .filter(|position| position.unit_price().is_none())
                .count(),
            net_total: lv.net_total(),
            // The images are counted in the content line already.
            warnings: import
                .read
                .warnings
                .iter()
                .filter(|warning| !is_image_note(warning))
                .cloned()
                .collect(),
            columns: import.read.columns.clone(),
            currency: lv.currency().to_string(),
            price_checks,
            price_difference,
            price_check_lines,
            facts: import.read.facts.clone(),
        }
    }

    /// „1.234,56 €“, „1.234,56 USD“ in the Währung of the LV.
    pub fn amount(&self, money: Money) -> String {
        amount(money, &self.currency)
    }

    /// „Euro (EUR)“
    pub fn currency_label(&self) -> String {
        match currency_name(&self.currency) {
            "" => self.currency.clone(),
            name => format!("{name} ({})", self.currency.to_uppercase()),
        }
    }

    /// GP-Prüfung: the positions whose GP in the file is not Menge × EP
    /// (with the list) and the sum of the differences; empty when all fit.
    pub fn price_check_entries(&self) -> Vec<ProtocolEntry> {
        if self.price_checks == 0 {
            return Vec::new();
        }
        vec![
            ProtocolEntry::count("GP-Abweichungen", self.price_checks)
                .with_details(self.price_check_lines.clone())
                .warning(),
            ProtocolEntry::number("Summe der Abweichungen", self.amount(self.price_difference))
                .warning(),
        ]
    }

    /// A bidder's offer (X84/D84, X85): belongs to „Angebote einlesen“.
    pub fn is_bid(&self) -> bool {
        matches!(self.phase, Some(84 | 85))
    }

    /// Vortexte, Herstellertexte: a description without prices (X81/D81, or
    /// any file in which no position has a price).
    pub fn is_description(&self) -> bool {
        self.phase == Some(81) || (self.positions > 0 && self.without_price == self.positions)
    }

    /// Positions with a unit price in the file (X82/D82, X84 …).
    pub fn priced_positions(&self) -> usize {
        self.positions - self.without_price
    }

    /// Default of „Preise übernehmen“: on when the file has prices.
    pub fn has_prices(&self) -> bool {
        self.priced_positions() > 0
    }

    /// Proposal for the LV number in the project: the file's, if it is a
    /// free short number („2“, „481“, „0000“), else the next free one.
    pub fn suggested_number(&self, project: &Project) -> String {
        let taken = |number: &str| project.lvs().any(|lv| lv.number().trim() == number);
        let short = self.lv_number.chars().count() <= 12
            && !self.lv_number.chars().any(char::is_whitespace);
        if !self.lv_number.is_empty() && short && !taken(&self.lv_number) {
            return self.lv_number.clone();
        }
        next_free_number(project)
    }

    /// Proposal for the LV title: the file's LV label, else the file name.
    pub fn suggested_title(&self) -> String {
        if !self.lv_title.is_empty() {
            return self.lv_title.clone();
        }
        Path::new(&self.file_name)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "Leistungsverzeichnis".to_string())
    }

    /// What the file holds, one fact per line (preview, protocol and MCP):
    /// Datei, Art, Format, erstellt mit, LV, OZ-Maske, the counts (only
    /// those > 0 besides Titel and Positionen), Summe netto, Positionen ohne
    /// Preis/Menge.
    pub fn entries(&self) -> Vec<ProtocolEntry> {
        let mut entries = vec![
            ProtocolEntry::text("Datei", &self.file_name),
            ProtocolEntry::text("Art", &self.kind),
        ];
        if !self.format.is_empty() {
            entries.push(ProtocolEntry::text("Format", &self.format));
        }
        entries.extend(
            self.facts
                .iter()
                .map(|(label, value)| ProtocolEntry::text(label, value)),
        );
        if !self.program.is_empty() {
            entries.push(ProtocolEntry::text("Erstellt mit", &self.program));
        }
        if !self.columns.is_empty() {
            entries.push(ProtocolEntry::text("Spalten", self.columns.join(", ")));
        }
        let lv = match (self.lv_number.as_str(), self.lv_title.as_str()) {
            ("", "") => "ohne Bezeichnung".to_string(),
            ("", title) => title.to_string(),
            (number, "") => number.to_string(),
            (number, title) => format!("{number} – {title}"),
        };
        entries.push(ProtocolEntry::text("LV", lv));
        if !self.project_label.is_empty() {
            let label = self
                .project_label
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            entries.push(ProtocolEntry::text("Projektbezeichnung", label));
        }
        entries.push(ProtocolEntry::text("OZ-Maske", &self.oz_mask));
        if !self.columns.is_empty() {
            entries.push(ProtocolEntry::text("Währung", self.currency_label()));
        }
        entries.push(ProtocolEntry::count("Titel", self.titles));
        entries.push(ProtocolEntry::count("Positionen", self.positions));
        for (label, value) in [
            ("Vorbemerkungen", self.preliminary),
            ("Hinweise", self.notes),
            ("Schlussbemerkungen", self.closing),
            ("Ausführungsbeschreibungen", self.performances),
            ("Bilder", self.images),
        ] {
            if value > 0 {
                entries.push(ProtocolEntry::count(label, value));
            }
        }
        entries.push(ProtocolEntry::number(
            "Summe netto",
            self.amount(self.net_total),
        ));
        if self.without_price > 0 {
            entries.push(ProtocolEntry::count(
                "Positionen ohne Preis",
                self.without_price,
            ));
        }
        if self.without_quantity > 0 {
            entries.push(ProtocolEntry::count(
                "Positionen ohne Menge",
                self.without_quantity,
            ));
        }
        entries
    }

    /// The preview before the import: [`Self::entries`], the GP-Prüfung and
    /// what struck the reader.
    pub fn protocol(&self) -> Protocol {
        let mut protocol = Protocol::new(format!("Vorschau {}", self.file_name));
        protocol.entries = self.entries();
        protocol.entries.extend(self.price_check_entries());
        if self.price_checks > 0 {
            protocol.push(ProtocolEntry::text(
                "GP-Prüfung",
                "errechneter GP (Menge × EP) gilt, die Zeilen werden orange markiert",
            ));
        }
        if !self.warnings.is_empty() {
            protocol.push(
                ProtocolEntry::count("Befunde beim Lesen", self.warnings.len())
                    .with_details(self.warnings.clone()),
            );
        }
        protocol
    }
}

/// The next number not used by an LV of the project (1, 2 …).
pub fn next_free_number(project: &Project) -> String {
    let used: Vec<String> = project
        .lvs()
        .map(|lv| lv.number().trim().to_string())
        .collect();
    let mut next = used
        .iter()
        .filter_map(|number| number.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    while used.contains(&next.to_string()) {
        next += 1;
    }
    next.to_string()
}

/// First words of every protocol title – a host can recognise a protocol
/// among plain messages by it.
pub const PROTOCOL_PREFIX: &str = "Importprotokoll ";

/// What became of an import: shown after every import, copyable – one fact
/// per line (see [`crate::protocol`]).
pub type ImportProtocol = Protocol;

/// Where the import went (for the protocol).
#[derive(Debug, Clone, PartialEq)]
pub enum ImportTarget {
    /// New LV of an existing project.
    Lv { project: String, label: String },
    /// New project file.
    Project { file: PathBuf },
    /// Rows under a title of the library.
    Library {
        file: PathBuf,
        title: String,
        rows: usize,
    },
    /// Source of the takeover window: the user picks rows there.
    Transfer,
}

/// What only [`add_as_lv`] knows: an empty LV it replaced and the
/// Projektangaben of the file it filled into a fresh project.
#[derive(Debug, Clone, Default)]
struct Extras {
    replaced: Option<String>,
    filled: Vec<&'static str>,
}

/// The protocol for `import` taken to `target` (`prices`: unit prices of
/// the file taken as planner's prices).
pub fn protocol(import: &GaebImport, target: &ImportTarget, prices: bool) -> ImportProtocol {
    protocol_with(import, target, prices, &Extras::default())
}

fn protocol_with(
    import: &GaebImport,
    target: &ImportTarget,
    prices: bool,
    extras: &Extras,
) -> ImportProtocol {
    let preview = ImportPreview::of(import);
    let mut protocol = Protocol::new(format!("{PROTOCOL_PREFIX}{}", preview.file_name))
        .with_hint("Was gelesen, übernommen, verworfen oder umgewandelt wurde.");
    protocol.entries = preview.entries();
    let info = import.read.project.info();
    let ordinals = match target {
        ImportTarget::Lv { project, label } => {
            protocol.push(ProtocolEntry::text(
                "Übernommen als",
                format!("neues {label}"),
            ));
            protocol.push(ProtocolEntry::text("Zielprojekt", project));
            "wie in der Datei"
        }
        ImportTarget::Project { file } => {
            protocol.push(ProtocolEntry::text("Übernommen als", "neues Projekt"));
            if let Some(folder) = file.parent() {
                protocol.push(ProtocolEntry::path("Speicherort", folder));
            }
            if let Some(name) = file.file_name() {
                protocol.push(ProtocolEntry::text("Projektdatei", name.to_string_lossy()));
            }
            "wie in der Datei"
        }
        ImportTarget::Library { file, title, rows } => {
            protocol.push(ProtocolEntry::text(
                "Übernommen als",
                "Zeilen in der Bibliothek",
            ));
            protocol.push(ProtocolEntry::path("Bibliothek", file));
            protocol.push(ProtocolEntry::text("Unter Titel", title));
            protocol.push(ProtocolEntry::count("Zeilen", *rows));
            "neu nummeriert, die Mengen der Datei bleiben"
        }
        ImportTarget::Transfer => {
            protocol.push(ProtocolEntry::text(
                "Übernommen als",
                "Quelle im Übernahme-Fenster",
            ));
            protocol.push(ProtocolEntry::text(
                "Weiter",
                "Titel oder Positionen dort wählen; „Übernehmen“ fügt sie hinter der Auswahl im aktiven LV ein",
            ));
            "frei am Einfügeort"
        }
    };
    if let Some(replaced) = &extras.replaced {
        protocol.push(ProtocolEntry::text(
            "Ersetzt",
            format!("das leere {replaced} des Projekts"),
        ));
    }
    if !extras.filled.is_empty() {
        protocol.push(
            ProtocolEntry::count("Projektangaben in leere Felder", extras.filled.len())
                .with_details(
                    extras
                        .filled
                        .iter()
                        .map(|label| label.to_string())
                        .collect(),
                ),
        );
    }
    protocol.push(ProtocolEntry::text("Ordnungszahlen", ordinals));
    if preview.has_prices() && prices {
        protocol.push(ProtocolEntry::text(
            "Preise",
            format!(
                "{} als Planerpreise",
                count(
                    preview.priced_positions(),
                    "Einheitspreis",
                    "Einheitspreise"
                )
            ),
        ));
    } else if preview.has_prices() {
        protocol.push(ProtocolEntry::text(
            "Preise",
            "nicht übernommen (Schalter aus)",
        ));
    } else if preview.positions > 0 {
        protocol.push(ProtocolEntry::text("Preise", "keine in der Datei"));
    }
    if preview.images > 0 {
        protocol.push(ProtocolEntry::count("Bilder übernommen", preview.images));
    }
    if preview.price_checks > 0 {
        protocol.entries.extend(preview.price_check_entries());
        protocol.push(ProtocolEntry::text(
            "GP-Prüfung",
            if prices {
                format!(
                    "errechneter GP (Menge × EP) gilt, Zeilen orange mit Status „{}“ bis zum Bestätigen",
                    proava_domain::PriceCheck::LABEL
                )
            } else {
                "ohne Preise übernommen, nichts markiert".to_string()
            },
        ));
    }
    if preview.positions > 0 && *target != ImportTarget::Transfer {
        protocol.push(ProtocolEntry::text(
            "Bearbeitungsstatus",
            "alle Positionen „offen“",
        ));
    }
    // Pure information already shown above.
    let mut changed: Vec<String> = import
        .read
        .warnings
        .iter()
        .filter(|warning| !is_image_note(warning))
        .cloned()
        .collect();
    if preview.has_prices() && !prices {
        changed.push(format!(
            "Einheitspreise der Datei nicht übernommen ({}) – Schalter „Preise übernehmen“ war aus.",
            count(preview.priced_positions(), "Position", "Positionen")
        ));
    }
    if preview.is_bid() {
        changed.push(
            "Bieterangaben der Datei nicht übernommen – Angebote liest „Angebote einlesen …“ im Menü Angebote ein."
                .to_string(),
        );
    }
    let keeps_info = matches!(
        target,
        ImportTarget::Lv { .. } | ImportTarget::Library { .. } | ImportTarget::Transfer
    );
    if keeps_info && extras.filled.is_empty() {
        let mut dropped = Vec::new();
        let label = short(&info.label);
        if !label.is_empty() {
            dropped.push(format!("Bezeichnung „{label}“"));
        }
        let owner = short(&info.owner.one_line());
        if !owner.is_empty() {
            dropped.push(format!("Auftraggeber „{owner}“"));
        }
        let award = short(&info.award_number);
        if !award.is_empty() {
            dropped.push(format!("Vergabenummer „{award}“"));
        }
        for (label, value) in [
            ("Abgabetermin", &info.submission_deadline),
            ("Ausführungsbeginn", &info.execution_start),
            ("Ausführungsende", &info.execution_end),
        ] {
            if !value.trim().is_empty() {
                dropped.push(format!("{label} {}", value.trim()));
            }
        }
        if !dropped.is_empty() {
            let reason = if matches!(target, ImportTarget::Library { .. }) {
                "die Bibliothek nimmt nur die Zeilen auf"
            } else {
                "das Projekt behält seine Stammdaten"
            };
            changed.push(format!(
                "Projektangaben der Datei nicht übernommen ({}) – {reason}.",
                dropped.join(", ")
            ));
        }
    }
    protocol.push(if changed.is_empty() {
        ProtocolEntry::text("Verworfen oder umgewandelt", "nichts")
    } else {
        ProtocolEntry::count("Verworfen oder umgewandelt", changed.len()).with_details(changed)
    });
    protocol
}

/// Puts the LV of `import` into `project` as a new LV with this number and
/// title (order numbers as in the file, images along, status „offen“).
/// A project whose only LV is still empty (fresh project) gets the import
/// instead of it. Returns the index of the new LV and the protocol; nothing
/// changes on an error.
pub fn add_as_lv(
    project: &mut Project,
    import: &GaebImport,
    number: &str,
    title: &str,
    prices: bool,
) -> Result<(usize, ImportProtocol), String> {
    let number = number.trim();
    let title = title.trim();
    if title.is_empty() {
        return Err("Bitte eine Bezeichnung für das LV angeben.".to_string());
    }
    let replace_empty = project.lv_count() == 1
        && project.lv().rows().is_empty()
        && project.bids_of(0).is_empty()
        && project.invitees_of(0).is_empty();
    if !replace_empty && !number.is_empty() && project.lvs().any(|lv| lv.number().trim() == number)
    {
        return Err(format!(
            "Die LV-Nummer „{number}“ gibt es im Projekt schon – bitte eine andere wählen (frei ist z. B. {}).",
            next_free_number(project)
        ));
    }
    let source = &import.read.project;
    let replaced = replace_empty.then(|| lv_label(project.lv()));
    let mut index = transfer::add_lv_from(project, source, source.lv());
    if replace_empty {
        project.set_active_lv(index);
        project.remove_lv(0)?;
        index = 0;
    }
    if let Some(lv) = project.lv_at_mut(index) {
        lv.set_number(number);
        lv.set_title(title);
        if !prices {
            clear_prices(lv);
        }
    }
    // A fresh project: the file's Projektangaben fill its empty fields.
    let filled = if replace_empty {
        fill_empty_info(project.info_mut(), source.info())
    } else {
        Vec::new()
    };
    let label = project.lv_at(index).map(lv_label).unwrap_or_default();
    let target = ImportTarget::Lv {
        project: project.name().as_str().to_string(),
        label,
    };
    let protocol = protocol_with(import, &target, prices, &Extras { replaced, filled });
    Ok((index, protocol))
}

/// Fills the empty Projektangaben of `info` from `source`; returns what was
/// taken (for the protocol). Filled fields stay as they are.
fn fill_empty_info(
    info: &mut proava_domain::ProjectInfo,
    source: &proava_domain::ProjectInfo,
) -> Vec<&'static str> {
    let mut filled = Vec::new();
    let texts: [(&'static str, &mut String, &String); 11] = [
        ("Bezeichnung", &mut info.label, &source.label),
        ("Kurzname", &mut info.short_name, &source.short_name),
        ("Baumaßnahme", &mut info.description, &source.description),
        (
            "Vergabenummer",
            &mut info.award_number,
            &source.award_number,
        ),
        (
            "Vergabeart",
            &mut info.award_procedure,
            &source.award_procedure,
        ),
        (
            "Abgabetermin",
            &mut info.submission_deadline,
            &source.submission_deadline,
        ),
        (
            "Abgabeort",
            &mut info.submission_place,
            &source.submission_place,
        ),
        (
            "Bindefrist",
            &mut info.binding_period_end,
            &source.binding_period_end,
        ),
        (
            "Ausführungsbeginn",
            &mut info.execution_start,
            &source.execution_start,
        ),
        (
            "Ausführungsende",
            &mut info.execution_end,
            &source.execution_end,
        ),
        (
            "Weitere Fristen",
            &mut info.execution_note,
            &source.execution_note,
        ),
    ];
    for (label, field, value) in texts {
        if field.trim().is_empty() && !value.trim().is_empty() {
            *field = value.clone();
            filled.push(label);
        }
    }
    if info.owner == proava_domain::Address::default()
        && source.owner != proava_domain::Address::default()
    {
        info.owner = source.owner.clone();
        filled.push("Auftraggeber");
    }
    if info.site == proava_domain::Address::default()
        && source.site != proava_domain::Address::default()
    {
        info.site = source.site.clone();
        filled.push("Baustelle");
    }
    filled
}

/// Removes the unit prices of all positions (import without prices).
pub fn clear_prices(lv: &mut Lv) {
    for index in 0..lv.rows().len() {
        if let Some(LvRow::Position(position)) = lv.row_mut(index) {
            position.set_unit_price(None);
            position.set_price_components(Vec::new());
        }
    }
}

/// The read project under another name (for „als neues Projekt“): same LV,
/// Stammdaten and images; every position „offen“; without `prices` the unit
/// prices are left out.
pub fn as_project(import: &GaebImport, name: &str, prices: bool) -> Result<Project, String> {
    let name = ProjectName::parse(name).map_err(|error| error.to_string())?;
    let source = &import.read.project;
    let mut project = Project::new(
        name,
        source.created_iso(),
        Lv::new("", source.lv().oz_mask().clone()),
    );
    *project.info_mut() = source.info().clone();
    let index = transfer::add_lv_from(&mut project, source, source.lv());
    project.set_active_lv(index);
    project.remove_lv(0)?;
    if !prices {
        clear_prices(project.lv_mut());
    }
    Ok(project)
}

/// All rows of the imported LV (titles stand for their blocks), e.g. for
/// „In Bibliothek übernehmen“.
pub fn all_rows(import: &GaebImport) -> Vec<RowId> {
    import
        .read
        .project
        .lv()
        .rows()
        .iter()
        .map(LvRow::id)
        .collect()
}

/// Takes all rows of the import under the title `title` (index) of `library`
/// with quantities (and prices, if wanted); returns the number of rows taken.
pub fn into_library(
    library: &mut Project,
    import: &GaebImport,
    title: usize,
    prices: bool,
    today: &str,
) -> Result<usize, String> {
    let source = &import.read.project;
    let mut taken = transfer::collect(
        source,
        source.lv(),
        &all_rows(import),
        TransferOptions {
            quantities: true,
            prices,
        },
    );
    proava_domain::library::prepare_for_library(&mut taken.rows, today);
    transfer::insert_under(library, title, &taken).map(|range| range.len())
}

/// The reader's note „1 Bild/n Bilder aus den Langtexten übernommen“ (no finding).
fn is_image_note(warning: &str) -> bool {
    warning.contains("Bild") && warning.contains(" aus den Langtexten übernommen")
}

/// One line of at most 80 characters (labels of some files span lines).
fn short(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= 80 {
        return line;
    }
    format!("{} …", line.chars().take(78).collect::<String>().trim_end())
}

/// „LV 2 – Elektro“
fn lv_label(lv: &Lv) -> String {
    match (lv.number().trim(), lv.title().trim()) {
        ("", title) => format!("LV „{title}“"),
        (number, title) => format!("LV {number} – {title}"),
    }
}

#[cfg(test)]
mod tests {
    use proava_domain::{
        Decimal, EditStatus, LvGroup, Note, OzMask, Position, RichText, TextBlock,
    };

    use super::*;

    fn read(phase: u8, warnings: Vec<String>) -> GaebImport {
        let mut lv = Lv::new("Vortexte Heizung", OzMask::parse("11.22.PPPP").unwrap());
        lv.set_number("1");
        let mut preliminary = Note::rich(RichText::plain("Allgemeine Vorbemerkungen"));
        preliminary.set_preliminary(true);
        lv.push_note(preliminary);
        lv.push_group(LvGroup::new(0, "01", "Heizflächen"));
        lv.push_group(LvGroup::new(1, "01.01", "Konvektoren"));
        let mut convector = Position::new(
            "01.01.0010",
            "Unterflurkonvektor",
            Decimal::ZERO,
            "St",
            None,
        );
        convector.set_status(EditStatus::Done);
        convector.set_long_text(RichText::new(vec![
            TextBlock::paragraph(vec![proava_domain::Run::plain("Mit Gebläse")]),
            TextBlock::Image {
                file: "konvektor.png".into(),
                width_mm: None,
            },
        ]));
        lv.push_position(convector);
        lv.push_position(Position::new(
            "01.01.0020",
            "Rost",
            Decimal::from(3),
            "m",
            Some(Decimal::from(20)),
        ));
        let mut project = Project::new(ProjectName::parse("Lieferung").unwrap(), "2026-11-17", lv);
        project.add_image("konvektor.png", vec![1, 2]);
        project.info_mut().label = "Neubau Bürohaus".into();
        project.info_mut().award_number = "V-17".into();
        GaebImport {
            file: PathBuf::from("/in/Hersteller Vortexte.d81"),
            read: LvImport {
                project,
                warnings,
                phase: Some(phase),
                format: "GAEB 90".into(),
                program: "Herstellerportal".into(),
                columns: Vec::new(),
                facts: Vec::new(),
                currency: None,
            },
        }
    }

    fn target() -> Project {
        let mut lv = Lv::new("Gebäudeautomation", OzMask::parse("11.PPPP").unwrap());
        lv.set_number("1");
        lv.push_group(LvGroup::new(0, "01", "Automation"));
        Project::new(ProjectName::parse("Schule").unwrap(), "2026-11-01", lv)
    }

    #[test]
    fn the_preview_counts_and_names_the_file() {
        let preview = ImportPreview::of(&read(81, vec!["GAEB 90, Zeile 4: unbekannt".into()]));
        assert_eq!(preview.kind, "D81 – Leistungsbeschreibung");
        assert_eq!(preview.titles, 2);
        assert_eq!(preview.positions, 2);
        assert_eq!(preview.preliminary, 1);
        assert_eq!(preview.notes, 0);
        assert_eq!(preview.images, 1);
        assert_eq!(preview.without_quantity, 1);
        assert_eq!(preview.without_price, 1);
        assert!(preview.is_description());
        assert!(!preview.is_bid());
        assert_eq!(preview.net_total.to_string(), "60,00 €");
        // One fact per line, counts only when there are some.
        let lines: Vec<String> = preview.entries().iter().map(|entry| entry.line()).collect();
        assert_eq!(
            lines,
            [
                "Datei: Hersteller Vortexte.d81",
                "Art: D81 – Leistungsbeschreibung",
                "Format: GAEB 90",
                "Erstellt mit: Herstellerportal",
                "LV: 1 – Vortexte Heizung",
                "Projektbezeichnung: Neubau Bürohaus",
                "OZ-Maske: 11.22.PPPP",
                "Titel: 2",
                "Positionen: 2",
                "Vorbemerkungen: 1",
                "Bilder: 1",
                "Summe netto: 60,00 €",
                "Positionen ohne Preis: 1",
                "Positionen ohne Menge: 1",
            ]
        );
        let entries = preview.entries();
        assert_eq!(entries[8].kind, crate::ValueKind::Number, "Positionen");
        assert_eq!(entries[11].kind, crate::ValueKind::Number, "Summe");
        let with_findings = preview.protocol();
        assert_eq!(with_findings.title, "Vorschau Hersteller Vortexte.d81");
        assert_eq!(
            with_findings.entry("Befunde beim Lesen").unwrap().details,
            ["GAEB 90, Zeile 4: unbekannt"]
        );
        assert!(ImportPreview::of(&read(84, Vec::new())).is_bid());
    }

    #[test]
    fn a_new_lv_keeps_order_numbers_status_open_and_reports_what_stayed_out() {
        let import = read(
            83,
            vec!["Kurztext fehlte, erste Langtextzeile übernommen: 01.01.0020".into()],
        );
        let mut project = target();
        let mut preview = ImportPreview::of(&import);
        assert_eq!(
            preview.suggested_number(&project),
            "2",
            "file number 1 is taken"
        );
        preview.lv_number = "Markthalle Musterstadt,".into();
        assert_eq!(
            preview.suggested_number(&project),
            "2",
            "a text is no LV number"
        );
        preview.lv_number = "0000".into();
        assert_eq!(preview.suggested_number(&project), "0000");
        let error = add_as_lv(&mut project, &import, "1", "Heizung", true).unwrap_err();
        assert!(
            error.contains("gibt es im Projekt schon") && error.contains("2"),
            "{error}"
        );
        assert_eq!(project.lv_count(), 1, "nothing changed on an error");
        assert!(add_as_lv(&mut project, &import, "2", "  ", true).is_err());

        let (index, protocol) = add_as_lv(&mut project, &import, "2", "Heizung", true).unwrap();
        assert_eq!(index, 1);
        let lv = project.lv_at(1).unwrap();
        assert_eq!((lv.number(), lv.title()), ("2", "Heizung"));
        assert_eq!(lv.oz_mask().pattern(), "11.22.PPPP");
        let first = lv.positions().next().unwrap();
        assert_eq!(first.ordinal(), "01.01.0010");
        assert_eq!(first.status(), EditStatus::Open);
        assert_eq!(project.images().len(), 1);
        let text = protocol.text();
        assert!(
            text.starts_with("Importprotokoll Hersteller Vortexte.d81\n"),
            "{text}"
        );
        for line in [
            "Übernommen als: neues LV 2 – Heizung\n",
            "Zielprojekt: Schule\n",
            "Ordnungszahlen: wie in der Datei\n",
            "Bearbeitungsstatus: alle Positionen „offen“\n",
            "Preise: 1 Einheitspreis als Planerpreise\n",
            "Bilder übernommen: 1\n",
            "Verworfen oder umgewandelt: 2\n",
        ] {
            assert!(text.contains(line), "{line} fehlt in {text}");
        }
        assert!(
            text.lines().all(|line| !line.contains(" · ")),
            "eine Angabe je Zeile: {text}"
        );
        assert_eq!(
            lv.positions().nth(1).unwrap().unit_price(),
            Some(Decimal::from(20))
        );

        let (index, protocol) =
            add_as_lv(&mut project, &import, "3", "Ohne Preise", false).unwrap();
        assert!(project
            .lv_at(index)
            .unwrap()
            .positions()
            .all(|position| position.unit_price().is_none()));
        assert!(protocol
            .text()
            .contains("Einheitspreise der Datei nicht übernommen (1 Position)"));
        assert!(text.contains("  – Kurztext fehlte"), "{text}");
        assert!(text.contains("Projektangaben der Datei nicht übernommen (Bezeichnung „Neubau Bürohaus“, Vergabenummer „V-17“)"), "{text}");
    }

    #[test]
    fn the_empty_lv_of_a_fresh_project_is_replaced() {
        let import = read(83, Vec::new());
        let mut fresh = Project::new(
            ProjectName::parse("Neu").unwrap(),
            "2026-11-17",
            Lv::new(
                "Leistungsverzeichnis",
                OzMask::parse("11.22.PPPPI").unwrap(),
            ),
        );
        fresh.lv_mut().set_number("1");
        fresh.info_mut().award_number = "eigene Nummer".into();
        let (index, protocol) = add_as_lv(&mut fresh, &import, "1", "Heizung", true).unwrap();
        assert_eq!((index, fresh.lv_count(), fresh.active_lv()), (0, 1, 0));
        assert_eq!(fresh.info().label, "Neubau Bürohaus", "leeres Feld gefüllt");
        assert_eq!(
            fresh.info().award_number,
            "eigene Nummer",
            "gefülltes Feld bleibt"
        );
        let filled = protocol.entry("Projektangaben in leere Felder").unwrap();
        assert_eq!(
            (filled.value.as_str(), filled.details.as_slice()),
            ("1", &["Bezeichnung".to_string()][..])
        );
        assert!(
            !protocol.text().contains("nicht übernommen"),
            "{}",
            protocol.text()
        );
        assert_eq!(fresh.lv().title(), "Heizung");
        assert_eq!(fresh.lv().position_count(), 2);
        assert_eq!(
            protocol.entry("Ersetzt").unwrap().value,
            "das leere LV 1 – Leistungsverzeichnis des Projekts"
        );
        assert_eq!(
            protocol.entry("Verworfen oder umgewandelt").unwrap().value,
            "nichts"
        );
        // With content it stays.
        let (index, _) = add_as_lv(&mut fresh, &import, "2", "Zweites", true).unwrap();
        assert_eq!((index, fresh.lv_count()), (1, 2));
        let transfer = protocol_of_transfer(&import);
        assert!(
            transfer.text().contains("Übernahme-Fenster"),
            "{}",
            transfer.text()
        );
        assert!(
            !transfer.text().contains("alle Positionen"),
            "{}",
            transfer.text()
        );
    }

    fn protocol_of_transfer(import: &GaebImport) -> ImportProtocol {
        protocol(import, &ImportTarget::Transfer, false)
    }

    #[test]
    fn a_new_project_gets_the_free_name_and_keeps_the_master_data() {
        let import = read(81, Vec::new());
        let project = as_project(&import, "Vortexte Kopie", true).unwrap();
        assert_eq!(project.name().as_str(), "Vortexte Kopie");
        assert_eq!(project.lv_count(), 1);
        assert_eq!(project.lv().title(), "Vortexte Heizung");
        assert_eq!(project.info().award_number, "V-17");
        assert_eq!(project.images().len(), 1);
        assert!(project
            .lv()
            .positions()
            .all(|position| position.status() == EditStatus::Open));
        assert!(as_project(&import, "", true).is_err());
        let without = as_project(&import, "Ohne", false).unwrap();
        assert_eq!(without.lv().net_total(), Money::ZERO);
        let protocol = protocol(
            &import,
            &ImportTarget::Project {
                file: PathBuf::from("/p/Vortexte Kopie/Vortexte Kopie.proava"),
            },
            true,
        );
        assert!(
            protocol
                .lines()
                .iter()
                .any(|line| line == "Verworfen oder umgewandelt: nichts"),
            "{protocol:?}"
        );
        assert_eq!(
            protocol.entry("Übernommen als").unwrap().value,
            "neues Projekt"
        );
        let place = protocol.entry("Speicherort").unwrap();
        assert_eq!(
            (place.value.as_str(), place.kind),
            ("/p/Vortexte Kopie", crate::ValueKind::Path)
        );
        assert_eq!(
            protocol.entry("Projektdatei").unwrap().value,
            "Vortexte Kopie.proava"
        );
    }

    #[test]
    fn everything_goes_under_a_library_title() {
        let import = read(81, Vec::new());
        let mut library = Project::new(
            ProjectName::parse("Bibliothek").unwrap(),
            "2026-11-17",
            Lv::new(
                "Positionsbibliothek",
                OzMask::parse("11.222.33.44.PPPP").unwrap(),
            ),
        );
        library
            .lv_mut()
            .push_group(LvGroup::new(0, "", "Herstellertexte"));
        library.lv_mut().renumber();
        let rows = into_library(&mut library, &import, 0, true, "17.11.2026").unwrap();
        assert_eq!(rows, 5, "Vorbemerkung, 2 Titel, 2 Positionen");
        assert_eq!(library.lv().position_count(), 2);
        assert_eq!(library.images().len(), 1);
        assert_eq!(all_rows(&import).len(), 5);
    }

    #[test]
    fn long_labels_become_one_short_line() {
        assert_eq!(
            short("  Angebotshinweis:\n\nProdukt:  Konvektor "),
            "Angebotshinweis: Produkt: Konvektor"
        );
        let long = "Wort ".repeat(30);
        let cut = short(&long);
        assert!(cut.ends_with(" …") && cut.chars().count() <= 80, "{cut}");
    }

    #[test]
    fn the_next_free_number_skips_used_ones() {
        let mut project = target();
        assert_eq!(next_free_number(&project), "2");
        let mut lv = Lv::new("B", OzMask::parse("11.PPPP").unwrap());
        lv.set_number("2");
        project.add_lv(lv);
        assert_eq!(next_free_number(&project), "3");
        assert_eq!(phase_label(86), "Auftragserteilung");
    }

    /// A CSV table as the reader hands it over: two positions, one with a GP
    /// that did not fit (marked), currency from the dialog.
    fn csv_read() -> GaebImport {
        let mut lv = Lv::new("Zulieferung", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Titel 01"));
        let mut controller = Position::new(
            "01.0010",
            "Regler",
            Decimal::from(2),
            "St",
            Some(Decimal::from(10)),
        );
        controller.set_price_check(Some(proava_domain::PriceCheck {
            stated: Money::from_cents(2400),
            computed: Money::from_cents(2000),
        }));
        lv.push_position(controller);
        lv.push_position(Position::new(
            "01.0020",
            "Fühler",
            Decimal::from(3),
            "St",
            Some(Decimal::from(5)),
        ));
        GaebImport {
            file: PathBuf::from("/in/Zulieferung.csv"),
            read: LvImport {
                project: Project::new(ProjectName::parse("Zulieferung").unwrap(), "2026-11-18", lv),
                warnings: vec![
                    "1 Titel aus den OZ angelegt (01) – Bezeichnung bitte ergänzen.".into(),
                ],
                phase: None,
                format: "CSV · Trenner Semikolon · UTF-8".into(),
                program: String::new(),
                facts: vec![
                    ("Trenner".into(), "Semikolon".into()),
                    ("Zeichensatz".into(), "UTF-8".into()),
                ],
                columns: ["OZ", "Positionstext", "Einheit", "Menge", "EP", "GP"]
                    .map(String::from)
                    .to_vec(),
                currency: None,
            },
        }
    }

    #[test]
    fn a_csv_preview_names_columns_currency_and_the_gp_check() {
        let mut import = csv_read();
        assert!(import.is_csv());
        import.set_currency("chf");
        let preview = ImportPreview::of(&import);
        assert_eq!(preview.kind, "CSV-Tabelle");
        assert_eq!(preview.currency, "CHF");
        assert_eq!(preview.currency_label(), "Schweizer Franken (CHF)");
        assert_eq!(
            (preview.price_checks, preview.price_difference),
            (1, Money::from_cents(400))
        );
        let checks = preview.price_check_entries();
        assert_eq!(
            checks.iter().map(|entry| entry.line()).collect::<Vec<_>>(),
            ["GP-Abweichungen: 1", "Summe der Abweichungen: 4,00 CHF"]
        );
        assert!(checks
            .iter()
            .all(|entry| entry.tone == crate::Tone::Warning));
        let lines: Vec<String> = preview.entries().iter().map(|entry| entry.line()).collect();
        assert_eq!(
            lines[..6],
            [
                "Datei: Zulieferung.csv",
                "Art: CSV-Tabelle",
                "Trenner: Semikolon",
                "Zeichensatz: UTF-8",
                "Spalten: OZ, Positionstext, Einheit, Menge, EP, GP",
                "LV: Zulieferung",
            ]
        );
        assert!(
            lines.contains(&"Währung: Schweizer Franken (CHF)".to_string()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"Summe netto: 35,00 CHF".to_string()),
            "{lines:?}"
        );
    }

    #[test]
    fn a_csv_import_keeps_the_marks_and_lists_them_in_the_protocol() {
        let mut import = csv_read();
        import.set_currency("USD");
        let mut project = target();
        let (index, protocol) = add_as_lv(&mut project, &import, "2", "Zulieferung", true).unwrap();
        let lv = project.lv_at(index).unwrap();
        assert_eq!(lv.currency(), "USD");
        assert_eq!(
            lv.positions()
                .filter(|position| position.price_check().is_some())
                .count(),
            1
        );
        let text = protocol.text();
        for line in [
            "GP-Abweichungen: 1\n  – 01.0010: GP laut Datei 24,00 USD, errechnet 20,00 USD – Abweichung 4,00 USD\n",
            "Summe der Abweichungen: 4,00 USD\n",
            "GP-Prüfung: errechneter GP (Menge × EP) gilt",
            "Verworfen oder umgewandelt: 1\n",
        ] {
            assert!(text.contains(line), "{line} fehlt in {text}");
        }

        // A fresh project: the count below „Verworfen oder umgewandelt“ stays right.
        let mut fresh = Project::new(
            ProjectName::parse("Neu").unwrap(),
            "2026-11-18",
            Lv::new("", OzMask::parse("11.PPPP").unwrap()),
        );
        let (_, protocol) = add_as_lv(&mut fresh, &import, "1", "Zulieferung", true).unwrap();
        assert_eq!(
            protocol.entry("Verworfen oder umgewandelt").unwrap().value,
            "1"
        );

        // Without prices nothing is marked.
        let project = as_project(&import, "Ohne Preise", false).unwrap();
        assert!(project
            .lv()
            .positions()
            .all(|position| position.price_check().is_none()));
        let protocol = protocol_of_project(&import);
        assert!(
            protocol.contains("GP-Prüfung: ohne Preise übernommen, nichts markiert"),
            "{protocol}"
        );
    }

    fn protocol_of_project(import: &GaebImport) -> String {
        protocol(
            import,
            &ImportTarget::Project {
                file: PathBuf::from("/neu/a.proava"),
            },
            false,
        )
        .text()
    }
}
