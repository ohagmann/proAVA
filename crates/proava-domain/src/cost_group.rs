//! Kostengruppen nach DIN 276: catalog of the LV, three-digit groups at
//! positions and titles (a title's group is the default for everything
//! below it) and the evaluation „Kosten nach Kostengruppen“.

use std::collections::{BTreeMap, HashMap};

use crate::bid::Bid;
use crate::lv::{Lv, LvRow, RowId};
use crate::money::Money;
use crate::position::Position;
use crate::title_default;

/// The catalog the cost groups refer to (GAEB Ctlg).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostCatalog {
    /// CtlgID in GAEB, e.g. "idDIN276-18".
    pub id: String,
    /// CtlgType, e.g. "cost group DIN 276 2018-12".
    pub kind: String,
    /// CtlgName, e.g. "DIN 276-18".
    pub name: String,
}

impl Default for CostCatalog {
    fn default() -> Self {
        Self {
            id: "idDIN276-18".into(),
            kind: "cost group DIN 276 2018-12".into(),
            name: "DIN 276-18".into(),
        }
    }
}

/// DIN 276 (2018-12), KG 400 Bauwerk – Technische Anlagen, 2nd and 3rd
/// level: suggestions for the field „Kostengruppe“ (free input allowed).
/// Wording as in Tabelle 1 of the norm (DIN-Taschenbuch 114, Beuth 2021,
/// public preview; see docs/FACHLICHES.md).
pub const DIN276_KG400: &[(&str, &str)] = &[
    ("410", "Abwasser-, Wasser-, Gasanlagen"),
    ("411", "Abwasseranlagen"),
    ("412", "Wasseranlagen"),
    ("413", "Gasanlagen"),
    ("419", "Sonstiges zur KG 410"),
    ("420", "Wärmeversorgungsanlagen"),
    ("421", "Wärmeerzeugungsanlagen"),
    ("422", "Wärmeverteilnetze"),
    ("423", "Raumheizflächen"),
    ("424", "Verkehrsheizflächen"),
    ("429", "Sonstiges zur KG 420"),
    ("430", "Raumlufttechnische Anlagen"),
    ("431", "Lüftungsanlagen"),
    ("432", "Teilklimaanlagen"),
    ("433", "Klimaanlagen"),
    ("434", "Kälteanlagen"),
    ("439", "Sonstiges zur KG 430"),
    ("440", "Elektrische Anlagen"),
    ("441", "Hoch- und Mittelspannungsanlagen"),
    ("442", "Eigenstromversorgungsanlagen"),
    ("443", "Niederspannungsschaltanlagen"),
    ("444", "Niederspannungsinstallationsanlagen"),
    ("445", "Beleuchtungsanlagen"),
    ("446", "Blitzschutz- und Erdungsanlagen"),
    ("447", "Fahrleitungssysteme"),
    ("449", "Sonstiges zur KG 440"),
    (
        "450",
        "Kommunikations-, sicherheits- und informationstechnische Anlagen",
    ),
    ("451", "Telekommunikationsanlagen"),
    ("452", "Such- und Signalanlagen"),
    ("453", "Zeitdienstanlagen"),
    ("454", "Elektroakustische Anlagen"),
    ("455", "Audiovisuelle Medien- und Antennenanlagen"),
    ("456", "Gefahrenmelde- und Alarmanlagen"),
    ("457", "Datenübertragungsnetze"),
    ("458", "Verkehrsbeeinflussungsanlagen"),
    ("459", "Sonstiges zur KG 450"),
    ("460", "Förderanlagen"),
    ("461", "Aufzugsanlagen"),
    ("462", "Fahrtreppen, Fahrsteige"),
    ("463", "Befahranlagen"),
    ("464", "Transportanlagen"),
    ("465", "Krananlagen"),
    ("466", "Hydraulikanlagen"),
    ("469", "Sonstiges zur KG 460"),
    (
        "470",
        "Nutzungsspezifische und verfahrenstechnische Anlagen",
    ),
    ("471", "Küchentechnische Anlagen"),
    ("472", "Wäscherei-, Reinigungs- und badetechnische Anlagen"),
    (
        "473",
        "Medienversorgungsanlagen, Medizin- und labortechnische Anlagen",
    ),
    ("474", "Feuerlöschanlagen"),
    ("475", "Prozesswärme-, kälte- und -luftanlagen"),
    ("476", "Weitere nutzungsspezifische Anlagen"),
    (
        "477",
        "Verfahrenstechnische Anlagen, Wasser, Abwasser und Gase",
    ),
    (
        "478",
        "Verfahrenstechnische Anlagen, Feststoffe, Wertstoffe und Abfälle",
    ),
    ("479", "Sonstiges zur KG 470"),
    ("480", "Gebäude- und Anlagenautomation"),
    ("481", "Automationseinrichtungen"),
    ("482", "Schaltschränke, Automationsschwerpunkte"),
    ("483", "Automationsmanagement"),
    ("484", "Kabel, Leitungen und Verlegesysteme"),
    ("485", "Datenübertragungsnetze"),
    ("489", "Sonstiges zur KG 480"),
    ("490", "Sonstige Maßnahmen für technische Anlagen"),
    ("491", "Baustelleneinrichtung"),
    ("492", "Gerüste"),
    ("493", "Sicherungsmaßnahmen"),
    ("494", "Abbruchmaßnahmen"),
    ("495", "Instandsetzungen"),
    ("496", "Materialentsorgung"),
    ("497", "Zusätzliche Maßnahmen"),
    ("498", "Provisorische technische Anlagen"),
    ("499", "Sonstiges zur KG 490"),
];

