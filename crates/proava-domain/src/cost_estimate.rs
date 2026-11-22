//! Kostenermittlung nach DIN 276 für die Gebäudeautomation (KG 480).
//!
//! Zwei Stufen mit derselben Datengrundlage – einer Anlagenliste mit der
//! Anzahl Datenpunkte je DP-Art, gegliedert nach Bauteilen:
//!
//! - **Kostenschätzung** (2. Ebene): alle Datenpunkte × Kostenkennwert
//!   (ein statistischer DP-Preis oder einer je DP-Art) = KG 480, wahlweise
//!   mit Spanne von–bis.
//! - **Kostenberechnung** (3. Ebene): Datenpunkte × EP je DP-Art =
//!   Gesamtsumme je Bauteil, prozentual auf KG 481–489 aufgeteilt
//!   (Vorgabe 40/12/10/30/5/3 %, je Bauteil änderbar).
//!
//! Rundung der Aufteilung: jeder KG-Betrag wird auf Cent gerundet; ergeben
//! die Anteile genau 100 %, trägt die größte KG (höchster Prozentsatz, bei
//! Gleichstand die erste) den Rundungsrest von höchstens ein paar Cent. So
//! stimmt die Summe der KG immer centgenau mit der Gesamtsumme überein, die
//! Prüfsumme zeigt nur einen echten Fehler (Anteile ≠ 100 %), und der Weg
//! lässt sich von Hand nachrechnen. Ergeben die Anteile nicht 100 %, wird
//! nichts ausgeglichen: die Prüfsumme zeigt die Lücke, dazu eine Warnung.

use std::collections::BTreeMap;

use rust_decimal::Decimal;

use crate::cost_group::{cost_group_label, cost_group_report};
use crate::money::{format_decimal_de, Money};
use crate::project::Project;

/// Identity of a cost estimate within its project (stored).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CostEstimateId(pub u64);

/// Stufe der Kostenermittlung nach DIN 276.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CostStage {
    /// Kostenschätzung, mindestens 2. Ebene (KG 480).
    Estimate,
    /// Kostenberechnung, mindestens 3. Ebene (KG 481–489).
    Calculation,
}

impl CostStage {
    pub const ALL: [CostStage; 2] = [CostStage::Estimate, CostStage::Calculation];

    pub fn label(self) -> &'static str {
        match self {
            CostStage::Estimate => "Kostenschätzung",
            CostStage::Calculation => "Kostenberechnung",
        }
    }

    /// Gliederungsebene nach DIN 276.
    pub fn level(self) -> u8 {
        match self {
            CostStage::Estimate => 2,
            CostStage::Calculation => 3,
        }
    }

    /// „DIN 276, 2. Ebene“
    pub fn level_label(self) -> String {
        format!("DIN 276, {}. Ebene", self.level())
    }

    /// Name of the template (Neues Projekt, Kosten-Menü).
    pub fn template_name(self) -> &'static str {
        match self {
            CostStage::Estimate => "Kostenschätzung DIN 276 (2. Ebene)",
            CostStage::Calculation => "Kostenberechnung DIN 276 (3. Ebene)",
        }
    }

    /// Key for the project file and the MCP server.
    pub fn key(self) -> &'static str {
        match self {
            CostStage::Estimate => "kostenschaetzung",
            CostStage::Calculation => "kostenberechnung",
        }
    }

    /// Reads the key, the label or the short forms „KS“/„KB“.
    pub fn from_key(text: &str) -> Option<Self> {
        let text = text.trim().to_lowercase();
        match text.as_str() {
            "kostenschaetzung" | "kostenschätzung" | "ks" | "schaetzung" | "schätzung" => {
                Some(CostStage::Estimate)
            }
            "kostenberechnung" | "kb" | "berechnung" => Some(CostStage::Calculation),
            _ => None,
        }
    }
}

/// One kind of data point (DP-Art), a column of the plant list.
#[derive(Debug, Clone, PartialEq)]
pub struct PointKind {
    /// Short name, e.g. „AI“ (unique within the estimate).
    pub key: String,
    /// e.g. „Analoge Eingabe“.
    pub label: String,
    /// Price per data point of this kind (EP in the Kostenberechnung; in the
    /// Kostenschätzung only with „Kennwert je DP-Art“).
    pub unit_price: Decimal,
}

/// Kürzel of the virtual DP-Arten (Wert über Bus/Software, kein Hardware-Kanal).
/// Anwender 18.11.: „AV und BV sind die virtuellen, der Rest physikalische Ein-/
/// Ausgänge“; MV (Mehrstufenwert) kommt dazu. Every other kind is physical.
pub const VIRTUAL_KEYS: [&str; 3] = ["AV", "BV", "MV"];

impl PointKind {
    pub fn new(key: &str, label: &str, unit_price: Decimal) -> Self {
        Self {
            key: key.trim().to_string(),
            label: label.trim().to_string(),
            unit_price,
        }
    }

    /// Virtual data point (from the Kürzel, see [`VIRTUAL_KEYS`]): numbered
    /// 1.2.n in the printout, the physical ones 1.1.n.
    pub fn is_virtual(&self) -> bool {
        let key = self.key.trim();
        VIRTUAL_KEYS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(key))
    }

    /// „Analoge Eingabe (AI)“, or the key alone without a label.
    pub fn caption(&self) -> String {
        match (self.label.trim(), self.key.trim()) {
            ("", key) => key.to_string(),
            (label, "") => label.to_string(),
            (label, key) => format!("{label} ({key})"),
        }
    }
}

