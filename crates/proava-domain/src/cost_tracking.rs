//! Kostenverfolgung nach DIN 276:2018-12 (4.4–4.6): one table over all LVs
//! and Kostenermittlungen of the project, per Kostengruppe (1st to 3rd
//! level) or per Vergabeeinheit (LV), from the budget to the
//! Kostenfeststellung. Everything is computed from the project data; stored
//! are only the budget (Kostenvorgabe), the notes per KG, the bid chosen per
//! LV before the award and the recorded states (Kostenstände).
//!
//! The chain per KG:
//! - **Budget** (Kostenvorgabe, 4.6): entered per KG or taken over from a
//!   Kostenermittlung. A group without its own budget sums its members.
//! - **Kostenschätzung, Kostenberechnung** (4.3.3/4.3.4): the parts of the
//!   stage ([`stage_parts`]): today the GA data-point estimates, the chosen
//!   one or the newest; Kosten-LVs (Kurztext) plug in as further parts.
//! - **Kostenvoranschlag** (4.3.5): planner's prices of the LVs.
//! - **Angebot, Auftrag, Abrechnung** (Kostenanschlag, 4.3.6): the awarded,
//!   chosen or lowest bid; contract with approved change orders (discount
//!   shared out over the KG); checked performance after discount.
//! - **Prognose** (4.2.13): per LV the Kostenfeststellung, else the contract
//!   with the open change orders that count in the forecast and with what
//!   is billed beyond the ordered quantities, else the bid, else the planner's
//!   prices; Kostengruppen of no LV from the Kostenberechnung, else the
//!   Kostenschätzung.
//! - **Kostenfeststellung** (4.3.7): the final invoices.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rust_decimal::Decimal;

use crate::bid::{Bid, BidId};
use crate::billing::InvoiceKind;
use crate::billing_check::{BillingContext, InvoiceCheck};
use crate::change_order::{billable_items, ChangeOrder, ChangeOrderStatus};
use crate::contract::Contract;
use crate::cost_estimate::CostStage;
use crate::cost_group::{cost_group_label, cost_group_report};
use crate::cost_sums::{CostSource, CostSourceRef};
use crate::german_date::GermanDate;
use crate::lv::{Lv, LvRow, RowId};
use crate::money::Money;
use crate::project::Project;

#[cfg(test)]
mod tests;

/// Columns of the Kostenverfolgung, in their order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrackStage {
    Budget,
    Estimate,
    Calculation,
    /// Kostenvoranschlag: planner's prices of the LVs.
    PreTender,
    Offer,
    /// Auftrag with the approved change orders.
    Contract,
    Forecast,
    /// Abrechnung: checked performance.
    Billed,
    /// Kostenfeststellung: final invoices.
    Final,
}

/// Number of columns.
pub const STAGES: usize = 9;

impl TrackStage {
    pub const ALL: [TrackStage; STAGES] = [
        TrackStage::Budget,
        TrackStage::Estimate,
        TrackStage::Calculation,
        TrackStage::PreTender,
        TrackStage::Offer,
        TrackStage::Contract,
        TrackStage::Forecast,
        TrackStage::Billed,
        TrackStage::Final,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            TrackStage::Budget => "Kostenvorgabe",
            TrackStage::Estimate => "Kostenschätzung",
            TrackStage::Calculation => "Kostenberechnung",
            TrackStage::PreTender => "Kostenvoranschlag",
            TrackStage::Offer => "Angebot",
            TrackStage::Contract => "Auftrag",
            TrackStage::Forecast => "Prognose",
            TrackStage::Billed => "Abrechnung",
            TrackStage::Final => "Kostenfeststellung",
        }
    }

    /// What the column holds (tooltips, legend of the printout).
    pub fn explanation(self) -> &'static str {
        match self {
            TrackStage::Budget => "Budget bzw. Kostenvorgabe des Bauherrn (DIN 276, 4.6): je KG eingegeben oder aus einer Kostenermittlung übernommen; eine Gruppe ohne eigene Kostenvorgabe summiert ihre Untergruppen",
            TrackStage::Estimate => "Kostenschätzung nach DIN 276, 4.3.3 (2. Ebene)",
            TrackStage::Calculation => "Kostenberechnung nach DIN 276, 4.3.4 (3. Ebene)",
            TrackStage::PreTender => "Kostenvoranschlag nach DIN 276, 4.3.5: Planerpreise der Leistungsverzeichnisse",
            TrackStage::Offer => "Kostenanschlag, 1. Schritt (DIN 276, 4.3.6): beauftragtes, gewähltes oder günstigstes Angebot, netto nach Nachlass",
            TrackStage::Contract => "Kostenanschlag, 2. Schritt: Auftragssumme und beauftragte Nachträge, Nachlass anteilig auf die KG",
            TrackStage::Forecast => "Prognose (DIN 276, 4.2.13): Kostenfeststellung, sonst Auftrag mit offenen Nachträgen in der Prognose und Mehrmengen der Abrechnung, sonst Angebot oder Kostenvoranschlag; nicht vergebene KG aus der Kostenberechnung",
            TrackStage::Billed => "Kostenanschlag, 3. Schritt: geprüfte Leistung kumuliert, nach Nachlass, vor Abzügen und Einbehalt",
            TrackStage::Final => "Kostenfeststellung nach DIN 276, 4.3.7: geprüfte Schlussrechnungen",
        }
    }

    /// Key for CSV and the MCP server.
    pub fn key(self) -> &'static str {
        match self {
            TrackStage::Budget => "budget",
            TrackStage::Estimate => "kostenschaetzung",
            TrackStage::Calculation => "kostenberechnung",
            TrackStage::PreTender => "kostenvoranschlag",
            TrackStage::Offer => "angebot",
            TrackStage::Contract => "auftrag",
            TrackStage::Forecast => "prognose",
            TrackStage::Billed => "abrechnung",
            TrackStage::Final => "kostenfeststellung",
        }
    }

    /// Earlier stages, nearest first, compared per Vergabeeinheit: each LV
    /// (and the Kostenermittlungen without LV) with the first of them it has
    /// a value for – so a stage that covers only some LVs is not set against
    /// all LVs of the stage before.
    pub fn previous(self) -> &'static [TrackStage] {
        use TrackStage::*;
        match self {
            Budget | Estimate | Calculation | PreTender => &[],
            Offer => &[PreTender],
            Contract => &[Offer, PreTender],
            Forecast => &[Contract, Offer, PreTender, Calculation, Estimate],
            Billed | Final => &[Contract],
        }
    }

    /// Earlier stages compared over the whole row (KG), nearest first: the
    /// Kostenermittlungen have no LV; a stage without an LV-wise base falls
    /// back to them (Kostenanschlag against Kostenberechnung).
    pub fn previous_planning(self) -> &'static [TrackStage] {
        use TrackStage::*;
        match self {
            Budget | Estimate | Billed | Final => &[],
            Calculation => &[Estimate],
            PreTender | Offer | Contract | Forecast => &[Calculation, Estimate],
        }
    }
}

/// How the budget binds (DIN 276, 4.6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BudgetKind {
    /// Obergrenze: must not be exceeded.
    #[default]
    Ceiling,
    /// Zielgröße: to be reached.
    Target,
}

impl BudgetKind {
    pub const ALL: [BudgetKind; 2] = [BudgetKind::Ceiling, BudgetKind::Target];

    pub fn label(self) -> &'static str {
        match self {
            BudgetKind::Ceiling => "Obergrenze",
            BudgetKind::Target => "Zielgröße",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            BudgetKind::Ceiling => "obergrenze",
            BudgetKind::Target => "zielgroesse",
        }
    }

    pub fn from_key(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "obergrenze" | "ceiling" => Some(BudgetKind::Ceiling),
            "zielgroesse" | "zielgröße" | "ziel" | "target" => Some(BudgetKind::Target),
            _ => None,
        }
    }
}

/// The bid that counts as „Angebot“ of an LV before the award, with the
/// reason (e.g. „wirtschaftlichstes Angebot nach Wertung“).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OfferChoice {
    pub bid: BidId,
    pub reason: String,
}

/// One cell of a recorded state: what one LV (or none) has in one KG.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SnapshotCell {
    /// „LV 2 Lüftung“; None = no LV (budget, Kostenermittlungen).
    pub lv: Option<String>,
    /// Kostengruppe as stored at the position; None = ohne KG.
    pub code: Option<String>,
    pub net: [Option<Money>; STAGES],
    pub gross: [Option<Money>; STAGES],
}

/// A recorded Kostenstand (DIN 276, 4.4.3): the chain at one day, kept to
/// compare later.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CostSnapshot {
    /// TT.MM.JJJJ.
    pub date: String,
    /// Why it was recorded, e.g. „Kostenberechnung LPH 3 abgestimmt“.
    pub note: String,
    pub cells: Vec<SnapshotCell>,
}

impl CostSnapshot {
    /// „18.11.2026 – Kostenberechnung abgestimmt“
    pub fn label(&self) -> String {
        if self.note.trim().is_empty() {
            self.date.clone()
        } else {
            format!("{} – {}", self.date, self.note.trim())
        }
    }
}

