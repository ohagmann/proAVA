//! Check of an invoice and the release calculation (Rechnungsprüfung,
//! Zahlungsfreigabe). All invoices of the sequence are run through in the
//! order they were received; quantities are brought to the cumulative state
//! per position, so increment and cumulative invoices can be mixed.
//!
//! Release per invoice (amounts rounded to cents at every step):
//! 1. checked performance net (cumulative, or this invoice's increment)
//! 2. − discount of the contract (on the positions where it applies)
//! 3. − deductions (Umlagen) in percent of 2  →  invoice net
//! 4. − security retention in percent of 3
//! 5. − releases credited (cumulative mode: all earlier releases)
//! 6. ± correction  →  release net, + VAT (0 % with § 13b)  →  release gross
//! 7. cash discount (Skonto) of the gross amount, for information.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rust_decimal::{Decimal, RoundingStrategy};

use crate::billing::{Billing, CheckBasis, Invoice, InvoiceId, InvoiceKind, Mode, VatRule};
use crate::change_order::{
    billable_items, change_order_overview, pending_change_order, BillableItem, ChangeOrder,
    ChangeOrderStatus,
};
use crate::contract::Contract;
use crate::lv::{Lv, RowId};
use crate::money::{format_decimal_de, Money};
use crate::position::PositionKind;

/// What a check works with.
#[derive(Clone, Copy)]
pub struct BillingContext<'a> {
    pub contract: &'a Contract,
    pub change_orders: &'a [ChangeOrder],
    pub lv: &'a Lv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A finding of the check (Prüfvermerk / Befund).
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// Billed, but neither in the contract nor in a change order.
    NotInContract,
    /// Belongs to a change order that is not ordered (yet).
    ChangeOrderPending {
        label: String,
        status: ChangeOrderStatus,
    },
    UnitPriceDiffers {
        claimed: Decimal,
        contract: Decimal,
    },
    ArithmeticError {
        stated: Money,
        computed: Money,
    },
    QuantityReduced {
        claimed: Decimal,
        checked: Decimal,
    },
    QuantityIncreased {
        claimed: Decimal,
        checked: Decimal,
    },
    /// Cumulative quantity above the ordered quantity.
    OverOrdered {
        cumulative: Decimal,
        ordered: Decimal,
    },
    /// Claimed again although the total claimed exceeds the ordered quantity.
    PossiblyBilledTwice {
        claimed_total: Decimal,
        ordered: Decimal,
    },
    /// Hourly work without proof (Stundennachweis).
    EvidenceMissing,
    /// Invoice quantity taken over without a check.
    Unchecked,
    ProgressDecreased {
        previous: Decimal,
        now: Decimal,
    },
    /// Several sources present; `used` counts.
    SeveralSources {
        used: CheckBasis,
        others: Vec<CheckBasis>,
    },
    /// Bedarfs- or Wahlposition billed: check that it was ordered.
    ProvisionalPosition,
    TakeoffError(String),
    ProgressInvalid(String),
    /// Invoice level.
    ClaimedSumDiffers {
        stated: Money,
        computed: Money,
    },
    CheckedAboveClaimed {
        checked: Money,
        claimed: Money,
    },
    Overpayment {
        amount: Money,
    },
    ContractExceeded {
        performed: Money,
        ordered: Money,
    },
    ForecastExceeds {
        forecast: Money,
        ordered: Money,
    },
    ReverseCharge,
}

impl Finding {
    pub fn severity(&self) -> Severity {
        match self {
            Finding::TakeoffError(_) | Finding::ProgressInvalid(_) => Severity::Error,
            Finding::NotInContract
            | Finding::ChangeOrderPending { .. }
            | Finding::UnitPriceDiffers { .. }
            | Finding::ArithmeticError { .. }
            | Finding::OverOrdered { .. }
            | Finding::PossiblyBilledTwice { .. }
            | Finding::EvidenceMissing
            | Finding::ClaimedSumDiffers { .. }
            | Finding::Overpayment { .. }
            | Finding::ContractExceeded { .. }
            | Finding::ForecastExceeds { .. } => Severity::Warning,
            Finding::QuantityReduced { .. }
            | Finding::QuantityIncreased { .. }
            | Finding::Unchecked
            | Finding::ProgressDecreased { .. }
            | Finding::SeveralSources { .. }
            | Finding::ProvisionalPosition
            | Finding::CheckedAboveClaimed { .. }
            | Finding::ReverseCharge => Severity::Info,
        }
    }

