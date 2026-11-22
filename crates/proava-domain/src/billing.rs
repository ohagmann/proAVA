//! Abrechnung: invoices of the contractor with the planner's check, the
//! quantity takeoff (Aufmaß) and the agreed progress (Leistungsfortschritt).
//! This module holds the data; the check and the release calculation are in
//! [`crate::billing_check`].

use std::collections::BTreeMap;

use rust_decimal::Decimal;

use crate::lv::RowId;
use crate::money::Money;
use crate::project::Deduction;
use crate::quantity_formula;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct InvoiceId(pub u64);

/// Kind of invoice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InvoiceKind {
    /// Abschlags- oder Teilrechnung: part of the sequence up to the final
    /// invoice.
    #[default]
    Partial,
    /// Schlussrechnung: the whole performance, always cumulative, all
    /// earlier releases are credited.
    Final,
    /// Einzelrechnung (Regie-, Nachtragsrechnung) outside the sequence; it is
    /// checked on its own and not credited against the final invoice.
    Single,
    /// Freigabe des Sicherheitseinbehalts (e.g. against a guarantee).
    SecurityRelease,
}

impl InvoiceKind {
    pub const ALL: [InvoiceKind; 4] = [
        InvoiceKind::Partial,
        InvoiceKind::Final,
        InvoiceKind::Single,
        InvoiceKind::SecurityRelease,
    ];

    pub fn label(self) -> &'static str {
        match self {
            InvoiceKind::Partial => "Abschlagsrechnung",
            InvoiceKind::Final => "Schlussrechnung",
            InvoiceKind::Single => "Einzelrechnung",
            InvoiceKind::SecurityRelease => "Freigabe Sicherheitseinbehalt",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            InvoiceKind::Partial => "partial",
            InvoiceKind::Final => "final",
            InvoiceKind::Single => "single",
            InvoiceKind::SecurityRelease => "security-release",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key.trim())
    }

    /// GAEB DA XML InvoiceType.
    pub fn gaeb(self) -> Option<&'static str> {
        match self {
            InvoiceKind::Partial => Some("deduction"),
            InvoiceKind::Final => Some("final account"),
            InvoiceKind::Single => Some("single invoice"),
            InvoiceKind::SecurityRelease => None,
        }
    }

    pub fn from_gaeb(value: &str) -> Option<Self> {
        Some(match value.trim() {
            "deduction" | "part final account" | "advance payment" => InvoiceKind::Partial,
            "final account" => InvoiceKind::Final,
            "single invoice" => InvoiceKind::Single,
            _ => return None,
        })
    }

    /// Part of the sequence whose releases are credited later.
    pub fn in_sequence(self) -> bool {
        !matches!(self, InvoiceKind::Single)
    }
}

/// How quantities are stated, and how the release is calculated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    /// Total reached so far (Gesamtstand). For the release: total minus the
    /// releases so far (anrechenbare Freigaben).
    #[default]
    Cumulative,
    /// Only what is new in this invoice (Zuwachs). For the release: this
    /// invoice on its own.
    Increment,
}

impl Mode {
    pub fn key(self) -> &'static str {
        match self {
            Mode::Cumulative => "cumulative",
            Mode::Increment => "increment",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "cumulative" => Some(Mode::Cumulative),
            "increment" => Some(Mode::Increment),
            _ => None,
        }
    }

    pub fn quantity_label(self) -> &'static str {
        match self {
            Mode::Cumulative => "kumuliert (Gesamtstand)",
            Mode::Increment => "Zuwachs (nur diese Rechnung)",
        }
    }

    pub fn release_label(self) -> &'static str {
        match self {
            Mode::Cumulative => "kumuliert, bisherige Freigaben angerechnet",
            Mode::Increment => "nur diese Rechnung",
        }
    }
}

/// Value added tax of an invoice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VatRule {
    Rate(Decimal),
    /// Steuerschuldnerschaft des Leistungsempfängers (§ 13b UStG): no VAT
    /// on the invoice.
    ReverseCharge,
}

impl Default for VatRule {
    fn default() -> Self {
        VatRule::Rate(Decimal::from(19))
    }
}

impl VatRule {
    pub fn percent(self) -> Decimal {
        match self {
            VatRule::Rate(percent) => percent,
            VatRule::ReverseCharge => Decimal::ZERO,
        }
    }

    pub fn label(self) -> String {
        match self {
            VatRule::Rate(percent) => format!("USt {} %", crate::format_decimal_de(percent, 0)),
            VatRule::ReverseCharge => "USt 0 % (§ 13b UStG)".to_string(),
        }
    }
}