/// DP-Arten of the templates: hardware AI/BI/AO/BO at 400 €, virtual AV/BV
/// at 200 € (Kostenberechnung).
pub const DEFAULT_POINT_KINDS: [(&str, &str, i64); 6] = [
    ("AI", "Analoge Eingabe", 400),
    ("BI", "Binäre Eingabe", 400),
    ("AO", "Analoge Ausgabe", 400),
    ("BO", "Binäre Ausgabe", 400),
    ("AV", "Analogwert", 200),
    ("BV", "Binärwert", 200),
];

pub fn default_point_kinds() -> Vec<PointKind> {
    DEFAULT_POINT_KINDS
        .iter()
        .map(|(key, label, price)| PointKind::new(key, label, Decimal::from(*price)))
        .collect()
}

/// Statistischer DP-Preis of the template Kostenschätzung (€ je DP).
pub const DEFAULT_RATE: i64 = 450;

/// One line of the plant list (Anlagenliste): where, what and how many data
/// points of each kind.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PointRow {
    /// Ort: ASP, Etage, Raum – free text.
    pub location: String,
    /// Gewerk, e.g. „RLT“.
    pub trade: String,
    /// Anlage, e.g. „TKA-2001“.
    pub plant: String,
    pub remark: String,
    /// Number of data points per kind, in the order of
    /// [`CostEstimate::kinds`] (missing = 0).
    pub counts: Vec<u32>,
}

impl PointRow {
    pub fn new(trade: &str, plant: &str) -> Self {
        Self {
            trade: trade.trim().to_string(),
            plant: plant.trim().to_string(),
            ..Self::default()
        }
    }

    pub fn count(&self, kind: usize) -> u32 {
        self.counts.get(kind).copied().unwrap_or(0)
    }

    pub fn set_count(&mut self, kind: usize, value: u32) {
        if self.counts.len() <= kind {
            self.counts.resize(kind + 1, 0);
        }
        self.counts[kind] = value;
    }

    /// All data points of the row.
    pub fn points(&self) -> u64 {
        self.counts.iter().map(|count| u64::from(*count)).sum()
    }

    /// Nothing typed in (an empty line).
    pub fn is_blank(&self) -> bool {
        self.location.trim().is_empty()
            && self.trade.trim().is_empty()
            && self.plant.trim().is_empty()
            && self.remark.trim().is_empty()
            && self.points() == 0
    }
}

/// Share of one cost group in the split (Kostenaufteilung nach Titelsummen).
#[derive(Debug, Clone, PartialEq)]
pub struct CostShare {
    /// Three digits, e.g. „481“.
    pub code: String,
    pub percent: Decimal,
}

impl CostShare {
    pub fn new(code: &str, percent: Decimal) -> Self {
        Self {
            code: code.trim().to_string(),
            percent,
        }
    }
}

/// Split of the templates: 481 40 %, 482 12 %, 483 10 %, 484 30 %, 485 5 %, 489 3 %.
pub const DEFAULT_SHARES: [(&str, i64); 6] = [
    ("481", 40),
    ("482", 12),
    ("483", 10),
    ("484", 30),
    ("485", 5),
    ("489", 3),
];

pub fn default_shares() -> Vec<CostShare> {
    DEFAULT_SHARES
        .iter()
        .map(|(code, percent)| CostShare::new(code, Decimal::from(*percent)))
        .collect()
}

/// A Bauteil or Abschnitt (Bestand, Neubau …) with its plant list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CostSection {
    pub name: String,
    pub rows: Vec<PointRow>,
    /// Own split of this section; None = the split of the estimate.
    pub shares: Option<Vec<CostShare>>,
}

impl CostSection {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.trim().to_string(),
            ..Self::default()
        }
    }
}

/// One Kostenschätzung or Kostenberechnung of a project.
#[derive(Debug, Clone, PartialEq)]
pub struct CostEstimate {
    pub id: CostEstimateId,
    pub stage: CostStage,
    /// e.g. „Kostenberechnung LPH 3“.
    pub title: String,
    /// Stand, TT.MM.JJJJ.
    pub date: String,
    /// Bearbeiter.
    pub author: String,
    pub remark: String,
    pub kinds: Vec<PointKind>,
    pub sections: Vec<CostSection>,
    /// Kostenschätzung: statistischer DP-Preis (€ je DP).
    pub rate: Decimal,
    /// Kostenschätzung: Spanne des Kennwerts von–bis (optional).
    pub rate_low: Option<Decimal>,
    pub rate_high: Option<Decimal>,
    /// Kostenschätzung: Kennwert je DP-Art ([`PointKind::unit_price`]) statt
    /// eines Preises für alle.
    pub rate_per_kind: bool,
    /// Kostenberechnung: Aufteilung auf KG 481–489 (Vorgabe für alle Bauteile).
    pub shares: Vec<CostShare>,
}

impl CostEstimate {
    /// The template of a stage: default DP-Arten, one Bauteil „Gebäude“ with
    /// one empty line, DP-Preis 450 € (Schätzung) or the EP of the kinds
    /// and the split 40/12/10/30/5/3 % (Berechnung).
    pub fn template(stage: CostStage, date: &str, author: &str) -> Self {
        let kinds = default_point_kinds();
        let mut section = CostSection::new("Gebäude");
        section.rows.push(PointRow {
            counts: vec![0; kinds.len()],
            ..PointRow::default()
        });
        Self {
            id: CostEstimateId(0),
            stage,
            title: stage.label().to_string(),
            date: date.trim().to_string(),
            author: author.trim().to_string(),
            remark: String::new(),
            kinds,
            sections: vec![section],
            rate: Decimal::from(DEFAULT_RATE),
            rate_low: None,
            rate_high: None,
            rate_per_kind: false,
            shares: default_shares(),
        }
    }

