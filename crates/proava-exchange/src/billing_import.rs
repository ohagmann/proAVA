//! Abrechnung from GAEB: X86/D86 as the contract, X89/X89B as an invoice of
//! the contractor, X31 as quantity takeoff. Positions are matched with the
//! LV by order number like the bids (without leading zeros; without index
//! when that is unique).

use std::collections::{BTreeMap, HashMap};

use proava_application::{ContractImport, InvoiceImport};
use proava_domain::{
    format_decimal_de, Award, Contract, Decimal, Invoice, InvoiceItem, InvoiceKind, Lv, Mode,
    Money, Project, RowId, TakeoffLine, VatRule,
};
use proava_gaeb::xml::{BillingPhase, GaebBilling, GaebTakeoff};
use proava_gaeb::GaebDocument;

use crate::oz::{match_key, position_keys, MaskLayout};
use crate::text::short_list;

/// LV positions by order number key.
struct PositionKeys {
    full: HashMap<String, RowId>,
    base: HashMap<String, Vec<RowId>>,
}

impl PositionKeys {
    fn of(lv: &Lv) -> Self {
        let layout = MaskLayout::of(lv.oz_mask());
        let mut full: HashMap<String, RowId> = HashMap::new();
        let mut base: HashMap<String, Vec<RowId>> = HashMap::new();
        for position in lv.positions() {
            for (full_key, base_key) in position_keys(&layout, position.ordinal()) {
                full.entry(full_key).or_insert(position.id());
                base.entry(base_key).or_default().push(position.id());
            }
        }
        Self { full, base }
    }

    /// `pieces` without the index, `index` may be empty.
    fn find(&self, pieces: &[String], index: &str) -> Option<RowId> {
        let mut all = pieces.to_vec();
        if !index.trim().is_empty() {
            all.push(index.trim().to_string());
        }
        self.full.get(&match_key(&all)).copied().or_else(|| {
            let candidates = self.base.get(&match_key(pieces))?;
            (candidates.len() == 1).then(|| candidates[0])
        })
    }
}

/// „2027-04-30“ → „30.04.2027“; anything else as it is.
fn german_date(iso: &str) -> String {
    let iso = iso.trim();
    let date = iso.get(..10).unwrap_or(iso);
    let parts: Vec<&str> = date.split('-').collect();
    match parts[..] {
        [year, month, day] if year.len() == 4 => format!("{day}.{month}.{year}"),
        _ => iso.to_string(),
    }
}

/// A number as a Rechenansatz („12,5“, negative with a leading minus).
fn formula_number(value: Decimal) -> String {
    let text = format_decimal_de(value, 3);
    let text = text
        .trim_end_matches('0')
        .trim_end_matches(',')
        .replace('.', "");
    text
}

/// Auftrags-LV (X86/D86) of the active LV as its contract: prices and
/// quantities of the file, discount from its totals.
pub fn contract_from_gaeb(
    project: &Project,
    document: &GaebDocument,
    source: &str,
) -> Result<ContractImport, String> {
    let lv = project.lv();
    let mut import = crate::match_bid(project, document, source);
    let mut warnings: Vec<String> = Vec::new();
    if document.phase.number() != 86 {
        warnings.push(format!(
            "Die Datei ist Phase {}, kein Auftrags-LV (86) – sie wird als Auftrag gelesen.",
            document.phase.number()
        ));
    }
    // The bid matcher speaks of offers; keep its findings except the phase
    // and quantity notes, which are said here for the contract.
    warnings.extend(import.warnings.drain(..).filter(|warning| {
        !warning.starts_with("Die Datei ist Phase") && !warning.contains("Mengen")
    }));
    let mut bid = import.bid;
    let total = bid.total_after_discount(lv);
    let date = document
        .date
        .as_deref()
        .map(german_date)
        .unwrap_or_default();
    bid.set_award(Some(Award { date, total }));
    let mut contract = Contract::from_award(lv, &bid, &project.info().terms)?;
    contract.bid = None;

    // Quantities of the Auftrags-LV.
    let keys = PositionKeys::of(lv);
    let mut changed = Vec::new();
    for (pieces, item) in document.award.boq.items() {
        let (pieces, index) = if item.rno_index.trim().is_empty() {
            (pieces.clone(), String::new())
        } else {
            (pieces[..pieces.len() - 1].to_vec(), item.rno_index.clone())
        };
        let (Some(row), Some(quantity)) = (keys.find(&pieces, &index), item.qty) else {
            continue;
        };
        if let Some(entry) = contract.items.get_mut(&row) {
            if entry.quantity != quantity {
                changed.push(format!(
                    "{} ({} statt {})",
                    entry.ordinal,
                    formula_number(quantity),
                    formula_number(entry.quantity)
                ));
                entry.quantity = quantity;
            }
        }
    }
    if !changed.is_empty() {
        let net = contract.net_before_discount();
        contract.total = net - net.percent(contract.discount_percent);
        warnings.push(format!(
            "Auftragsmengen aus der Datei übernommen, abweichend vom LV: {}",
            short_list(&changed, 15)
        ));
    }
    Ok(ContractImport { contract, warnings })
}