/// Name of a cost group from the list, empty when unknown.
pub fn cost_group_label(code: &str) -> &'static str {
    DIN276_KG400
        .iter()
        .find(|(known, _)| *known == code.trim())
        .map(|(_, label)| *label)
        .unwrap_or("")
}

/// Checks an input: empty = no cost group; otherwise three digits.
pub fn parse_cost_group(text: &str) -> Result<Option<String>, String> {
    let code = text.trim();
    if code.is_empty() {
        return Ok(None);
    }
    // "481 Automationseinrichtungen" from the suggestion list is fine too.
    let code = code.split_whitespace().next().unwrap_or(code);
    if code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_digit()) && !code.starts_with('0') {
        Ok(Some(code.to_string()))
    } else {
        Err(format!(
            "„{}“ ist keine Kostengruppe – bitte drei Ziffern nach DIN 276, z. B. 481.",
            text.trim()
        ))
    }
}

/// Own cost group of a title or position.
fn own_cost_group(row: &LvRow) -> Option<&str> {
    match row {
        LvRow::Position(position) => position.cost_group(),
        LvRow::Group(group) => group.cost_group(),
        _ => None,
    }
}

/// Question before a title's cost group changes over finished positions
/// (`count` from [`Lv::locked_cost_group_heirs`]).
pub fn cost_group_heirs_question(count: usize, code: Option<&str>) -> String {
    let (subject, verb_new, verb_lose) = if count == 1 {
        ("1 fertige Position".to_string(), "erbt", "verliert")
    } else {
        (format!("{count} fertige Positionen"), "erben", "verlieren")
    };
    match code {
        Some(code) => format!("{subject} {verb_new} die neue KG {code}."),
        None => format!("{subject} {verb_lose} die KG des Titels."),
    }
}

impl Lv {
    /// Cost group that applies to a row: its own, else that of the nearest
    /// title above it (titles: their own, else their parent's). `true` =
    /// inherited.
    pub fn effective_cost_group(&self, index: usize) -> Option<(String, bool)> {
        title_default::effective_value(self, index, own_cost_group)
    }

    /// Index of the title a row inherits its cost group from (None: own
    /// group or none at all) – for „vom Titel 01.481“.
    pub fn cost_group_source(&self, index: usize) -> Option<usize> {
        title_default::inherited_from(self, index, own_cost_group)
    }

    /// [`Lv::effective_cost_group`] of every row in one pass.
    pub fn effective_cost_groups(&self) -> Vec<Option<(String, bool)>> {
        title_default::effective_values(self, own_cost_group, &HashMap::new())
    }

    /// Finished positions whose cost group would change if the titles at
    /// `titles` got `code`: they have none of their own and inherit it.
    /// Rows that are no titles are ignored.
    pub fn locked_cost_group_heirs(&self, titles: &[usize], code: Option<&str>) -> Vec<RowId> {
        title_default::locked_heirs(self, titles, code, own_cost_group)
    }

