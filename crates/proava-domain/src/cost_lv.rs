//! Kosten-LV: a Kostenschätzung or Kostenberechnung nach DIN 276 as a
//! Kurztext-LV – positions with Menge, Einheit (Bezugseinheit),
//! EP = Kostenkennwert and Kostengruppe (Anwender 18.11.: the normal case for
//! all Gewerke; the GA-Datenpunktrechnung stays the Sonderfall).
//!
//! DIN 276:2018-12, 4.3: the Kostenschätzung needs the 2nd level of the
//! Kostengliederung, the Kostenberechnung the 3rd. The template lays the
//! titles out in these levels with the KG as OZ (420 Wärmeversorgungsanlagen,
//! 420.421 Wärmeerzeugungsanlagen …), so a position takes its KG from the
//! title; „LV prüfen“ reports positions whose KG is too coarse or missing.

use crate::cost_estimate::CostStage;
use crate::cost_group::DIN276_KG400;
use crate::lv::{Lv, LvGroup, LvRow, Note, RowId};
use crate::lv_purpose::LvPurpose;
use crate::oz_mask::OzMask;
use crate::rich_text::RichText;

/// OZ-Maske of a Kostenschätzung: titles = KG of the 2nd level (420).
pub const ESTIMATE_MASK: &str = "111.PPPP";
/// OZ-Maske of a Kostenberechnung: titles = KG of the 2nd level, subtitles =
/// KG of the 3rd level (420.421).
pub const CALCULATION_MASK: &str = "111.222.PPPP";

/// Level of a Kostengruppe in the Kostengliederung of DIN 276 (5.1): „400“ =
/// 1, „420“ = 2, „421“ = 3; `None` when it is no code of KG 100 … 800.
pub fn din_level(code: &str) -> Option<u8> {
    let code = code.trim();
    if code.len() != 3
        || !code.chars().all(|c| c.is_ascii_digit())
        || !('1'..='8').contains(&code.chars().next().unwrap_or('0'))
    {
        return None;
    }
    Some(if code.ends_with("00") {
        1
    } else if code.ends_with('0') {
        2
    } else {
        3
    })
}

/// Kostengruppen of the 1st level (DIN 276:2018-12, 5.1).
const MAIN_GROUPS: [(&str, &str); 8] = [
    ("100", "Grundstück"),
    ("200", "Vorbereitende Maßnahmen"),
    ("300", "Bauwerk – Baukonstruktionen"),
    ("400", "Bauwerk – Technische Anlagen"),
    ("500", "Außenanlagen und Freiflächen"),
    ("600", "Ausstattung und Kunstwerke"),
    ("700", "Baunebenkosten"),
    ("800", "Finanzierung"),
];

/// Name of a Kostengruppe: the 1st level (100 … 800) and the KG-400 list;
/// empty when unknown.
pub fn din_group_label(code: &str) -> &'static str {
    let code = code.trim();
    MAIN_GROUPS
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, label)| *label)
        .unwrap_or_else(|| crate::cost_group::cost_group_label(code))
}

/// The Kostengruppen of the 2nd level of KG 400 (410 … 490) with their names.
pub fn kg400_groups() -> Vec<(&'static str, &'static str)> {
    DIN276_KG400
        .iter()
        .copied()
        .filter(|(code, _)| din_level(code) == Some(2))
        .collect()
}

/// The Kostengruppen of the 3rd level below `group` (2nd level), e.g. 421 … 429
/// below 420.
pub fn kg400_subgroups(group: &str) -> Vec<(&'static str, &'static str)> {
    let prefix = &group.trim()[..group.trim().len().min(2)];
    DIN276_KG400
        .iter()
        .copied()
        .filter(|(code, _)| din_level(code) == Some(3) && code.starts_with(prefix))
        .collect()
}

/// The OZ-Maske of a Kosten-LV of this Stufe.
pub fn cost_lv_mask(stage: CostStage) -> OzMask {
    let pattern = match stage {
        CostStage::Estimate => ESTIMATE_MASK,
        CostStage::Calculation => CALCULATION_MASK,
    };
    OzMask::parse(pattern).expect("Maske des Kosten-LV ist gültig")
}