    pub fn message(&self) -> String {
        let qty = |value: &Decimal| {
            format_decimal_de(*value, 3)
                .trim_end_matches('0')
                .trim_end_matches(',')
                .to_string()
        };
        match self {
            Finding::NotInContract => "Position nicht im Auftrag – nicht abrechenbar, Nachtrag nötig.".into(),
            Finding::ChangeOrderPending { label, status } => {
                format!("Nachtrag {label} ist {} – erst nach Beauftragung abrechenbar.", status.label())
            }
            Finding::UnitPriceDiffers { claimed, contract } => format!(
                "EP der Rechnung {} weicht vom Auftrag ab – geprüft mit dem EP des Auftrags {}.",
                format_decimal_de(*claimed, 2),
                format_decimal_de(*contract, 2)
            ),
            Finding::ArithmeticError { stated, computed } => format!(
                "Rechenfehler: GP der Rechnung {} statt {}.",
                stated.plain(),
                computed.plain()
            ),
            Finding::QuantityReduced { claimed, checked } => {
                format!("Menge gekürzt: {} statt {}.", qty(checked), qty(claimed))
            }
            Finding::QuantityIncreased { claimed, checked } => {
                format!("Menge erhöht: {} statt {}.", qty(checked), qty(claimed))
            }
            Finding::OverOrdered { cumulative, ordered } => {
                let percent = if ordered.is_zero() {
                    String::new()
                } else {
                    format!(" ({} %)", format_decimal_de(*cumulative / *ordered * Decimal::ONE_HUNDRED, 1))
                };
                let vob = if !ordered.is_zero() && *cumulative > *ordered * Decimal::new(11, 1) {
                    " Mehrmenge über 10 %: neuer Preis auf Verlangen (§ 2 Abs. 3 VOB/B)."
                } else {
                    ""
                };
                format!(
                    "Abgerechnet {} über der beauftragten Menge {}{percent}.{vob}",
                    qty(cumulative),
                    qty(ordered)
                )
            }
            Finding::PossiblyBilledTwice { claimed_total, ordered } => format!(
                "Möglicherweise doppelt abgerechnet: insgesamt {} in Rechnung gestellt, beauftragt {}.",
                qty(claimed_total),
                qty(ordered)
            ),
            Finding::EvidenceMissing => "Stundennachweis fehlt.".into(),
            Finding::Unchecked => "Rechnungsmenge ungeprüft übernommen.".into(),
            Finding::ProgressDecreased { previous, now } => format!(
                "Leistungsfortschritt geht zurück: {} statt bisher {}.",
                format_decimal_de(*now, 4),
                format_decimal_de(*previous, 4)
            ),
            Finding::SeveralSources { used, others } => format!(
                "Es gilt {}; {} liegt als Information vor.",
                used.label(),
                others.iter().map(|other| other.label()).collect::<Vec<_>>().join(", ")
            ),
            Finding::ProvisionalPosition => {
                "Bedarfs- oder Wahlposition – Anordnung bzw. Beauftragung prüfen.".into()
            }
            Finding::TakeoffError(error) | Finding::ProgressInvalid(error) => error.clone(),
            Finding::ClaimedSumDiffers { stated, computed } => format!(
                "Rechnungsbetrag netto {} passt nicht zur Summe der Positionen {}.",
                stated.plain(),
                computed.plain()
            ),
            Finding::CheckedAboveClaimed { checked, claimed } => format!(
                "Geprüfte Leistung {} liegt über der Forderung {}.",
                checked.plain(),
                claimed.plain()
            ),
            Finding::Overpayment { amount } => format!(
                "Überzahlung: {} mehr freigegeben als geprüft – Rückforderung.",
                (-*amount).plain()
            ),
            Finding::ContractExceeded { performed, ordered } => format!(
                "Auftragssumme überschritten: geprüft {} bei beauftragt {}.",
                performed.plain(),
                ordered.plain()
            ),
            Finding::ForecastExceeds { forecast, ordered } => format!(
                "Prognose laut Aufmaß {} liegt über der Auftragssumme {}.",
                forecast.plain(),
                ordered.plain()
            ),
            Finding::ReverseCharge => {
                "Steuerschuldnerschaft des Leistungsempfängers (§ 13b UStG): Rechnung ohne USt.".into()
            }
        }
    }
}

