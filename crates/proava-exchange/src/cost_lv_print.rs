//! The active Kosten-LV as print model: Angaben nach DIN 276,
//! Zusammenstellung nach Kostengruppen in den Ebenen der Stufe (Kostenschätzung
//! 1.–2., Kostenberechnung 1.–3. Ebene), Kurztext-Liste.

use std::collections::BTreeMap;

use proava_domain::{
    cost_group_at_level, cost_group_report, din_group_label, din_level, CostStage, LvRow, Money,
    Project,
};
use proava_print::{PrintCostLv, PrintCostLvGroup, PrintCostLvItem, PrintCostLvPoints};

/// Options of the print dialog that apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostLvPrintOptions {
    pub notes: bool,
    pub long_texts: bool,
    /// Kurztext-Liste after the Zusammenstellung.
    pub list: bool,
    pub landscape: bool,
}

pub fn cost_lv_to_print(
    project: &Project,
    date: &str,
    options: CostLvPrintOptions,
) -> Result<PrintCostLv, String> {
    let lv = project.lv();
    let stage = lv.purpose().cost_stage().ok_or_else(|| {
        "Das LV ist keine Kostenermittlung – die Kostenermittlung (Kosten-LV) druckt nur Kosten-LVs."
            .to_string()
    })?;
    let mut header = crate::to_print::header(project, date);
    header.document_kind = Some(format!("{} nach DIN 276", stage.label()));
    let section = match stage {
        CostStage::Estimate => "4.3.3",
        CostStage::Calculation => "4.3.4",
    };
    let facts = vec![
        (
            "Stufe".to_string(),
            format!(
                "{} nach DIN 276:2018-12 ({section}), Gliederung bis zur {}. Ebene",
                stage.label(),
                stage.level()
            ),
        ),
        (
            "Umsatzsteuer".to_string(),
            "Beträge netto, die Umsatzsteuer steht unter der Zusammenstellung (4.2.15)".to_string(),
        ),
        (
            "Kostengliederung".to_string(),
            lv.cost_catalog().name.clone(),
        ),
    ];
    let notes = if options.notes {
        lv.rows()
            .iter()
            .filter_map(|row| match row {
                LvRow::Note(note) => Some((
                    note.heading().trim().to_string(),
                    note.text()
                        .to_plain()
                        .lines()
                        .map(str::to_string)
                        .filter(|line| !line.trim().is_empty())
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            })
            .filter(|(heading, text)| !heading.is_empty() || !text.is_empty())
            .collect()
    } else {
        Vec::new()
    };
    Ok(PrintCostLv {
        header,
        facts,
        notes,
        groups: groups(project, stage.level()),
        items: if options.list {
            items(project, options.long_texts)
        } else {
            Vec::new()
        },
        net_total: lv.net_total().to_decimal(),
        vat_percent: lv.vat_percent(),
        list: options.list,
        landscape: options.landscape,
        points: points(project, date),
    })
}

/// The Datenpunkt-Abschnitte (GU-Fall) for their appendix: the tables of the
/// Datenpunktrechnung (Zusammenfassung, Bauteile, Anlagenliste), without
/// Kostenkontrolle.
fn points(project: &Project, date: &str) -> Vec<PrintCostLvPoints> {
    let lv = project.lv();
    lv.point_sections()
        .into_iter()
        .filter_map(|index| {
            let row = &lv.rows()[index];
            let points = row.as_group()?.points()?;
            let options = proava_print::PrintCostOptions {
                cover: false,
                landscape: false,
                chart: false,
                plants: true,
            };
            Some(PrintCostLvPoints {
                name: section_name(row),
                estimate: crate::cost_print::estimate_to_print(
                    project, points, date, options, false,
                ),
            })
        })
        .collect()
}

/// „480 Gebäude- und Anlagenautomation“.
fn section_name(row: &LvRow) -> String {
    [row.ordinal().trim(), row.short_text().trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Zusammenstellung: the KG of the 1st level, below them those of the 2nd and –
/// down to `depth` – the 3rd, each with its sum. An amount that sits on a
/// coarser KG than `depth` (e.g. a position with KG 420 in a Kostenberechnung)
/// gets its own line „ohne Aufteilung auf die 3. Ebene“, so the lines below a
/// group add up to it. Positions without KG and codes outside DIN 276 come last.
fn groups(project: &Project, depth: u8) -> Vec<PrintCostLvGroup> {
    let report = cost_group_report(project.lv(), &[]);
    let mut leaves: BTreeMap<String, Money> = BTreeMap::new();
    for line in report.lines.iter().filter(|line| !line.subtotal) {
        *leaves
            .entry(line.code.clone().unwrap_or_default().trim().to_string())
            .or_insert(Money::ZERO) += line.planner;
    }
    let sum_at = |code: &str, level: u8| -> Money {
        leaves
            .iter()
            .filter(|(leaf, _)| {
                din_level(leaf).is_some() && cost_group_at_level(leaf, level) == code
            })
            .map(|(_, amount)| *amount)
            .sum()
    };
    let mut lines = Vec::new();
    let mut push = |level: u8, code: &str, label: String, amount: Money| {
        lines.push(PrintCostLvGroup {
            level,
            code: code.to_string(),
            label,
            amount: amount.to_decimal(),
        });
    };
    let codes_at = |level: u8| -> Vec<String> {
        let mut codes: Vec<String> = leaves
            .keys()
            .filter(|leaf| din_level(leaf).is_some_and(|own| own >= level))
            .map(|leaf| cost_group_at_level(leaf, level))
            .collect();
        codes.sort();
        codes.dedup();
        codes
    };
    let rest = |level: u8| format!("ohne Aufteilung auf die {level}. Ebene");
    for main in codes_at(1) {
        push(
            1,
            &main,
            din_group_label(&main).to_string(),
            sum_at(&main, 1),
        );
        if depth < 2 {
            continue;
        }
        if let Some(own) = leaves.get(&main).filter(|amount| !amount.is_zero()) {
            push(2, &main, rest(2), *own);
        }
        for group in codes_at(2)
            .into_iter()
            .filter(|group| group.starts_with(&main[..1]))
        {
            push(
                2,
                &group,
                din_group_label(&group).to_string(),
                sum_at(&group, 2),
            );
            if depth < 3 {
                continue;
            }
            if let Some(own) = leaves.get(&group).filter(|amount| !amount.is_zero()) {
                push(3, &group, rest(3), *own);
            }
            for code in codes_at(3)
                .into_iter()
                .filter(|code| code.starts_with(&group[..2]))
            {
                push(
                    3,
                    &code,
                    din_group_label(&code).to_string(),
                    sum_at(&code, 3),
                );
            }
        }
    }
    for (code, amount) in &leaves {
        if din_level(code).is_none() && !amount.is_zero() {
            let label = if code.is_empty() {
                "ohne Kostengruppe".to_string()
            } else {
                "Kostengruppe außerhalb von DIN 276".to_string()
            };
            push(0, code, label, *amount);
        }
    }
    lines
}

/// Kurztext-Liste: titles with their sums (only those with positions below)
/// and the positions with KG, Menge, Einheit, Kostenkennwert and Betrag.
fn items(project: &Project, long_texts: bool) -> Vec<PrintCostLvItem> {
    let lv = project.lv();
    let rows = lv.rows();
    let effective = lv.effective_cost_groups();
    // Title sums as in the LV (Zuschläge, Aufteilungszeilen, Pauschalsummen,
    // Datenpunkt-Abschnitte); Zuschläge and Aufteilung with their base.
    let totals = lv.group_totals();
    let computation = lv.split_computation();
    let mut items = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        match row {
            LvRow::Group(group) if group.is_point_section() => {
                let points = group.points().map(|points| points.result());
                let (count, amount) = points
                    .map(|result| (result.points, result.total))
                    .unwrap_or_default();
                items.push(PrintCostLvItem::Points {
                    ordinal: group.ordinal().to_string(),
                    text: group.short_text().to_string(),
                    // The split on 481–489 is in the appendix.
                    cost_group: proava_domain::POINTS_GROUP.to_string(),
                    points: count,
                    total: amount.to_decimal(),
                });
            }
            LvRow::Group(group) => {
                // Only titles with positions (or Datenpunkt-Abschnitte) below.
                let any = rows[lv.block_range(index)].iter().skip(1).any(|later| {
                    later.is_position()
                        || later
                            .as_group()
                            .is_some_and(proava_domain::LvGroup::is_point_section)
                });
                let total = totals
                    .iter()
                    .find(|total| total.group_row == index)
                    .map(|total| total.total)
                    .unwrap_or(Money::ZERO);
                if any {
                    items.push(PrintCostLvItem::Title {
                        level: group.level(),
                        ordinal: group.ordinal().to_string(),
                        text: group.short_text().to_string(),
                        total: total.to_decimal(),
                    });
                }
            }
            LvRow::Position(position) => {
                let value = computation.values.get(&position.id());
                let basis = computation.bases.contains(&position.id());
                // Zuschlag / Aufteilung: „40 % von 118.200,00 €“, no quantity.
                let note = match (position.surcharge(), value) {
                    (Some(_), Some(value)) => {
                        proava_domain::surcharge::share_text(position.unit_price(), value.base)
                    }
                    _ if basis => "Rechengrundlage, aufgeteilt".to_string(),
                    _ => String::new(),
                };
                items.push(PrintCostLvItem::Position {
                    ordinal: position.ordinal().to_string(),
                    text: position.short_text().to_string(),
                    cost_group: effective[index]
                        .as_ref()
                        .map(|(code, _)| code.clone())
                        .unwrap_or_default(),
                    quantity: (!position.is_surcharge()).then(|| position.quantity()),
                    unit: position.unit().to_string(),
                    unit_price: (!position.is_surcharge())
                        .then(|| position.unit_price())
                        .flatten(),
                    total: value
                        .map(|value| value.amount)
                        .unwrap_or_else(|| position.total_price())
                        .to_decimal(),
                    counts: position.counts_in_total() && !basis,
                    note,
                    long_text: if long_texts {
                        position
                            .long_text()
                            .to_plain()
                            .lines()
                            .map(str::to_string)
                            .filter(|line| !line.trim().is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    },
                });
            }
            _ => {}
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{cost_lv_template, Decimal, Lv, LvPurpose, Position, ProjectName};

    fn project() -> Project {
        let mut lv = cost_lv_template(
            CostStage::Calculation,
            "Kostenberechnung Heizung",
            &["420"],
            10,
        );
        lv.set_number("7");
        let rows = lv.rows().len();
        // Under 420.421: a Wärmepumpe; directly under 420 (too coarse): Heizung pauschal.
        let mut pump = Position::new(
            "420.421.0010",
            "Wärmepumpe",
            Decimal::from(40),
            "kW",
            Some(Decimal::from(1500)),
        );
        pump.set_cost_group(Some("421".into()));
        lv.insert_row(
            2,
            LvRow::Position(Position::new(
                "420.0010",
                "Heizung pauschal",
                Decimal::ONE,
                "psch",
                Some(Decimal::from(2000)),
            )),
        );
        lv.insert_row(4, LvRow::Position(pump));
        assert_eq!(lv.rows().len(), rows + 2);
        let mut free = Position::new(
            "0010",
            "Inbetriebnahme",
            Decimal::ONE,
            "psch",
            Some(Decimal::from(500)),
        );
        free.set_cost_group(Some("999".into()));
        lv.push_position(free);
        let tender = Lv::new("Ausschreibung", lv.oz_mask().clone());
        let mut project = Project::new(
            ProjectName::parse("Rathaus 3").unwrap(),
            "2026-11-18",
            tender,
        );
        let index = project.add_lv(lv);
        project.set_active_lv(index);
        project
    }

    #[test]
    fn summary_has_the_levels_of_the_stage() {
        let project = project();
        let options = CostLvPrintOptions {
            notes: true,
            long_texts: false,
            list: true,
            landscape: false,
        };
        let model = cost_lv_to_print(&project, "18.11.2026", options).unwrap();
        let lines: Vec<(u8, &str, &str, Decimal)> = model
            .groups
            .iter()
            .map(|group| {
                (
                    group.level,
                    group.code.as_str(),
                    group.label.as_str(),
                    group.amount,
                )
            })
            .collect();
        assert_eq!(
            lines,
            [
                (
                    1,
                    "400",
                    "Bauwerk – Technische Anlagen",
                    Decimal::from(62_000)
                ),
                (2, "420", "Wärmeversorgungsanlagen", Decimal::from(62_000)),
                (
                    3,
                    "420",
                    "ohne Aufteilung auf die 3. Ebene",
                    Decimal::from(2000)
                ),
                (3, "421", "Wärmeerzeugungsanlagen", Decimal::from(60_000)),
                (
                    0,
                    "999",
                    "Kostengruppe außerhalb von DIN 276",
                    Decimal::from(500)
                ),
            ]
        );
        assert_eq!(model.net_total, Decimal::from(62_500));
        assert_eq!(
            model.header.document_kind.as_deref(),
            Some("Kostenberechnung nach DIN 276")
        );
        assert!(model.notes[0].0.contains("Angaben zur Kostenberechnung"));
        // Only titles with positions, not the empty subtitles 422–424.
        let titles: Vec<&str> = model
            .items
            .iter()
            .filter_map(|item| match item {
                PrintCostLvItem::Title { ordinal, .. } => Some(ordinal.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(titles, ["420", "420.421", "420.429"]);
        let (bytes, _) = proava_print::render_cost_lv_pdf_counted(&model).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    /// GU-Fall: the Datenpunkt-Abschnitt 480 counts in the Zusammenfassung
    /// (481–489 by its split), has one line in the Kurztext-Liste and an
    /// appendix „Datenpunkte“ with the plant list.
    #[test]
    fn a_point_section_is_summed_listed_and_appended() {
        let mut project = project();
        let lv = project.lv_mut();
        let index = lv.insert_point_section().unwrap();
        if let Some(LvRow::Group(group)) = lv.row_mut(index) {
            let points = group.points_mut().unwrap();
            points.sections[0].name = "Gebäude".into();
            points.sections[0].rows[0] = proava_domain::PointRow {
                location: "ASP01".into(),
                trade: "RLT".into(),
                plant: "RLT-01".into(),
                remark: String::new(),
                counts: vec![100, 0, 0, 0, 0, 0],
            };
        }
        let options = CostLvPrintOptions {
            notes: false,
            long_texts: false,
            list: true,
            landscape: false,
        };
        let model = cost_lv_to_print(&project, "19.11.2026", options).unwrap();
        let line = |code: &str| {
            model
                .groups
                .iter()
                .find(|group| group.code == code)
                .map(|group| group.amount)
        };
        assert_eq!(line("480"), Some(Decimal::from(40_000)), "100 AI × 400 €");
        assert_eq!(line("481"), Some(Decimal::from(16_000)), "40 %");
        assert_eq!(line("400"), Some(Decimal::from(102_000)));
        assert_eq!(model.net_total, Decimal::from(102_500));
        assert!(model.items.iter().any(|item| matches!(item,
            PrintCostLvItem::Points { ordinal, points: 100, cost_group, .. }
                if ordinal == "480" && cost_group == "480")));
        assert_eq!(model.points.len(), 1);
        assert_eq!(model.points[0].name, "480 Gebäude- und Anlagenautomation");
        assert_eq!(model.points[0].estimate.points, 100);
        let (bytes, pages) = proava_print::render_cost_lv_pdf_counted(&model).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(pages >= 2, "Anhang auf eigener Seite");
    }

    /// Aufteilungszeilen: the Zusammenstellung has 481–489 from the split,
    /// the Kurztext-Liste „40 % von 118.200,00 €“ and the Datenpunkte in
    /// brackets as Rechengrundlage; the LV print marks them the same way.
    #[test]
    fn aufteilungszeilen_print_with_share_and_basis() {
        let mut lv = cost_lv_template(CostStage::Calculation, "Kostenberechnung GA", &["480"], 10);
        let title = lv
            .rows()
            .iter()
            .position(|row| row.ordinal() == "480")
            .unwrap();
        let range = lv.insert_points_template(Some(title)).unwrap();
        let ids: Vec<proava_domain::RowId> = lv.rows()[range].iter().map(LvRow::id).collect();
        // Made up: 100 AI, 150 BI, 20 AO, 15 BO, 11 AV, 10 BV = 118.200 €.
        for (id, quantity) in ids[1..7].iter().zip([100, 150, 20, 15, 11, 10]) {
            let index = lv.index_of(*id).unwrap();
            lv.row_mut(index)
                .unwrap()
                .as_position_mut()
                .unwrap()
                .set_quantity(Decimal::from(quantity));
        }
        let tender = Lv::new("Ausschreibung", lv.oz_mask().clone());
        let mut project = Project::new(
            ProjectName::parse("Rathaus 3").unwrap(),
            "2026-11-19",
            tender,
        );
        let index = project.add_lv(lv);
        project.set_active_lv(index);
        let options = CostLvPrintOptions {
            notes: false,
            long_texts: false,
            list: true,
            landscape: false,
        };
        let model = cost_lv_to_print(&project, "19.11.2026", options).unwrap();
        let line = |code: &str| {
            model
                .groups
                .iter()
                .find(|group| group.code == code)
                .map(|group| group.amount)
        };
        assert_eq!(line("481"), Some(Decimal::from(47_280)));
        assert_eq!(line("489"), Some(Decimal::from(3_546)));
        assert_eq!(line("480"), Some(Decimal::from(118_200)));
        assert_eq!(model.net_total, Decimal::from(118_200));
        let positions: Vec<(&str, Option<Decimal>, Decimal, bool, &str)> = model
            .items
            .iter()
            .filter_map(|item| match item {
                PrintCostLvItem::Position {
                    text,
                    quantity,
                    total,
                    counts,
                    note,
                    ..
                } => Some((text.as_str(), *quantity, *total, *counts, note.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            positions[0],
            (
                "1.1.1 Analoge Eingabe (AI)",
                Some(Decimal::from(100)),
                Decimal::from(40_000),
                false,
                "Rechengrundlage, aufgeteilt"
            )
        );
        assert_eq!(
            positions[6],
            (
                "KG 481 Automationseinrichtungen",
                None,
                Decimal::from(47_280),
                true,
                "40 % von 118.200,00 €"
            )
        );
        let title_total = model.items.iter().find_map(|item| match item {
            PrintCostLvItem::Title { text, total, .. }
                if text == proava_domain::POINTS_TEMPLATE_TITLE =>
            {
                Some(*total)
            }
            _ => None,
        });
        assert_eq!(title_total, Some(Decimal::from(118_200)));
        let (bytes, _) = proava_print::render_cost_lv_pdf_counted(&model).unwrap();
        assert!(bytes.starts_with(b"%PDF"));

        // The LV print: Rechengrundlage in brackets, the line „Aufteilung nach KG“.
        let print =
            crate::to_print::lv_to_print(&project, proava_print::LvPrintMode::Priced, "19.11.2026");
        let mut found = Vec::new();
        fn walk(items: &[proava_print::PrintItem], found: &mut Vec<(String, bool, Vec<String>)>) {
            for item in items {
                match item {
                    proava_print::PrintItem::Title(title) => walk(&title.items, found),
                    proava_print::PrintItem::Position(position) => found.push((
                        position.short_text.clone(),
                        position.in_total,
                        position
                            .kinds
                            .iter()
                            .map(|kind| format!("{kind:?}"))
                            .collect(),
                    )),
                    _ => {}
                }
            }
        }
        walk(&print.items, &mut found);
        let ai = found
            .iter()
            .find(|(text, ..)| text.ends_with("(AI)"))
            .unwrap();
        assert!(
            !ai.1 && ai.2.iter().any(|kind| kind.contains("Rechengrundlage")),
            "{ai:?}"
        );
        let kg = found
            .iter()
            .find(|(text, ..)| text.starts_with("KG 481"))
            .unwrap();
        assert!(
            kg.1 && kg.2.iter().any(|kind| kind.contains("Aufteilung nach KG")),
            "{kg:?}"
        );
    }

    #[test]
    fn a_tender_lv_is_refused() {
        let mut project = project();
        project.lv_mut().set_purpose(LvPurpose::Tender);
        let options = CostLvPrintOptions {
            notes: false,
            long_texts: false,
            list: false,
            landscape: true,
        };
        assert!(cost_lv_to_print(&project, "", options).is_err());
    }
}