/// Progress agreed for a position (Leistungsfortschritt): the share of the
/// ordered quantity (or of the lump sum) reached so far, 0 … 1 in steps of
/// 0,0001, with a note why (e.g. „Absprache mit AN vom …“).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Progress {
    pub factor: Decimal,
    pub note: String,
}

impl Progress {
    /// Highest number of decimals of the factor (37,50 % = 0,3750).
    pub const DECIMALS: u32 = 4;

    /// Checks range, step and note.
    pub fn problem(&self) -> Option<String> {
        if self.factor.is_sign_negative() || self.factor > Decimal::ONE {
            return Some(format!(
                "Leistungsfortschritt {} liegt nicht zwischen 0 und 1 – mehr als die beauftragte Menge nur über Aufmaß oder geprüfte Menge.",
                crate::format_decimal_de(self.factor, 4)
            ));
        }
        if self.factor.normalize().scale() > Self::DECIMALS {
            return Some(format!(
                "Leistungsfortschritt {}: höchstens vier Nachkommastellen (Schritt 0,0001).",
                self.factor.normalize()
            ));
        }
        if self.note.trim().is_empty() {
            return Some(
                "Leistungsfortschritt ohne Vermerk – bitte angeben, worauf er beruht.".to_string(),
            );
        }
        None
    }

    /// Factor from a percent value with two decimals (37,50 → 0,3750).
    pub fn factor_from_percent(percent: Decimal) -> Decimal {
        percent / Decimal::ONE_HUNDRED
    }
}

/// Which value counts as checked for a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CheckBasis {
    /// The contractor's quantity, taken over unchecked.
    Claimed,
    /// Quantity checked and entered by hand.
    Quantity,
    /// Sum of the takeoff lines of this invoice (Zuwachs).
    Takeoff,
    /// Agreed progress (cumulative share of the ordered quantity).
    Progress,
}

impl CheckBasis {
    pub fn label(self) -> &'static str {
        match self {
            CheckBasis::Claimed => "Rechnungsmenge",
            CheckBasis::Quantity => "geprüfte Menge",
            CheckBasis::Takeoff => "Aufmaß",
            CheckBasis::Progress => "Leistungsfortschritt",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            CheckBasis::Claimed => "claimed",
            CheckBasis::Quantity => "quantity",
            CheckBasis::Takeoff => "takeoff",
            CheckBasis::Progress => "progress",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [
            CheckBasis::Claimed,
            CheckBasis::Quantity,
            CheckBasis::Takeoff,
            CheckBasis::Progress,
        ]
        .into_iter()
        .find(|basis| basis.key() == key.trim())
    }
}

/// One position of an invoice: what the contractor claims and what the
/// planner checked. Quantities are meant in the invoice's [`Mode`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InvoiceItem {
    pub claimed_quantity: Option<Decimal>,
    pub claimed_unit_price: Option<Decimal>,
    /// GP as written on the invoice (for the arithmetic check).
    pub claimed_total: Option<Decimal>,
    /// Checked quantity entered by hand.
    pub checked_quantity: Option<Decimal>,
    pub progress: Option<Progress>,
    /// Which source counts; None = automatic (progress, then checked
    /// quantity, then takeoff, then the invoice quantity).
    pub basis: Option<CheckBasis>,
    /// Planner's note for the position (Prüfvermerk).
    pub note: String,
    /// Proof present (Stundennachweis, Aufmaßblatt, Lieferschein).
    pub evidence: bool,
}

/// A correction of the release with its reason (Rundung / Korrektur).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Correction {
    pub amount: Money,
    pub text: String,
}

/// An invoice of the contractor and its check.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Invoice {
    pub id: InvoiceId,
    /// Invoice number of the contractor.
    pub number: String,
    /// Invoice date, date received, payment due (TT.MM.JJJJ).
    pub date: String,
    pub received: String,
    pub due: String,
    /// Service period (Leistungszeitraum).
    pub period_start: String,
    pub period_end: String,
    pub kind: InvoiceKind,
    /// How the contractor states quantities.
    pub quantities: Mode,
    /// How the release is calculated (Final: always cumulative).
    pub release: Mode,
    /// Net amount claimed on the invoice (Forderung netto).
    pub claimed_net: Option<Money>,
    /// A lump amount claimed without positions (pauschaler Abschlag), meant
    /// in the release mode; checked lump: None = as claimed.
    pub claimed_lump: Option<Money>,
    pub checked_lump: Option<Money>,
    pub items: BTreeMap<RowId, InvoiceItem>,
    pub vat: VatRule,
    /// Security retention in percent; None = from the contract terms
    /// (Vertragserfüllung, at the final invoice Mängelansprüche).
    pub security_percent: Option<Decimal>,
    /// Deductions; None = from the contract terms.
    pub deductions: Option<Vec<Deduction>>,
    /// Releases credited, entered by hand; None = sum of the earlier
    /// releases (cumulative release) or nothing (increment).
    pub credited: Option<Money>,
    pub correction: Option<Correction>,
    /// For [`InvoiceKind::SecurityRelease`]: amount released; None = all
    /// that is withheld.
    pub security_release: Option<Money>,
    /// Note for the whole invoice (Prüfvermerk), date and person checking.
    pub note: String,
    pub checked_date: String,
    pub checked_by: String,
}