/// What the Kostenverfolgung stores in the project.
#[derive(Debug, Clone, PartialEq)]
pub struct CostTracking {
    /// Budget per KG, net. A code of any level; a group without its own
    /// budget sums its members.
    pub budget: BTreeMap<String, Money>,
    pub budget_kind: BudgetKind,
    /// Stand of the budget (TT.MM.JJJJ) and where it comes from.
    pub budget_date: String,
    pub budget_note: String,
    /// Erläuterung/Maßnahme per KG (DIN 276, 4.4.3 and 4.5).
    pub notes: BTreeMap<String, String>,
    /// Kostenermittlungen that count other than by default (key of
    /// [`source_key`] → counts): by default every Kosten-LV counts (each
    /// Gewerk adds its part) and of the GA-Datenpunktrechnungen the newest
    /// per stage.
    pub source_choice: BTreeMap<String, bool>,
    /// A stage more than this many percent above its previous stage is marked.
    pub threshold_percent: Decimal,
    pub snapshots: Vec<CostSnapshot>,
}

impl Default for CostTracking {
    fn default() -> Self {
        Self {
            budget: BTreeMap::new(),
            budget_kind: BudgetKind::default(),
            budget_date: String::new(),
            budget_note: String::new(),
            notes: BTreeMap::new(),
            source_choice: BTreeMap::new(),
            threshold_percent: Decimal::from(10),
            snapshots: Vec::new(),
        }
    }
}

impl CostTracking {
    /// Nothing entered (the project file leaves the element out).
    pub fn is_empty(&self) -> bool {
        *self == CostTracking::default()
    }

    /// Replaces the budget with sums per KG (e.g. of a Kostenberechnung).
    pub fn take_budget(&mut self, sums: &BTreeMap<String, Money>, source: &str, date: &str) {
        self.budget = sums
            .iter()
            .filter(|(_, amount)| !amount.is_zero())
            .map(|(code, amount)| (code.clone(), *amount))
            .collect();
        self.budget_note = format!("aus {source}");
        self.budget_date = date.to_string();
    }

    /// Sets the budget of one KG; zero removes it.
    pub fn set_budget(&mut self, code: &str, amount: Money) {
        if amount.is_zero() {
            self.budget.remove(code);
        } else {
            self.budget.insert(code.to_string(), amount);
        }
    }

    /// Sets the note of one KG; blank text removes it.
    pub fn set_note(&mut self, code: &str, text: &str) {
        if text.trim().is_empty() {
            self.notes.remove(code);
        } else {
            self.notes.insert(code.to_string(), text.to_string());
        }
    }

    /// Budget of a KG: its own, else the sum of its members one level down
    /// (recursively). None when neither has one.
    pub fn budget_of(&self, code: &str) -> Option<Money> {
        self.budget_with(code, &|_, amount| amount)
    }

    /// [`Self::budget_of`] with every own amount converted first (brutto
    /// with the rate of its KG).
    pub fn budget_with(&self, code: &str, convert: &dyn Fn(&str, Money) -> Money) -> Option<Money> {
        if let Some(amount) = self.budget.get(code) {
            return Some(convert(code, *amount));
        }
        let level = code_level(code);
        if level >= 3 || !is_standard(code) {
            return None;
        }
        let members: BTreeSet<String> = self
            .budget
            .keys()
            .filter(|other| other.as_str() != code && descends_from(other, code))
            .filter_map(|other| at_level(other, level + 1))
            .collect();
        let sums: Vec<Money> = members
            .iter()
            .filter_map(|member| self.budget_with(member, convert))
            .collect();
        (!sums.is_empty()).then(|| sums.into_iter().sum())
    }

    /// Budget of the whole project: the 1st-level groups (with their rule)
    /// and the codes outside DIN 276.
    pub fn budget_total(&self) -> Option<Money> {
        self.budget_total_with(&|_, amount| amount)
    }

    /// [`Self::budget_total`] with every own amount converted first.
    pub fn budget_total_with(&self, convert: &dyn Fn(&str, Money) -> Money) -> Option<Money> {
        if self.budget.is_empty() {
            return None;
        }
        let tops: BTreeSet<String> = self
            .budget
            .keys()
            .filter(|code| is_standard(code))
            .filter_map(|code| at_level(code, 1))
            .collect();
        let mut total: Money = tops
            .iter()
            .filter_map(|code| self.budget_with(code, convert))
            .sum();
        total += self
            .budget
            .iter()
            .filter(|(code, _)| !is_standard(code))
            .map(|(code, amount)| convert(code, *amount))
            .sum();
        Some(total)
    }

    /// Budgets of groups smaller than the sum of their members (hints).
    pub fn budget_conflicts(&self) -> Vec<String> {
        let mut conflicts = Vec::new();
        for (code, amount) in &self.budget {
            let level = code_level(code);
            if level >= 3 || !is_standard(code) {
                continue;
            }
            let members: BTreeSet<String> = self
                .budget
                .keys()
                .filter(|other| other.as_str() != code && descends_from(other, code))
                .filter_map(|other| at_level(other, level + 1))
                .collect();
            let sum: Money = members
                .iter()
                .filter_map(|member| self.budget_of(member))
                .sum();
            if !members.is_empty() && sum > *amount {
                conflicts.push(format!(
                    "Kostenvorgabe KG {code} ({amount}) ist kleiner als die Summe ihrer Untergruppen ({sum})."
                ));
            }
        }
        conflicts
    }
}

// ---------------------------------------------------------------------------
// Kostengruppen: levels and names
// ---------------------------------------------------------------------------

/// DIN 276:2018-12, 1st level.
pub const DIN276_LEVEL1: &[(&str, &str)] = &[
    ("100", "Grundstück"),
    ("200", "Vorbereitende Maßnahmen"),
    ("300", "Bauwerk – Baukonstruktionen"),
    ("400", "Bauwerk – Technische Anlagen"),
    ("500", "Außenanlagen und Freiflächen"),
    ("600", "Ausstattung und Kunstwerke"),
    ("700", "Baunebenkosten"),
    ("800", "Finanzierung"),
];

/// Name of a KG of any level (empty when unknown).
pub fn group_label(code: &str) -> &'static str {
    DIN276_LEVEL1
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, label)| *label)
        .unwrap_or_else(|| cost_group_label(code))
}

/// Three digits: a code of the DIN 276 outline.
fn is_standard(code: &str) -> bool {
    code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_digit())
}

/// 1 for 400, 2 for 480, 3 for 481 (and codes outside DIN 276).
pub fn code_level(code: &str) -> u8 {
    if !is_standard(code) {
        3
    } else if code.ends_with("00") {
        1
    } else if code.ends_with('0') {
        2
    } else {
        3
    }
}

/// The group of `code` at `level` (481 at level 2 → 480); a code that is
/// coarser than `level` stays as it is. None for codes outside DIN 276
/// asked for a coarser level.
fn at_level(code: &str, level: u8) -> Option<String> {
    if !is_standard(code) {
        return (level >= 3).then(|| code.to_string());
    }
    if code_level(code) <= level {
        return Some(code.to_string());
    }
    Some(match level {
        1 => format!("{}00", &code[..1]),
        2 => format!("{}0", &code[..2]),
        _ => code.to_string(),
    })
}

/// Whether `code` lies in the group `group` (or is it).
fn descends_from(code: &str, group: &str) -> bool {
    if code == group {
        return true;
    }
    if !is_standard(code) || !is_standard(group) {
        return false;
    }
    match code_level(group) {
        1 => code[..1] == group[..1],
        2 => code[..2] == group[..2],
        _ => false,
    }
}

/// Whether two codes overlap (one contains the other).
fn overlap(a: &str, b: &str) -> bool {
    descends_from(a, b) || descends_from(b, a)
}

// ---------------------------------------------------------------------------
// Parts of the Kostenermittlungen (the narrow interface)
// ---------------------------------------------------------------------------

/// What one Kostenermittlung contributes to a stage, per KG (net).
#[derive(Debug, Clone, PartialEq)]
pub struct StagePart {
    /// Stable key for the choice ([`source_key`]).
    pub key: String,
    /// „Kostenberechnung LPH 3 (12.11.2026)“, „Kostenberechnung Heizung“
    pub name: String,
    /// „nach Positionen“ (Kosten-LV) or „nach Datenpunkten“ (GA).
    pub method: &'static str,
    pub stage: CostStage,
    /// TT.MM.JJJJ of a Datenpunktrechnung; empty for a Kosten-LV.
    pub date: String,
    /// Net per KG at the 3rd level (coarser sources keep their code); None =
    /// ohne Kostengruppe.
    pub sums: BTreeMap<Option<String>, Money>,
    /// Whether it counts in the Kostenverfolgung.
    pub counts: bool,
    /// Whether it counts by default (without a choice).
    pub by_default: bool,
    /// Umsatzsteuer for the gross view: a Kosten-LV its own, a
    /// Datenpunktrechnung the usual rate of the project ([`default_vat`]).
    pub vat_percent: Decimal,
}

impl StagePart {
    pub fn total(&self) -> Money {
        self.sums.values().copied().sum()
    }
}

/// Key of a Kostenermittlung for the choice: the id of a
/// Datenpunktrechnung, the name of a Kosten-LV (number and title – the index
/// changes when LVs move).
pub fn source_key(project: &Project, source: &CostSource) -> String {
    match source.source {
        CostSourceRef::Estimate(id) => format!("datenpunkte:{}", id.0),
        CostSourceRef::Lv(index) => format!(
            "lv:{}",
            project
                .lv_at(index)
                .map(|lv| lv_label(lv, index))
                .unwrap_or_default()
        ),
    }
}