    /// Whether any row carries a cost group.
    pub fn has_cost_groups(&self) -> bool {
        self.rows().iter().any(|row| match row {
            LvRow::Position(position) => position.cost_group().is_some(),
            LvRow::Group(group) => group.cost_group().is_some(),
            _ => false,
        })
    }
}

/// One line of „Kosten nach Kostengruppen“.
#[derive(Debug, Clone, PartialEq)]
pub struct CostGroupLine {
    /// Three digits, or "480" for a sum line; None = ohne Kostengruppe.
    pub code: Option<String>,
    pub label: String,
    /// Sum of the KG x80 group (e.g. 481–489) instead of a single group.
    pub subtotal: bool,
    pub positions: usize,
    /// Planner's prices (Kostenvoranschlag), counted positions only.
    pub planner: Money,
    /// One per bid in [`CostGroupReport::bidders`], before discount.
    pub bidders: Vec<Money>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CostGroupReport {
    /// Names of the bids (excluded bids left out).
    pub bidders: Vec<String>,
    pub lines: Vec<CostGroupLine>,
    pub planner_total: Money,
    pub bidder_totals: Vec<Money>,
}

/// Sums per cost group (inherited from titles), with a sum line per group of
/// ten (481–489 → 480) when it holds more than one group, and a line for
/// positions without cost group. Bid sums are net before discount.
pub fn cost_group_report(lv: &Lv, bids: &[Bid]) -> CostGroupReport {
    let bids: Vec<&Bid> = bids.iter().filter(|bid| !bid.is_excluded()).collect();
    let mut sums: BTreeMap<Option<String>, (usize, Money, Vec<Money>)> = BTreeMap::new();
    let effective = lv.effective_cost_groups();
    // Counted totals per row, Zuschläge included (planner and each bidder).
    let planner = lv.row_totals_with(Position::counted_total, Position::unit_price);
    let per_bid: Vec<Vec<Money>> = bids
        .iter()
        .map(|bid| {
            lv.row_totals_with(
                |position| bid.counted_total_for(position),
                |position| bid.unit_price(position.id()),
            )
        })
        .collect();
    // The Rechengrundlage of Aufteilungszeilen: its amount is in them.
    let bases = lv.split_bases();
    for (index, (row, effective)) in lv.rows().iter().zip(effective).enumerate() {
        let LvRow::Position(position) = row else {
            continue;
        };
        if bases.contains(&position.id()) {
            continue;
        }
        let code = effective.map(|(code, _)| code);
        let entry = sums
            .entry(code)
            .or_insert_with(|| (0, Money::ZERO, vec![Money::ZERO; bids.len()]));
        entry.0 += 1;
        entry.1 += planner[index];
        for (sum, totals) in entry.2.iter_mut().zip(&per_bid) {
            *sum += totals[index];
        }
    }
    // Datenpunkt-Abschnitte of a Kosten-LV: their amounts per Kostengruppe
    // (KS: 480, KB: 481–489), planner's prices only (a Kosten-LV has no bids).
    for row in lv.rows() {
        let Some(points) = row.as_group().and_then(|group| group.points()) else {
            continue;
        };
        for line in points.result().groups {
            if line.amount.is_zero() {
                continue;
            }
            sums.entry(Some(line.code))
                .or_insert_with(|| (0, Money::ZERO, vec![Money::ZERO; bids.len()]))
                .1 += line.amount;
        }
    }
    let mut lines = Vec::new();
    let mut without = None;
    let mut groups: BTreeMap<String, Vec<CostGroupLine>> = BTreeMap::new();
    for (code, (positions, planner, bidders)) in sums {
        let line = CostGroupLine {
            label: code
                .as_deref()
                .map(cost_group_label)
                .unwrap_or("ohne Kostengruppe")
                .to_string(),
            code: code.clone(),
            subtotal: false,
            positions,
            planner,
            bidders,
        };
        match code {
            None => without = Some(line),
            // `get`: codes from a foreign project or GAEB file are not
            // checked – „4“ or „€“ would end mid-character or before byte 2.
            Some(code) => groups
                .entry(format!("{}0", code.get(..2).unwrap_or(code.as_str())))
                .or_default()
                .push(line),
        }
    }
    for (group, members) in groups {
        if members.len() > 1 {
            let label = match cost_group_label(&group) {
                "" => format!("KG {group}"),
                known => known.to_string(),
            };
            let mut sum = CostGroupLine {
                label,
                code: Some(group.clone()),
                subtotal: true,
                positions: 0,
                planner: Money::ZERO,
                bidders: vec![Money::ZERO; bids.len()],
            };
            for member in &members {
                sum.positions += member.positions;
                sum.planner += member.planner;
                for (total, value) in sum.bidders.iter_mut().zip(&member.bidders) {
                    *total += *value;
                }
            }
            lines.extend(members);
            lines.push(sum);
        } else {
            lines.extend(members);
        }
    }
    lines.extend(without);
    let detail = lines.iter().filter(|line| !line.subtotal);
    let mut report = CostGroupReport {
        bidders: bids.iter().map(|bid| bid.name().to_string()).collect(),
        planner_total: Money::ZERO,
        bidder_totals: vec![Money::ZERO; bids.len()],
        lines: Vec::new(),
    };
    for line in detail {
        report.planner_total += line.planner;
        for (total, value) in report.bidder_totals.iter_mut().zip(&line.bidders) {
            *total += *value;
        }
    }
    report.lines = lines;
    report
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;
    use crate::{Bid, BidPrice, LvGroup, OzMask, Position};

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        let mut automation = LvGroup::new(0, "", "Automation");
        automation.set_cost_group(Some("481".into()));
        lv.push_group(automation);
        lv.push_group(LvGroup::new(1, "", "ASP01"));
        lv.push_position(Position::new(
            "",
            "DDC",
            Decimal::ONE,
            "St",
            Some(Decimal::from(1000)),
        ));
        let mut cabinet =
            Position::new("", "Schrank", Decimal::ONE, "St", Some(Decimal::from(500)));
        cabinet.set_cost_group(Some("482".into()));
        lv.push_position(cabinet);
        lv.push_group(LvGroup::new(0, "", "Feld"));
        lv.push_position(Position::new(
            "",
            "Fühler",
            Decimal::TEN,
            "St",
            Some(Decimal::from(20)),
        ));
        lv.renumber();
        lv
    }

