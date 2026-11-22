//! Checks of the LV before a GAEB export (and on request).

use std::collections::HashMap;

use proava_application::{ExchangeFiles, GaebExport, GaebRuleSeverity};
use proava_domain::{normalize_ordinal, Lv, LvRow, PositionKind, Project, RowId};

/// Kurztext and unit as GAEB carries them (shared with the MCP server).
pub use proava_domain::input_limits::{SHORT_TEXT_LIMIT, UNIT_LIMIT};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "Fehler",
            Severity::Warning => "Warnung",
            Severity::Hint => "Hinweis",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub severity: Severity,
    pub text: String,
    pub row: Option<RowId>,
}

/// Rule findings that repeat a check of the LV above: left out when that
/// check already names the same row with the same severity.
const COVERED_RULES: [&str; 5] = [
    "R3.duplicate",
    "R5.without_base",
    "R5.without_alternative",
    "R9.qty",
    "R9.unit",
];

/// [`check`] plus the GAEB-Fachregeln R1–R13 of the file the export writes
/// (without `export` those of the Angebotsaufforderung X83). Writes the LV
/// in memory – for the dialogs, not for every frame.
pub fn check_with_rules(
    project: &Project,
    export: Option<GaebExport>,
    exchange: &dyn ExchangeFiles,
) -> Vec<Finding> {
    let lv = project.lv();
    let mut findings = check(project, export);
    let own: Vec<(Severity, Option<RowId>)> = findings
        .iter()
        .map(|finding| (finding.severity, finding.row))
        .collect();
    // A Kosten-LV goes to no bidder: the GAEB rules of the Angebotsaufforderung
    // apply only to an export it is really written for.
    if export.is_none() && !lv.purpose().is_tender() {
        return findings;
    }
    let kind = export.unwrap_or(GaebExport::RequestForQuotation);
    for rule in exchange.check_gaeb_export(project, kind) {
        let severity = match rule.severity {
            GaebRuleSeverity::Error => Severity::Error,
            GaebRuleSeverity::Warning => Severity::Warning,
            GaebRuleSeverity::Hint => Severity::Hint,
        };
        let row = rule.ordinal.as_deref().and_then(|oz| row_of(lv, oz));
        if COVERED_RULES.contains(&rule.id.as_str()) && own.contains(&(severity, row)) {
            continue;
        }
        let phase = if export.is_none() { " (X83)" } else { "" };
        findings.push(Finding {
            severity,
            text: format!(
                "Fachregel {}{phase}: {} [{}]",
                rule.rule(),
                rule.text,
                rule.reference
            ),
            row,
        });
    }
    // Pictures as GAEB carries them: PDF clips as PNG, very large ones.
    if export.is_some() {
        findings.extend(picture_findings(&exchange.gaeb_pictures(project)));
    }
    findings.sort_by_key(|finding| finding.severity);
    findings
}

/// The findings of one LV of „Alle LVs prüfen“.
#[derive(Debug, Clone, PartialEq)]
pub struct LvFindings {
    /// Index of the LV in the project.
    pub index: usize,
    /// „LV 2 – Lüftung“
    pub label: String,
    pub findings: Vec<Finding>,
}

/// The LVs „Alle LVs prüfen“ checks: every tender LV; the Kostenermittlungen
/// only when the active LV is one (they go to no bidder).
pub fn lvs_to_check(project: &Project) -> Vec<usize> {
    let mut indices = project.tender_lv_indices();
    if !project.lv_purpose(project.active_lv()).is_tender() {
        indices.extend(project.cost_lv_indices());
        indices.sort_unstable();
    }
    indices
}

/// [`check_with_rules`] for every LV of [`lvs_to_check`], in their order.
pub fn check_all(project: &Project, exchange: &dyn ExchangeFiles) -> Vec<LvFindings> {
    let mut copy = project.clone();
    let mut groups = Vec::new();
    for index in lvs_to_check(project) {
        if copy.set_active_lv(index) {
            groups.push(LvFindings {
                index,
                label: crate::lv_properties::lv_label_with_stage(copy.lv()),
                findings: check_with_rules(&copy, None, exchange),
            });
        }
    }
    groups
}

/// Size of the PNG of every PDF clip and a warning for very large pictures.
pub fn picture_findings(pictures: &[proava_application::GaebPictureInfo]) -> Vec<Finding> {
    let large = proava_application::pdf_clip::LARGE_GAEB_PICTURE;
    let mut findings = Vec::new();
    for picture in pictures {
        if let Some(error) = &picture.error {
            findings.push(Finding {
                severity: Severity::Warning,
                text: format!(
                    "PDF-Ausschnitt „{}“ lässt sich nicht als Bild für GAEB rechnen: {error}",
                    picture.file
                ),
                row: None,
            });
            continue;
        }
        if let Some((width, height)) = picture.clip_pixels {
            findings.push(Finding {
                severity: Severity::Hint,
                text: format!(
                    "PDF-Ausschnitt „{}“ geht als PNG mit {width} × {height} Pixeln (300 dpi, {}) in die GAEB-Datei.",
                    picture.file,
                    proava_domain::size_text(picture.bytes)
                ),
                row: None,
            });
        }
        if picture.bytes > large {
            findings.push(Finding {
                severity: Severity::Warning,
                text: format!(
                    "Bild „{}“ ist mit {} sehr groß für GAEB – im Langtext schmaler einstellen oder „Bilder mitgeben“ ausschalten.",
                    picture.file,
                    proava_domain::size_text(picture.bytes)
                ),
                row: None,
            });
        }
    }
    findings
}