/// Every Kostenermittlung of the project with whether it counts: by
/// default every Kosten-LV (each Gewerk adds its part) and the newest
/// Datenpunktrechnung per stage (by date, the later one on the same day);
/// the choice in [`CostTracking::source_choice`] overrides that. Kosten-LVs
/// and Datenpunktrechnungen come from [`Project::cost_sources`].
pub fn stage_parts(project: &Project) -> Vec<StagePart> {
    let sources = project.cost_sources(3);
    let newest: Vec<Option<usize>> = CostStage::ALL
        .iter()
        .map(|stage| {
            sources
                .iter()
                .enumerate()
                .filter(|(_, source)| {
                    source.stage == *stage && matches!(source.source, CostSourceRef::Estimate(_))
                })
                .max_by_key(|(index, source)| (GermanDate::parse(&source.date).ok(), *index))
                .map(|(index, _)| index)
        })
        .collect();
    let choice = &project.cost_tracking().source_choice;
    let usual = default_vat(project);
    sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let key = source_key(project, source);
            let by_default = match source.source {
                CostSourceRef::Lv(_) => true,
                CostSourceRef::Estimate(_) => newest.contains(&Some(index)),
            };
            StagePart {
                counts: choice.get(&key).copied().unwrap_or(by_default),
                by_default,
                name: source.title.clone(),
                method: match source.source {
                    CostSourceRef::Lv(_) => "nach Positionen",
                    CostSourceRef::Estimate(_) => "nach Datenpunkten",
                },
                stage: source.stage,
                date: source.date.clone(),
                vat_percent: match source.source {
                    CostSourceRef::Lv(index) => {
                        project.lv_at(index).map(Lv::vat_percent).unwrap_or(usual)
                    }
                    CostSourceRef::Estimate(_) => usual,
                },
                sums: source
                    .sums
                    .iter()
                    .filter(|(_, amount)| !amount.is_zero())
                    .map(|(code, amount)| ((!code.is_empty()).then(|| code.clone()), *amount))
                    .collect(),
                key,
            }
        })
        .collect()
}

/// Kostengruppen that two parts of a stage both hold (one warning each).
fn double_groups(stage: CostStage, parts: &[&StagePart]) -> Vec<String> {
    let mut warnings = Vec::new();
    for (index, first) in parts.iter().enumerate() {
        for second in &parts[index + 1..] {
            let doubled: BTreeSet<&str> = first
                .sums
                .keys()
                .flatten()
                .filter(|code| {
                    second
                        .sums
                        .keys()
                        .flatten()
                        .any(|other| overlap(code, other))
                })
                .map(String::as_str)
                .collect();
            if !doubled.is_empty() {
                warnings.push(format!(
                    "{}: KG {} doppelt erfasst – in „{}“ ({}) und „{}“ ({}); im Reiter Kostenverfolgung unter „Kostenermittlungen“ abwählen, was nicht gilt.",
                    stage.label(),
                    doubled.into_iter().collect::<Vec<_>>().join(", "),
                    first.name,
                    first.method,
                    second.name,
                    second.method
                ));
            }
        }
    }
    warnings
}

// ---------------------------------------------------------------------------
// The chain per LV and KG
// ---------------------------------------------------------------------------

/// Values of one LV in one KG (net and gross).
#[derive(Debug, Clone, Default)]
struct Cell {
    net: [Option<Money>; STAGES],
    gross: [Option<Money>; STAGES],
}

impl Cell {
    fn add(&mut self, stage: TrackStage, net: Money, vat_percent: Decimal) {
        let index = stage.index();
        *self.net[index].get_or_insert(Money::ZERO) += net;
        *self.gross[index].get_or_insert(Money::ZERO) += net + net.percent(vat_percent);
    }

    fn values(&self, gross: bool) -> &[Option<Money>; STAGES] {
        if gross {
            &self.gross
        } else {
            &self.net
        }
    }
}

/// (LV index, KG): None = no LV / ohne KG.
type Key = (Option<usize>, Option<String>);

/// Splits `total` over the parts in proportion to their amounts, exactly
/// (the rounding rest goes to the largest part). Without a base the total
/// goes to the first part.
fn allocate(parts: &[(Option<String>, Money)], total: Money) -> Vec<(Option<String>, Money)> {
    let base: i128 = parts.iter().map(|(_, amount)| amount.cents() as i128).sum();
    if parts.is_empty() {
        return if total.is_zero() {
            Vec::new()
        } else {
            vec![(None, total)]
        };
    }
    if base == 0 {
        let mut out: Vec<(Option<String>, Money)> = parts
            .iter()
            .map(|(code, _)| (code.clone(), Money::ZERO))
            .collect();
        out[0].1 = total;
        return out;
    }
    let mut out: Vec<(Option<String>, Money)> = parts
        .iter()
        .map(|(code, amount)| {
            let share = amount.cents() as i128 * total.cents() as i128 / base;
            (code.clone(), Money::from_cents(share as i64))
        })
        .collect();
    let rest = total - out.iter().map(|(_, amount)| *amount).sum::<Money>();
    if !rest.is_zero() {
        let largest = parts
            .iter()
            .enumerate()
            .max_by_key(|(_, (_, amount))| amount.cents().abs())
            .map(|(index, _)| index)
            .unwrap_or(0);
        out[largest].1 += rest;
    }
    out
}

/// Sums per KG in the order of first appearance.
#[derive(Default)]
struct BySum {
    order: Vec<Option<String>>,
    sums: HashMap<Option<String>, Money>,
}

impl BySum {
    fn add(&mut self, code: Option<String>, amount: Money) {
        if !self.sums.contains_key(&code) {
            self.order.push(code.clone());
        }
        *self.sums.entry(code).or_default() += amount;
    }

    fn parts(&self) -> Vec<(Option<String>, Money)> {
        self.order
            .iter()
            .map(|code| (code.clone(), self.sums[code]))
            .collect()
    }

    fn total(&self) -> Money {
        self.sums.values().copied().sum()
    }
}

/// Where the „Angebot“ of an LV comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum OfferSource {
    Awarded,
    Chosen(String),
    Lowest,
}

impl OfferSource {
    pub fn describe(&self, bidder: &str) -> String {
        match self {
            OfferSource::Awarded => format!("{bidder} (beauftragt)"),
            OfferSource::Chosen(reason) if reason.trim().is_empty() => {
                format!("{bidder} (gewählt)")
            }
            OfferSource::Chosen(reason) => format!("{bidder} (gewählt: {})", reason.trim()),
            OfferSource::Lowest => format!("{bidder} (Mindestbieter)"),
        }
    }
}

/// The bid that counts as „Angebot“ of the LV at `index`: the awarded one,
/// else the chosen one, else the lowest of the bids not excluded.
pub fn offer_of(project: &Project, index: usize) -> Option<(&Bid, OfferSource)> {
    let lv = project.lv_at(index)?;
    let bids = project.bids_of(index);
    if let Some(bid) = bids.iter().find(|bid| bid.is_awarded()) {
        return Some((bid, OfferSource::Awarded));
    }
    if let Some(choice) = project.offer_choice_of(index) {
        if let Some(bid) = bids
            .iter()
            .find(|bid| bid.id() == choice.bid && !bid.is_excluded())
        {
            return Some((bid, OfferSource::Chosen(choice.reason.clone())));
        }
    }
    bids.iter()
        .filter(|bid| !bid.is_excluded())
        .map(|bid| (bid, bid.total_after_discount(lv)))
        .filter(|(_, total)| total.cents() > 0)
        .min_by_key(|(_, total)| *total)
        .map(|(bid, _)| (bid, OfferSource::Lowest))
}

/// One Vergabeeinheit in the Kostenverfolgung.
#[derive(Debug, Clone, PartialEq)]
pub struct LvTrack {
    pub index: usize,
    /// „LV 2 Lüftung“
    pub label: String,
    pub contractor: String,
    /// „Muster GmbH (Mindestbieter)“
    pub offer: Option<String>,
    /// Releases (Freigaben) of all invoices, net and gross.
    pub released_net: Money,
    pub released_gross: Money,
    /// The Schlussrechnung is checked.
    pub final_invoice: bool,
    pub has_contract: bool,
    /// Where the Prognose of this LV comes from.
    pub forecast_basis: &'static str,
    pub vat_percent: Decimal,
}

/// A position without Kostengruppe (row „ohne KG“, jump to the position).
#[derive(Debug, Clone, PartialEq)]
pub struct WithoutGroup {
    pub lv: usize,
    pub lv_label: String,
    pub row: RowId,
    pub ordinal: String,
    pub short_text: String,
    pub amount: Money,
}

/// „LV 2 Lüftung“ (number and title, whatever is there).
pub fn lv_label(lv: &Lv, index: usize) -> String {
    let number = lv.number().trim();
    let title = lv.title().trim();
    match (number.is_empty(), title.is_empty()) {
        (false, false) => format!("LV {number} {title}"),
        (false, true) => format!("LV {number}"),
        (true, false) => title.to_string(),
        (true, true) => format!("LV {}", index + 1),
    }
}

/// KG of every row id of the LV (inherited from the titles).
fn groups_by_row(lv: &Lv) -> HashMap<RowId, Option<String>> {
    lv.rows()
        .iter()
        .zip(lv.effective_cost_groups())
        .map(|(row, effective)| (row.id(), effective.map(|(code, _)| code)))
        .collect()
}