fn kind_of(raw: &str, warnings: &mut Vec<String>) -> InvoiceKind {
    match raw.trim().to_lowercase().as_str() {
        "" | "deduction" => InvoiceKind::Partial,
        "final account" => InvoiceKind::Final,
        "single invoice" => InvoiceKind::Single,
        other => {
            let label = match other {
                "part final account" => "Teilschlussrechnung",
                "advance payment" => "Vorauszahlung",
                "pro forma invoice" => "Proforma-Rechnung",
                "reviewed invoice" => "geprüfte Rechnung",
                _ => other,
            };
            warnings.push(format!(
                "Rechnungsart „{label}“ als Abschlagsrechnung angelegt – Art im Rechnungskopf prüfen."
            ));
            InvoiceKind::Partial
        }
    }
}

/// Takeoff lines of one item: the REB rows as text (they are not computed
/// here), then the quantity of the file as the line that counts. For a
/// cumulative invoice that line is the Zuwachs: cumulative minus the
/// quantity checked before.
fn takeoff_lines(
    takeoff: &GaebTakeoff,
    source: &str,
    previous: Option<Decimal>,
) -> Vec<TakeoffLine> {
    let mut lines: Vec<TakeoffLine> = takeoff
        .rows
        .iter()
        .filter(|row| !row.row.trim().is_empty() || !row.explanation.is_empty())
        .map(|row| TakeoffLine {
            label: if row.explanation.is_empty() {
                format!("REB: {}", row.row.trim())
            } else {
                format!("{} (REB: {})", row.explanation, row.row.trim())
            },
            formula: String::new(),
            invoice: None,
        })
        .collect();
    if let Some(qty) = takeoff.qty {
        let (label, formula) = match previous.filter(|previous| !previous.is_zero()) {
            Some(previous) => (
                format!("Aufmaßmenge laut {source}, kumuliert, abzüglich bisher geprüft"),
                format!("{}-{}", formula_number(qty), formula_number(previous)),
            ),
            None => (format!("Aufmaßmenge laut {source}"), formula_number(qty)),
        };
        lines.push(TakeoffLine {
            label,
            formula,
            invoice: None,
        });
    }
    lines
}

const REB_NOTE: &str =
    "Die Aufmaßzeilen nach REB 23.003 stehen als Text beim Aufmaß; es zählt je Position die \
                        Aufmaßmenge der Datei (proAVA rechnet die REB-Formeln nicht nach).";

fn has_reb_rows(matched: &[(RowId, &proava_gaeb::xml::GaebBillItem)]) -> bool {
    matched.iter().any(|(_, item)| {
        item.takeoff
            .as_ref()
            .is_some_and(|takeoff| !takeoff.rows.is_empty())
    })
}