/// All findings of the LV itself, errors first. `export` adds the checks of
/// that GAEB phase.
pub fn check(project: &Project, export: Option<GaebExport>) -> Vec<Finding> {
    let lv = project.lv();
    let mut findings = Vec::new();
    let mut push = |severity, text: String, row: Option<RowId>| {
        findings.push(Finding {
            severity,
            text,
            row,
        });
    };

    // Projektangaben the input does not accept, taken over from a GAEB file.
    for problem in project.info().problems_as_delivered() {
        push(Severity::Hint, problem, None);
    }

    // Kosten-LV: every position needs a Kostengruppe of the level the Stufe
    // asks for (DIN 276, 4.3.3/4.3.4).
    for (row, text) in proava_domain::cost_lv_findings(lv) {
        push(Severity::Warning, text, Some(row));
    }

    // Ausführungsbeschreibungen: GAEB PerfNo 1–9999, each number once (Befund 7).
    for (row, text) in lv.performance_number_problems() {
        push(Severity::Error, text, Some(row));
    }

    for ordinal in lv.duplicate_ordinals() {
        let row = lv
            .rows()
            .iter()
            .find(|row| normalize_ordinal(row.ordinal()) == normalize_ordinal(&ordinal))
            .map(LvRow::id);
        push(
            Severity::Error,
            format!("OZ {ordinal} kommt mehrfach vor."),
            row,
        );
    }

    for (level, label) in lv.level_labels().iter().enumerate() {
        let length = label.chars().count();
        if length > proava_domain::mask_spec::MAX_LABEL_LENGTH {
            push(
                Severity::Warning,
                format!(
                    "Stufenbezeichnung {} „{label}“ hat {length} Zeichen – GAEB überträgt höchstens {} (Gliederung und OZ-Maske).",
                    level + 1,
                    proava_domain::mask_spec::MAX_LABEL_LENGTH
                ),
                None,
            );
        }
    }

    let mut seen_group = false;
    let mut spacers = 0;
    let mut provisional_or_alternative = 0;
    let mut images = 0;
    let mut priced = 0;
    // Textergänzungen: (vom Bieter, vom Auftraggeber, Zeilen mit welchen).
    let mut fields = (0usize, 0usize, 0usize);
    let mut bases: HashMap<u32, RowId> = HashMap::new();
    let mut alternatives: HashMap<u32, RowId> = HashMap::new();

    for (index, row) in lv.rows().iter().enumerate() {
        match row {
            LvRow::Group(group) => {
                seen_group = true;
                let range = lv.block_range(index);
                if !lv.rows()[range].iter().any(LvRow::is_position) {
                    push(
                        Severity::Warning,
                        format!("Titel {} enthält keine Positionen.", group.ordinal()),
                        Some(row.id()),
                    );
                }
                if group.short_text().trim().is_empty() {
                    push(
                        Severity::Warning,
                        format!("Titel {} ohne Text.", group.ordinal()),
                        Some(row.id()),
                    );
                }
            }
            LvRow::Note(note) => {
                if note.text().has_images() {
                    images += note.text().images().count();
                }
                count_fields(&mut fields, note.complements());
                for text in field_problems(note.complements()) {
                    push(
                        Severity::Warning,
                        format!("Hinweis {}: {text}", place_of(lv, index)),
                        Some(row.id()),
                    );
                }
                // Like a position without quantity: an empty hint is mostly
                // meant as an empty line – the Abstandshalter does that.
                if note.is_blank() {
                    push(
                        Severity::Warning,
                        format!(
                            "Hinweis ohne Text {} – für eine Leerzeile in der Bearbeitung den Abstandshalter verwenden.",
                            place_of(lv, index)
                        ),
                        Some(row.id()),
                    );
                }
            }
            LvRow::Spacer(_) => spacers += 1,
            LvRow::Performance(performance) => {
                images += performance.long_text().images().count();
                count_fields(&mut fields, performance.complements());
                for text in field_problems(performance.complements()) {
                    push(
                        Severity::Warning,
                        format!("{}: {text}", performance.mark()),
                        Some(row.id()),
                    );
                }
            }
            LvRow::Position(position) => {
                let oz = position.ordinal();
                let id = Some(row.id());
                if !seen_group && lv.level_count() > 0 {
                    push(
                        Severity::Error,
                        format!("Position {oz} steht vor dem ersten Titel."),
                        id,
                    );
                }
                if position.is_stlb_placeholder() {
                    push(
                        Severity::Error,
                        format!("Position {oz}: STLB-Bau-Platzhalter ist noch offen."),
                        id,
                    );
                }
                // STLB-Bau for public clients: unchanged text, TA filled, numbers.
                for text in proava_domain::stlb::stlb_problems(position) {
                    push(Severity::Warning, format!("Position {oz}: {text}"), id);
                }
                count_fields(&mut fields, position.complements());
                for text in field_problems(position.complements()) {
                    push(Severity::Warning, format!("Position {oz}: {text}"), id);
                }
                // Manufacturer text (AUSSCHREIBEN.DE): the text stays as
                // delivered, the planner checks product neutrality.
                if let Some(text) = manufacturer_text_hint(position) {
                    push(Severity::Hint, format!("Position {oz}: {text}"), id);
                }
                let length = position.short_text().chars().count();
                if position.short_text().trim().is_empty() {
                    push(
                        Severity::Warning,
                        format!("Position {oz} ohne Kurztext."),
                        id,
                    );
                } else if length > SHORT_TEXT_LIMIT {
                    push(
                        Severity::Warning,
                        format!("Position {oz}: Kurztext hat {length} Zeichen (mehr als {SHORT_TEXT_LIMIT})."),
                        id,
                    );
                }
                if position.unit().trim().is_empty()
                    && !position.is_stlb_placeholder()
                    && !position.is_surcharge()
                {
                    push(Severity::Error, format!("Position {oz} ohne Einheit."), id);
                } else if position.unit().trim().chars().count() > UNIT_LIMIT {
                    push(
                        Severity::Warning,
                        format!(
                            "Position {oz}: Einheit „{}“ hat mehr als {UNIT_LIMIT} Zeichen – GAEB kürzt sie.",
                            position.unit().trim()
                        ),
                        id,
                    );
                }
                if position.quantity_missing() {
                    push(Severity::Error, format!("Position {oz} ohne Menge."), id);
                }
                if position.surcharge().is_some_and(|surcharge| {
                    surcharge.kind == proava_domain::SurchargeKind::Listed
                        && surcharge.references.is_empty()
                }) {
                    push(
                        Severity::Error,
                        format!(
                            "Zuschlagsposition {oz} „auf genannte Positionen“ ohne Bezug – Positionen im Fenster Eigenschaften wählen."
                        ),
                        id,
                    );
                }
                if position.unit_price().is_some() {
                    priced += 1;
                } else if export == Some(GaebExport::PricedLv) {
                    push(
                        Severity::Warning,
                        format!("Position {oz} ohne Einheitspreis."),
                        id,
                    );
                }
                images += position.long_text().images().count();
                match position.kind() {
                    PositionKind::Provisional { .. } => provisional_or_alternative += 1,
                    PositionKind::Alternative { group, .. } => {
                        provisional_or_alternative += 1;
                        alternatives.entry(group).or_insert(row.id());
                    }
                    PositionKind::Base { group } => {
                        bases.entry(group).or_insert(row.id());
                    }
                    _ => {}
                }
            }
        }
    }

    for (group, row) in &bases {
        if !alternatives.contains_key(group) {
            push(
                Severity::Error,
                format!("Grundposition der Gruppe {group} ohne Wahlposition."),
                Some(*row),
            );
        }
    }
    for (group, row) in &alternatives {
        if !bases.contains_key(group) {
            push(
                Severity::Error,
                format!("Wahlposition der Gruppe {group} ohne Grundposition."),
                Some(*row),
            );
        }
    }
    if provisional_or_alternative > 0 {
        push(
            Severity::Hint,
            format!("{provisional_or_alternative} Bedarfs- oder Wahlpositionen (VOB/A § 7: nur wenn nötig)."),
            None,
        );
    }
    if export.is_some() && images > 0 {
        let (_, bytes) = project.lv_images();
        let clips = project
            .lv_image_uses()
            .iter()
            .filter(|(asset, _)| asset.is_some_and(|asset| asset.is_pdf_clip()))
            .count();
        let clip_note = match clips {
            0 => String::new(),
            1 => " (ohne den PDF-Ausschnitt, der als PNG mitgeht)".to_string(),
            count => format!(" (ohne die {count} PDF-Ausschnitte, die als PNG mitgehen)"),
        };
        push(
            Severity::Hint,
            format!(
                "{} im Langtext, zusammen {}{clip_note} – gehen mit „Bilder mitgeben“ in die GAEB-Datei (nur GAEB DA XML, nicht GAEB 90).",
                crate::format::count(images, "Bild", "Bilder"),
                proava_domain::size_text(bytes)
            ),
            None,
        );
    }
    if export.is_some() && spacers > 0 {
        push(Severity::Hint, spacer_hint(spacers), None);
    }
    // LV prüfen: how many entries the bidders have to make (information).
    if export.is_none() && fields.0 + fields.1 > 0 {
        push(Severity::Hint, fields_hint(fields), None);
    }
    if export == Some(GaebExport::RequestForQuotation) && priced > 0 {
        push(
            Severity::Hint,
            format!("{priced} Einheitspreise werden in der X83 nicht übertragen."),
            None,
        );
    }
    let status = lv.status_counts();
    if status.any_marked() && status.done < status.total() {
        push(
            Severity::Hint,
            format!(
                "{} von {} Positionen als fertig markiert ({} offen, {} in Bearbeitung).",
                status.done,
                status.total(),
                status.open,
                status.in_progress
            ),
            None,
        );
    }
    if lv.position_count() == 0 {
        push(
            Severity::Error,
            "Das LV enthält keine Positionen.".to_string(),
            None,
        );
    }

    // Anwender 18.11.: Menge nach dem Angebotseingang geändert → Massenmehrung
    // (Nachtrag), keine andere Wertung.
    let changed = crate::open_points::quantity_changes(project);
    for position in lv
        .positions()
        .filter(|position| changed.contains_key(&position.id()))
    {
        let change = changed[&position.id()];
        push(
            Severity::Hint,
            format!(
                "{} nach Angebotseingang: OZ {} – {}; gewertet mit der Angebotsmenge, die Differenz gehört in einen Nachtrag.",
                change.label(),
                position.ordinal(),
                change.hint(position.unit()).trim_start_matches("Achtung ")
            ),
            Some(position.id()),
        );
    }
    // Anwender 18.11.: nach dem Versand geändert → neu versenden.
    if export.is_none() && lv.changed_since_sent() {
        push(Severity::Hint, sent_change_note(project), None);
    }

    findings.sort_by_key(|finding| finding.severity);
    findings
}