/// Check of one position in one invoice.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionCheck {
    pub position: RowId,
    pub ordinal: String,
    pub short_text: String,
    pub unit: String,
    /// Ordered quantity and price; None = not billable.
    pub ordered: Option<Decimal>,
    pub unit_price: Option<Decimal>,
    pub claimed_quantity: Option<Decimal>,
    pub claimed_unit_price: Option<Decimal>,
    pub claimed_total: Option<Money>,
    /// Checked quantity in the invoice's quantity mode.
    pub checked: Decimal,
    /// Cumulative before and after this invoice, and the increment.
    pub previous: Decimal,
    pub cumulative: Decimal,
    pub increment: Decimal,
    pub basis: Option<CheckBasis>,
    pub progress: Option<Decimal>,
    /// Amounts before discount.
    pub cumulative_total: Money,
    pub increment_total: Money,
    pub note: String,
    pub findings: Vec<Finding>,
}

/// The release calculation of one invoice.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Release {
    pub mode: Mode,
    /// Checked performance net (cumulative or increment, see `mode`).
    pub performance: Money,
    pub discount_percent: Decimal,
    pub discount: Money,
    pub after_discount: Money,
    /// (label, percent, amount)
    pub deductions: Vec<(String, Decimal, Money)>,
    /// Invoice net after deductions.
    pub net: Money,
    pub security_percent: Decimal,
    pub security: Money,
    pub after_security: Money,
    pub credited: Money,
    pub correction: Money,
    pub correction_text: String,
    pub release_net: Money,
    pub vat: VatRule,
    pub vat_amount: Money,
    pub release_gross: Money,
    pub cash_discount_percent: Option<Decimal>,
    pub cash_discount: Money,
    /// Releases of the sequence including this one (aktuell abgerechnet).
    pub released_total: Money,
    /// Retained after this invoice (security and rounding).
    pub withheld: Money,
}

/// An earlier release of the sequence (for the list on the release sheet).
#[derive(Debug, Clone, PartialEq)]
pub struct EarlierRelease {
    pub invoice: InvoiceId,
    pub label: String,
    pub number: String,
    pub date: String,
    pub performance: Money,
    pub release_net: Money,
    pub release_gross: Money,
}

/// Check and release of one invoice.
#[derive(Debug, Clone, PartialEq)]
pub struct InvoiceCheck {
    pub invoice: InvoiceId,
    pub kind: InvoiceKind,
    /// „3. Abschlagsrechnung“.
    pub label: String,
    /// Positions billed or changed in this invoice (cumulative release: all
    /// with a quantity so far).
    pub positions: Vec<PositionCheck>,
    /// The other positions with a quantity so far, unchanged here (for
    /// tables that show every position).
    pub unchanged: Vec<PositionCheck>,
    /// Sum of the claimed positions and lump amount.
    pub claimed_total: Money,
    pub release: Release,
    pub earlier: Vec<EarlierRelease>,
    /// Contract sum as awarded, approved change orders (after discount),
    /// together.
    pub contract_total: Money,
    pub change_orders_total: Money,
    pub ordered_total: Money,
    /// Checked lump amount without positions (pauschaler Abschlag),
    /// cumulative, before discount – part of `performed`.
    pub lump: Money,
    /// Checked performance cumulative after discount (Stand).
    pub performed: Money,
    pub performed_percent: Decimal,
    /// Forecast from the takeoff (after discount).
    pub forecast: Money,
    pub findings: Vec<Finding>,
}

impl InvoiceCheck {
    pub fn all_findings(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().chain(
            self.positions
                .iter()
                .flat_map(|position| position.findings.iter()),
        )
    }
}

/// Cumulative state while running through the sequence.
#[derive(Default)]
struct Running {
    cumulative: HashMap<RowId, Decimal>,
    amounts: HashMap<RowId, Money>,
    claimed: HashMap<RowId, Decimal>,
    claimed_count: HashMap<RowId, usize>,
    factors: HashMap<RowId, Decimal>,
    claimed_prices: HashMap<RowId, Decimal>,
    lump: Money,
    released: Money,
    earned: Money,
    earlier: Vec<EarlierRelease>,
    partials: usize,
}

