//! Abrechnung for print: Aufmaßprüfung and Rechnungsfreigabe of one invoice
//! of the active LV, from the check in the domain (`Billing::check`).

use std::collections::BTreeMap;

use proava_domain::{
    billable_items, format_decimal_de, Address, BillableSource, CheckBasis, Decimal, Finding,
    Invoice, InvoiceCheck, InvoiceId, InvoiceKind, LvRow, Mode, Money, PositionCheck, Project,
    RowId, Severity, VatRule,
};
use proava_print::{
    PrintAmountLine, PrintBillingCheck, PrintBillingPosition, PrintBillingRow, PrintEarlierRelease,
    PrintHeader, PrintMark, PrintRelease,
};

/// Check of `invoice` (None = the last one) with the invoice itself.
fn checked(
    project: &Project,
    invoice: Option<InvoiceId>,
) -> Result<(InvoiceCheck, Invoice), String> {
    let context = project
        .billing_context()
        .ok_or("Für das LV ist noch kein Auftrag festgeschrieben (Reiter Abrechnung).")?;
    let id = invoice
        .or_else(|| {
            project
                .billing()
                .invoices()
                .last()
                .map(|invoice| invoice.id)
        })
        .ok_or("Im LV ist noch keine Rechnung erfasst.")?;
    let check = project.billing().check(&context, id)?;
    let invoice = project
        .billing()
        .invoice(id)
        .cloned()
        .ok_or("Die Rechnung gibt es nicht (mehr).")?;
    Ok((check, invoice))
}

fn sense(mode: Mode) -> &'static str {
    match mode {
        Mode::Cumulative => "kumuliert",
        Mode::Increment => "Zuwachs",
    }
}

fn percent_text(value: Decimal) -> String {
    let text = format_decimal_de(value, 2);
    let text = text.trim_end_matches('0').trim_end_matches(',');
    format!("{text} %")
}

/// „3. Abschlagsrechnung AR-3 vom 30.06.2027“.
fn invoice_title(check: &InvoiceCheck, invoice: &Invoice) -> String {
    let mut title = check.label.clone();
    if !invoice.number.trim().is_empty() {
        title = format!("{title} {}", invoice.number.trim());
    }
    if !invoice.date.trim().is_empty() {
        title = format!("{title} vom {}", invoice.date.trim());
    }
    title
}

fn period(invoice: &Invoice) -> String {
    match (invoice.period_start.trim(), invoice.period_end.trim()) {
        ("", "") => String::new(),
        (start, "") => format!("ab {start}"),
        ("", end) => format!("bis {end}"),
        (start, end) => format!("{start} – {end}"),
    }
}