/// A new Kosten-LV: a Vorbemerkung for the information DIN 276 asks for
/// (Kostenstand, Grundlagen, Verfahren, Quellen der Kennwerte, Umsatzsteuer),
/// one title per Kostengruppe of the 2nd level in `groups` (e.g. „420“) and –
/// in a Kostenberechnung – one subtitle per KG of the 3rd level below it.
/// Codes without a name in the KG-400 list are taken with an empty name.
pub fn cost_lv_template(stage: CostStage, title: &str, groups: &[&str], step: u32) -> Lv {
    let mask = cost_lv_mask(stage);
    let mut lv = Lv::new(title.trim(), mask.clone());
    lv.set_purpose(LvPurpose::Cost(stage));
    lv.set_position_step(step);
    let mut note = Note::rich(cost_lv_note(stage, "", ""));
    note.set_heading(format!("Angaben zur {}", stage.label()));
    note.set_preliminary(true);
    lv.push_note(note);
    let names = kg400_groups();
    for group in groups {
        let code = group.trim();
        if din_level(code) != Some(2) {
            continue;
        }
        let number: u32 = code.parse().unwrap_or(0);
        let name = names
            .iter()
            .find(|(known, _)| *known == code)
            .map(|(_, name)| *name)
            .unwrap_or("");
        let mut title = LvGroup::new(0, mask.format_group(&[number]), name);
        title.set_cost_group(Some(code.to_string()));
        lv.push_group(title);
        if stage == CostStage::Calculation {
            for (sub, sub_name) in kg400_subgroups(code) {
                let mut subtitle = LvGroup::new(
                    1,
                    mask.format_group(&[number, sub.parse().unwrap_or(0)]),
                    sub_name,
                );
                subtitle.set_cost_group(Some(sub.to_string()));
                lv.push_group(subtitle);
            }
        }
    }
    lv
}

/// Text of the Vorbemerkung of a Kosten-LV: what DIN 276 asks for
/// (Kostenstand, Grundlagen, Verfahren und Quellen der Kennwerte, Umsatzsteuer);
/// `stand` and `basis` fill in what is known („…“ otherwise).
pub fn cost_lv_note(stage: CostStage, stand: &str, basis: &str) -> RichText {
    let or_dots = |text: &str| match text.trim() {
        "" => "…".to_string(),
        text => text.to_string(),
    };
    RichText::plain(&format!(
        "{} nach DIN 276:2018-12, Gliederung bis zur {}. Ebene der Kostengliederung.\n\
         Kostenstand: {}\n\
         Grundlagen (Planungsunterlagen, Stand): {}\n\
         Verfahren und Quellen der Kostenkennwerte: …\n\
         Alle Beträge netto ohne Umsatzsteuer.",
        stage.label(),
        stage.level(),
        or_dots(stand),
        or_dots(basis)
    ))
}

/// „Kostenberechnung daraus“: a copy of a Kosten-LV as Kostenberechnung –
/// the positions stay, their KG is refined afterwards (LV prüfen names the
/// positions whose KG is not yet on the 3rd level). The title gets
/// „Kostenberechnung“ for „Kostenschätzung“; the LV number is the caller's.
pub fn calculation_from(lv: &Lv) -> Lv {
    let mut copy = lv.clone();
    // Datenpunkt-Abschnitte become Kostenberechnungen (EP je DP-Art, split).
    copy.set_purpose(LvPurpose::Cost(CostStage::Calculation));
    let title = lv.title().trim();
    let new_title = if title.contains(CostStage::Estimate.label()) {
        title.replace(CostStage::Estimate.label(), CostStage::Calculation.label())
    } else if title.is_empty() {
        CostStage::Calculation.label().to_string()
    } else {
        format!("{} – {title}", CostStage::Calculation.label())
    };
    copy.set_title(new_title);
    // A new document: nothing of the copy was sent anywhere.
    copy.set_request_exported(None);
    copy.set_sent_fingerprint(None);
    copy.set_sent_file(None);
    copy
}