/// Invoice (X89/X89B) or takeoff (X31) of the contractor for the active LV.
pub fn invoice_from_gaeb(
    project: &Project,
    billing: &GaebBilling,
    source: &str,
) -> Result<InvoiceImport, String> {
    let context = project
        .billing_context()
        .ok_or("Für das LV ist noch kein Auftrag festgeschrieben – erst im Reiter Abrechnung den Auftrag übernehmen.")?;
    let lv = project.lv();
    let keys = PositionKeys::of(lv);
    let mut warnings = Vec::new();
    let mut unknown = Vec::new();
    let mut matched: Vec<(RowId, &proava_gaeb::xml::GaebBillItem)> = Vec::new();
    let mut seen: BTreeMap<RowId, ()> = BTreeMap::new();
    let mut twice = Vec::new();
    for item in &billing.items {
        let shown = item.pieces.join(".");
        match keys.find(&item.pieces, &item.rno_index) {
            Some(row) if seen.insert(row, ()).is_none() => matched.push((row, item)),
            Some(_) => twice.push(shown),
            None => unknown.push(shown),
        }
    }
    if !unknown.is_empty() {
        warnings.push(format!(
            "{} Position(en) der Datei gibt es im LV nicht – nicht übernommen: {}",
            unknown.len(),
            short_list(&unknown, 15)
        ));
    }
    if !twice.is_empty() {
        warnings.push(format!(
            "Doppelt in der Datei, nur die erste übernommen: {}",
            short_list(&twice, 15)
        ));
    }
    let not_billable: Vec<String> = matched
        .iter()
        .filter(|(row, _)| !context.contract.items.contains_key(row))
        .filter_map(|(row, _)| {
            lv.position_by_id(*row)
                .map(|position| position.ordinal().to_string())
        })
        .collect();
    if !not_billable.is_empty() && billing.phase != BillingPhase::Takeoff {
        warnings.push(format!(
            "Nicht im Auftrag (Nachtrag prüfen): {}",
            short_list(&not_billable, 15)
        ));
    }
    let format = billing.phase.label().to_string();

    if billing.phase == BillingPhase::Takeoff {
        let takeoff: Vec<(RowId, Vec<TakeoffLine>)> = matched
            .iter()
            .filter_map(|(row, item)| {
                let lines = takeoff_lines(item.takeoff.as_ref()?, source, None);
                (!lines.is_empty()).then_some((*row, lines))
            })
            .collect();
        if takeoff.is_empty() {
            return Err(
                "Die Mengenermittlung enthält keine Aufmaßmengen für Positionen des LV."
                    .to_string(),
            );
        }
        // Only REB rows and no quantity (some programs write no Qty): the
        // takeoff of these positions counts 0 – say so instead of keeping quiet.
        let without_quantity: Vec<String> = matched
            .iter()
            .filter(|(_, item)| {
                item.takeoff
                    .as_ref()
                    .is_some_and(|takeoff| takeoff.qty.is_none() && !takeoff.rows.is_empty())
            })
            .filter_map(|(row, _)| {
                lv.position_by_id(*row)
                    .map(|position| position.ordinal().to_string())
            })
            .collect();
        if !without_quantity.is_empty() {
            warnings.push(format!(
                "{} Position(en) nur mit REB-Zeilen, ohne Aufmaßmenge in der Datei – ihr Aufmaß zählt 0, \
                 bis die Menge eingetragen ist: {}",
                without_quantity.len(),
                short_list(&without_quantity, 15)
            ));
        }
        if has_reb_rows(&matched) {
            warnings.push(REB_NOTE.to_string());
        }
        return Ok(InvoiceImport {
            format,
            invoice: None,
            takeoff,
            warnings,
        });
    }

    let header = billing.header.clone().unwrap_or_default();
    let mut invoice = Invoice::new(
        kind_of(&header.kind, &mut warnings),
        header.number.clone(),
        german_date(&header.date),
    );
    if header.credit_note {
        warnings
            .push("Die Datei ist eine Gutschrift – Beträge im Rechnungskopf prüfen.".to_string());
    }
    invoice.period_start = german_date(&header.period_start);
    invoice.period_end = german_date(&header.period_end);
    invoice.quantities = if header.settlement.trim().eq_ignore_ascii_case("periodic") {
        Mode::Increment
    } else {
        Mode::Cumulative
    };
    if let Some(last) = project.billing().last_in_sequence() {
        invoice.release = last.release;
        invoice.vat = last.vat;
    } else {
        invoice.vat = VatRule::Rate(lv.vat_percent());
    }
    invoice.claimed_net = billing.total.map(Money::from_decimal);
    for (row, item) in &matched {
        let quantity = item
            .bill_qty
            .or_else(|| item.takeoff.as_ref().and_then(|takeoff| takeoff.qty));
        let entry = InvoiceItem {
            claimed_quantity: quantity,
            claimed_unit_price: item.unit_price,
            claimed_total: item.total,
            note: item.check_note.clone(),
            ..InvoiceItem::default()
        };
        if entry != InvoiceItem::default() {
            invoice.items.insert(*row, entry);
        }
    }
    if invoice.items.is_empty() {
        return Err("Die Rechnung enthält keine Positionen des LV.".to_string());
    }

    // Cumulative: the takeoff line is the Zuwachs over what was checked before.
    let previous: HashMap<RowId, Decimal> = if invoice.quantities == Mode::Cumulative {
        let mut copy = project.clone();
        let id = copy.billing_mut().add_invoice(invoice.clone());
        copy.billing_context()
            .and_then(|context| copy.billing().check(&context, id).ok())
            .map(|check| {
                check
                    .positions
                    .iter()
                    .chain(check.unchanged.iter())
                    .map(|line| (line.position, line.previous))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    let takeoff: Vec<(RowId, Vec<TakeoffLine>)> = matched
        .iter()
        .filter_map(|(row, item)| {
            let lines = takeoff_lines(item.takeoff.as_ref()?, source, previous.get(row).copied());
            Some((*row, lines))
        })
        .filter(|(_, lines)| !lines.is_empty())
        .collect();
    if has_reb_rows(&matched) {
        warnings.push(REB_NOTE.to_string());
    }
    if invoice.number.trim().is_empty() {
        warnings.push(
            "Die Datei nennt keine Rechnungsnummer – bitte im Rechnungskopf eintragen.".to_string(),
        );
    }
    if let Some(contractor) = &billing.contractor {
        let name = contractor.name1.trim();
        if !name.is_empty() && !name.eq_ignore_ascii_case(context.contract.contractor.trim()) {
            warnings.push(format!(
                "Auftragnehmer der Datei „{name}“ weicht vom Auftrag ab („{}“).",
                context.contract.contractor
            ));
        }
    }
    Ok(InvoiceImport {
        format,
        invoice: Some(invoice),
        takeoff,
        warnings,
    })
}