fn count_fields(fields: &mut (usize, usize, usize), complements: &[proava_domain::TextComplement]) {
    let (bidder, owner) = proava_domain::complement_text::count_kinds(complements);
    fields.0 += bidder;
    fields.1 += owner;
    fields.2 += usize::from(bidder + owner > 0);
}

/// Bieterangaben without Vortext: the bidder does not know what to fill in.
fn field_problems(complements: &[proava_domain::TextComplement]) -> Vec<String> {
    complements
        .iter()
        .filter(|complement| {
            complement.kind == proava_domain::ComplementKind::Bidder
                && complement.caption.trim().is_empty()
                && complement.tail.trim().is_empty()
        })
        .map(|complement| {
            format!(
                "Bieterangabe {} ohne Vortext – der Bieter weiß nicht, was er eintragen soll.",
                complement.marker.trim()
            )
        })
        .collect()
}

/// „5 Bieterangaben (vom Bieter einzutragen) und 2 Angaben des Auftraggebers in 4 Zeilen.“
fn fields_hint((bidder, owner, rows): (usize, usize, usize)) -> String {
    let mut parts = Vec::new();
    if bidder > 0 {
        parts.push(format!(
            "{} (vom Bieter einzutragen)",
            crate::format::count(bidder, "Bieterangabe", "Bieterangaben")
        ));
    }
    if owner > 0 {
        parts.push(crate::format::count(
            owner,
            "Angabe des Auftraggebers",
            "Angaben des Auftraggebers",
        ));
    }
    format!(
        "{} in {}.",
        parts.join(" und "),
        crate::format::count(rows, "Zeile", "Zeilen")
    )
}