    /// „Kostenberechnung aus Kostenschätzung“: same DP-Arten, Bauteile and
    /// plant lists; the EP of the kinds keep their values (in a Schätzung
    /// with one DP-Preis for all: the EP of the template for the known kinds,
    /// the DP-Preis for the others), split of the template.
    pub fn to_calculation(&self, date: &str, author: &str) -> Self {
        let defaults = default_point_kinds();
        let kinds = self
            .kinds
            .iter()
            .map(|kind| {
                let mut kind = kind.clone();
                if self.stage == CostStage::Estimate && !self.rate_per_kind {
                    kind.unit_price = defaults
                        .iter()
                        .find(|known| known.key.eq_ignore_ascii_case(&kind.key))
                        .map(|known| known.unit_price)
                        .unwrap_or(self.rate);
                }
                kind
            })
            .collect();
        let title = match self.title.trim() {
            "" => CostStage::Calculation.label().to_string(),
            title if title.contains(CostStage::Estimate.label()) => {
                title.replace(CostStage::Estimate.label(), CostStage::Calculation.label())
            }
            _ => CostStage::Calculation.label().to_string(),
        };
        Self {
            id: CostEstimateId(0),
            stage: CostStage::Calculation,
            title,
            date: date.trim().to_string(),
            author: author.trim().to_string(),
            remark: match self.date.trim() {
                "" => format!("aus {}", self.display_name()),
                date => format!("aus {} vom {date}", self.title.trim()),
            },
            kinds,
            sections: self.sections.clone(),
            rate: self.rate,
            rate_low: self.rate_low,
            rate_high: self.rate_high,
            rate_per_kind: self.rate_per_kind,
            shares: if self.stage == CostStage::Calculation {
                self.shares.clone()
            } else {
                default_shares()
            },
        }
    }

    /// „Kostenberechnung LPH 3 (12.11.2026)“
    pub fn display_name(&self) -> String {
        let title = match self.title.trim() {
            "" => self.stage.label(),
            title => title,
        };
        match self.date.trim() {
            "" => title.to_string(),
            date => format!("{title} ({date})"),
        }
    }

    /// The split that applies to `section`: its own or the estimate's.
    pub fn shares_of(&self, section: usize) -> &[CostShare] {
        self.sections
            .get(section)
            .and_then(|section| section.shares.as_deref())
            .unwrap_or(&self.shares)
    }

    /// Adds a DP-Art at the end (every row gets a 0).
    pub fn add_kind(&mut self, kind: PointKind) -> usize {
        self.kinds.push(kind);
        let count = self.kinds.len();
        for row in self.rows_mut() {
            row.counts.resize(count, 0);
        }
        count - 1
    }

    /// Removes a DP-Art with its counts in every row.
    pub fn remove_kind(&mut self, index: usize) -> Option<PointKind> {
        if index >= self.kinds.len() {
            return None;
        }
        let kind = self.kinds.remove(index);
        for row in self.rows_mut() {
            if index < row.counts.len() {
                row.counts.remove(index);
            }
        }
        Some(kind)
    }

    /// Moves a DP-Art one place to the left (`left`) or right, counts with it.
    pub fn move_kind(&mut self, index: usize, left: bool) -> bool {
        let other = if left {
            index.checked_sub(1)
        } else {
            Some(index + 1)
        };
        let Some(other) =
            other.filter(|other| *other < self.kinds.len() && index < self.kinds.len())
        else {
            return false;
        };
        self.kinds.swap(index, other);
        let count = self.kinds.len();
        for row in self.rows_mut() {
            row.counts.resize(count, 0);
            row.counts.swap(index, other);
        }
        true
    }

    /// Index of the DP-Art with this key (case ignored).
    pub fn kind_index(&self, key: &str) -> Option<usize> {
        let key = key.trim();
        self.kinds
            .iter()
            .position(|kind| kind.key.eq_ignore_ascii_case(key))
    }

    fn rows_mut(&mut self) -> impl Iterator<Item = &mut PointRow> {
        self.sections
            .iter_mut()
            .flat_map(|section| section.rows.iter_mut())
    }

    /// Number of plant lines (all Bauteile).
    pub fn row_count(&self) -> usize {
        self.sections.iter().map(|section| section.rows.len()).sum()
    }

    /// Price per data point of `kind` in this estimate: the EP of the kind
    /// (Berechnung, Schätzung „je DP-Art“) or the DP-Preis.
    pub fn price_of(&self, kind: usize) -> Decimal {
        match (self.stage, self.rate_per_kind) {
            (CostStage::Estimate, false) => self.rate,
            _ => self
                .kinds
                .get(kind)
                .map(|kind| kind.unit_price)
                .unwrap_or_default(),
        }
    }

    /// Whether the Schätzung shows a span (both ends given, one DP-Preis).
    pub fn has_span(&self) -> bool {
        self.stage == CostStage::Estimate
            && !self.rate_per_kind
            && self.rate_low.is_some()
            && self.rate_high.is_some()
    }

    /// Sums of the whole estimate.
    pub fn result(&self) -> CostResult {
        let sections: Vec<SectionResult> = (0..self.sections.len())
            .map(|index| self.section_result(index))
            .collect();
        let mut kinds: Vec<KindSum> = self
            .kinds
            .iter()
            .enumerate()
            .map(|(index, _)| KindSum {
                count: 0,
                unit_price: self.price_of(index),
                amount: Money::ZERO,
            })
            .collect();
        let mut total = Money::ZERO;
        let mut low = self.has_span().then_some(Money::ZERO);
        let mut high = self.has_span().then_some(Money::ZERO);
        let mut groups: BTreeMap<String, Money> = BTreeMap::new();
        for section in &sections {
            for (sum, part) in kinds.iter_mut().zip(&section.kinds) {
                sum.count += part.count;
                sum.amount += part.amount;
            }
            total += section.total;
            if let (Some(low), Some(part)) = (low.as_mut(), section.low) {
                *low += part;
            }
            if let (Some(high), Some(part)) = (high.as_mut(), section.high) {
                *high += part;
            }
            for share in &section.shares {
                *groups.entry(share.code.clone()).or_default() += share.amount;
            }
        }
        let points = kinds.iter().map(|kind| kind.count).sum();
        let groups: Vec<ShareLine> = match self.stage {
            CostStage::Estimate => vec![ShareLine {
                code: "480".into(),
                label: cost_group_label("480").to_string(),
                percent: Decimal::ONE_HUNDRED,
                amount: total,
            }],
            CostStage::Calculation => groups
                .into_iter()
                .map(|(code, amount)| ShareLine {
                    label: cost_group_label(&code).to_string(),
                    percent: share_of(amount, total),
                    code,
                    amount,
                })
                .collect(),
        };
        let split: Money = groups.iter().map(|group| group.amount).sum();
        CostResult {
            check: split - total,
            sections,
            kinds,
            points,
            total,
            low,
            high,
            groups,
            warnings: self.warnings(),
        }
    }

