//! Kostenverfolgung als CSV: die Tabelle wie gezeigt (Ansicht, Ebene,
//! netto/brutto). Je Zeile Zeilenart, KG, Bezeichnung und Vergabeeinheit,
//! die gezeigten Stufen, die Abweichungen der Prognose zum Budget und zur
//! Vorstufe, auf Wunsch der Vergleich mit einem festgehaltenen Kostenstand
//! und die Erläuterung. Deutsche Zahlen ohne Tausenderpunkte, Trenner „;“,
//! CRLF, UTF-8 mit BOM oder Windows-1252 – wie der LV-Export.

use proava_application::CsvEncoding;
use proava_domain::{Deviation, Money, TrackReport, TrackRow, TrackRowKind, TrackStage};

use crate::csv_export::{encode_csv, field, number, CsvExport};

fn amount(value: Option<Money>) -> String {
    value
        .map(|value| number(value.to_decimal(), 2))
        .unwrap_or_default()
}

fn percent(deviation: Option<&Deviation>) -> String {
    deviation
        .and_then(|deviation| deviation.percent)
        .map(|percent| number(percent, 1))
        .unwrap_or_default()
}

fn kind_text(row: &TrackRow) -> &'static str {
    match row.kind {
        TrackRowKind::Group => "Gruppe",
        TrackRowKind::Leaf => "Kostengruppe",
        TrackRowKind::Lv => "Vergabeeinheit",
        TrackRowKind::NoLv => "ohne Vergabeeinheit",
        TrackRowKind::Without => "ohne Kostengruppe",
        TrackRowKind::Total => "Gesamt",
    }
}