    #[test]
    fn inputs_and_labels() {
        assert_eq!(parse_cost_group(" 481 "), Ok(Some("481".into())));
        assert_eq!(
            parse_cost_group("481 Automationseinrichtungen"),
            Ok(Some("481".into()))
        );
        assert_eq!(parse_cost_group(""), Ok(None));
        assert!(parse_cost_group("48").is_err());
        assert!(parse_cost_group("4811").is_err());
        assert!(parse_cost_group("4a1").is_err());
        assert_eq!(
            cost_group_label("484"),
            "Kabel, Leitungen und Verlegesysteme"
        );
        assert_eq!(cost_group_label("999"), "");
    }

    #[test]
    fn titles_pass_their_group_down() {
        let lv = lv();
        assert_eq!(lv.effective_cost_group(0), Some(("481".into(), false)));
        assert_eq!(lv.effective_cost_group(1), Some(("481".into(), true)));
        assert_eq!(lv.effective_cost_group(2), Some(("481".into(), true)));
        assert_eq!(lv.effective_cost_group(3), Some(("482".into(), false)));
        assert_eq!(lv.effective_cost_group(5), None);
        assert!(lv.has_cost_groups());
    }

    /// Gefunden beim Erweitern der Eigenschaftstests (18.11.): Kostengruppen
    /// aus einer fremden Projekt- oder GAEB-Datei werden nicht geprüft; „4“
    /// oder „€1“ ließen „Kosten nach Kostengruppen“ (und die Kostenkontrolle)
    /// abstürzen – `&code[..2]`.
    #[test]
    fn odd_codes_from_files_do_not_panic() {
        let mut lv = lv();
        let codes = ["4", "€1", "▓", "48", "4810"];
        let ids: Vec<RowId> = lv.positions().map(|position| position.id()).collect();
        for (id, code) in ids.iter().zip(codes) {
            let index = lv.index_of(*id).unwrap();
            if let Some(LvRow::Position(position)) = lv.row_mut(index) {
                position.set_cost_group(Some(code.into()));
            }
        }
        let report = cost_group_report(&lv, &[]);
        assert!(!report.lines.is_empty());
    }