impl Invoice {
    pub fn new(kind: InvoiceKind, number: impl Into<String>, date: impl Into<String>) -> Self {
        Self {
            kind,
            number: number.into(),
            date: date.into(),
            ..Self::default()
        }
    }

    /// Release mode actually used.
    pub fn release_mode(&self) -> Mode {
        match self.kind {
            InvoiceKind::Final => Mode::Cumulative,
            InvoiceKind::Single => Mode::Increment,
            _ => self.release,
        }
    }

    /// Quantity mode actually used.
    pub fn quantity_mode(&self) -> Mode {
        match self.kind {
            InvoiceKind::Final => Mode::Cumulative,
            InvoiceKind::Single => Mode::Increment,
            _ => self.quantities,
        }
    }

    pub fn item_mut(&mut self, position: RowId) -> &mut InvoiceItem {
        self.items.entry(position).or_default()
    }
}

/// One line of a quantity takeoff (Aufmaßzeile): place and calculation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TakeoffLine {
    /// Where / what, e.g. „EG Raum 0.12, Strang 3“.
    pub label: String,
    /// Rechenansatz with German decimals, e.g. „2*12,5+3“.
    pub formula: String,
    /// Invoice the line belongs to (it counts as Zuwachs there); None = not
    /// yet assigned.
    pub invoice: Option<InvoiceId>,
}

impl TakeoffLine {
    pub fn result(&self) -> Result<Decimal, String> {
        if self.formula.trim().is_empty() {
            return Ok(Decimal::ZERO);
        }
        quantity_formula::evaluate(&self.formula)
    }
}

/// Billing of one LV: invoices in the order they were received, and the
/// takeoff per position.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Billing {
    invoices: Vec<Invoice>,
    takeoff: BTreeMap<RowId, Vec<TakeoffLine>>,
    next_invoice_id: u64,
}

impl Billing {
    pub fn is_empty(&self) -> bool {
        self.invoices.is_empty() && self.takeoff.values().all(Vec::is_empty)
    }

    pub fn invoices(&self) -> &[Invoice] {
        &self.invoices
    }

    pub fn invoice(&self, id: InvoiceId) -> Option<&Invoice> {
        self.invoices.iter().find(|invoice| invoice.id == id)
    }

    pub fn invoice_mut(&mut self, id: InvoiceId) -> Option<&mut Invoice> {
        self.invoices.iter_mut().find(|invoice| invoice.id == id)
    }

    /// Adds an invoice at the end and returns its id. A fresh id is handed
    /// out unless the invoice already has a free one (project file).
    pub fn add_invoice(&mut self, mut invoice: Invoice) -> InvoiceId {
        if invoice.id.0 == 0 || self.invoice(invoice.id).is_some() {
            invoice.id = InvoiceId(self.next_invoice_id.max(1));
        }
        self.next_invoice_id = self.next_invoice_id.max(invoice.id.0 + 1);
        let id = invoice.id;
        self.invoices.push(invoice);
        id
    }

    /// Removes an invoice; its takeoff lines stay, unassigned.
    pub fn remove_invoice(&mut self, id: InvoiceId) -> Option<Invoice> {
        let index = self.invoices.iter().position(|invoice| invoice.id == id)?;
        for line in self.takeoff.values_mut().flatten() {
            if line.invoice == Some(id) {
                line.invoice = None;
            }
        }
        Some(self.invoices.remove(index))
    }

    /// Previous invoice of the same sequence (for defaults of a new one).
    pub fn last_in_sequence(&self) -> Option<&Invoice> {
        self.invoices
            .iter()
            .rev()
            .find(|invoice| invoice.kind.in_sequence())
    }

    pub fn takeoff(&self) -> &BTreeMap<RowId, Vec<TakeoffLine>> {
        &self.takeoff
    }