fn address_lines(address: &Address) -> Vec<String> {
    let city = format!("{} {}", address.postal_code.trim(), address.city.trim());
    [
        address.name1.trim(),
        address.name2.trim(),
        address.street.trim(),
        city.trim(),
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .map(str::to_string)
    .collect()
}

fn remarks(invoice: &Invoice) -> Vec<String> {
    let mut lines = Vec::new();
    if !invoice.note.trim().is_empty() {
        lines.push(format!("Prüfvermerk: {}", invoice.note.trim()));
    }
    match (invoice.checked_date.trim(), invoice.checked_by.trim()) {
        ("", "") => {}
        (date, "") => lines.push(format!("Geprüft am {date}.")),
        ("", by) => lines.push(format!("Geprüft von {by}.")),
        (date, by) => lines.push(format!("Geprüft am {date} von {by}.")),
    }
    lines
}

/// Where the checked quantity comes from, short.
fn basis_text(line: &PositionCheck) -> String {
    match line.basis {
        None => String::new(),
        Some(CheckBasis::Claimed) => "Rechnung".into(),
        Some(CheckBasis::Quantity) => "geprüft".into(),
        Some(CheckBasis::Takeoff) => "Aufmaß".into(),
        Some(CheckBasis::Progress) => match line.progress {
            Some(factor) => format!(
                "Fortschritt {}",
                percent_text(factor * Decimal::ONE_HUNDRED)
            ),
            None => "Fortschritt".into(),
        },
    }
}

/// Findings printed under a position: everything that explains the check,
/// not the routine notes (taken over unchecked, several sources).
fn printed(finding: &Finding) -> bool {
    !matches!(
        finding,
        Finding::Unchecked | Finding::SeveralSources { .. } | Finding::ReverseCharge
    )
}

/// Aufmaßprüfung of `invoice` (None = the last one) of the active LV.
pub fn billing_check_to_print(
    project: &Project,
    invoice: Option<InvoiceId>,
    date: &str,
) -> Result<PrintBillingCheck, String> {
    let (check, invoice) = checked(project, invoice)?;
    let contract = project.contract().ok_or("Kein Auftrag.")?;
    let context = project.billing_context().ok_or("Kein Auftrag.")?;
    let billable = billable_items(context.contract, context.change_orders, context.lv);
    let quantity_mode = invoice.quantity_mode();
    let lines: BTreeMap<RowId, &PositionCheck> = check
        .positions
        .iter()
        .map(|line| (line.position, line))
        .collect();

    let mut rows: Vec<PrintBillingRow> = Vec::new();
    // Titles above, not yet printed: (level, index in rows once printed, OZ, text).
    let mut stack: Vec<(usize, Option<usize>, String, String)> = Vec::new();
    let mut claimed_sum = Decimal::ZERO;
    let mut checked_sum = Decimal::ZERO;
    for row in project.lv().rows() {
        match row {
            LvRow::Group(group) => {
                stack.retain(|(level, ..)| *level < group.level());
                stack.push((
                    group.level(),
                    None,
                    group.ordinal().to_string(),
                    group.short_text().to_string(),
                ));
            }
            LvRow::Position(position) => {
                let Some(line) = lines.get(&position.id()) else {
                    continue;
                };
                for entry in stack.iter_mut() {
                    if entry.1.is_none() {
                        entry.1 = Some(rows.len());
                        rows.push(PrintBillingRow::Title {
                            level: entry.0,
                            ordinal: entry.2.clone(),
                            text: entry.3.clone(),
                            claimed: Decimal::ZERO,
                            checked: Decimal::ZERO,
                        });
                    }
                }
                let checked_total = match quantity_mode {
                    Mode::Cumulative => line.cumulative_total,
                    Mode::Increment => line.increment_total,
                }
                .to_decimal();
                let claimed_total = line.claimed_total.map(Money::to_decimal);
                claimed_sum += claimed_total.unwrap_or_default();
                checked_sum += checked_total;
                for (_, index, ..) in &stack {
                    if let Some(PrintBillingRow::Title {
                        claimed, checked, ..
                    }) = index.and_then(|index| rows.get_mut(index))
                    {
                        *claimed += claimed_total.unwrap_or_default();
                        *checked += checked_total;
                    }
                }
                let mut notes = Vec::new();
                if !line.note.trim().is_empty() {
                    notes.push(line.note.trim().to_string());
                }
                notes.extend(
                    line.findings
                        .iter()
                        .filter(|f| printed(f))
                        .map(Finding::message),
                );
                let mark = match line.findings.iter().map(Finding::severity).max() {
                    Some(Severity::Error) => PrintMark::Error,
                    Some(Severity::Warning) => PrintMark::Warning,
                    _ => PrintMark::None,
                };
                rows.push(PrintBillingRow::Position(Box::new(PrintBillingPosition {
                    ordinal: line.ordinal.clone(),
                    text: line.short_text.clone(),
                    unit: line.unit.clone(),
                    ordered: line.ordered,
                    previous: line.previous,
                    claimed: line.claimed_quantity,
                    checked: line.checked,
                    unit_price: line.unit_price.or(line.claimed_unit_price),
                    claimed_total,
                    checked_total,
                    reached_percent: line
                        .ordered
                        .filter(|ordered| !ordered.is_zero())
                        .map(|ordered| line.cumulative / ordered * Decimal::ONE_HUNDRED),
                    basis: basis_text(line),
                    notes,
                    mark,
                    change_order: billable
                        .get(&line.position)
                        .is_some_and(|item| matches!(item.source, BillableSource::ChangeOrder(_))),
                })));
            }
            _ => {}
        }
    }

    let release = &check.release;
    let mut totals = Vec::new();
    let lump_claimed = invoice.claimed_lump.map(Money::to_decimal);
    let lump_checked = invoice
        .checked_lump
        .or(invoice.claimed_lump)
        .map(Money::to_decimal);
    if let Some(lump) = lump_claimed {
        totals.push(PrintAmountLine::new(
            "Pauschaler Betrag laut Rechnung",
            lump,
        ));
    }
    totals.push(PrintAmountLine::new(
        format!("Summe Rechnung (Firma), netto {}", sense(quantity_mode)),
        claimed_sum + lump_claimed.unwrap_or_default(),
    ));
    if let Some(stated) = invoice.claimed_net {
        if (stated.to_decimal() - claimed_sum - lump_claimed.unwrap_or_default()).abs()
            >= Decimal::new(1, 2)
        {
            totals.push(PrintAmountLine::new(
                "Forderung laut Rechnung, netto",
                stated.to_decimal(),
            ));
        }
    }
    totals.push(PrintAmountLine::new(
        format!("Summe geprüft, netto {}", sense(quantity_mode)),
        checked_sum + lump_checked.unwrap_or_default(),
    ));
    totals.push(PrintAmountLine::new(
        "Differenz geprüft – Rechnung",
        checked_sum + lump_checked.unwrap_or_default()
            - claimed_sum
            - lump_claimed.unwrap_or_default(),
    ));
    if check.kind != InvoiceKind::SecurityRelease {
        totals.push(PrintAmountLine::strong(
            format!("Leistung geprüft, netto {}", sense(release.mode)),
            release.performance.to_decimal(),
        ));
        if !release.discount.is_zero() {
            totals.push(
                PrintAmountLine::new("− Nachlass", -release.discount.to_decimal())
                    .with_extra(percent_text(release.discount_percent)),
            );
            totals.push(PrintAmountLine::strong(
                "= Leistung nach Nachlass, netto",
                release.after_discount.to_decimal(),
            ));
        }
        let vat = release.after_discount.percent(invoice.vat.percent());
        totals.push(PrintAmountLine::new(
            format!("+ {}", invoice.vat.label()),
            vat.to_decimal(),
        ));
        totals.push(PrintAmountLine::strong(
            "= Leistung geprüft, brutto",
            (release.after_discount + vat).to_decimal(),
        ));
    }

    let mut facts: Vec<(String, String)> = vec![
        ("Auftragnehmer".into(), contract.contractor.clone()),
        ("Auftrags-Nr.".into(), contract.number.clone()),
        ("Rechnungs-Nr.".into(), invoice.number.clone()),
        ("Rechnungsdatum".into(), invoice.date.clone()),
        ("Eingang".into(), invoice.received.clone()),
        ("Leistungszeitraum".into(), period(&invoice)),
        (
            "Mengen der Firma".into(),
            quantity_mode.quantity_label().into(),
        ),
        (
            "Auftrag mit Nachträgen".into(),
            format!(
                "{} netto",
                proava_domain::price_check::amount(check.ordered_total, project.lv().currency())
            ),
        ),
        (
            "Stand geprüft".into(),
            format!(
                "{} netto ({} %)",
                proava_domain::price_check::amount(check.performed, project.lv().currency()),
                format_decimal_de(check.performed_percent, 1)
            ),
        ),
    ];
    if !contract.discount_percent.is_zero() {
        facts.push(("Nachlass".into(), percent_text(contract.discount_percent)));
    }

    Ok(PrintBillingCheck {
        header: PrintHeader {
            document_kind: Some("Aufmaßprüfung".into()),
            ..crate::to_print::header(project, date)
        },
        invoice: invoice_title(&check, &invoice),
        facts,
        sense: sense(quantity_mode).into(),
        rows,
        totals,
        findings: check
            .findings
            .iter()
            .filter(|f| printed(f))
            .map(Finding::message)
            .collect(),
        remarks: remarks(&invoice),
        header_text: String::new(),
        footer_text: String::new(),
    })
}

/// Rechnungsfreigabe of `invoice` (None = the last one) of the active LV.
pub fn release_to_print(
    project: &Project,
    invoice: Option<InvoiceId>,
    date: &str,
) -> Result<PrintRelease, String> {
    let (check, invoice) = checked(project, invoice)?;
    let contract = project.contract().ok_or("Kein Auftrag.")?;
    let info = project.info();
    let release = &check.release;
    let money = |value: Money| value.to_decimal();

    let claimed = invoice.claimed_net.unwrap_or(check.claimed_total);
    let facts: Vec<(String, String)> = vec![
        ("Rechnungs-Nr.".into(), invoice.number.clone()),
        ("Rechnungsdatum".into(), invoice.date.clone()),
        ("Eingang".into(), invoice.received.clone()),
        ("Zahlungsziel".into(), invoice.due.clone()),
        ("Leistungszeitraum".into(), period(&invoice)),
        (
            "Forderung netto".into(),
            proava_domain::price_check::amount(claimed, project.lv().currency()),
        ),
        ("Auftrags-Nr.".into(), contract.number.clone()),
        ("Auftrag vom".into(), contract.date.clone()),
        (
            "Freigabe".into(),
            invoice.release_mode().release_label().into(),
        ),
    ];

    let contractor = contract
        .bid
        .and_then(|bid| {
            project
                .bids()
                .iter()
                .find(|candidate| candidate.id() == bid)
        })
        .map(|bid| address_lines(bid.bidder()))
        .filter(|lines| !lines.is_empty())
        .unwrap_or_else(|| vec![contract.contractor.clone()]);
    let mut addresses = vec![("Auftragnehmer".to_string(), contractor)];
    addresses.push(("Bauherr".to_string(), address_lines(&info.owner)));
    if info.client_differs() {
        addresses.push(("Auftraggeber".to_string(), address_lines(info.client())));
    }

    let vat_percent = invoice.vat.percent();
    let gross = |value: Money| money(value + value.percent(vat_percent));
    let overview = vec![
        (
            "Auftrag, netto".to_string(),
            Some(money(check.contract_total)),
            None,
        ),
        (
            "Nachträge (beauftragt), netto".to_string(),
            Some(money(check.change_orders_total)),
            None,
        ),
        (
            "Gesamt, netto".to_string(),
            Some(money(check.ordered_total)),
            Some(money(check.performed)),
        ),
        (
            format!("Gesamt, brutto ({})", invoice.vat.label()),
            Some(gross(check.ordered_total)),
            Some(gross(check.performed)),
        ),
    ];

    let mut calculation = Vec::new();
    if check.kind == InvoiceKind::SecurityRelease {
        calculation.push(PrintAmountLine::strong(
            "Freigabe Sicherheitseinbehalt, netto",
            money(release.release_net),
        ));
    } else {
        calculation.push(PrintAmountLine::new(
            format!("Leistung geprüft, netto {}", sense(release.mode)),
            money(release.performance),
        ));
        if !release.discount.is_zero() {
            calculation.push(
                PrintAmountLine::new("− Nachlass", -money(release.discount))
                    .with_extra(percent_text(release.discount_percent)),
            );
        }
        for (label, percent, amount) in &release.deductions {
            calculation.push(
                PrintAmountLine::new(format!("− {label}"), -money(*amount))
                    .with_extra(percent_text(*percent)),
            );
        }
        calculation.push(PrintAmountLine::strong(
            "= Rechnung geprüft, netto",
            money(release.net),
        ));
        calculation.push(
            PrintAmountLine::new("− Sicherheitseinbehalt", -money(release.security))
                .with_extra(percent_text(release.security_percent)),
        );
        calculation.push(PrintAmountLine::new(
            "= Zwischensumme",
            money(release.after_security),
        ));
        if release.mode == Mode::Cumulative || !release.credited.is_zero() {
            calculation.push(PrintAmountLine::new(
                "− anrechenbare Freigaben (bisher)",
                -money(release.credited),
            ));
        }
        if !release.correction.is_zero() {
            let text = if release.correction_text.trim().is_empty() {
                "Rundung / Korrektur".to_string()
            } else {
                release.correction_text.trim().to_string()
            };
            calculation.push(PrintAmountLine::new(
                format!("± {text}"),
                money(release.correction),
            ));
        }
        calculation.push(PrintAmountLine::strong(
            "= Freigabe, netto",
            money(release.release_net),
        ));
    }
    calculation.push(PrintAmountLine::new(
        format!("+ {}", invoice.vat.label()),
        money(release.vat_amount),
    ));
    calculation.push(PrintAmountLine::strong(
        "= Freigabe, brutto",
        money(release.release_gross),
    ));
    let mut notes = Vec::new();
    match release.cash_discount_percent {
        Some(percent) => {
            let days = contract
                .terms
                .cash_discount_days
                .map(|days| format!(" bei Zahlung innerhalb von {days} Tagen"))
                .unwrap_or_default();
            calculation.push(
                PrintAmountLine::new(format!("Skonto{days}"), -money(release.cash_discount))
                    .with_extra(percent_text(percent)),
            );
            calculation.push(PrintAmountLine::strong(
                "Freigabebetrag abzüglich Skonto, brutto",
                money(release.release_gross - release.cash_discount),
            ));
        }
        None => notes.push("Keine Skontovereinbarung.".to_string()),
    }
    calculation.push(PrintAmountLine::new(
        "Freigaben gesamt einschließlich dieser, netto",
        money(release.released_total),
    ));
    if !release.withheld.is_zero() {
        calculation.push(PrintAmountLine::new(
            "Einbehalt danach, netto",
            money(release.withheld),
        ));
    }
    if invoice.vat == VatRule::ReverseCharge {
        notes.push(
            "Steuerschuldnerschaft des Leistungsempfängers nach § 13b UStG: Die Rechnung enthält keine Umsatzsteuer."
                .to_string(),
        );
    }
    notes.push(
        "Liegt keine gültige Freistellungsbescheinigung des Auftragnehmers vor, sind 15 % des \
         Auszahlungsbetrags als Bauabzugsteuer an das zuständige Finanzamt abzuführen (§ 48 EStG)."
            .to_string(),
    );
    notes.extend(
        check
            .findings
            .iter()
            .filter(|finding| finding.severity() >= Severity::Warning)
            .map(Finding::message),
    );

    let planner = info.planner.name1.trim();
    let owner = info.owner.name1.trim();
    let signatures = vec![
        if planner.is_empty() {
            "Planer – sachlich und rechnerisch richtig".to_string()
        } else {
            format!("{planner} (Planer) – sachlich und rechnerisch richtig")
        },
        if owner.is_empty() {
            "Bauherr – zur Zahlung freigegeben".to_string()
        } else {
            format!("{owner} (Bauherr) – zur Zahlung freigegeben")
        },
    ];

    Ok(PrintRelease {
        header: PrintHeader {
            document_kind: Some("Rechnungsfreigabe".into()),
            ..crate::to_print::header(project, date)
        },
        invoice: invoice_title(&check, &invoice),
        facts,
        addresses,
        overview,
        performed_percent: check.performed_percent,
        calculation,
        earlier: check
            .earlier
            .iter()
            .map(|earlier| PrintEarlierRelease {
                label: format!("{} {}", earlier.label, earlier.number)
                    .trim()
                    .to_string(),
                date: earlier.date.clone(),
                performance: money(earlier.performance),
                release_net: money(earlier.release_net),
                release_gross: money(earlier.release_gross),
            })
            .collect(),
        notes,
        remarks: remarks(&invoice),
        signatures,
        header_text: String::new(),
        footer_text: String::new(),
    })
}