/// Everything the computation gathers before the rows are built.
struct Gathered {
    cells: BTreeMap<Key, Cell>,
    lvs: Vec<LvTrack>,
    without: Vec<WithoutGroup>,
    warnings: Vec<String>,
    sources: Vec<String>,
    assumptions: Vec<String>,
    /// Net Prognose per KG and Umsatzsteuersatz (the rate of a KG for the
    /// gross budget).
    forecast_rates: BTreeMap<String, BTreeMap<Decimal, Money>>,
    /// (LV or Kostenermittlung, rate) of everything that counts.
    vat_rates: Vec<(String, Decimal)>,
    default_vat: Decimal,
}

impl Gathered {
    /// Rate of a KG for its gross budget: the rate of most of its Prognose
    /// (its LVs or Kostenermittlungen), else the usual rate of the project.
    fn rate_of(&self, code: &str) -> Decimal {
        let mut by_rate: BTreeMap<Decimal, Money> = BTreeMap::new();
        for (other, rates) in &self.forecast_rates {
            if overlap(code, other) {
                for (rate, amount) in rates {
                    *by_rate.entry(*rate).or_default() += *amount;
                }
            }
        }
        by_rate
            .into_iter()
            .max_by_key(|(_, amount)| *amount)
            .map(|(rate, _)| rate)
            .unwrap_or(self.default_vat)
    }

    /// A budget amount of `code` in the view (gross with the rate of its KG).
    fn budget_amount(&self, code: &str, amount: Money, gross: bool) -> Money {
        if gross {
            amount + amount.percent(self.rate_of(code))
        } else {
            amount
        }
    }
}

/// The usual Umsatzsteuersatz of the project – the rate most of its LVs
/// (tender and Kosten-LVs) carry, 19 % among equals; without LVs 19 %. For
/// Datenpunktrechnungen, which have no rate of their own.
pub fn default_vat(project: &Project) -> Decimal {
    let standard = Decimal::from(19);
    let mut counts: BTreeMap<Decimal, usize> = BTreeMap::new();
    for index in 0..project.lv_count() {
        if let Some(lv) = project.lv_at(index) {
            *counts.entry(lv.vat_percent().normalize()).or_default() += 1;
        }
    }
    let most = counts.values().copied().max().unwrap_or(0);
    if most == 0 || counts.get(&standard) == Some(&most) {
        return standard;
    }
    counts
        .into_iter()
        .find(|(_, count)| *count == most)
        .map(|(rate, _)| rate)
        .unwrap_or(standard)
}

/// Net amounts per rate (a KG of several Kostenermittlungen).
type ByRate = BTreeMap<Decimal, Money>;

fn gather(project: &Project) -> Gathered {
    let mut cells: BTreeMap<Key, Cell> = BTreeMap::new();
    let mut lvs = Vec::new();
    let mut without = Vec::new();
    let mut warnings = Vec::new();
    let mut sources = Vec::new();
    let mut assumptions = Vec::new();
    let mut forecast_rates: BTreeMap<String, ByRate> = BTreeMap::new();
    let mut vat_rates: Vec<(String, Decimal)> = Vec::new();

    // Kostenermittlungen.
    let mut calculated: BTreeMap<String, ByRate> = BTreeMap::new();
    let mut estimated: BTreeMap<String, ByRate> = BTreeMap::new();
    let parts = stage_parts(project);
    for stage in CostStage::ALL {
        let counting: Vec<&StagePart> = parts
            .iter()
            .filter(|part| part.stage == stage && part.counts)
            .collect();
        warnings.extend(double_groups(stage, &counting));
        let column = match stage {
            CostStage::Estimate => TrackStage::Estimate,
            CostStage::Calculation => TrackStage::Calculation,
        };
        for part in counting {
            // „Kostenberechnung LPH 3 (12.10.2026)“ already names its date.
            let date = if part.date.is_empty() || part.name.contains(&part.date) {
                String::new()
            } else {
                format!(", Stand {}", part.date)
            };
            sources.push(format!(
                "{}: {} ({}{date})",
                stage.label(),
                part.name,
                part.method
            ));
            vat_rates.push((
                format!("{}: {}", stage.label(), part.name),
                part.vat_percent,
            ));
            let rate = part.vat_percent.normalize();
            for (code, amount) in &part.sums {
                cells
                    .entry((None, code.clone()))
                    .or_default()
                    .add(column, *amount, rate);
                if let Some(code) = code {
                    let target = match stage {
                        CostStage::Estimate => &mut estimated,
                        CostStage::Calculation => &mut calculated,
                    };
                    *target
                        .entry(code.clone())
                        .or_default()
                        .entry(rate)
                        .or_default() += *amount;
                }
            }
        }
    }

    // Vergabeeinheiten.
    let mut covered: BTreeSet<String> = BTreeSet::new();
    for index in project.tender_lv_indices() {
        let Some(lv) = project.lv_at(index) else {
            continue;
        };
        let label = lv_label(lv, index);
        let lv_vat = lv.vat_percent().normalize();
        vat_rates.push((label.clone(), lv_vat));
        let groups = groups_by_row(lv);
        let group_of = |row: RowId| groups.get(&row).cloned().flatten();
        let mut track = LvTrack {
            index,
            label: label.clone(),
            contractor: String::new(),
            offer: None,
            released_net: Money::ZERO,
            released_gross: Money::ZERO,
            final_invoice: false,
            has_contract: false,
            forecast_basis: "",
            vat_percent: lv_vat,
        };
        let mut lv_cells: BTreeMap<Option<String>, Cell> = BTreeMap::new();

        // Kostenvoranschlag: planner's prices per KG.
        let planner = cost_group_report(lv, &[]);
        let mut pre_tender = BySum::default();
        for line in planner.lines.iter().filter(|line| !line.subtotal) {
            if !line.planner.is_zero() {
                pre_tender.add(line.code.clone(), line.planner);
            }
        }
        for (code, amount) in pre_tender.parts() {
            lv_cells
                .entry(code)
                .or_default()
                .add(TrackStage::PreTender, amount, lv_vat);
        }

        // Positions without KG (the Rechengrundlage of Aufteilungszeilen
        // carries no money of its own).
        let bases = lv.split_bases();
        for (row, effective) in lv.rows().iter().zip(lv.effective_cost_groups()) {
            if let (LvRow::Position(position), None) = (row, effective) {
                if bases.contains(&position.id()) {
                    continue;
                }
                without.push(WithoutGroup {
                    lv: index,
                    lv_label: label.clone(),
                    row: position.id(),
                    ordinal: position.ordinal().to_string(),
                    short_text: position.short_text().to_string(),
                    amount: position.counted_total(),
                });
            }
        }

        // Angebot.
        let mut offer = BySum::default();
        if let Some((bid, source)) = offer_of(project, index) {
            track.offer = Some(source.describe(bid.name()));
            let report = cost_group_report(lv, std::slice::from_ref(bid));
            let mut parts = BySum::default();
            for line in report.lines.iter().filter(|line| !line.subtotal) {
                if let Some(amount) = line.bidders.first().filter(|amount| !amount.is_zero()) {
                    parts.add(line.code.clone(), *amount);
                }
            }
            for (code, amount) in allocate(&parts.parts(), bid.total_after_discount(lv)) {
                offer.add(code, amount);
            }
            for (code, amount) in offer.parts() {
                lv_cells
                    .entry(code)
                    .or_default()
                    .add(TrackStage::Offer, amount, lv_vat);
            }
        }

        // Auftrag, Nachträge, Abrechnung.
        let contract = project.contract_of(index);
        let orders = project.change_orders_of(index);
        let mut forecast: Option<BySum> = None;
        let mut basis = if !offer.order.is_empty() {
            "Angebot"
        } else if !pre_tender.order.is_empty() {
            "Kostenvoranschlag"
        } else {
            ""
        };
        if let Some(contract) = contract {
            track.has_contract = true;
            track.contractor = contract.contractor.clone();
            let ordered = contract_cells(contract, orders, &group_of);
            for (code, amount) in ordered.parts() {
                lv_cells
                    .entry(code)
                    .or_default()
                    .add(TrackStage::Contract, amount, lv_vat);
            }
            let mut predicted = BySum::default();
            for (code, amount) in ordered.parts() {
                predicted.add(code, amount);
            }
            let (open, open_by_row) = open_change_orders(contract, orders, &group_of);
            for (code, amount) in open.parts() {
                predicted.add(code, amount);
            }
            basis = "Auftrag mit Nachträgen";
            let billing = project.billing_of(index);
            if let Some(billing) = billing.filter(|billing| !billing.invoices().is_empty()) {
                let context = BillingContext {
                    contract,
                    change_orders: orders,
                    lv,
                };
                let checks = billing.check_all(&context);
                let billed = billed_cells(contract, orders, lv, &checks, &open_by_row, &group_of);
                for check in &checks {
                    track.released_net += check.release.release_net;
                    track.released_gross += check.release.release_gross;
                }
                let billed_vat = checks
                    .iter()
                    .rev()
                    .find(|check| check.kind != InvoiceKind::SecurityRelease)
                    .map(|check| check.release.vat.percent())
                    .unwrap_or(lv_vat);
                for (code, amount) in billed.performed.parts() {
                    lv_cells
                        .entry(code)
                        .or_default()
                        .add(TrackStage::Billed, amount, billed_vat);
                }
                for (code, amount) in billed.beyond_order.parts() {
                    predicted.add(code, amount);
                }
                if billed.is_final {
                    track.final_invoice = true;
                    for (code, amount) in billed.performed.parts() {
                        lv_cells.entry(code).or_default().add(
                            TrackStage::Final,
                            amount,
                            billed_vat,
                        );
                    }
                    predicted = BySum::default();
                    for (code, amount) in billed.performed.parts() {
                        predicted.add(code, amount);
                    }
                    basis = "Kostenfeststellung";
                } else if !billed.beyond_order.order.is_empty() {
                    basis = "Auftrag mit Nachträgen und Mehrmengen der Abrechnung";
                }
            }
            forecast = Some(predicted);
        }
        let forecast = forecast.unwrap_or(if !offer.order.is_empty() {
            offer
        } else {
            pre_tender
        });
        for (code, amount) in forecast.parts() {
            if let Some(code) = &code {
                if !amount.is_zero() {
                    covered.insert(code.clone());
                    *forecast_rates
                        .entry(code.clone())
                        .or_default()
                        .entry(lv_vat)
                        .or_default() += amount;
                }
            }
            lv_cells
                .entry(code)
                .or_default()
                .add(TrackStage::Forecast, amount, lv_vat);
        }
        track.forecast_basis = basis;
        if !basis.is_empty() {
            let open: Vec<String> = orders
                .iter()
                .filter(|order| order.status.is_open() && order.in_forecast && contract.is_some())
                .map(ChangeOrder::label)
                .collect();
            let mut text = format!("{label}: {basis}");
            if !open.is_empty() && basis.starts_with("Auftrag") {
                text.push_str(&format!(
                    " (offene Nachträge in der Prognose: {})",
                    open.join(", ")
                ));
            }
            assumptions.push(text);
        }
        for (code, cell) in lv_cells {
            let target = cells.entry((Some(index), code)).or_default();
            for stage in TrackStage::ALL {
                let i = stage.index();
                if let Some(value) = cell.net[i] {
                    *target.net[i].get_or_insert(Money::ZERO) += value;
                }
                if let Some(value) = cell.gross[i] {
                    *target.gross[i].get_or_insert(Money::ZERO) += value;
                }
            }
        }
        lvs.push(track);
    }

    // Kostengruppen of no LV: Kostenberechnung, else Kostenschätzung.
    let mut from_calculation = Vec::new();
    let mut forecast_from = |code: &String, rates: &ByRate, cells: &mut BTreeMap<Key, Cell>| {
        for (rate, amount) in rates {
            cells.entry((None, Some(code.clone()))).or_default().add(
                TrackStage::Forecast,
                *amount,
                *rate,
            );
            *forecast_rates
                .entry(code.clone())
                .or_default()
                .entry(*rate)
                .or_default() += *amount;
        }
    };
    let total = |rates: &ByRate| -> Money { rates.values().copied().sum() };
    for (code, rates) in &calculated {
        if covered.iter().any(|other| overlap(code, other)) || total(rates).is_zero() {
            continue;
        }
        forecast_from(code, rates, &mut cells);
        from_calculation.push(code.clone());
    }
    let mut from_estimate = Vec::new();
    for (code, rates) in &estimated {
        let taken = covered
            .iter()
            .chain(calculated.keys())
            .any(|other| overlap(code, other));
        if taken || total(rates).is_zero() {
            continue;
        }
        forecast_from(code, rates, &mut cells);
        from_estimate.push(code.clone());
    }
    if !from_calculation.is_empty() {
        assumptions.push(format!(
            "KG {}: noch nicht vergeben – aus der Kostenberechnung",
            from_calculation.join(", ")
        ));
    }
    if !from_estimate.is_empty() {
        assumptions.push(format!(
            "KG {}: noch nicht vergeben – aus der Kostenschätzung",
            from_estimate.join(", ")
        ));
    }

    let without_count: BTreeMap<String, usize> =
        without.iter().fold(BTreeMap::new(), |mut counts, item| {
            *counts.entry(item.lv_label.clone()).or_default() += 1;
            counts
        });
    for (label, count) in without_count {
        warnings.push(format!(
            "{label}: {} ohne Kostengruppe.",
            if count == 1 {
                "1 Position".to_string()
            } else {
                format!("{count} Positionen")
            }
        ));
    }
    warnings.extend(project.cost_tracking().budget_conflicts());

    Gathered {
        cells,
        lvs,
        without,
        warnings,
        sources,
        assumptions,
        forecast_rates,
        vat_rates,
        default_vat: default_vat(project),
    }
}