    /// Sums of one Bauteil (empty result for an index out of range).
    pub fn section_result(&self, index: usize) -> SectionResult {
        let Some(section) = self.sections.get(index) else {
            return SectionResult::default();
        };
        let kinds: Vec<KindSum> = (0..self.kinds.len())
            .map(|kind| {
                let count: u64 = section
                    .rows
                    .iter()
                    .map(|row| u64::from(row.count(kind)))
                    .sum();
                let unit_price = self.price_of(kind);
                KindSum {
                    count,
                    unit_price,
                    amount: Money::total(unit_price, Decimal::from(count)),
                }
            })
            .collect();
        let points: u64 = kinds.iter().map(|kind| kind.count).sum();
        let total = match (self.stage, self.rate_per_kind) {
            (CostStage::Estimate, false) => Money::total(self.rate, Decimal::from(points)),
            _ => kinds.iter().map(|kind| kind.amount).sum(),
        };
        let span = |rate: Option<Decimal>| {
            rate.filter(|_| self.has_span())
                .map(|rate| Money::total(rate, Decimal::from(points)))
        };
        let shares = match self.stage {
            CostStage::Estimate => Vec::new(),
            CostStage::Calculation => split(total, self.shares_of(index)),
        };
        let percent_sum: Decimal = match self.stage {
            CostStage::Estimate => Decimal::ONE_HUNDRED,
            CostStage::Calculation => percent_sum(self.shares_of(index)),
        };
        let split_sum: Money = shares.iter().map(|share| share.amount).sum();
        SectionResult {
            name: section.name.clone(),
            kinds,
            points,
            total,
            low: span(self.rate_low),
            high: span(self.rate_high),
            check: match self.stage {
                CostStage::Estimate => Money::ZERO,
                CostStage::Calculation => split_sum - total,
            },
            shares,
            percent_sum,
            own_shares: section.shares.is_some(),
        }
    }

    /// Hints that do not block printing: split not 100 %, codes twice or
    /// wrong, kinds without price, an empty span …
    pub fn warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut keys: Vec<String> = Vec::new();
        for kind in &self.kinds {
            let key = kind.key.trim().to_uppercase();
            if key.is_empty() {
                warnings.push("Eine DP-Art hat kein Kürzel.".to_string());
            } else if keys.contains(&key) {
                warnings.push(format!("Die DP-Art „{}“ gibt es zweimal.", kind.key.trim()));
            }
            keys.push(key);
        }
        match self.stage {
            CostStage::Estimate => {
                if !self.rate_per_kind && self.rate.is_zero() {
                    warnings.push("Der Kostenkennwert (DP-Preis) ist 0,00.".to_string());
                }
                if let (Some(low), Some(high)) = (self.rate_low, self.rate_high) {
                    if low > high {
                        warnings.push(format!(
                            "Die Spanne des Kennwerts ist vertauscht ({} bis {}).",
                            format_decimal_de(low, 2),
                            format_decimal_de(high, 2)
                        ));
                    }
                }
            }
            CostStage::Calculation => {
                for (index, section) in self.sections.iter().enumerate() {
                    let name = section_label(section, index);
                    let shares = self.shares_of(index);
                    let sum = percent_sum(shares);
                    if sum != Decimal::ONE_HUNDRED {
                        let result = self.section_result(index);
                        warnings.push(format!(
                            "{name}: Die Aufteilung ergibt {} % statt 100 % (Prüfsumme {}).",
                            format_decimal_de(sum.normalize(), decimals(sum)),
                            result.check.plain()
                        ));
                    }
                    let mut seen: Vec<&str> = Vec::new();
                    for share in shares {
                        let code = share.code.trim();
                        if crate::cost_group::parse_cost_group(code)
                            .ok()
                            .flatten()
                            .is_none()
                        {
                            warnings.push(format!("{name}: „{code}“ ist keine Kostengruppe."));
                        } else if seen.contains(&code) {
                            warnings.push(format!(
                                "{name}: KG {code} steht zweimal in der Aufteilung."
                            ));
                        }
                        seen.push(code);
                    }
                }
            }
        }
        if self.stage == CostStage::Calculation || self.rate_per_kind {
            for (index, kind) in self.kinds.iter().enumerate() {
                let used = self
                    .sections
                    .iter()
                    .flat_map(|section| &section.rows)
                    .any(|row| row.count(index) > 0);
                if used && kind.unit_price.is_zero() {
                    warnings.push(format!(
                        "Die DP-Art „{}“ hat keinen Preis.",
                        kind.key.trim()
                    ));
                }
            }
        }
        warnings
    }
}

fn section_label(section: &CostSection, index: usize) -> String {
    match section.name.trim() {
        "" => format!("Bauteil {}", index + 1),
        name => format!("Bauteil „{name}“"),
    }
}