/// „Seit dem Versand am 18.11.2026 an 3 Bieter geändert – das LV muss neu
/// versendet werden.“ (persistent hint, LV prüfen).
pub fn sent_change_note(project: &Project) -> String {
    format!(
        "Seit dem Versand{} geändert – das LV muss neu versendet werden (Änderungen: LV › Mit anderem Stand vergleichen …).",
        sent_when(project)
    )
}

/// „ am 18.11.2026 an 3 Bieter“ – what is known about the sending.
pub fn sent_when(project: &Project) -> String {
    let lv = project.lv();
    let date = lv
        .request_exported()
        .map(crate::format::iso_to_german_date)
        .or_else(|| {
            project
                .invitees()
                .iter()
                .map(|invitee| invitee.requested.trim())
                .find(|date| !date.is_empty())
                .map(str::to_string)
        });
    let bidders = project
        .invitees()
        .iter()
        .filter(|invitee| !invitee.requested.trim().is_empty())
        .count();
    let mut text = String::new();
    if let Some(date) = date {
        text.push_str(&format!(" am {date}"));
    }
    if bidders > 0 {
        text.push_str(&format!(
            " an {}",
            crate::format::count(bidders, "Bieter", "Bieter")
        ));
    }
    text
}

/// „nach 01.02.0010“ (the last row with an OZ above), „am Anfang“ without one.
fn place_of(lv: &Lv, index: usize) -> String {
    lv.rows()[..index.min(lv.rows().len())]
        .iter()
        .rev()
        .find(|row| row.has_ordinal() && !row.ordinal().trim().is_empty())
        .map(|row| format!("nach {}", row.ordinal()))
        .unwrap_or_else(|| "am Anfang".to_string())
}

const SPACER_HINT_END: &str = "nicht übertragen (GAEB kennt keine Leerzeile).";

/// Export information: Abstandshalter stay at home.
fn spacer_hint(count: usize) -> String {
    let verb = if count == 1 { "wird" } else { "werden" };
    format!(
        "{} {verb} {SPACER_HINT_END}",
        crate::format::count(count, "Abstandshalter", "Abstandshalter")
    )
}

/// Whether the export should stop for the check: the information about
/// Abstandshalter alone does not (it would come with every export).
pub fn worth_a_stop(findings: &[Finding]) -> bool {
    findings.iter().any(|finding| {
        !(finding.severity == Severity::Hint && finding.text.ends_with(SPACER_HINT_END))
    })
}