/// Contract per KG (contract sum shared out in proportion to the positions,
/// so the discount falls on every KG) plus approved change orders.
fn contract_cells(
    contract: &Contract,
    orders: &[ChangeOrder],
    group_of: &dyn Fn(RowId) -> Option<String>,
) -> BySum {
    let mut items = BySum::default();
    for (row, item) in contract.items.iter().filter(|(_, item)| item.counts()) {
        items.add(group_of(*row), item.total());
    }
    let mut out = BySum::default();
    for (code, amount) in allocate(&items.parts(), contract.total) {
        out.add(code, amount);
    }
    for order in orders {
        let mut parts = BySum::default();
        for item in order
            .items
            .iter()
            .filter(|item| order.item_status(item).is_billable())
        {
            parts.add(group_of(item.position), item.checked_total(Some(contract)));
        }
        let approved = order.approved_total(Some(contract));
        let after = approved - order.discount_on(approved, Some(contract));
        for (code, amount) in allocate(&parts.parts(), after) {
            out.add(code, amount);
        }
    }
    out
}

/// Open change orders that count in the forecast (checked, after discount),
/// per KG – the rule of the change-order overview.
fn open_change_orders(
    contract: &Contract,
    orders: &[ChangeOrder],
    group_of: &dyn Fn(RowId) -> Option<String>,
) -> (BySum, HashMap<RowId, Money>) {
    let mut out = BySum::default();
    let mut by_row: HashMap<RowId, Money> = HashMap::new();
    for order in orders
        .iter()
        .filter(|order| order.status.is_open() && order.in_forecast)
    {
        let mut parts = BySum::default();
        for item in order.items.iter().filter(|item| {
            !matches!(
                order.item_status(item),
                ChangeOrderStatus::Rejected | ChangeOrderStatus::Withdrawn
            )
        }) {
            let amount = item.checked_total(Some(contract));
            parts.add(group_of(item.position), amount);
            *by_row.entry(item.position).or_default() +=
                amount - order.discount_on(amount, Some(contract));
        }
        let open = order.checked_total(Some(contract));
        let after = open - order.discount_on(open, Some(contract));
        for (code, amount) in allocate(&parts.parts(), after) {
            out.add(code, amount);
        }
    }
    (out, by_row)
}

/// Abrechnung of one LV per KG.
struct Billed {
    /// Checked performance after discount (last invoice of the sequence plus
    /// the Einzelrechnungen).
    performed: BySum,
    /// What is billed beyond the ordered amounts and the open change orders
    /// in the forecast (after discount).
    beyond_order: BySum,
    is_final: bool,
}

fn billed_cells(
    contract: &Contract,
    orders: &[ChangeOrder],
    lv: &Lv,
    checks: &[InvoiceCheck],
    open_by_row: &HashMap<RowId, Money>,
    group_of: &dyn Fn(RowId) -> Option<String>,
) -> Billed {
    let billable = billable_items(contract, orders, lv);
    let discount = contract.discount_percent;
    let mut performed = BySum::default();
    let mut beyond = BySum::default();
    let sequence = checks
        .iter()
        .rfind(|check| check.kind.in_sequence() && check.kind != InvoiceKind::SecurityRelease);
    let singles = checks
        .iter()
        .filter(|check| check.kind == InvoiceKind::Single);
    for check in sequence.into_iter().chain(singles) {
        let mut parts = BySum::default();
        let positions = check.positions.iter().chain(&check.unchanged);
        for position in positions {
            let item = billable.get(&position.position);
            let with_discount = item.is_none_or(|item| item.discount);
            let amount = if with_discount {
                position.cumulative_total - position.cumulative_total.percent(discount)
            } else {
                position.cumulative_total
            };
            let code = group_of(position.position);
            parts.add(code.clone(), amount);
            if check.kind != InvoiceKind::Single {
                let ordered = item
                    .filter(|item| item.kind.counts_in_total())
                    .map(|item| Money::total(item.unit_price, item.quantity))
                    .unwrap_or(Money::ZERO);
                // Ordered plus what an open change order already brings
                // into the forecast for this position.
                let ordered = if with_discount {
                    ordered - ordered.percent(discount)
                } else {
                    ordered
                } + open_by_row
                    .get(&position.position)
                    .copied()
                    .unwrap_or_default();
                if amount > ordered {
                    beyond.add(code, amount - ordered);
                }
            }
        }
        if !check.lump.is_zero() {
            parts.add(None, check.lump - check.lump.percent(discount));
        }
        let total = if check.kind == InvoiceKind::Single {
            parts.total()
        } else {
            check.performed
        };
        for (code, amount) in allocate(&parts.parts(), total) {
            performed.add(code, amount);
        }
    }
    let is_final = checks.iter().any(|check| check.kind == InvoiceKind::Final);
    Billed {
        performed,
        beyond_order: beyond,
        is_final,
    }
}

// ---------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------

/// By Kostengruppe or by Vergabeeinheit (LV).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackView {
    #[default]
    ByGroup,
    ByLv,
}