fn decimals(value: Decimal) -> u32 {
    value.normalize().scale().min(2)
}

/// `amount` in percent of `total` (0 for a total of 0).
fn share_of(amount: Money, total: Money) -> Decimal {
    if total.is_zero() {
        Decimal::ZERO
    } else {
        amount.to_decimal() * Decimal::ONE_HUNDRED / total.to_decimal()
    }
}

/// Sum of the percentages of a split; absurd values (a damaged file) stay
/// at the largest decimal instead of overflowing.
pub fn percent_sum(shares: &[CostShare]) -> Decimal {
    shares.iter().fold(Decimal::ZERO, |sum, share| {
        sum.checked_add(share.percent)
            .unwrap_or(if share.percent.is_sign_negative() {
                Decimal::MIN
            } else {
                Decimal::MAX
            })
    })
}

/// Splits `total` by percent, each part rounded to cents; with exactly
/// 100 % the largest share takes the rounding rest (see module docs).
pub fn split(total: Money, shares: &[CostShare]) -> Vec<ShareLine> {
    let mut lines: Vec<ShareLine> = shares
        .iter()
        .map(|share| ShareLine {
            code: share.code.trim().to_string(),
            label: cost_group_label(&share.code).to_string(),
            percent: share.percent,
            amount: total.percent(share.percent),
        })
        .collect();
    if percent_sum(shares) == Decimal::ONE_HUNDRED {
        let rest = total - lines.iter().map(|line| line.amount).sum::<Money>();
        let largest = lines.iter().enumerate().fold(
            None::<(usize, Decimal)>,
            |best, (index, line)| match best {
                Some((_, percent)) if percent >= line.percent => best,
                _ => Some((index, line.percent)),
            },
        );
        if let Some((index, _)) = largest {
            lines[index].amount += rest;
        }
    }
    lines
}

/// Count and amount of one DP-Art.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KindSum {
    pub count: u64,
    /// EP or DP-Preis used.
    pub unit_price: Decimal,
    /// count × unit_price, rounded to cents.
    pub amount: Money,
}

/// One cost group with its share and amount.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShareLine {
    pub code: String,
    pub label: String,
    pub percent: Decimal,
    pub amount: Money,
}

/// Sums of one Bauteil.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SectionResult {
    pub name: String,
    pub kinds: Vec<KindSum>,
    pub points: u64,
    /// Schätzung: KG 480; Berechnung: Gesamtsumme der DP.
    pub total: Money,
    /// Schätzung mit Spanne: DP × Kennwert von / bis.
    pub low: Option<Money>,
    pub high: Option<Money>,
    /// Berechnung: KG 481–489.
    pub shares: Vec<ShareLine>,
    pub percent_sum: Decimal,
    /// Σ KG − Gesamtsumme (0,00 € when the split is 100 %).
    pub check: Money,
    /// The Bauteil has its own split.
    pub own_shares: bool,
}

/// Sums of a whole estimate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CostResult {
    pub sections: Vec<SectionResult>,
    /// Per DP-Art over all Bauteile.
    pub kinds: Vec<KindSum>,
    pub points: u64,
    pub total: Money,
    pub low: Option<Money>,
    pub high: Option<Money>,
    /// Per cost group over all Bauteile (Schätzung: one line KG 480).
    pub groups: Vec<ShareLine>,
    /// Σ KG − Gesamtsumme.
    pub check: Money,
    pub warnings: Vec<String>,
}

/// One line of the Kostenkontrolle: Kostenermittlung ↔ LV (Planerpreise) ↔
/// Auftrag (beauftragte Angebote, netto vor Nachlass) for one cost group.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ControlLine {
    /// Three digits; empty = ohne Kostengruppe.
    pub code: String,
    pub label: String,
    pub estimate: Money,
    pub planner: Money,
    pub awarded: Money,
}

/// Kostenkontrolle je KG over all LVs of the project.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CostControl {
    pub lines: Vec<ControlLine>,
    pub estimate: Money,
    pub planner: Money,
    pub awarded: Money,
    /// Some LV has an awarded bid (otherwise the column stays empty).
    pub has_award: bool,
    /// Some LV position carries a price.
    pub has_planner: bool,
}

impl ControlLine {
    /// LV − Kostenermittlung.
    pub fn planner_difference(&self) -> Money {
        self.planner - self.estimate
    }

    /// Auftrag − Kostenermittlung.
    pub fn awarded_difference(&self) -> Money {
        self.awarded - self.estimate
    }
}