/// The table of the Kostenverfolgung as CSV.
pub fn tracking_to_csv(report: &TrackReport, encoding: CsvEncoding) -> CsvExport {
    let stages = report.shown_stages();
    let amounts = if report.options.gross {
        "brutto"
    } else {
        "netto"
    };
    let mut header: Vec<String> = vec![
        "Zeile".into(),
        "KG".into(),
        "Bezeichnung".into(),
        "Vergabeeinheit".into(),
    ];
    header.extend(
        stages
            .iter()
            .map(|stage| format!("{} {amounts}", stage.label())),
    );
    header.extend([
        "Prognose zur Kostenvorgabe".into(),
        "Prognose zur Kostenvorgabe %".into(),
        "Prognose zur Vorstufe".into(),
        "Prognose zur Vorstufe %".into(),
    ]);
    // Compared with a recorded state: each stage as it was then.
    if let Some(snapshot) = &report.snapshot {
        let date = snapshot.split(" – ").next().unwrap_or(snapshot);
        header.extend(
            stages
                .iter()
                .map(|stage| format!("{} Stand {date}", stage.label())),
        );
    }
    header.push("Erläuterung / Maßnahme".into());
    let mut lines = vec![header
        .iter()
        .map(|cell| field(cell))
        .collect::<Vec<_>>()
        .join(";")];

    // In the view by Vergabeeinheit the KG rows carry their LV too.
    let mut current_lv: Option<String> = None;
    for row in report.rows.iter().chain(std::iter::once(&report.total)) {
        let lv = match row.kind {
            TrackRowKind::Lv => {
                current_lv = report.lv_label(row).map(str::to_string);
                current_lv.clone()
            }
            TrackRowKind::NoLv | TrackRowKind::Total => {
                current_lv = None;
                None
            }
            _ => current_lv.clone(),
        };
        // A KG row has its code in a column of its own.
        let mut cells: Vec<String> = vec![
            kind_text(row).to_string(),
            row.code.clone().unwrap_or_default(),
            row.label.clone(),
            lv.unwrap_or_default(),
        ];
        cells.extend(stages.iter().map(|stage| amount(row.value(*stage))));
        let to_budget = row.to_budget(TrackStage::Forecast);
        let to_previous = row.to_previous(TrackStage::Forecast);
        cells.push(amount(to_budget.map(|deviation| deviation.amount)));
        cells.push(percent(to_budget.as_ref()));
        cells.push(amount(
            to_previous.map(|previous| previous.deviation.amount),
        ));
        cells.push(percent(
            to_previous.as_ref().map(|previous| &previous.deviation),
        ));
        if report.snapshot.is_some() {
            cells.extend(stages.iter().map(|stage| amount(row.compared(*stage))));
        }
        cells.push(row.note.clone());
        lines.push(
            cells
                .iter()
                .map(|cell| field(cell))
                .collect::<Vec<_>>()
                .join(";"),
        );
    }
    let mut text = lines.join("\r\n");
    text.push_str("\r\n");
    let mut notes = Vec::new();
    let bytes = encode_csv(&text, encoding, &mut notes);
    CsvExport { bytes, notes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{
        cost_tracking, CostStage, Lv, LvGroup, LvPurpose, OzMask, Position, Project, ProjectName,
        TrackOptions, TrackView,
    };

    fn project() -> Project {
        let mut lv = Lv::new("Kostenberechnung", OzMask::parse("11.22.PPPP").unwrap());
        let mut group = LvGroup::new(0, "", "Wärme");
        group.set_cost_group(Some("421".into()));
        lv.push_group(group);
        lv.push_position(Position::new(
            "",
            "Wärmepumpe",
            1.into(),
            "St",
            Some(50_000.into()),
        ));
        lv.renumber();
        lv.set_purpose(LvPurpose::Cost(CostStage::Calculation));
        let mut project = Project::new(
            ProjectName::parse("Rathaus 3").unwrap(),
            "2026-11-18T12:00:00",
            lv,
        );
        let tracking = project.cost_tracking_mut();
        tracking.set_budget("420", Money::from_cents(4_800_000));
        tracking.set_note("421", "Variante mit Förderung; prüfen");
        project
    }

    fn text(export: &CsvExport) -> String {
        String::from_utf8(export.bytes[3..].to_vec()).unwrap()
    }

    #[test]
    fn the_table_as_shown_with_header_rows_and_total() {
        let project = project();
        let report = cost_tracking(
            &project,
            &TrackOptions {
                level: 3,
                ..TrackOptions::default()
            },
        );
        let export = tracking_to_csv(&report, CsvEncoding::Utf8Bom);
        assert!(export.bytes.starts_with(b"\xEF\xBB\xBF"));
        let text = text(&export);
        let lines: Vec<&str> = text.split("\r\n").collect();
        assert_eq!(
            lines[0],
            "Zeile;KG;Bezeichnung;Vergabeeinheit;Kostenvorgabe netto;Kostenberechnung netto;Prognose netto;\
             Prognose zur Kostenvorgabe;Prognose zur Kostenvorgabe %;Prognose zur Vorstufe;Prognose zur Vorstufe %;\
             Erläuterung / Maßnahme"
        );
        assert!(lines.contains(
            &"Gruppe;420;Wärmeversorgungsanlagen;;48000,00;50000,00;50000,00;2000,00;4,2;0,00;0,0;"
        ));
        // The note has a „;“: in quotes.
        assert!(lines.iter().any(|line| line
            .starts_with("Kostengruppe;421;Wärmeerzeugungsanlagen;;;50000,00;50000,00;")
            && line.ends_with(";\"Variante mit Förderung; prüfen\"")));
        assert!(lines.iter().any(
            |line| line.starts_with("Gesamt;;Gesamt;;48000,00;50000,00;50000,00;2000,00;4,2;")
        ));
        assert_eq!(lines.last(), Some(&""));
    }

    #[test]
    fn gross_names_the_columns_and_windows_1252_is_written() {
        let project = project();
        let report = cost_tracking(
            &project,
            &TrackOptions {
                gross: true,
                view: TrackView::ByLv,
                ..TrackOptions::default()
            },
        );
        let export = tracking_to_csv(&report, CsvEncoding::Windows1252);
        assert!(!export.bytes.starts_with(b"\xEF\xBB\xBF"));
        // „ä“ in Windows-1252 is one byte (0xE4).
        assert!(export.bytes.windows(2).any(|pair| pair == b"\xE4r"));
        assert!(export.notes.is_empty());
        let text =
            proava_gaeb::datan::decode(&export.bytes, proava_gaeb::datan::Encoding::Windows1252);
        assert!(text.starts_with("Zeile;KG;Bezeichnung;Vergabeeinheit;Kostenvorgabe brutto;"));
        assert!(text.contains("ohne Vergabeeinheit;"));
    }

    #[test]
    fn a_recorded_state_adds_each_stage_as_it_was() {
        let mut project = project();
        let snapshot = proava_domain::record_snapshot(&project, "18.11.2026", "abgestimmt");
        project.cost_tracking_mut().snapshots.push(snapshot);
        project
            .cost_tracking_mut()
            .set_budget("420", Money::from_cents(5_000_000));
        let report = cost_tracking(
            &project,
            &TrackOptions {
                level: 3,
                snapshot: Some(0),
                ..TrackOptions::default()
            },
        );
        let text = text(&tracking_to_csv(&report, CsvEncoding::Utf8Bom));
        let lines: Vec<&str> = text.split("\r\n").collect();
        assert!(
            lines[0].contains(";Kostenvorgabe Stand 18.11.2026;Kostenberechnung Stand 18.11.2026;Prognose Stand 18.11.2026;"),
            "{}",
            lines[0]
        );
        // 420: budget 50.000 now, 48.000 then.
        assert!(lines
            .iter()
            .any(|line| line.starts_with("Gruppe;420;")
                && line.contains(";48000,00;50000,00;50000,00;")));
    }
}