impl TrackView {
    pub fn key(self) -> &'static str {
        match self {
            TrackView::ByGroup => "kostengruppen",
            TrackView::ByLv => "vergabeeinheiten",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_lowercase().as_str() {
            "kostengruppen" | "kg" => Some(TrackView::ByGroup),
            "vergabeeinheiten" | "lv" => Some(TrackView::ByLv),
            _ => None,
        }
    }
}

/// Which deviation the cells show below their value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackDeviation {
    #[default]
    Budget,
    Previous,
    /// Change since the recorded state compared with ([`TrackOptions::snapshot`]).
    Snapshot,
    Off,
}

impl TrackDeviation {
    pub const ALL: [TrackDeviation; 4] = [
        TrackDeviation::Budget,
        TrackDeviation::Previous,
        TrackDeviation::Snapshot,
        TrackDeviation::Off,
    ];

    /// „zur Kostenvorgabe“, „zur Vorstufe“, „keine“
    pub fn label(self) -> &'static str {
        match self {
            TrackDeviation::Budget => "zur Kostenvorgabe",
            TrackDeviation::Previous => "zur Vorstufe",
            TrackDeviation::Snapshot => "zum Stand",
            TrackDeviation::Off => "keine",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            TrackDeviation::Budget => "budget",
            TrackDeviation::Previous => "vorstufe",
            TrackDeviation::Snapshot => "stand",
            TrackDeviation::Off => "keine",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_lowercase();
        // „kostenvorgabe“ is the name after DIN 276; „budget“ stays the key.
        if key == "kostenvorgabe" {
            return Some(TrackDeviation::Budget);
        }
        Self::ALL.into_iter().find(|mode| mode.key() == key)
    }

    /// The deviation of a cell in this mode.
    pub fn of(self, row: &TrackRow, stage: TrackStage) -> Option<Deviation> {
        match self {
            TrackDeviation::Budget => row.to_budget(stage),
            TrackDeviation::Previous => row.to_previous(stage).map(|previous| previous.deviation),
            TrackDeviation::Snapshot => row.to_snapshot(stage),
            TrackDeviation::Off => None,
        }
    }
}

/// What the table shows.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackOptions {
    /// Gliederungsebene 1–3.
    pub level: u8,
    pub view: TrackView,
    pub gross: bool,
    /// Index of a recorded state to compare the Prognose with.
    pub snapshot: Option<usize>,
    /// Deviation shown under the values (view and print only).
    pub deviation: TrackDeviation,
}

impl Default for TrackOptions {
    fn default() -> Self {
        Self {
            level: 2,
            view: TrackView::ByGroup,
            gross: false,
            snapshot: None,
            deviation: TrackDeviation::Budget,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackRowKind {
    /// Sum of a Kostengruppe of a higher level, its members follow.
    Group,
    /// A Kostengruppe of the chosen level (or a coarser one not divided further).
    Leaf,
    /// Amounts without Kostengruppe.
    Without,
    /// A Vergabeeinheit, its Kostengruppen follow.
    Lv,
    /// Amounts of no LV (Kostenermittlungen, not yet tendered).
    NoLv,
    Total,
}

/// A deviation in euro and percent (None when the base is zero).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deviation {
    pub amount: Money,
    pub percent: Option<Decimal>,
}

impl Deviation {
    pub fn of(value: Money, base: Money) -> Self {
        let amount = value - base;
        let percent = (!base.is_zero()).then(|| {
            (Decimal::from(amount.cents()) * Decimal::ONE_HUNDRED / Decimal::from(base.cents()))
                .round_dp(1)
        });
        Self { amount, percent }
    }
}

/// The deviation „zur Vorstufe“ of a cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviousDeviation {
    /// The earlier stage; None when the LVs of the row compare with
    /// different stages (e.g. Prognose: Auftrag of one LV, Angebot of
    /// another).
    pub stage: Option<TrackStage>,
    pub deviation: Deviation,
}

impl PreviousDeviation {
    /// „zur Kostenberechnung“, „zum Auftrag“, „zur Vorstufe je LV“
    pub fn label(&self) -> &'static str {
        match self.stage {
            None => "zur Vorstufe je LV",
            Some(TrackStage::Budget) => "zur Kostenvorgabe",
            Some(TrackStage::Estimate) => "zur Kostenschätzung",
            Some(TrackStage::Calculation) => "zur Kostenberechnung",
            Some(TrackStage::PreTender) => "zum Kostenvoranschlag",
            Some(TrackStage::Offer) => "zum Angebot",
            Some(TrackStage::Contract) => "zum Auftrag",
            Some(TrackStage::Forecast) => "zur Prognose",
            Some(TrackStage::Billed) => "zur Abrechnung",
            Some(TrackStage::Final) => "zur Kostenfeststellung",
        }
    }
}

/// Values of one cell of a row with the unit it compares in: an LV, or a
/// KG of the Kostenermittlungen without LV.
type Member = (Key, [Option<Money>; STAGES]);

/// Cells of one LV by the KG shown (view by Vergabeeinheit).
type ByShownCode<'a> = BTreeMap<Option<String>, Vec<(Key, &'a Cell)>>;

/// The unit a cell compares in ([`previous_of`]).
fn unit(lv: Option<usize>, code: &Option<String>) -> Key {
    match lv {
        Some(_) => (lv, None),
        None => (None, code.clone()),
    }
}

/// The deviation of every stage of a row from its previous stage: per LV
/// with [`TrackStage::previous`], else over the row with
/// [`TrackStage::previous_planning`].
fn previous_of(
    members: &[Member],
    values: &[Option<Money>; STAGES],
) -> [Option<PreviousDeviation>; STAGES] {
    std::array::from_fn(|index| {
        let stage = TrackStage::ALL[index];
        let value = values[index]?;
        let mut lvs: Vec<&Key> = members.iter().map(|(unit, _)| unit).collect();
        lvs.sort();
        lvs.dedup();
        let (mut current, mut base) = (Money::ZERO, Money::ZERO);
        let mut used: Vec<TrackStage> = Vec::new();
        for lv in lvs {
            let of_lv: Vec<&[Option<Money>; STAGES]> = members
                .iter()
                .filter(|(member, _)| member == lv)
                .map(|(_, values)| values)
                .collect();
            let sum = |stage: TrackStage| -> Option<Money> {
                of_lv
                    .iter()
                    .filter_map(|values| values[stage.index()])
                    .reduce(|a, b| a + b)
            };
            let Some(own) = sum(stage) else {
                continue;
            };
            let Some((earlier, amount)) = stage
                .previous()
                .iter()
                .find_map(|earlier| sum(*earlier).map(|amount| (*earlier, amount)))
            else {
                continue;
            };
            current += own;
            base += amount;
            if !used.contains(&earlier) {
                used.push(earlier);
            }
        }
        if !used.is_empty() && !base.is_zero() {
            return Some(PreviousDeviation {
                stage: (used.len() == 1).then(|| used[0]),
                deviation: Deviation::of(current, base),
            });
        }
        stage.previous_planning().iter().find_map(|earlier| {
            values[earlier.index()]
                .filter(|base| !base.is_zero())
                .map(|base| PreviousDeviation {
                    stage: Some(*earlier),
                    deviation: Deviation::of(value, base),
                })
        })
    })
}

/// Marking of a cell (as a background, no symbols).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    /// More than the threshold above the previous stage.
    AbovePrevious,
    /// Above the budget.
    OverBudget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackRow {
    pub kind: TrackRowKind,
    /// Indent: rows of a group or LV follow with a greater depth.
    pub depth: u8,
    pub code: Option<String>,
    pub label: String,
    pub lv: Option<usize>,
    pub values: [Option<Money>; STAGES],
    /// Erläuterung/Maßnahme of the KG.
    pub note: String,
    /// Every stage of the recorded state compared with (None = no value then).
    pub compare: [Option<Money>; STAGES],
    /// Deviation of each stage from its previous stage.
    pub previous: [Option<PreviousDeviation>; STAGES],
}

impl TrackRow {
    pub fn value(&self, stage: TrackStage) -> Option<Money> {
        self.values[stage.index()]
    }

    /// Deviation of `stage` from the budget.
    pub fn to_budget(&self, stage: TrackStage) -> Option<Deviation> {
        if stage == TrackStage::Budget {
            return None;
        }
        Some(Deviation::of(
            self.value(stage)?,
            self.value(TrackStage::Budget)?,
        ))
    }

    /// Deviation of `stage` from its previous stage (per LV, see
    /// [`TrackStage::previous`]).
    pub fn to_previous(&self, stage: TrackStage) -> Option<PreviousDeviation> {
        self.previous[stage.index()]
    }

    /// The value of `stage` in the recorded state compared with.
    pub fn compared(&self, stage: TrackStage) -> Option<Money> {
        self.compare[stage.index()]
    }

    /// Change of `stage` since the recorded state (a stage without value
    /// then or now counts as zero); None when neither has one.
    pub fn to_snapshot(&self, stage: TrackStage) -> Option<Deviation> {
        let (now, then) = (self.value(stage), self.compared(stage));
        if now.is_none() && then.is_none() {
            return None;
        }
        Some(Deviation::of(
            now.unwrap_or_default(),
            then.unwrap_or_default(),
        ))
    }

    /// „481 Automationseinrichtungen“
    pub fn title(&self) -> String {
        match &self.code {
            Some(code) if self.kind != TrackRowKind::Lv => format!("{code} {}", self.label),
            _ => self.label.clone(),
        }
    }
}