/// Compares the estimate per cost group with the tender LVs of the project
/// (sums of the planner's prices per KG, inherited from the titles) and the
/// awarded bids. A Schätzung compares KG 480 with the sum of 481–489.
/// Kosten-LVs (Kostenermittlungen as Kurztext-LV) are no tender LVs and do
/// not count here.
pub fn cost_control(project: &Project, estimate: &CostEstimate) -> CostControl {
    let result = estimate.result();
    let mut lines: BTreeMap<String, ControlLine> = BTreeMap::new();
    fn entry<'a>(lines: &'a mut BTreeMap<String, ControlLine>, code: &str) -> &'a mut ControlLine {
        lines
            .entry(code.to_string())
            .or_insert_with(|| ControlLine {
                code: code.to_string(),
                label: if code.is_empty() {
                    "ohne Kostengruppe".to_string()
                } else {
                    cost_group_label(code).to_string()
                },
                ..ControlLine::default()
            })
    }
    let to_estimate_code = |code: &str| -> String {
        if estimate.stage == CostStage::Estimate && code.starts_with("48") {
            "480".to_string()
        } else {
            code.to_string()
        }
    };
    for group in &result.groups {
        entry(&mut lines, &group.code).estimate += group.amount;
    }
    let mut has_award = false;
    let mut has_planner = false;
    for index in project.tender_lv_indices() {
        let Some(lv) = project.lv_at(index) else {
            continue;
        };
        let awarded: Vec<crate::bid::Bid> = project
            .bids_of(index)
            .iter()
            .filter(|bid| bid.is_awarded())
            .cloned()
            .collect();
        has_award |= !awarded.is_empty();
        let report = cost_group_report(lv, &awarded);
        for line in report.lines.iter().filter(|line| !line.subtotal) {
            let code = to_estimate_code(line.code.as_deref().unwrap_or(""));
            let target = entry(&mut lines, &code);
            target.planner += line.planner;
            has_planner |= !line.planner.is_zero();
            if let Some(value) = line.bidders.first() {
                target.awarded += *value;
            }
        }
    }
    let mut control = CostControl {
        has_award,
        has_planner,
        ..CostControl::default()
    };
    // Cost groups in order, „ohne Kostengruppe“ last.
    let (without, with): (Vec<ControlLine>, Vec<ControlLine>) =
        lines.into_values().partition(|line| line.code.is_empty());
    for line in with.into_iter().chain(without) {
        if line.estimate.is_zero() && line.planner.is_zero() && line.awarded.is_zero() {
            continue;
        }
        control.estimate += line.estimate;
        control.planner += line.planner;
        control.awarded += line.awarded;
        control.lines.push(line);
    }
    control
}

impl Project {
    /// Kostenschätzungen und -berechnungen of the project, in their order.
    pub fn cost_estimates(&self) -> &[CostEstimate] {
        self.cost_estimate_list()
    }

    pub fn cost_estimate(&self, id: CostEstimateId) -> Option<&CostEstimate> {
        self.cost_estimate_list()
            .iter()
            .find(|estimate| estimate.id == id)
    }

    pub fn cost_estimate_mut(&mut self, id: CostEstimateId) -> Option<&mut CostEstimate> {
        self.cost_estimate_list_mut()
            .iter_mut()
            .find(|estimate| estimate.id == id)
    }

    /// Adds an estimate at the end; it gets a new id (unless its id is free
    /// and not 0, as when reading a file).
    pub fn add_cost_estimate(&mut self, mut estimate: CostEstimate) -> CostEstimateId {
        let list = self.cost_estimate_list();
        if estimate.id.0 == 0 || list.iter().any(|other| other.id == estimate.id) {
            estimate.id =
                CostEstimateId(list.iter().map(|other| other.id.0).max().unwrap_or(0) + 1);
        }
        let id = estimate.id;
        self.cost_estimate_list_mut().push(estimate);
        id
    }

    pub fn remove_cost_estimate(&mut self, id: CostEstimateId) -> Option<CostEstimate> {
        let list = self.cost_estimate_list_mut();
        let index = list.iter().position(|estimate| estimate.id == id)?;
        Some(list.remove(index))
    }

    /// Puts another list in place (undo) and returns the one before.
    pub fn replace_cost_estimates(&mut self, estimates: Vec<CostEstimate>) -> Vec<CostEstimate> {
        std::mem::replace(self.cost_estimate_list_mut(), estimates)
    }