/// Row of a title or position by its OZ as the GAEB file writes it
/// (numbers may be padded differently).
fn row_of(lv: &Lv, ordinal: &str) -> Option<RowId> {
    let key = |ordinal: &str| -> Vec<String> {
        normalize_ordinal(ordinal)
            .split('.')
            .map(|piece| {
                let stripped = piece.trim_start_matches('0');
                if stripped.is_empty() {
                    "0".to_string()
                } else {
                    stripped.to_uppercase()
                }
            })
            .collect()
    };
    let wanted = key(ordinal);
    lv.rows()
        .iter()
        .find(|row| {
            matches!(row, LvRow::Position(_) | LvRow::Group(_))
                && !row.ordinal().is_empty()
                && key(row.ordinal()) == wanted
        })
        .map(LvRow::id)
}

pub fn count(findings: &[Finding], severity: Severity) -> usize {
    findings
        .iter()
        .filter(|finding| finding.severity == severity)
        .count()
}

#[cfg(test)]
mod tests {

    /// PDF-Ausschnitte in GAEB: Pixel und Größe als Hinweis, sehr große Bilder als Warnung.
    #[test]
    fn pdf_clip_pictures_in_the_export_check() {
        use proava_application::GaebPictureInfo;
        let pictures = vec![
            GaebPictureInfo {
                file: "topologie_S1.pdf".into(),
                bytes: 400_000,
                clip_pixels: Some((1772, 1000)),
                error: None,
            },
            GaebPictureInfo {
                file: "a0_S1.pdf".into(),
                bytes: 6 * 1024 * 1024,
                clip_pixels: Some((2126, 3000)),
                error: None,
            },
            GaebPictureInfo {
                file: "foto.jpg".into(),
                bytes: 20_000,
                clip_pixels: None,
                error: None,
            },
            GaebPictureInfo {
                file: "kaputt_S1.pdf".into(),
                bytes: 0,
                clip_pixels: None,
                error: Some("Die PDF ist verschlüsselt".into()),
            },
        ];
        let findings = super::picture_findings(&pictures);
        let texts: Vec<(super::Severity, &str)> = findings
            .iter()
            .map(|f| (f.severity, f.text.as_str()))
            .collect();
        assert!(
            texts.iter().any(|(s, t)| *s == super::Severity::Hint
                && t.contains(
                    "„topologie_S1.pdf“ geht als PNG mit 1772 × 1000 Pixeln (300 dpi, 391 KB)"
                )),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|(s, t)| *s == super::Severity::Warning
                && t.contains("„a0_S1.pdf“ ist mit 6,0 MB sehr groß")),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|(s, t)| *s == super::Severity::Warning
                && t.contains("„kaputt_S1.pdf“ lässt sich nicht")),
            "{texts:?}"
        );
        assert!(
            !texts.iter().any(|(_, t)| t.contains("foto.jpg")),
            "kleines Foto: nichts zu melden"
        );
    }
    use proava_application::GaebExport;
    use proava_domain::{
        Decimal, Lv, LvGroup, OzMask, Position, PositionKind, Project, ProjectName,
    };

    use proava_exchange::GaebPdfExchange;

    use super::{check, check_with_rules, count, Severity};

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
        lv.push_position(Position::new("", "", Decimal::ZERO, "", None));
        let mut base = Position::new("", "Grund", Decimal::ONE, "St", None);
        base.set_kind(PositionKind::Base { group: 1 });
        lv.push_position(base);
        lv.push_position(Position::stlb("offen"));
        lv.push_position(Position::new("", "Kabel", Decimal::TEN, "Meter", None));
        lv.renumber();
        Project::new(ProjectName::parse("P").unwrap(), "2026-11-17", lv)
    }

    /// Projektangaben, die die Eingabe nicht annimmt (Ausführungsende vor dem
    /// Beginn), kommen nur aus einer Datei: Hinweis „wie aus der Datei
    /// übernommen“. Zuschlagspositionen haben keine Menge und Einheit.
    #[test]
    fn delivered_dates_and_zuschlaege_in_the_check() {
        let mut project = project();
        project.info_mut().execution_start = "18.12.2020".into();
        project.info_mut().execution_end = "31.01.2020".into();
        let mut zuschlag = Position::new("", "Zuschlag", Decimal::ZERO, "", Some(Decimal::TEN));
        zuschlag.set_surcharge(Some(proava_domain::Surcharge::default()));
        project.lv_mut().push_position(zuschlag);
        project.lv_mut().renumber();
        let findings = check(&project, None);
        assert!(
            findings.iter().any(|finding| finding.severity == Severity::Hint
                && finding.text.contains("liegt vor dem Ausführungsbeginn (18.12.2020) – wie aus der Datei übernommen")),
            "{findings:?}"
        );
        // „auf genannte Positionen“ without any: an error (GAEB needs one)
        let mut listed = project.clone();
        let index = listed.lv().rows().len() - 1;
        if let Some(position) = listed
            .lv_mut()
            .row_mut(index)
            .and_then(|row| row.as_position_mut())
        {
            position.set_surcharge(Some(proava_domain::Surcharge {
                split: false,
                kind: proava_domain::SurchargeKind::Listed,
                references: Vec::new(),
            }));
        }
        assert!(check(&listed, None)
            .iter()
            .any(|finding| finding.severity == Severity::Error
                && finding.text.contains("ohne Bezug")));
        let zuschlag = project.lv().rows().last().unwrap().ordinal().to_string();
        assert!(
            !findings
                .iter()
                .any(|finding| finding.text.contains(&zuschlag)),
            "Zuschlag ohne Menge/Einheit ist richtig: {findings:?}"
        );
    }

    /// Anwender 18.11.: Massenmehrung nach Angebotseingang und „seit dem Versand
    /// geändert“ stehen als Hinweis in „LV prüfen“.
    #[test]
    fn quantity_after_bids_and_change_after_sending_are_hints() {
        let mut project = project();
        project
            .lv_mut()
            .mark_sent("2026-11-18T09:00:00".into(), None);
        let station = project.lv().positions().next().unwrap().id();
        let mut bid = proava_domain::Bid::new(proava_domain::Address::named("A"), "a.x84");
        bid.set_unit_price(station, Some(Decimal::TEN));
        project.add_bid(bid);
        let hints = |project: &Project| -> Vec<String> {
            check(project, None)
                .into_iter()
                .filter(|finding| finding.severity == Severity::Hint)
                .map(|finding| finding.text)
                .collect()
        };
        assert!(!hints(&project)
            .iter()
            .any(|text| text.contains("Angebotseingang") || text.contains("Versand")));
        let index = project.lv().index_of(station).unwrap();
        project
            .lv_mut()
            .row_mut(index)
            .unwrap()
            .as_position_mut()
            .unwrap()
            .set_quantity(Decimal::from(3));
        let texts = hints(&project);
        let ordinal = project.lv().rows()[index].ordinal().to_string();
        assert!(
            texts.contains(&format!(
                "Massenmehrung nach Angebotseingang: OZ {ordinal} – Massenmehrung: LV-Menge 3 statt 1 St (+2); \
                 gewertet mit der Angebotsmenge, die Differenz gehört in einen Nachtrag."
            )),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|text| text.starts_with(
                "Seit dem Versand am 18.11.2026 geändert – das LV muss neu versendet werden"
            )),
            "{texts:?}"
        );
    }

    #[test]
    fn long_level_labels_are_reported() {
        let mut project = project();
        project
            .lv_mut()
            .set_level_labels(vec!["Titel".into(), "y".repeat(45)]);
        let findings = check(&project, None);
        assert!(findings
            .iter()
            .any(|finding| finding.text.contains("Stufenbezeichnung 2")
                && finding.text.contains("45 Zeichen")));
    }

    #[test]
    fn findings_name_the_problems() {
        let project = project();
        let findings = check(&project, Some(GaebExport::PricedLv));
        let texts: Vec<&str> = findings
            .iter()
            .map(|finding| finding.text.as_str())
            .collect();
        assert!(texts.iter().any(|text| text.contains("ohne Kurztext")));
        assert!(texts.iter().any(|text| text.contains("ohne Menge")));
        assert!(texts.iter().any(|text| text.contains("ohne Einheit")));
        assert!(texts.iter().any(|text| text.contains("STLB")));
        assert!(texts.iter().any(|text| text.contains("ohne Wahlposition")));
        assert!(texts.iter().any(|text| text.contains("ohne Einheitspreis")));
        assert!(texts
            .iter()
            .any(|text| text.contains("„Meter“ hat mehr als 4")));
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings
            .iter()
            .all(|finding| finding.row.is_some() || finding.severity == Severity::Hint));
        assert_eq!(count(&findings, Severity::Error), 4);
    }

    #[test]
    fn fachregeln_of_the_written_file_are_shown_once_with_their_row() {
        let project = project();
        let findings = check_with_rules(
            &project,
            Some(GaebExport::RequestForQuotation),
            &GaebPdfExchange,
        );
        let rules: Vec<_> = findings
            .iter()
            .filter(|finding| finding.text.starts_with("Fachregel"))
            .collect();
        // The empty position has neither short nor long text (FD Item 7).
        let empty = project.lv().positions().nth(1).unwrap().id();
        assert!(
            rules
                .iter()
                .any(|finding| finding.text.starts_with("Fachregel R9: ")
                    && finding.text.contains("ohne Beschreibung")
                    && finding.row == Some(empty)
                    && finding.severity == Severity::Error),
            "{rules:#?}"
        );
        // Grund- ohne Wahlposition and the missing unit stand once (own check).
        assert!(!rules.iter().any(|finding| finding.text.contains("R5:")));
        assert!(!rules
            .iter()
            .any(|finding| finding.text.contains("ohne Mengeneinheit")));
        let plain = check_with_rules(&project, None, &GaebPdfExchange);
        assert!(plain
            .iter()
            .any(|finding| finding.text.starts_with("Fachregel R9 (X83): ")));
        // Without the rules (position window): only the LV's own checks.
        assert!(!check(&project, None)
            .iter()
            .any(|finding| finding.text.starts_with("Fachregel")));
    }

    #[test]
    fn missing_quantities_count_for_lump_sums_and_status_is_reported() {
        let mut project = project();
        let lv = project.lv_mut();
        let mut lump = Position::new("", "Pauschal", Decimal::ZERO, "psch", None);
        lump.set_lump_sum(true);
        lv.push_position(lump);
        let first = lv.positions().next().unwrap().id();
        lv.set_status(&[first], proava_domain::EditStatus::Done);
        lv.renumber();
        let findings = check(&project, None);
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.text.contains("ohne Menge"))
                .count(),
            2
        );
        assert!(findings
            .iter()
            .any(|finding| finding.text.starts_with("1 von 6 Positionen als fertig")));
    }

    /// LV prüfen zählt die Bieterangaben (Information); eine Bieterangabe
    /// ohne Vortext ist eine Warnung, auch in der Exportprüfung.
    #[test]
    fn bidder_fields_are_counted_and_one_without_caption_is_named() {
        use proava_domain::{ComplementKind, TextComplement};
        let mut project = project();
        let lv = project.lv_mut();
        let first = lv.positions().next().unwrap().id();
        let index = lv.index_of(first).unwrap();
        let field = |marker: &str, kind, caption: &str| TextComplement {
            marker: marker.into(),
            kind,
            caption: caption.into(),
            ..TextComplement::default()
        };
        lv.row_mut(index)
            .unwrap()
            .as_position_mut()
            .unwrap()
            .set_complements(vec![
                field("01", ComplementKind::Bidder, "Fabrikat: "),
                field("02", ComplementKind::Bidder, ""),
                field("03", ComplementKind::Owner, "Nennweite "),
            ]);
        let findings = check(&project, None);
        assert!(
            findings.iter().any(|finding| finding.severity == Severity::Hint
                && finding.text == "2 Bieterangaben (vom Bieter einzutragen) und 1 Angabe des Auftraggebers in 1 Zeile."),
            "{findings:#?}"
        );
        let missing: Vec<_> = findings
            .iter()
            .filter(|finding| finding.text.contains("ohne Vortext"))
            .collect();
        assert_eq!(missing.len(), 1, "{findings:#?}");
        assert!(missing[0].text.contains("Bieterangabe 02 ohne Vortext"));
        assert_eq!(
            (missing[0].severity, missing[0].row),
            (Severity::Warning, Some(first))
        );
        // Exportprüfung: the warning stays, the count does not stop the export.
        let export = check(&project, Some(GaebExport::RequestForQuotation));
        assert!(export
            .iter()
            .any(|finding| finding.text.contains("ohne Vortext")));
        assert!(!export
            .iter()
            .any(|finding| finding.text.contains("vom Bieter einzutragen")));
    }

    #[test]
    fn request_for_quotation_mentions_prices() {
        let findings = check(&project(), Some(GaebExport::RequestForQuotation));
        assert!(findings
            .iter()
            .any(|finding| finding.text.contains("nicht übertragen")));
        assert!(!findings
            .iter()
            .any(|finding| finding.text.contains("ohne Einheitspreis")));
    }

    #[test]
    fn stlb_positions_are_checked_for_public_clients() {
        use proava_domain::{
            ComplementKind, ComplementNumber, RichText, StlbReference, TextComplement,
        };
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.push_group(LvGroup::new(0, "", "Titel"));
        lv.push_group(LvGroup::new(1, "", "Untertitel"));
        let complement = |marker: &str, kind, body: &str| TextComplement {
            marker: marker.into(),
            kind,
            caption: "Wert ".into(),
            body: body.into(),
            tail: String::new(),
            anchor: None,
        };
        let mut position = Position::new("", "Datenkabel", Decimal::ONE, "St", None);
        position.set_long_text(RichText::plain("Datenkabel Kategorie 7"));
        position.set_complements(vec![
            complement("11", ComplementKind::Owner, "1,5"),
            complement("12", ComplementKind::Owner, "acht"),
            complement("13", ComplementKind::Owner, ""),
            complement("51", ComplementKind::Bidder, ""),
        ]);
        position.attach_stlb(StlbReference {
            catalog: "STLB-Bau".into(),
            group: 9001,
            complement_numbers: vec![
                ("11".into(), ComplementNumber::Decimal),
                ("12".into(), ComplementNumber::Integer),
            ],
            ..StlbReference::default()
        });
        lv.push_position(position);
        lv.renumber();
        let mut project = Project::new(ProjectName::parse("P").unwrap(), "2026-11-17", lv);
        let texts = |project: &Project| -> Vec<String> {
            check(project, Some(GaebExport::RequestForQuotation))
                .into_iter()
                .filter(|finding| finding.severity == Severity::Warning)
                .map(|finding| finding.text)
                .collect()
        };
        let found = texts(&project);
        assert!(
            found
                .iter()
                .any(|t| t.contains("Textergänzung 12: „acht“ ist kein Zahlenwert")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|t| t.contains("Textergänzung 13 des Ausschreibenden (TA) ist leer")),
            "{found:?}"
        );
        assert!(
            !found.iter().any(|t| t.contains("Textergänzung 11")),
            "1,5 ist ein Zahlenwert"
        );
        assert!(
            !found.iter().any(|t| t.contains("Textergänzung 51")),
            "TB füllt der Bieter"
        );
        assert!(!found.iter().any(|t| t.contains("STLB-Bau*")));
        let id = project.lv().positions().next().unwrap().id();
        let index = project.lv().index_of(id).unwrap();
        if let Some(position) = project
            .lv_mut()
            .row_mut(index)
            .and_then(|row| row.as_position_mut())
        {
            position.set_short_text("Datenkabel, geändert");
        }
        assert!(
            !texts(&project).iter().any(|t| t.contains("STLB-Bau*")),
            "der Kurztext ist frei"
        );
        if let Some(position) = project
            .lv_mut()
            .row_mut(index)
            .and_then(|row| row.as_position_mut())
        {
            position.set_long_text(proava_domain::RichText::plain("Eigener Text"));
        }
        assert!(texts(&project)
            .iter()
            .any(|t| t.contains(proava_domain::stlb::STLB_CHANGED)));
    }

    #[test]
    fn hints_without_text_warn_and_spacers_only_inform_the_export() {
        use proava_domain::{LvRow, Note, Spacer};
        let mut project = project();
        let lv = project.lv_mut();
        let blank = lv.push_note(Note::new("  "));
        lv.insert_numbered(3, LvRow::Spacer(Spacer::new())).unwrap();
        lv.push_spacer();
        let findings = check(&project, None);
        let warning = findings
            .iter()
            .find(|finding| finding.row == Some(blank))
            .expect("Warnung für den leeren Hinweis");
        assert_eq!(warning.severity, Severity::Warning);
        assert!(
            warning.text.starts_with("Hinweis ohne Text nach 01.01.0050 – für eine Leerzeile in der Bearbeitung den Abstandshalter"),
            "{}",
            warning.text
        );
        assert!(!findings
            .iter()
            .any(|finding| finding.text.contains("Abstandshalter wird")));
        let export = check(&project, Some(GaebExport::RequestForQuotation));
        assert!(export
            .iter()
            .any(|finding| finding.severity == Severity::Hint
                && finding.text
                    == "2 Abstandshalter werden nicht übertragen (GAEB kennt keine Leerzeile)."));
        assert!(super::worth_a_stop(&export));
        // The information alone does not stop the export.
        let only: Vec<_> = export
            .into_iter()
            .filter(|finding| finding.text.contains("Abstandshalter werden"))
            .collect();
        assert_eq!(only.len(), 1);
        assert!(!super::worth_a_stop(&only));
        // Spacers bring no Fachregel findings of their own.
        let mut plain = project.clone();
        let spacers: Vec<_> = plain
            .lv()
            .rows()
            .iter()
            .filter(|row| row.is_spacer())
            .map(LvRow::id)
            .collect();
        for id in spacers {
            let index = plain.lv().index_of(id).unwrap();
            plain.lv_mut().remove_row(index);
        }
        let rules = |project: &Project| -> Vec<String> {
            check_with_rules(project, None, &GaebPdfExchange)
                .into_iter()
                .filter(|finding| finding.text.starts_with("Fachregel"))
                .map(|finding| finding.text)
                .collect()
        };
        assert_eq!(rules(&project), rules(&plain));
    }
}