    pub fn takeoff_of(&self, position: RowId) -> &[TakeoffLine] {
        self.takeoff
            .get(&position)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn takeoff_mut(&mut self, position: RowId) -> &mut Vec<TakeoffLine> {
        self.takeoff.entry(position).or_default()
    }

    /// Sum of the valid takeoff lines of a position for one invoice
    /// (None = not assigned) and the errors of the others.
    pub fn takeoff_sum(
        &self,
        position: RowId,
        invoice: Option<InvoiceId>,
    ) -> (Decimal, Vec<String>) {
        let mut sum = Decimal::ZERO;
        let mut errors = Vec::new();
        for line in self
            .takeoff_of(position)
            .iter()
            .filter(|line| line.invoice == invoice)
        {
            match line.result() {
                Ok(value) => sum += value,
                Err(error) => errors.push(format!("Aufmaßzeile „{}“: {error}", line.label.trim())),
            }
        }
        (sum, errors)
    }

    /// Appends takeoff lines read from a file (X89/X31), all assigned to
    /// `invoice` (None = not yet assigned).
    pub fn add_takeoff(
        &mut self,
        lines: Vec<(RowId, Vec<TakeoffLine>)>,
        invoice: Option<InvoiceId>,
    ) -> usize {
        let mut count = 0;
        for (row, new_lines) in lines {
            for mut line in new_lines {
                line.invoice = invoice;
                self.takeoff_mut(row).push(line);
                count += 1;
            }
        }
        count
    }

    pub fn next_invoice_id(&self) -> u64 {
        self.next_invoice_id
    }

    /// Keeps ids unique after reading a project file.
    pub fn reserve_invoice_ids(&mut self, next: u64) {
        self.next_invoice_id = self.next_invoice_id.max(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    #[test]
    fn progress_limits() {
        let progress = |factor: &str, note: &str| Progress {
            factor: dec(factor),
            note: note.into(),
        };
        for ok in ["0", "0.0001", "0.375", "0.9999", "1", "1.0000"] {
            assert_eq!(progress(ok, "Absprache").problem(), None, "{ok}");
        }
        assert!(progress("1.0001", "x")
            .problem()
            .unwrap()
            .contains("nicht zwischen 0 und 1"));
        assert!(progress("-0.0001", "x")
            .problem()
            .unwrap()
            .contains("nicht zwischen 0 und 1"));
        assert!(progress("0.12345", "x")
            .problem()
            .unwrap()
            .contains("vier Nachkommastellen"));
        assert!(progress("0.5", " ").problem().unwrap().contains("Vermerk"));
        assert_eq!(Progress::factor_from_percent(dec("37.50")), dec("0.375"));
    }

    #[test]
    fn invoices_and_takeoff() {
        let mut billing = Billing::default();
        let first = billing.add_invoice(Invoice::new(InvoiceKind::Partial, "R1", "01.02.2026"));
        let second = billing.add_invoice(Invoice::new(InvoiceKind::Single, "R2", ""));
        assert_ne!(first, second);
        assert_eq!(billing.last_in_sequence().unwrap().id, first);
        let position = RowId(7);
        billing.takeoff_mut(position).extend([
            TakeoffLine {
                label: "EG".into(),
                formula: "2*12,5".into(),
                invoice: Some(first),
            },
            TakeoffLine {
                label: "OG".into(),
                formula: "3+".into(),
                invoice: Some(first),
            },
            TakeoffLine {
                label: "DG".into(),
                formula: "4".into(),
                invoice: None,
            },
        ]);
        let (sum, errors) = billing.takeoff_sum(position, Some(first));
        assert_eq!(sum, dec("25"));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("„OG“"));
        assert_eq!(billing.takeoff_sum(position, None).0, dec("4"));
        billing.remove_invoice(first);
        assert_eq!(billing.takeoff_sum(position, None).0, dec("29"));
        assert!(billing.invoice(first).is_none());
        // An id from the file is kept when free.
        let mut invoice = Invoice::new(InvoiceKind::Final, "S", "");
        invoice.id = InvoiceId(10);
        assert_eq!(billing.add_invoice(invoice), InvoiceId(10));
        assert_eq!(billing.add_invoice(Invoice::default()), InvoiceId(11));
        assert_eq!(
            InvoiceKind::from_gaeb("final account"),
            Some(InvoiceKind::Final)
        );
        assert_eq!(VatRule::ReverseCharge.label(), "USt 0 % (§ 13b UStG)");
        assert_eq!(VatRule::default().label(), "USt 19 %");
    }
}