    /// Finds an estimate by number (1-based, in list order), id or a part of its name.
    pub fn find_cost_estimate(&self, text: &str) -> Option<&CostEstimate> {
        let list = self.cost_estimate_list();
        let text = text.trim();
        if text.is_empty() {
            return list.last();
        }
        if let Ok(number) = text.parse::<usize>() {
            return number.checked_sub(1).and_then(|index| list.get(index));
        }
        let lower = text.to_lowercase();
        list.iter()
            .find(|estimate| estimate.display_name().to_lowercase() == lower)
            .or_else(|| {
                list.iter()
                    .find(|estimate| estimate.display_name().to_lowercase().contains(&lower))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lv, LvGroup, OzMask, Position, ProjectName};

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    fn calculation() -> CostEstimate {
        let mut estimate = CostEstimate::template(CostStage::Calculation, "12.11.2026", "Prüfer");
        estimate.sections.clear();
        for (name, counts) in [
            ("Bestand", [336, 350, 195, 292, 227, 26]),
            ("Neubau", [225, 328, 128, 250, 158, 20]),
        ] {
            let mut section = CostSection::new(name);
            section.rows.push(PointRow {
                plant: "Datenpunkte".into(),
                counts: counts.to_vec(),
                ..PointRow::default()
            });
            estimate.sections.push(section);
        }
        estimate
    }

    #[test]
    fn calculation_sums_points_times_unit_price() {
        let result = calculation().result();
        assert_eq!(result.sections[0].total, Money::from_cents(51_980_000));
        assert_eq!(result.sections[1].total, Money::from_cents(40_800_000));
        assert_eq!(result.total, Money::from_cents(92_780_000));
        assert_eq!(result.kinds[0].count, 561);
        assert_eq!(result.kinds[4].amount, Money::from_cents(7_700_000));
        assert_eq!(result.points, 2535);
        let first = &result.sections[0].shares;
        assert_eq!(first[0].code, "481");
        assert_eq!(first[0].amount, Money::from_cents(20_792_000));
        assert_eq!(first[5].amount, Money::from_cents(1_559_400));
        assert_eq!(result.sections[0].check, Money::ZERO);
        assert_eq!(result.check, Money::ZERO);
        assert_eq!(result.groups.len(), 6);
        assert_eq!(
            result.groups[3].amount,
            Money::from_cents(15_594_000 + 12_240_000)
        );
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }

    #[test]
    fn the_largest_group_takes_the_rounding_rest() {
        // 1.000,01 € × 40/12/10/30/5/3 % gives 400,00 + 120,00 + 100,00 + 300,00
        // + 50,00 + 30,00 = 1.000,00 €: the cent goes to KG 481.
        let lines = split(Money::from_cents(100_001), &default_shares());
        let sum: Money = lines.iter().map(|line| line.amount).sum();
        assert_eq!(sum, Money::from_cents(100_001));
        assert_eq!(lines[0].amount, Money::from_cents(40_001));
        // Three equal thirds: the first of the largest takes the rest.
        let thirds = [
            CostShare::new("481", dec("33.34")),
            CostShare::new("482", dec("33.33")),
            CostShare::new("484", dec("33.33")),
        ];
        let lines = split(Money::from_cents(1_000), &thirds);
        assert_eq!(
            lines.iter().map(|line| line.amount).sum::<Money>(),
            Money::from_cents(1_000)
        );
        // Every part stays within a few cents of the exact value.
        for line in &lines {
            let exact = dec("10") * line.percent / Decimal::ONE_HUNDRED;
            assert!((line.amount.to_decimal() - exact).abs() <= dec("0.03"));
        }
    }

    #[test]
    fn a_split_without_100_percent_shows_the_gap() {
        let mut estimate = calculation();
        estimate.sections[1].shares = Some(vec![
            CostShare::new("481", dec("50")),
            CostShare::new("484", dec("48")),
        ]);
        let result = estimate.result();
        let neubau = &result.sections[1];
        assert!(neubau.own_shares);
        assert_eq!(neubau.percent_sum, dec("98"));
        assert_eq!(neubau.check, Money::from_cents(-816_000));
        assert_eq!(result.check, Money::from_cents(-816_000));
        assert_eq!(result.warnings.len(), 1);
        assert!(
            result.warnings[0].contains("Bauteil „Neubau“"),
            "{}",
            result.warnings[0]
        );
        assert!(
            result.warnings[0].contains("98 %"),
            "{}",
            result.warnings[0]
        );
        // Bestand keeps the split of the estimate.
        assert!(!result.sections[0].own_shares);
        assert_eq!(result.sections[0].check, Money::ZERO);
    }

    #[test]
    fn wrong_or_double_codes_and_missing_prices_warn() {
        let mut estimate = calculation();
        estimate.shares[1].code = "481".into();
        estimate.shares[2].code = "48".into();
        estimate.kinds[5].unit_price = Decimal::ZERO;
        let warnings = estimate.warnings();
        assert!(
            warnings.iter().any(|w| w.contains("KG 481 steht zweimal")),
            "{warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("„48“ ist keine Kostengruppe")),
            "{warnings:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("„BV“ hat keinen Preis")),
            "{warnings:?}"
        );
    }

    #[test]
    fn estimate_is_points_times_rate_with_span() {
        let mut estimate = CostEstimate::template(CostStage::Estimate, "09.11.2025", "");
        estimate.sections[0].rows[0] = PointRow {
            trade: "RLT".into(),
            plant: "TKA-2001".into(),
            counts: vec![12, 30, 6, 6, 0, 0],
            ..PointRow::default()
        };
        let mut row = PointRow::new("HZG", "FW-Station");
        row.set_count(1, 6);
        row.set_count(4, 1);
        estimate.sections[0].rows.push(row);
        let result = estimate.result();
        assert_eq!(result.points, 61);
        assert_eq!(result.total, Money::from_cents(61 * 45_000));
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].code, "480");
        assert_eq!(result.groups[0].label, "Gebäude- und Anlagenautomation");
        assert!(result.low.is_none());
        estimate.rate_low = Some(dec("400"));
        estimate.rate_high = Some(dec("500"));
        let result = estimate.result();
        assert_eq!(result.low, Some(Money::from_cents(61 * 40_000)));
        assert_eq!(result.high, Some(Money::from_cents(61 * 50_000)));
        // Kennwert je DP-Art: EP of the kinds, no span.
        estimate.rate_per_kind = true;
        let result = estimate.result();
        assert!(result.low.is_none());
        assert_eq!(
            result.total,
            Money::from_cents((54 * 400 + 200 + 6 * 400) * 100)
        );
        assert!(estimate.warnings().is_empty());
        estimate.rate_per_kind = false;
        estimate.rate_low = Some(dec("600"));
        assert!(estimate.warnings()[0].contains("vertauscht"));
    }

    #[test]
    fn kinds_can_be_added_moved_and_removed_with_their_counts() {
        let mut estimate = calculation();
        let index = estimate.add_kind(PointKind::new("MV", "Mehrstufenwert", dec("200")));
        assert_eq!(index, 6);
        assert_eq!(estimate.sections[0].rows[0].counts.len(), 7);
        estimate.sections[0].rows[0].set_count(6, 9);
        assert!(estimate.move_kind(6, true));
        assert_eq!(estimate.kinds[5].key, "MV");
        assert_eq!(estimate.sections[0].rows[0].count(5), 9);
        assert_eq!(estimate.sections[0].rows[0].count(6), 26);
        assert!(!estimate.move_kind(0, true));
        assert_eq!(estimate.kind_index("mv"), Some(5));
        let removed = estimate.remove_kind(5).unwrap();
        assert_eq!(removed.key, "MV");
        assert_eq!(
            estimate.sections[0].rows[0].counts,
            vec![336, 350, 195, 292, 227, 26]
        );
    }