impl Billing {
    /// Check and release of one invoice.
    pub fn check(&self, context: &BillingContext, id: InvoiceId) -> Result<InvoiceCheck, String> {
        self.check_all(context)
            .into_iter()
            .find(|check| check.invoice == id)
            .ok_or_else(|| "Die Rechnung gibt es nicht (mehr).".to_string())
    }

    /// Checks of all invoices in their order.
    pub fn check_all(&self, context: &BillingContext) -> Vec<InvoiceCheck> {
        let billable = billable_items(context.contract, context.change_orders, context.lv);
        let overview = change_order_overview(Some(context.contract), context.change_orders);
        let order = position_order(context.lv);
        let mut running = Running::default();
        let mut checks = Vec::new();
        for invoice in self.invoices() {
            let check = match invoice.kind {
                InvoiceKind::Single => {
                    let mut alone = Running::default();
                    self.check_one(context, &billable, &order, invoice, &mut alone, &overview)
                }
                _ => self.check_one(context, &billable, &order, invoice, &mut running, &overview),
            };
            checks.push(check);
        }
        checks
    }

    fn check_one(
        &self,
        context: &BillingContext,
        billable: &BTreeMap<RowId, BillableItem>,
        order: &HashMap<RowId, usize>,
        invoice: &Invoice,
        running: &mut Running,
        overview: &crate::change_order::ChangeOrderOverview,
    ) -> InvoiceCheck {
        let contract = context.contract;
        let label = match invoice.kind {
            InvoiceKind::Partial => {
                running.partials += 1;
                format!("{}. Abschlagsrechnung", running.partials)
            }
            kind => kind.label().to_string(),
        };
        let earlier = running.earlier.clone();
        if invoice.kind == InvoiceKind::SecurityRelease {
            return self.security_release(context, invoice, running, overview, label, earlier);
        }
        let quantity_mode = invoice.quantity_mode();
        let release_mode = invoice.release_mode();

        // Positions of this invoice: items, takeoff lines, earlier quantities.
        let mut positions: BTreeSet<RowId> = invoice.items.keys().copied().collect();
        positions.extend(
            self.takeoff()
                .iter()
                .filter(|(_, lines)| lines.iter().any(|line| line.invoice == Some(invoice.id)))
                .map(|(id, _)| *id),
        );
        positions.extend(running.cumulative.keys().copied());
        let mut positions: Vec<RowId> = positions.into_iter().collect();
        positions.sort_by_key(|id| {
            (
                order.get(id).copied().unwrap_or(usize::MAX),
                billable
                    .get(id)
                    .map(|item| item.ordinal.clone())
                    .unwrap_or_default(),
            )
        });

        let mut all = Vec::new();
        let mut claimed_total = Money::ZERO;
        for id in positions {
            let check = self.check_position(context, billable, invoice, running, id);
            if let Some(total) = check.claimed_total {
                claimed_total += total;
            }
            running.cumulative.insert(id, check.cumulative);
            running.amounts.insert(id, check.cumulative_total);
            all.push(check);
        }

        // Lump amount without positions.
        let lump_checked = invoice.checked_lump.or(invoice.claimed_lump);
        let (lump_cumulative, lump_increment) = match quantity_mode {
            Mode::Cumulative => {
                let now = lump_checked.unwrap_or(Money::ZERO);
                (now, now - running.lump)
            }
            Mode::Increment => {
                let increment = lump_checked.unwrap_or(Money::ZERO);
                (running.lump + increment, increment)
            }
        };
        running.lump = lump_cumulative;
        claimed_total += invoice.claimed_lump.unwrap_or(Money::ZERO);

        // Performance cumulative and increment, split by discount.
        let mut cumulative = Split::default();
        for (id, amount) in &running.amounts {
            cumulative.add(*amount, billable.get(id).is_none_or(|item| item.discount));
        }
        cumulative.add(lump_cumulative, true);
        let mut increment = Split::default();
        for check in &all {
            increment.add(
                check.increment_total,
                billable
                    .get(&check.position)
                    .is_none_or(|item| item.discount),
            );
        }
        increment.add(lump_increment, true);
        let (checks, unchanged): (Vec<PositionCheck>, Vec<PositionCheck>) =
            all.into_iter().partition(|check| {
                invoice.items.contains_key(&check.position)
                    || !check.increment.is_zero()
                    || !check.increment_total.is_zero()
                    || (release_mode == Mode::Cumulative && !check.cumulative.is_zero())
            });

        let deductions = invoice
            .deductions
            .clone()
            .unwrap_or_else(|| contract.terms.deductions.clone());
        let security_percent = invoice
            .security_percent
            .unwrap_or_else(|| match invoice.kind {
                InvoiceKind::Final => contract
                    .terms
                    .warranty_security_percent
                    .or(contract.terms.performance_security_percent)
                    .unwrap_or_default(),
                _ => contract
                    .terms
                    .performance_security_percent
                    .unwrap_or_default(),
            });
        let basis = match release_mode {
            Mode::Cumulative => cumulative,
            Mode::Increment => increment,
        };
        let NetChain {
            performance,
            discount,
            after_discount,
            deductions: deduction_lines,
            net,
        } = net_chain(basis, contract.discount_percent, &deductions);
        let security = net.percent(security_percent);
        let after_security = net - security;
        let credited = invoice.credited.unwrap_or(match release_mode {
            Mode::Cumulative => running.released,
            Mode::Increment => Money::ZERO,
        });
        let correction = invoice.correction.clone().unwrap_or_default();
        let release_net = after_security - credited + correction.amount;
        let vat_amount = release_net.percent(invoice.vat.percent());
        let release_gross = release_net + vat_amount;
        let cash_discount_percent = contract
            .terms
            .cash_discount_percent
            .filter(|p| !p.is_zero());
        let cash_discount = cash_discount_percent
            .map(|percent| release_gross.percent(percent))
            .unwrap_or_default();

        let cumulative_chain = net_chain(cumulative, contract.discount_percent, &deductions);
        let (cumulative_discount, earned) = (cumulative_chain.discount, cumulative_chain.net);
        running.earned = earned;
        running.released += release_net;
        let release = Release {
            mode: release_mode,
            performance,
            discount_percent: contract.discount_percent,
            discount,
            after_discount,
            deductions: deduction_lines,
            net,
            security_percent,
            security,
            after_security,
            credited,
            correction: correction.amount,
            correction_text: correction.text,
            release_net,
            vat: invoice.vat,
            vat_amount,
            release_gross,
            cash_discount_percent,
            cash_discount,
            released_total: running.released,
            withheld: earned - running.released,
        };
        running.earlier.push(EarlierRelease {
            invoice: invoice.id,
            label: label.clone(),
            number: invoice.number.clone(),
            date: invoice.date.clone(),
            performance,
            release_net,
            release_gross,
        });

        // Ordered sum, performance, forecast.
        let ordered_total = overview.total_with_change_orders;
        let performed = cumulative.total() - cumulative_discount;
        let forecast = forecast(billable, running, contract.discount_percent);

        let mut findings = Vec::new();
        if let Some(stated) = invoice.claimed_net {
            if (!invoice.items.is_empty() || invoice.claimed_lump.is_some())
                && (stated - claimed_total).cents().abs() > 1
            {
                findings.push(Finding::ClaimedSumDiffers {
                    stated,
                    computed: claimed_total,
                });
            }
            let checked = match quantity_mode {
                Mode::Cumulative => cumulative.total(),
                Mode::Increment => increment.total(),
            };
            if (checked - stated).cents() > 1 {
                findings.push(Finding::CheckedAboveClaimed {
                    checked,
                    claimed: stated,
                });
            }
        }
        if release_net.cents() < 0 {
            findings.push(Finding::Overpayment {
                amount: release_net,
            });
        }
        if performed > ordered_total {
            findings.push(Finding::ContractExceeded {
                performed,
                ordered: ordered_total,
            });
        } else if forecast > ordered_total {
            findings.push(Finding::ForecastExceeds {
                forecast,
                ordered: ordered_total,
            });
        }
        if invoice.vat == VatRule::ReverseCharge {
            findings.push(Finding::ReverseCharge);
        }

        InvoiceCheck {
            invoice: invoice.id,
            kind: invoice.kind,
            label,
            positions: checks,
            unchanged,
            claimed_total,
            release,
            earlier,
            contract_total: overview.contract_total,
            change_orders_total: overview.approved,
            ordered_total,
            lump: lump_cumulative,
            performed,
            performed_percent: percent_of(performed, ordered_total),
            forecast,
            findings,
        }
    }