/// Findings of „LV prüfen“ for a Kosten-LV: positions without Kostengruppe
/// and positions whose KG is coarser than the Stufe asks for (DIN 276, 4.3.3:
/// Kostenschätzung 2. Ebene, 4.3.4: Kostenberechnung 3. Ebene). Empty for a
/// tender LV.
pub fn cost_lv_findings(lv: &Lv) -> Vec<(RowId, String)> {
    let LvPurpose::Cost(stage) = lv.purpose() else {
        // Aufteilungszeilen do not belong into a tender LV.
        return crate::cost_split::split_findings(lv);
    };
    let effective = lv.effective_cost_groups();
    // The Rechengrundlage of Aufteilungszeilen carries no money: its KG
    // does not matter.
    let bases = lv.split_bases();
    let mut findings = Vec::new();
    for (row, effective) in lv.rows().iter().zip(effective) {
        let LvRow::Position(position) = row else {
            continue;
        };
        if !position.counts_in_total() || bases.contains(&position.id()) {
            continue;
        }
        let name = match position.ordinal().trim() {
            "" => position.short_text().trim().to_string(),
            ordinal => ordinal.to_string(),
        };
        match effective {
            None => findings.push((
                row.id(),
                format!(
                    "{name}: ohne Kostengruppe – in der {} fehlt der Betrag in der Zusammenstellung nach DIN 276.",
                    stage.label()
                ),
            )),
            Some((code, _)) => {
                if let Some(level) = din_level(&code).filter(|level| *level < stage.level()) {
                    findings.push((
                        row.id(),
                        format!(
                            "{name}: KG {code} ist die {level}. Ebene – die {} verlangt die {}. Ebene (DIN 276, {}).",
                            stage.label(),
                            stage.level(),
                            match stage {
                                CostStage::Estimate => "4.3.3",
                                CostStage::Calculation => "4.3.4",
                            }
                        ),
                    ));
                }
            }
        }
    }
    findings.extend(crate::cost_points::point_section_findings(lv));
    findings.extend(crate::cost_split::split_findings(lv));
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::Position;
    use rust_decimal::Decimal;

    #[test]
    fn levels_of_the_cost_groups() {
        assert_eq!(din_level("400"), Some(1));
        assert_eq!(din_level("420"), Some(2));
        assert_eq!(din_level("421"), Some(3));
        assert_eq!(din_level("489"), Some(3));
        assert_eq!(din_level("42"), None);
        assert_eq!(din_level("4210"), None);
        assert_eq!(din_level("A20"), None);
        assert_eq!(din_level("040"), None);
        assert_eq!(din_level("999"), None, "DIN 276 reicht bis KG 800");
        assert_eq!(din_level("810"), Some(2));
        let groups: Vec<&str> = kg400_groups().iter().map(|(code, _)| *code).collect();
        assert_eq!(
            groups,
            ["410", "420", "430", "440", "450", "460", "470", "480", "490"]
        );
        let below: Vec<&str> = kg400_subgroups("420")
            .iter()
            .map(|(code, _)| *code)
            .collect();
        assert_eq!(below, ["421", "422", "423", "424", "429"]);
        assert_eq!(din_group_label("400"), "Bauwerk – Technische Anlagen");
        assert_eq!(din_group_label("421"), "Wärmeerzeugungsanlagen");
        assert_eq!(din_group_label("999"), "");
    }

    #[test]
    fn estimate_template_has_titles_of_the_second_level() {
        let lv = cost_lv_template(
            CostStage::Estimate,
            "Kostenschätzung KG 400",
            &["420", "430", "999", "421"],
            10,
        );
        assert_eq!(lv.purpose(), LvPurpose::Cost(CostStage::Estimate));
        assert_eq!(lv.oz_mask().pattern(), ESTIMATE_MASK);
        let titles: Vec<(String, String, Option<String>)> = lv
            .rows()
            .iter()
            .filter_map(|row| match row {
                LvRow::Group(group) => Some((
                    group.ordinal().to_string(),
                    group.short_text().to_string(),
                    group.cost_group().map(str::to_string),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            titles,
            [
                (
                    "420".into(),
                    "Wärmeversorgungsanlagen".into(),
                    Some("420".into())
                ),
                (
                    "430".into(),
                    "Raumlufttechnische Anlagen".into(),
                    Some("430".into())
                ),
            ],
            "nur Kostengruppen der 2. Ebene"
        );
        assert!(matches!(&lv.rows()[0], LvRow::Note(note) if note.is_preliminary()));
    }

    #[test]
    fn calculation_template_has_subtitles_of_the_third_level() {
        let lv = cost_lv_template(CostStage::Calculation, "KB", &["420"], 10);
        assert_eq!(lv.oz_mask().pattern(), CALCULATION_MASK);
        let titles: Vec<(usize, String)> = lv
            .rows()
            .iter()
            .filter_map(|row| match row {
                LvRow::Group(group) => Some((group.level(), group.ordinal().to_string())),
                _ => None,
            })
            .collect();
        assert_eq!(
            titles,
            [
                (0, "420".to_string()),
                (1, "420.421".to_string()),
                (1, "420.422".to_string()),
                (1, "420.423".to_string()),
                (1, "420.424".to_string()),
                (1, "420.429".to_string()),
            ]
        );
    }

    #[test]
    fn findings_name_missing_and_coarse_cost_groups() {
        let mut lv = cost_lv_template(CostStage::Calculation, "KB", &["420"], 10);
        // Under 420 directly (before its first subtitle): only the 2nd level.
        lv.insert_row(
            2,
            LvRow::Position(Position::new(
                "420.0010",
                "Heizung pauschal",
                Decimal::ONE,
                "psch",
                Some(Decimal::from(1000)),
            )),
        );
        let mut fine = Position::new(
            "420.421.0010",
            "Wärmepumpe",
            Decimal::from(40),
            "kW",
            Some(Decimal::from(1500)),
        );
        fine.set_cost_group(Some("421".into()));
        lv.push_position(fine);
        let without = Position::new(
            "0010",
            "Sonstiges",
            Decimal::ONE,
            "psch",
            Some(Decimal::ONE),
        );
        let mut free = Lv::new("frei", cost_lv_mask(CostStage::Calculation));
        free.set_purpose(LvPurpose::Cost(CostStage::Calculation));
        free.push_position(without);
        let texts: Vec<String> = cost_lv_findings(&lv)
            .into_iter()
            .map(|(_, text)| text)
            .collect();
        assert_eq!(texts.len(), 1, "{texts:?}");
        assert!(texts[0].contains("KG 420 ist die 2. Ebene"), "{texts:?}");
        assert!(texts[0].contains("4.3.4"));
        let texts: Vec<String> = cost_lv_findings(&free)
            .into_iter()
            .map(|(_, text)| text)
            .collect();
        assert!(texts[0].contains("ohne Kostengruppe"), "{texts:?}");
        // A tender LV has none of these findings.
        let mut tender = lv.clone();
        tender.set_purpose(LvPurpose::Tender);
        assert!(cost_lv_findings(&tender).is_empty());
    }

    #[test]
    fn calculation_from_an_estimate_keeps_the_positions() {
        let mut estimate =
            cost_lv_template(CostStage::Estimate, "Kostenschätzung Heizung", &["420"], 10);
        estimate.push_position(Position::new(
            "420.0010",
            "Heizung",
            Decimal::ONE,
            "psch",
            Some(Decimal::from(1000)),
        ));
        estimate.set_sent_file(Some("x.X83".into()));
        let calculation = calculation_from(&estimate);
        assert_eq!(
            calculation.purpose(),
            LvPurpose::Cost(CostStage::Calculation)
        );
        assert_eq!(calculation.title(), "Kostenberechnung Heizung");
        assert_eq!(calculation.position_count(), 1);
        assert_eq!(calculation.sent_file(), None);
        // The KG 420 is too coarse now: LV prüfen says so.
        assert_eq!(cost_lv_findings(&calculation).len(), 1);
        let mut other = estimate.clone();
        other.set_title("Heizung");
        assert_eq!(
            calculation_from(&other).title(),
            "Kostenberechnung – Heizung"
        );
    }
}