/// The Kostenverfolgung as shown and printed.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackReport {
    pub options: TrackOptions,
    pub rows: Vec<TrackRow>,
    pub total: TrackRow,
    /// Columns with a value anywhere.
    pub used: [bool; STAGES],
    pub lvs: Vec<LvTrack>,
    pub without_group: Vec<WithoutGroup>,
    pub warnings: Vec<String>,
    /// Kostenermittlungen that count (DIN 276, 4.2.5/4.2.7).
    pub sources: Vec<String>,
    /// Where the Prognose comes from (4.2.13).
    pub assumptions: Vec<String>,
    pub budget_kind: BudgetKind,
    pub budget_date: String,
    pub budget_note: String,
    pub threshold: Decimal,
    pub released_net: Money,
    pub released_gross: Money,
    pub contract_lvs: usize,
    pub final_lvs: usize,
    /// The recorded state compared with.
    pub snapshot: Option<String>,
    /// Umsatzsteuer of everything that counts: (LV or Kostenermittlung, rate).
    pub vat_rates: Vec<(String, Decimal)>,
}

impl TrackReport {
    /// How a cell is marked.
    pub fn mark(&self, row: &TrackRow, stage: TrackStage) -> Mark {
        if row
            .to_budget(stage)
            .is_some_and(|deviation| deviation.amount.cents() > 0)
        {
            return Mark::OverBudget;
        }
        if stage != TrackStage::Billed
            && row
                .to_previous(stage)
                .and_then(|previous| previous.deviation.percent)
                .is_some_and(|percent| percent > self.threshold)
        {
            return Mark::AbovePrevious;
        }
        Mark::None
    }

    /// „2 von 3 LVs schlussgerechnet“
    pub fn final_text(&self) -> String {
        format!(
            "{} von {} {} schlussgerechnet",
            self.final_lvs,
            self.contract_lvs,
            if self.contract_lvs == 1 {
                "beauftragten LV"
            } else {
                "beauftragten LVs"
            }
        )
    }

    /// „netto“ / „brutto (USt je LV, Budget und Kostenermittlungen 19 %)“
    /// Stages shown as columns: Budget and Prognose always, the others when
    /// some row has a value.
    pub fn shown_stages(&self) -> Vec<TrackStage> {
        TrackStage::ALL
            .into_iter()
            .filter(|stage| {
                matches!(stage, TrackStage::Budget | TrackStage::Forecast)
                    || self.used[stage.index()]
                    // compared with a recorded state: also what it had
                    || (self.snapshot.is_some() && self.total.compared(*stage).is_some())
            })
            .collect()
    }

    /// Label of a row's Vergabeeinheit („LV 2 Lüftung“).
    pub fn lv_label(&self, row: &TrackRow) -> Option<&str> {
        let index = row.lv?;
        self.lvs
            .iter()
            .find(|lv| lv.index == index)
            .map(|lv| lv.label.as_str())
    }

    /// The Umsatzsteuersätze in use, ascending.
    pub fn vat_list(&self) -> Vec<Decimal> {
        let mut rates: Vec<Decimal> = self.vat_rates.iter().map(|(_, rate)| *rate).collect();
        rates.sort();
        rates.dedup();
        rates
    }

    /// Several rates: who carries which („LV 420 Heizung: 7 %“).
    pub fn vat_details(&self) -> Vec<String> {
        if self.vat_list().len() < 2 {
            return Vec::new();
        }
        self.vat_rates
            .iter()
            .map(|(name, rate)| {
                format!(
                    "{name}: {} %",
                    crate::money::format_decimal_de(*rate, rate.scale())
                )
            })
            .collect()
    }

    /// „netto“, „brutto (USt 19 %)“, „brutto (USt je LV und Kostenermittlung:
    /// 7 % und 19 %)“
    pub fn amounts_text(&self) -> String {
        if !self.options.gross {
            return "netto".to_string();
        }
        let rates: Vec<String> = self
            .vat_list()
            .iter()
            .map(|rate| format!("{} %", crate::money::format_decimal_de(*rate, rate.scale())))
            .collect();
        match rates.as_slice() {
            [] => "brutto".to_string(),
            [one] => format!("brutto (USt {one})"),
            [first @ .., last] => format!(
                "brutto (USt je LV und Kostenermittlung: {} und {last})",
                first.join(", ")
            ),
        }
    }
}

/// Values of the cells a row sums.
fn sum_cells<'a>(
    cells: impl Iterator<Item = &'a [Option<Money>; STAGES]>,
) -> [Option<Money>; STAGES] {
    let mut out = [None; STAGES];
    for values in cells {
        for (target, value) in out.iter_mut().zip(values) {
            if let Some(value) = value {
                *target.get_or_insert(Money::ZERO) += *value;
            }
        }
    }
    out
}

/// Sort key: 400 before 480 before 481; codes outside DIN 276 and „ohne
/// KG“ at the end.
fn sort_key(code: &str) -> (u8, String) {
    if is_standard(code) {
        (
            0,
            code.trim_end_matches('0').to_string() + &"/".repeat(code_level(code) as usize),
        )
    } else {
        (1, code.to_string())
    }
}

/// The budget of a shown leaf: its own level only (a coarser code shown as a
/// leaf keeps the group's budget on the group row).
fn leaf_budget(
    tracking: &CostTracking,
    code: &str,
    level: u8,
    convert: &dyn Fn(&str, Money) -> Money,
) -> Option<Money> {
    if code_level(code) == level || !is_standard(code) || level == 1 {
        tracking.budget_with(code, convert)
    } else {
        None
    }
}