    fn check_position(
        &self,
        context: &BillingContext,
        billable: &BTreeMap<RowId, BillableItem>,
        invoice: &Invoice,
        running: &mut Running,
        id: RowId,
    ) -> PositionCheck {
        let quantity_mode = invoice.quantity_mode();
        let item = invoice.items.get(&id).cloned().unwrap_or_default();
        let target = billable.get(&id);
        let previous = running.cumulative.get(&id).copied().unwrap_or_default();
        let previous_amount = running.amounts.get(&id).copied().unwrap_or_default();
        let mut findings = Vec::new();

        // Sources.
        let progress = match &item.progress {
            Some(progress) => match progress.problem() {
                Some(problem) => {
                    findings.push(Finding::ProgressInvalid(problem));
                    None
                }
                None if target.is_none() => None,
                None => Some(progress.factor),
            },
            None => None,
        };
        let has_lines = self
            .takeoff_of(id)
            .iter()
            .any(|line| line.invoice == Some(invoice.id));
        let (takeoff, errors) = self.takeoff_sum(id, Some(invoice.id));
        findings.extend(errors.into_iter().map(Finding::TakeoffError));
        let mut available = Vec::new();
        if progress.is_some() {
            available.push(CheckBasis::Progress);
        }
        if item.checked_quantity.is_some() {
            available.push(CheckBasis::Quantity);
        }
        if has_lines {
            available.push(CheckBasis::Takeoff);
        }
        if item.claimed_quantity.is_some() {
            available.push(CheckBasis::Claimed);
        }
        let basis = item
            .basis
            .filter(|basis| available.contains(basis))
            .or_else(|| available.first().copied());
        let measured: Vec<CheckBasis> = available
            .iter()
            .copied()
            .filter(|other| Some(*other) != basis && *other != CheckBasis::Claimed)
            .collect();
        if let Some(used) = basis {
            if !measured.is_empty() {
                findings.push(Finding::SeveralSources {
                    used,
                    others: measured,
                });
            }
        }

        // Cumulative quantity.
        let ordered = target.map(|target| target.quantity);
        let cumulative = match basis {
            Some(CheckBasis::Progress) => {
                ordered.unwrap_or_default() * progress.unwrap_or_default()
            }
            Some(CheckBasis::Takeoff) => previous + takeoff,
            Some(CheckBasis::Quantity) => in_sense(quantity_mode, previous, item.checked_quantity),
            Some(CheckBasis::Claimed) => in_sense(quantity_mode, previous, item.claimed_quantity),
            None => previous,
        };
        let checked = match quantity_mode {
            Mode::Cumulative => cumulative,
            Mode::Increment => cumulative - previous,
        };
        let unit_price = target.map(|target| target.unit_price);
        let cumulative_total = unit_price
            .map(|price| Money::total(price, cumulative))
            .unwrap_or_default();
        let increment_total = cumulative_total - previous_amount;

        // Claimed.
        let claimed_price = item.claimed_unit_price.or(unit_price);
        let claimed_total = item.claimed_total.map(Money::from_decimal).or_else(|| {
            item.claimed_quantity
                .zip(claimed_price)
                .map(|(quantity, price)| Money::total(price, quantity))
        });
        if let Some(claimed) = item.claimed_quantity {
            let total = match quantity_mode {
                Mode::Cumulative => claimed,
                Mode::Increment => running.claimed.get(&id).copied().unwrap_or_default() + claimed,
            };
            running.claimed.insert(id, total);
            *running.claimed_count.entry(id).or_default() += 1;
        }
        if let Some(price) = item.claimed_unit_price {
            running.claimed_prices.insert(id, price);
        }

        // Findings.
        let billed = !cumulative.is_zero() || item.claimed_quantity.is_some_and(|q| !q.is_zero());
        match target {
            None if billed => match pending_change_order(context.change_orders, id) {
                Some((order, status)) => findings.push(Finding::ChangeOrderPending {
                    label: order.label(),
                    status,
                }),
                None => findings.push(Finding::NotInContract),
            },
            None => {}
            Some(target) => {
                if let Some(claimed) = item.claimed_unit_price {
                    if claimed != target.unit_price {
                        findings.push(Finding::UnitPriceDiffers {
                            claimed,
                            contract: target.unit_price,
                        });
                    }
                }
                if !target.quantity.is_zero() && cumulative > target.quantity {
                    findings.push(Finding::OverOrdered {
                        cumulative,
                        ordered: target.quantity,
                    });
                }
                if quantity_mode == Mode::Increment
                    && running.claimed_count.get(&id).copied().unwrap_or_default() > 1
                    && item.claimed_quantity.is_some()
                {
                    let total = running.claimed.get(&id).copied().unwrap_or_default();
                    if total > target.quantity {
                        findings.push(Finding::PossiblyBilledTwice {
                            claimed_total: total,
                            ordered: target.quantity,
                        });
                    }
                }
                if target.hourly
                    && item.claimed_quantity.is_some_and(|q| !q.is_zero())
                    && !item.evidence
                {
                    findings.push(Finding::EvidenceMissing);
                }
                if billed
                    && matches!(
                        target.kind,
                        PositionKind::Provisional { .. } | PositionKind::Alternative { .. }
                    )
                {
                    findings.push(Finding::ProvisionalPosition);
                }
            }
        }
        if let (Some(quantity), Some(price), Some(stated)) = (
            item.claimed_quantity,
            item.claimed_unit_price,
            item.claimed_total,
        ) {
            let computed = Money::total(price, quantity);
            let stated = Money::from_decimal(stated);
            if (stated - computed).cents().abs() > 1 {
                findings.push(Finding::ArithmeticError { stated, computed });
            }
        }
        if let Some(claimed) = item.claimed_quantity {
            if basis != Some(CheckBasis::Claimed) && basis.is_some() {
                if checked < claimed {
                    findings.push(Finding::QuantityReduced { claimed, checked });
                } else if checked > claimed {
                    findings.push(Finding::QuantityIncreased { claimed, checked });
                }
            } else if basis == Some(CheckBasis::Claimed) && !claimed.is_zero() {
                findings.push(Finding::Unchecked);
            }
        }
        if let Some(factor) = progress.filter(|_| basis == Some(CheckBasis::Progress)) {
            if let Some(before) = running.factors.get(&id).copied() {
                if factor < before {
                    findings.push(Finding::ProgressDecreased {
                        previous: before,
                        now: factor,
                    });
                }
            }
            running.factors.insert(id, factor);
        }

        let (ordinal, short_text, unit) = match target {
            Some(target) => (
                target.ordinal.clone(),
                target.short_text.clone(),
                target.unit.clone(),
            ),
            None => match context.lv.row_by_id(id) {
                Some(row) => (
                    row.ordinal().to_string(),
                    row.short_text().to_string(),
                    row.as_position()
                        .map(|p| p.unit().to_string())
                        .unwrap_or_default(),
                ),
                None => (
                    String::new(),
                    "Position nicht mehr im LV".to_string(),
                    String::new(),
                ),
            },
        };
        PositionCheck {
            position: id,
            ordinal,
            short_text,
            unit,
            ordered,
            unit_price,
            claimed_quantity: item.claimed_quantity,
            claimed_unit_price: item.claimed_unit_price,
            claimed_total,
            checked,
            previous,
            cumulative,
            increment: cumulative - previous,
            basis,
            progress,
            cumulative_total,
            increment_total,
            note: item.note,
            findings,
        }
    }