    #[test]
    fn calculation_from_estimate_keeps_the_plant_list() {
        let mut estimate = CostEstimate::template(CostStage::Estimate, "09.11.2025", "OH");
        estimate.title = "Kostenschätzung LPH 2".into();
        estimate
            .kinds
            .push(PointKind::new("MV", "Mehrstufenwert", Decimal::ZERO));
        estimate.sections[0].rows[0].counts = vec![1, 2, 3, 4, 5, 6, 7];
        let calculation = estimate.to_calculation("12.11.2026", "OH");
        assert_eq!(calculation.stage, CostStage::Calculation);
        assert_eq!(calculation.title, "Kostenberechnung LPH 2");
        assert_eq!(
            calculation.remark,
            "aus Kostenschätzung LPH 2 vom 09.11.2025"
        );
        assert_eq!(calculation.sections, estimate.sections);
        assert_eq!(calculation.kinds[0].unit_price, dec("400"));
        assert_eq!(calculation.kinds[4].unit_price, dec("200"));
        // Unknown kind: the DP-Preis of the Schätzung.
        assert_eq!(calculation.kinds[6].unit_price, dec("450"));
        assert_eq!(calculation.shares, default_shares());
    }

    #[test]
    fn virtual_kinds_follow_the_key() {
        let kinds = default_point_kinds();
        let shown: Vec<(String, bool)> = kinds
            .iter()
            .map(|kind| (kind.key.clone(), kind.is_virtual()))
            .collect();
        assert_eq!(
            shown,
            [
                ("AI", false),
                ("BI", false),
                ("AO", false),
                ("BO", false),
                ("AV", true),
                ("BV", true)
            ]
            .map(|(key, virtual_point)| (key.to_string(), virtual_point))
        );
        assert!(PointKind::new(" mv ", "", Decimal::ZERO).is_virtual());
        assert!(!PointKind::new("MI", "Mehrstufige Eingabe", Decimal::ZERO).is_virtual());
        assert!(
            !PointKind::new("", "", Decimal::ZERO).is_virtual(),
            "neue Art: physikalisch"
        );
    }

    #[test]
    fn stage_keys_and_names() {
        for stage in CostStage::ALL {
            assert_eq!(CostStage::from_key(stage.key()), Some(stage));
            assert_eq!(CostStage::from_key(stage.label()), Some(stage));
        }
        assert_eq!(CostStage::from_key("KB"), Some(CostStage::Calculation));
        assert_eq!(CostStage::Calculation.level_label(), "DIN 276, 3. Ebene");
        assert_eq!(
            PointKind::new("AI", "Analoge Eingabe", Decimal::ZERO).caption(),
            "Analoge Eingabe (AI)"
        );
    }

    #[test]
    fn project_keeps_a_list_with_ids() {
        let mask = OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        let mut project = Project::new(
            ProjectName::parse("Test").unwrap(),
            "2026-11-18",
            Lv::new("LV", mask),
        );
        let first = project.add_cost_estimate(CostEstimate::template(CostStage::Estimate, "", ""));
        let second = project.add_cost_estimate(calculation());
        assert_eq!((first, second), (CostEstimateId(1), CostEstimateId(2)));
        assert_eq!(project.find_cost_estimate("2").map(|e| e.id), Some(second));
        assert_eq!(
            project.find_cost_estimate("schätz").map(|e| e.id),
            Some(first)
        );
        assert_eq!(project.find_cost_estimate("").map(|e| e.id), Some(second));
        let before = project.replace_cost_estimates(Vec::new());
        assert!(project.cost_estimates().is_empty());
        project.replace_cost_estimates(before);
        assert!(project.remove_cost_estimate(first).is_some());
        assert_eq!(project.cost_estimates().len(), 1);
        // A new one after removing does not take an id twice.
        assert_eq!(project.add_cost_estimate(calculation()), CostEstimateId(3));
    }

    #[test]
    fn control_compares_with_the_lvs() {
        let mask = OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        let mut lv = Lv::new("GA", mask);
        let mut automation = LvGroup::new(0, "", "Automation");
        automation.set_cost_group(Some("481".into()));
        lv.push_group(automation);
        lv.push_position(Position::new(
            "",
            "DDC",
            Decimal::ONE,
            "St",
            Some(Decimal::from(200_000)),
        ));
        let mut cabinet = LvGroup::new(0, "", "Schaltschränke");
        cabinet.set_cost_group(Some("482".into()));
        lv.push_group(cabinet);
        lv.push_position(Position::new(
            "",
            "Schrank",
            Decimal::ONE,
            "St",
            Some(Decimal::from(50_000)),
        ));
        lv.renumber();
        let mut project = Project::new(ProjectName::parse("Test").unwrap(), "2026-11-18", lv);
        let control = cost_control(&project, &calculation());
        let codes: Vec<&str> = control
            .lines
            .iter()
            .map(|line| line.code.as_str())
            .collect();
        assert_eq!(codes, vec!["481", "482", "483", "484", "485", "489"]);
        assert_eq!(control.lines[0].planner, Money::from_cents(20_000_000));
        assert_eq!(
            control.lines[0].estimate,
            Money::from_cents(20_792_000 + 16_320_000)
        );
        assert!(!control.has_award);
        assert!(control.has_planner);
        assert_eq!(control.planner, Money::from_cents(25_000_000));
        // A Schätzung compares KG 480 with the sum of 481–489.
        let estimate = CostEstimate::template(CostStage::Estimate, "", "");
        project.add_cost_estimate(estimate.clone());
        let control = cost_control(&project, &estimate);
        assert_eq!(control.lines.len(), 1);
        assert_eq!(control.lines[0].code, "480");
        assert_eq!(control.lines[0].planner, Money::from_cents(25_000_000));
        assert_eq!(
            control.lines[0].planner_difference(),
            Money::from_cents(25_000_000)
        );
        // A Kosten-LV (Kostenermittlung as Kurztext-LV) is no tender LV: its
        // prices do not count as planner's prices (else twice).
        let mut cost_lv = project.lv().clone();
        cost_lv.set_purpose(crate::lv_purpose::LvPurpose::Cost(CostStage::Calculation));
        project.add_lv(cost_lv);
        let control = cost_control(&project, &estimate);
        assert_eq!(control.lines[0].planner, Money::from_cents(25_000_000));
    }
}