/// The Kostenverfolgung of the project.
pub fn cost_tracking(project: &Project, options: &TrackOptions) -> TrackReport {
    let level = options.level.clamp(1, 3);
    let gathered = gather(project);
    let tracking = project.cost_tracking();
    let gross = options.gross;
    let convert = |code: &str, amount: Money| gathered.budget_amount(code, amount, gross);
    let budget_of = |code: &str| tracking.budget_with(code, &convert);
    let snapshot = options
        .snapshot
        .and_then(|index| tracking.snapshots.get(index));
    // The budget of the recorded state as it was (net or gross per KG).
    let snapshot_budget = snapshot.map(|snapshot| CostTracking {
        budget: snapshot
            .cells
            .iter()
            .filter(|cell| cell.lv.is_none())
            .filter_map(|cell| {
                let value = if gross { cell.gross[0] } else { cell.net[0] };
                Some((cell.code.clone()?, value?))
            })
            .collect(),
        ..CostTracking::default()
    });
    let same = |_: &str, amount: Money| amount;
    let lv_labels: Vec<String> = gathered.lvs.iter().map(|lv| lv.label.clone()).collect();
    let note_of = |code: &Option<String>| {
        code.as_ref()
            .and_then(|code| tracking.notes.get(code))
            .cloned()
            .unwrap_or_default()
    };
    // Every stage of the recorded state over the cells a row matches (the
    // budget comes as given); None where it had no value.
    let compare_of = |matches: &dyn Fn(Option<&str>, Option<&str>) -> bool,
                      budget: Option<Money>|
     -> [Option<Money>; STAGES] {
        let mut values = [None; STAGES];
        let Some(snapshot) = snapshot else {
            return values;
        };
        values[0] = budget;
        for cell in snapshot
            .cells
            .iter()
            .filter(|cell| matches(cell.lv.as_deref(), cell.code.as_deref()))
        {
            let source = if gross { &cell.gross } else { &cell.net };
            for (target, value) in values.iter_mut().zip(source).skip(1) {
                if let Some(value) = value {
                    *target.get_or_insert(Money::ZERO) += *value;
                }
            }
        }
        values
    };

    let mut rows = Vec::new();
    match options.view {
        TrackView::ByGroup => {
            // Leaves per shown code; groups above them.
            let mut leaves: BTreeMap<String, Vec<(Key, &Cell)>> = BTreeMap::new();
            let mut without: Vec<(Key, &Cell)> = Vec::new();
            for ((lv, code), cell) in &gathered.cells {
                let key = unit(*lv, code);
                match code.as_deref().and_then(|code| at_level(code, level)) {
                    Some(leaf) => leaves.entry(leaf).or_default().push((key, cell)),
                    None => without.push((key, cell)),
                }
            }
            let mut nodes: BTreeSet<String> = leaves.keys().cloned().collect();
            // A budget makes its row appear even without other values.
            for code in tracking.budget.keys() {
                if let Some(shown) = at_level(code, level) {
                    nodes.insert(shown);
                }
            }
            // Group headers for every shown code finer than the group level.
            let mut groups: BTreeSet<String> = BTreeSet::new();
            for code in &nodes {
                for group_level in 1..level {
                    if let Some(group) = at_level(code, group_level) {
                        if code_level(&group) == group_level && is_standard(code) {
                            groups.insert(group);
                        }
                    }
                }
            }
            let mut order: Vec<(String, bool)> = groups
                .iter()
                .map(|code| (code.clone(), true))
                .chain(nodes.iter().map(|code| (code.clone(), false)))
                .collect();
            order.sort_by_key(|(code, group)| (sort_key(code), !*group));
            for (code, is_group) in order {
                let cell_values = |(key, cell): &(Key, &Cell)| (key.clone(), *cell.values(gross));
                if is_group {
                    let members: Vec<Member> = leaves
                        .iter()
                        .filter(|(leaf, _)| descends_from(leaf, &code))
                        .flat_map(|(_, cells)| cells.iter().map(cell_values))
                        .collect();
                    let mut values = sum_cells(members.iter().map(|(_, values)| values));
                    values[0] = budget_of(&code);
                    // A group whose only member is itself is shown once.
                    let own_only = nodes
                        .iter()
                        .filter(|leaf| descends_from(leaf, &code))
                        .all(|leaf| *leaf == code);
                    rows.push(TrackRow {
                        kind: if own_only {
                            TrackRowKind::Leaf
                        } else {
                            TrackRowKind::Group
                        },
                        depth: code_level(&code) - 1,
                        label: group_label(&code).to_string(),
                        lv: None,
                        note: note_of(&Some(code.clone())),
                        compare: compare_of(
                            &|_, cell| cell.is_some_and(|cell| descends_from(cell, &code)),
                            snapshot_budget
                                .as_ref()
                                .and_then(|budget| budget.budget_with(&code, &same)),
                        ),
                        previous: previous_of(&members, &values),
                        values,
                        code: Some(code),
                    });
                } else {
                    if groups.contains(&code) {
                        let has_others = nodes
                            .iter()
                            .any(|leaf| *leaf != code && descends_from(leaf, &code));
                        if !has_others {
                            continue;
                        }
                    }
                    let members: Vec<Member> = leaves
                        .get(&code)
                        .map(|cells| cells.iter().map(cell_values).collect())
                        .unwrap_or_default();
                    let mut values = sum_cells(members.iter().map(|(_, values)| values));
                    values[0] = leaf_budget(tracking, &code, level, &convert);
                    let coarse = is_standard(&code) && code_level(&code) < level;
                    // Under its group; a coarse code one step under the group of
                    // the same code.
                    let depth = match (is_standard(&code), coarse) {
                        (false, _) => 0,
                        (true, true) => code_level(&code),
                        (true, false) => level - 1,
                    };
                    let label = match (coarse, group_label(&code)) {
                        (true, label) if !label.is_empty() => {
                            format!("{label} (nicht weiter gegliedert)")
                        }
                        (true, _) => "nicht weiter gegliedert".to_string(),
                        (false, label) => label.to_string(),
                    };
                    let exact = code.clone();
                    rows.push(TrackRow {
                        kind: TrackRowKind::Leaf,
                        depth,
                        label,
                        lv: None,
                        note: note_of(&Some(code.clone())),
                        compare: compare_of(
                            &|_, cell| {
                                cell.is_some_and(|cell| {
                                    at_level(cell, level).as_deref() == Some(exact.as_str())
                                })
                            },
                            snapshot_budget
                                .as_ref()
                                .and_then(|budget| leaf_budget(budget, &exact, level, &same)),
                        ),
                        previous: previous_of(&members, &values),
                        values,
                        code: Some(code),
                    });
                }
            }
            if !without.is_empty() {
                let members: Vec<Member> = without
                    .iter()
                    .map(|(key, cell)| (key.clone(), *cell.values(gross)))
                    .collect();
                let values = sum_cells(members.iter().map(|(_, values)| values));
                rows.push(TrackRow {
                    kind: TrackRowKind::Without,
                    depth: 0,
                    code: None,
                    label: "ohne Kostengruppe".to_string(),
                    lv: None,
                    previous: previous_of(&members, &values),
                    values,
                    note: String::new(),
                    compare: compare_of(&|_, cell| cell.is_none(), None),
                });
            }
        }
        TrackView::ByLv => {
            let mut by_lv: BTreeMap<Option<usize>, ByShownCode> = BTreeMap::new();
            for ((lv, code), cell) in &gathered.cells {
                let shown = code.as_deref().and_then(|code| at_level(code, level));
                by_lv
                    .entry(*lv)
                    .or_default()
                    .entry(shown)
                    .or_default()
                    .push((unit(*lv, code), cell));
            }
            // LVs in their order, „ohne Vergabeeinheit“ last.
            let mut keys: Vec<Option<usize>> = by_lv.keys().copied().collect();
            keys.sort_by_key(|key| key.unwrap_or(usize::MAX));
            for key in keys {
                let groups = &by_lv[&key];
                let all: Vec<Member> = groups
                    .values()
                    .flat_map(|cells| {
                        cells
                            .iter()
                            .map(|(unit, cell)| (unit.clone(), *cell.values(gross)))
                    })
                    .collect();
                let all_values = sum_cells(all.iter().map(|(_, values)| values));
                let lv_name = key.and_then(|index| lv_labels.get(index).cloned());
                let matches_lv = |cell_lv: Option<&str>| cell_lv == lv_name.as_deref();
                rows.push(TrackRow {
                    kind: if key.is_some() {
                        TrackRowKind::Lv
                    } else {
                        TrackRowKind::NoLv
                    },
                    depth: 0,
                    code: None,
                    label: lv_name.clone().unwrap_or_else(|| {
                        "ohne Vergabeeinheit (Kostenermittlungen, noch nicht vergeben)".to_string()
                    }),
                    lv: key,
                    previous: previous_of(&all, &all_values),
                    values: all_values,
                    note: String::new(),
                    compare: compare_of(&|cell_lv, _| matches_lv(cell_lv), None),
                });
                let mut codes: Vec<&Option<String>> = groups.keys().collect();
                codes.sort_by_key(|code| {
                    code.as_deref().map(sort_key).unwrap_or((2, String::new()))
                });
                for code in codes {
                    let members: Vec<Member> = groups[code]
                        .iter()
                        .map(|(unit, cell)| (unit.clone(), *cell.values(gross)))
                        .collect();
                    let values = sum_cells(members.iter().map(|(_, values)| values));
                    let shown = code.clone();
                    rows.push(TrackRow {
                        kind: if code.is_some() {
                            TrackRowKind::Leaf
                        } else {
                            TrackRowKind::Without
                        },
                        depth: 1,
                        label: code
                            .as_deref()
                            .map(group_label)
                            .unwrap_or("ohne Kostengruppe")
                            .to_string(),
                        lv: key,
                        previous: previous_of(&members, &values),
                        values,
                        note: note_of(code),
                        compare: compare_of(
                            &|cell_lv, cell_code| {
                                matches_lv(cell_lv)
                                    && cell_code.and_then(|cell| at_level(cell, level)) == shown
                            },
                            None,
                        ),
                        code: code.clone(),
                    });
                }
            }
        }
    }

    let all: Vec<Member> = gathered
        .cells
        .iter()
        .map(|((lv, code), cell)| (unit(*lv, code), *cell.values(gross)))
        .collect();
    let mut total_values = sum_cells(all.iter().map(|(_, values)| values));
    total_values[0] = tracking.budget_total_with(&convert);
    let total = TrackRow {
        kind: TrackRowKind::Total,
        depth: 0,
        code: None,
        label: "Gesamt".to_string(),
        lv: None,
        previous: previous_of(&all, &total_values),
        values: total_values,
        note: String::new(),
        compare: compare_of(
            &|_, _| true,
            snapshot_budget
                .as_ref()
                .and_then(|budget| budget.budget_total_with(&same)),
        ),
    };
    let mut used = [false; STAGES];
    for (flag, value) in used.iter_mut().zip(&total.values) {
        *flag = value.is_some();
    }
    let contract_lvs = gathered.lvs.iter().filter(|lv| lv.has_contract).count();
    let final_lvs = gathered.lvs.iter().filter(|lv| lv.final_invoice).count();
    let released_net = gathered.lvs.iter().map(|lv| lv.released_net).sum();
    let released_gross = gathered.lvs.iter().map(|lv| lv.released_gross).sum();
    TrackReport {
        options: TrackOptions {
            level,
            ..options.clone()
        },
        rows,
        total,
        used,
        lvs: gathered.lvs,
        without_group: gathered.without,
        warnings: gathered.warnings,
        sources: gathered.sources,
        assumptions: gathered.assumptions,
        budget_kind: tracking.budget_kind,
        budget_date: tracking.budget_date.clone(),
        budget_note: tracking.budget_note.clone(),
        threshold: tracking.threshold_percent,
        released_net,
        released_gross,
        contract_lvs,
        final_lvs,
        snapshot: snapshot.map(CostSnapshot::label),
        vat_rates: gathered.vat_rates.clone(),
    }
}

/// The chain of today as a recorded state.
pub fn record_snapshot(project: &Project, date: &str, note: &str) -> CostSnapshot {
    let gathered = gather(project);
    let labels: Vec<String> = gathered.lvs.iter().map(|lv| lv.label.clone()).collect();
    let mut cells: Vec<SnapshotCell> = gathered
        .cells
        .iter()
        .map(|((lv, code), cell)| SnapshotCell {
            lv: lv.and_then(|index| labels.get(index).cloned()),
            code: code.clone(),
            net: cell.net,
            gross: cell.gross,
        })
        .collect();
    // The budget as it was (its own codes, no LV; gross with the rate of its KG).
    for (code, amount) in &project.cost_tracking().budget {
        let mut cell = SnapshotCell {
            lv: None,
            code: Some(code.clone()),
            ..SnapshotCell::default()
        };
        cell.net[0] = Some(*amount);
        cell.gross[0] = Some(gathered.budget_amount(code, *amount, true));
        cells.push(cell);
    }
    CostSnapshot {
        date: date.to_string(),
        note: note.to_string(),
        cells,
    }
}