    fn security_release(
        &self,
        context: &BillingContext,
        invoice: &Invoice,
        running: &mut Running,
        overview: &crate::change_order::ChangeOrderOverview,
        label: String,
        earlier: Vec<EarlierRelease>,
    ) -> InvoiceCheck {
        let withheld = running.earned - running.released;
        let release_net = invoice.security_release.unwrap_or(withheld);
        let vat_amount = release_net.percent(invoice.vat.percent());
        running.released += release_net;
        let release = Release {
            mode: Mode::Cumulative,
            credited: Money::ZERO,
            release_net,
            vat: invoice.vat,
            vat_amount,
            release_gross: release_net + vat_amount,
            released_total: running.released,
            withheld: running.earned - running.released,
            ..Release::default()
        };
        running.earlier.push(EarlierRelease {
            invoice: invoice.id,
            label: label.clone(),
            number: invoice.number.clone(),
            date: invoice.date.clone(),
            performance: Money::ZERO,
            release_net,
            release_gross: release.release_gross,
        });
        let mut findings = Vec::new();
        if release_net > withheld {
            findings.push(Finding::Overpayment {
                amount: withheld - release_net,
            });
        }
        if invoice.vat == VatRule::ReverseCharge {
            findings.push(Finding::ReverseCharge);
        }
        let performed = running.earned;
        InvoiceCheck {
            invoice: invoice.id,
            kind: invoice.kind,
            label,
            positions: Vec::new(),
            unchanged: Vec::new(),
            claimed_total: invoice.claimed_net.unwrap_or_default(),
            release,
            earlier,
            contract_total: context.contract.total,
            change_orders_total: overview.approved,
            ordered_total: overview.total_with_change_orders,
            lump: running.lump,
            performed,
            performed_percent: percent_of(performed, overview.total_with_change_orders),
            forecast: Money::ZERO,
            findings,
        }
    }
}