    #[test]
    fn one_pass_matches_single_rows() {
        let mut lv = lv();
        // A subtitle with its own group and a note below a title.
        lv.push_group(LvGroup::new(0, "", "Schaltschränke"));
        lv.push_group(LvGroup::new(1, "", "ASP02"));
        let index = lv.rows().len() - 1;
        if let Some(LvRow::Group(group)) = lv.row_mut(index) {
            group.set_cost_group(Some("482".into()));
        }
        lv.push_position(Position::new("", "Schrank", Decimal::ONE, "St", None));
        lv.push_group(LvGroup::new(1, "", "ASP03"));
        lv.push_position(Position::new("", "Schrank", Decimal::ONE, "St", None));
        lv.renumber();
        let all = lv.effective_cost_groups();
        for (index, effective) in all.iter().enumerate() {
            assert_eq!(*effective, lv.effective_cost_group(index), "Zeile {index}");
        }
    }

    #[test]
    fn finished_heirs_of_a_title() {
        let mut lv = lv();
        let ids: Vec<RowId> = lv.positions().map(|position| position.id()).collect();
        // DDC (inherits 481 from „Automation“) and Schrank (own 482) finished.
        lv.set_status(&ids[..2], crate::EditStatus::Done);
        // Title „Automation“ at 0, subtitle „ASP01“ at 1, title „Feld“ at 4.
        assert_eq!(lv.locked_cost_group_heirs(&[0], Some("484")), vec![ids[0]]);
        assert_eq!(lv.locked_cost_group_heirs(&[0], None), vec![ids[0]]);
        assert_eq!(lv.locked_cost_group_heirs(&[1], Some("484")), vec![ids[0]]);
        // Same group as before: nothing changes.
        assert!(lv.locked_cost_group_heirs(&[0], Some("481")).is_empty());
        assert!(lv.locked_cost_group_heirs(&[1], Some("481")).is_empty());
        // „ASP01“ keeps nothing of its own: clearing it changes nothing.
        assert!(lv.locked_cost_group_heirs(&[1], None).is_empty());
        // No finished position below „Feld“; positions are no titles.
        assert!(lv.locked_cost_group_heirs(&[4], Some("484")).is_empty());
        assert!(lv.locked_cost_group_heirs(&[2], Some("484")).is_empty());
        // Both titles at once count each position once.
        assert_eq!(
            lv.locked_cost_group_heirs(&[0, 1], Some("484")),
            vec![ids[0]]
        );
    }

    #[test]
    fn report_sums_per_group_with_bidders() {
        let lv = lv();
        let mut bid = Bid::new(crate::Address::named("Bieter A"), "a.x84");
        for position in lv.positions() {
            bid.set_price(
                position.id(),
                BidPrice {
                    unit_price: Some(Decimal::from(10)),
                    ..BidPrice::default()
                },
            );
        }
        let mut excluded = Bid::new(crate::Address::named("Raus"), "b.x84");
        excluded.set_excluded(true);
        let report = cost_group_report(&lv, &[bid, excluded]);
        assert_eq!(report.bidders, vec!["Bieter A".to_string()]);
        let codes: Vec<_> = report
            .lines
            .iter()
            .map(|line| (line.code.clone(), line.subtotal))
            .collect();
        assert_eq!(
            codes,
            vec![
                (Some("481".into()), false),
                (Some("482".into()), false),
                (Some("480".into()), true),
                (None, false)
            ]
        );
        assert_eq!(report.lines[0].planner, Money::from_cents(100_000));
        assert_eq!(report.lines[2].planner, Money::from_cents(150_000));
        assert_eq!(report.lines[3].planner, Money::from_cents(20_000));
        assert_eq!(report.planner_total, Money::from_cents(170_000));
        assert_eq!(report.lines[3].bidders[0], Money::from_cents(10_000));
        assert_eq!(report.bidder_totals[0], Money::from_cents(12_000));
    }
}