/// Hint for a manufacturer text from AUSSCHREIBEN.DE: product neutrality
/// (VOB/A § 7 Abs. 2) is the planner's call; missing „oder gleichwertig“ is
/// named. Nothing for other texts.
pub fn manufacturer_text_hint(position: &proava_domain::Position) -> Option<String> {
    let origin = position.text_origin()?;
    if origin.source != proava_domain::text_origin::AUSSCHREIBEN_DE {
        return None;
    }
    let text = format!(
        "{} {}",
        position.short_text(),
        position.long_text().to_plain()
    );
    let equivalent = text.to_lowercase().contains("gleichwertig");
    let supplier = origin.supplier.trim();
    let from = if supplier.is_empty() {
        origin.source.clone()
    } else {
        format!("{}, {supplier}", origin.source)
    };
    Some(if equivalent {
        format!("Herstellertext ({from}) – Produktneutralität prüfen (VOB/A § 7 Abs. 2).")
    } else {
        format!(
            "Herstellertext ({from}) ohne „oder gleichwertig“ – Produktneutralität prüfen (VOB/A § 7 Abs. 2)."
        )
    })
}

#[cfg(test)]
mod manufacturer_tests {
    use proava_domain::{Decimal, Position, RichText, TextOrigin};

    use super::manufacturer_text_hint;

    #[test]
    fn manufacturer_texts_get_a_hint_others_none() {
        let mut position = Position::new("01.0010", "Drehantrieb", Decimal::ONE, "St", None);
        position.set_long_text(RichText::plain("Hersteller: Muster GmbH"));
        assert_eq!(manufacturer_text_hint(&position), None);
        position.set_text_origin(Some(TextOrigin {
            source: "AUSSCHREIBEN.DE".into(),
            supplier: "Muster GmbH".into(),
            ..TextOrigin::default()
        }));
        let hint = manufacturer_text_hint(&position).unwrap();
        assert!(hint.contains("ohne „oder gleichwertig“"), "{hint}");
        assert!(hint.contains("AUSSCHREIBEN.DE, Muster GmbH"), "{hint}");
        position.set_long_text(RichText::plain(
            "Hersteller: Muster GmbH\noder gleichwertig",
        ));
        let hint = manufacturer_text_hint(&position).unwrap();
        assert!(!hint.contains("ohne"), "{hint}");
        position.set_text_origin(Some(TextOrigin {
            source: "STLB-Bau".into(),
            ..TextOrigin::default()
        }));
        assert_eq!(manufacturer_text_hint(&position), None);
    }
}