/// Amounts with and without the contract discount.
#[derive(Debug, Clone, Copy, Default)]
struct Split {
    with_discount: Money,
    without_discount: Money,
}

impl Split {
    fn add(&mut self, amount: Money, discount: bool) {
        if discount {
            self.with_discount += amount;
        } else {
            self.without_discount += amount;
        }
    }

    fn total(self) -> Money {
        self.with_discount + self.without_discount
    }
}

/// Steps 1–3 of the release: performance, discount, deductions, net.
struct NetChain {
    performance: Money,
    discount: Money,
    after_discount: Money,
    deductions: Vec<(String, Decimal, Money)>,
    net: Money,
}

fn net_chain(
    basis: Split,
    discount_percent: Decimal,
    deductions: &[crate::project::Deduction],
) -> NetChain {
    let performance = basis.total();
    let discount = basis.with_discount.percent(discount_percent);
    let after_discount = performance - discount;
    let lines: Vec<(String, Decimal, Money)> = deductions
        .iter()
        .filter(|deduction| !deduction.percent.is_zero())
        .map(|deduction| {
            (
                deduction.label.clone(),
                deduction.percent,
                after_discount.percent(deduction.percent),
            )
        })
        .collect();
    let net = after_discount - lines.iter().map(|(_, _, amount)| *amount).sum::<Money>();
    NetChain {
        performance,
        discount,
        after_discount,
        deductions: lines,
        net,
    }
}

fn in_sense(mode: Mode, previous: Decimal, value: Option<Decimal>) -> Decimal {
    let value = value.unwrap_or_default();
    match mode {
        Mode::Cumulative => value,
        Mode::Increment => previous + value,
    }
}

/// Forecast: per billable position the larger of ordered and cumulative
/// quantity, plus positions billed without contract at the claimed price.
fn forecast(
    billable: &BTreeMap<RowId, BillableItem>,
    running: &Running,
    discount_percent: Decimal,
) -> Money {
    let mut split = Split::default();
    for (id, item) in billable {
        let cumulative = running.cumulative.get(id).copied().unwrap_or_default();
        // Bedarfs- and Wahlpositionen count only as far as they are billed.
        let quantity = if item.kind.counts_in_total() {
            cumulative.max(item.quantity)
        } else {
            cumulative
        };
        split.add(Money::total(item.unit_price, quantity), item.discount);
    }
    let unordered: Money = running
        .cumulative
        .iter()
        .filter(|(id, quantity)| !billable.contains_key(id) && !quantity.is_zero())
        .filter_map(|(id, quantity)| {
            running
                .claimed_prices
                .get(id)
                .map(|price| Money::total(*price, *quantity))
        })
        .sum();
    split.total() - split.with_discount.percent(discount_percent) + unordered
}

fn percent_of(part: Money, whole: Money) -> Decimal {
    if whole.is_zero() {
        return Decimal::ZERO;
    }
    (part.to_decimal() / whole.to_decimal() * Decimal::ONE_HUNDRED)
        .round_dp_with_strategy(1, RoundingStrategy::MidpointAwayFromZero)
}

fn position_order(lv: &Lv) -> HashMap<RowId, usize> {
    lv.rows()
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id(), index))
        .collect()
}

#[cfg(test)]
mod tests;
