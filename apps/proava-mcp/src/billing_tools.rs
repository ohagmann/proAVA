//! Abrechnung: contract, invoices of the contractor with the planner's
//! check (quantities, progress, takeoff) and the release calculation
//! (`proava_domain::billing_check`, rules in `docs/FACHLICHES.md`).

use std::path::Path;

use proava_domain::{
    change_order_overview, format_decimal_de, Correction, Decimal, Finding, Invoice, InvoiceCheck,
    InvoiceId, InvoiceItem, InvoiceKind, Mode, Money, PositionCheck, Progress, Project, RowId,
    Severity, TakeoffLine, VatRule,
};

use crate::args::{find_by_ordinal, Args};
use crate::format::{money, percent, quantity};
use crate::server::Server;

const LIST_LIMIT: usize = 400;

fn kind_from(text: &str) -> Result<InvoiceKind, String> {
    match text.trim().to_lowercase().as_str() {
        "abschlag" | "abschlagsrechnung" | "teil" | "teilrechnung" => Ok(InvoiceKind::Partial),
        "schluss" | "schlussrechnung" => Ok(InvoiceKind::Final),
        "einzel" | "einzelrechnung" => Ok(InvoiceKind::Single),
        "sicherheit" | "sicherheitseinbehalt" => Ok(InvoiceKind::SecurityRelease),
        other => Err(format!(
            "art: „{other}“ – bitte abschlag, schluss, einzel oder sicherheit."
        )),
    }
}

fn mode_from(key: &str, text: &str) -> Result<Mode, String> {
    match text.trim().to_lowercase().as_str() {
        "kumuliert" | "gesamtstand" => Ok(Mode::Cumulative),
        "zuwachs" => Ok(Mode::Increment),
        other => Err(format!("{key}: „{other}“ – bitte kumuliert oder zuwachs.")),
    }
}

fn vat_from(text: &str) -> Result<VatRule, String> {
    let cleaned = text.trim().trim_end_matches('%').trim().to_lowercase();
    if cleaned == "13b" || cleaned == "§13b" || cleaned == "§ 13b" {
        return Ok(VatRule::ReverseCharge);
    }
    proava_domain::parse_decimal_de(&cleaned)
        .map(VatRule::Rate)
        .map_err(|_| {
            format!(
                "ust: „{}“ – bitte Prozentsatz (19, 7) oder 13b.",
                text.trim()
            )
        })
}

fn invoice_id(args: &Args, project: &Project) -> Result<InvoiceId, String> {
    let id = InvoiceId(
        args.opt_count("rechnung")?
            .ok_or("Angabe „rechnung“ (Id aus abrechnung_lesen) fehlt.")?,
    );
    project
        .billing()
        .invoice(id)
        .map(|_| id)
        .ok_or_else(|| format!("Eine Rechnung #{} gibt es in diesem LV nicht.", id.0))
}

fn position_of(project: &Project, oz: &str) -> Result<RowId, String> {
    let index = find_by_ordinal(project.lv(), oz)?;
    let row = &project.lv().rows()[index];
    row.as_position()
        .map(|position| position.id())
        .ok_or_else(|| format!("{oz} ist keine Position."))
}

fn checks(project: &Project) -> Result<Vec<InvoiceCheck>, String> {
    let context = project
        .billing_context()
        .ok_or("Für das LV ist noch kein Auftrag festgeschrieben (auftrag_festschreiben).")?;
    Ok(project.billing().check_all(&context))
}

fn check_of(project: &Project, id: InvoiceId) -> Result<InvoiceCheck, String> {
    checks(project)?
        .into_iter()
        .find(|check| check.invoice == id)
        .ok_or_else(|| format!("Eine Rechnung #{} gibt es in diesem LV nicht.", id.0))
}

fn name_of(check: &InvoiceCheck, invoice: &Invoice) -> String {
    let number = if invoice.number.trim().is_empty() {
        "(ohne Nr.)"
    } else {
        invoice.number.trim()
    };
    format!("#{} {} {number}", invoice.id.0, check.label)
}

fn findings_text(findings: &[Finding], hints: bool) -> Vec<String> {
    findings
        .iter()
        .filter(|finding| hints || finding.severity() >= Severity::Warning)
        .map(|finding| {
            let level = match finding.severity() {
                Severity::Error => "Fehler",
                Severity::Warning => "Warnung",
                Severity::Info => "Hinweis",
            };
            format!("{level}: {}", finding.message())
        })
        .collect()
}

fn basis_text(line: &PositionCheck) -> String {
    match (line.basis, line.progress) {
        (Some(proava_domain::CheckBasis::Progress), Some(factor)) => {
            format!("Fortschritt {}", percent(factor * Decimal::ONE_HUNDRED))
        }
        (Some(basis), _) => basis.label().to_string(),
        (None, _) => String::new(),
    }
}

/// Overview of the Abrechnung, or one invoice with its positions.
pub(crate) fn read_billing(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let Some(contract) = project.contract() else {
        let awarded = project
            .awarded_bid()
            .map(|bid| format!(" Beauftragt ist {} – auftrag_festschreiben übernimmt den Auftrag.", bid.name()))
            .unwrap_or_else(|| " Noch kein Angebot beauftragt (auftrag_erteilen) – oder Auftrag aus X86 (auftrag_festschreiben mit datei).".into());
        return Ok(format!(
            "Für das LV „{}“ ist noch kein Auftrag festgeschrieben.{awarded}",
            project.lv().title()
        ));
    };
    let checks = checks(project)?;
    let hints = args.flag("hinweise")?;
    if let Some(id) = args.opt_count("rechnung")?.map(InvoiceId) {
        let check = checks
            .iter()
            .find(|check| check.invoice == id)
            .ok_or_else(|| format!("Eine Rechnung #{} gibt es in diesem LV nicht.", id.0))?;
        let invoice = project
            .billing()
            .invoice(id)
            .ok_or("Rechnung verschwunden.")?;
        return Ok(invoice_detail(
            project,
            check,
            invoice,
            args.flag("alle_positionen")?,
            hints,
        ));
    }
    let overview = change_order_overview(Some(contract), project.change_orders());
    let mut out = format!(
        "Auftrag: {}{}{} – Auftrag netto {}, Nachlass {}, beauftragte Nachträge {}, gesamt {}.\n",
        contract.contractor,
        if contract.date.is_empty() {
            String::new()
        } else {
            format!(" vom {}", contract.date)
        },
        if contract.number.is_empty() {
            String::new()
        } else {
            format!(", Auftrags-Nr. {}", contract.number)
        },
        money(overview.contract_total),
        percent(contract.discount_percent),
        money(overview.approved),
        money(overview.total_with_change_orders),
    );
    let terms = &contract.terms;
    out.push_str(&format!(
        "Vertragsbedingungen: Sicherheit Vertragserfüllung {}, Mängelansprüche {}, Skonto {}, Abzüge {}.\n",
        terms.performance_security_percent.map(percent).unwrap_or_else(|| "–".into()),
        terms.warranty_security_percent.map(percent).unwrap_or_else(|| "–".into()),
        terms.cash_discount_percent.map(percent).unwrap_or_else(|| "–".into()),
        if terms.deductions.iter().all(|d| d.percent.is_zero()) {
            "keine".to_string()
        } else {
            terms
                .deductions
                .iter()
                .filter(|d| !d.percent.is_zero())
                .map(|d| format!("{} {}", d.label, percent(d.percent)))
                .collect::<Vec<_>>()
                .join(", ")
        },
    ));
    if checks.is_empty() {
        out.push_str("Noch keine Rechnung erfasst (rechnung_anlegen).");
        return Ok(out);
    }
    out.push_str(&format!("Rechnungen ({}):\n", checks.len()));
    for check in &checks {
        let Some(invoice) = project.billing().invoice(check.invoice) else {
            continue;
        };
        let warnings = check
            .all_findings()
            .filter(|f| f.severity() >= Severity::Warning)
            .count();
        out.push_str(&format!(
            "- {} vom {} – Forderung netto {}, Leistung geprüft {} ({}), Freigabe netto {}, brutto {}; {}{}\n",
            name_of(check, invoice),
            if invoice.date.is_empty() { "–" } else { &invoice.date },
            invoice.claimed_net.map(money).unwrap_or_else(|| money(check.claimed_total)),
            money(check.release.performance),
            match check.release.mode {
                Mode::Cumulative => "kumuliert",
                Mode::Increment => "Zuwachs",
            },
            money(check.release.release_net),
            money(check.release.release_gross),
            if invoice.checked_date.is_empty() { "in Prüfung".to_string() } else { format!("geprüft {}", invoice.checked_date) },
            if warnings > 0 { format!(", {warnings} Befunde") } else { String::new() },
        ));
    }
    if let Some(last) = checks.last() {
        out.push_str(&format!(
            "Stand geprüft {} von {} ({} %), Prognose laut Aufmaß {}.",
            money(last.performed),
            money(last.ordered_total),
            format_decimal_de(last.performed_percent, 1),
            money(last.forecast)
        ));
    }
    Ok(out)
}

fn invoice_detail(
    project: &Project,
    check: &InvoiceCheck,
    invoice: &Invoice,
    all: bool,
    hints: bool,
) -> String {
    let mut out = format!(
        "{} – Datum {}, Eingang {}, Zahlungsziel {}, Leistungszeitraum {} bis {}\n",
        name_of(check, invoice),
        or_dash(&invoice.date),
        or_dash(&invoice.received),
        or_dash(&invoice.due),
        or_dash(&invoice.period_start),
        or_dash(&invoice.period_end),
    );
    out.push_str(&format!(
        "Mengen der Firma {}, Freigabe {}, {}, Forderung netto {}{}\n",
        invoice.quantity_mode().quantity_label(),
        invoice.release_mode().release_label(),
        invoice.vat.label(),
        invoice.claimed_net.map(money).unwrap_or_else(|| "–".into()),
        if invoice.note.trim().is_empty() {
            String::new()
        } else {
            format!(", Prüfvermerk: {}", invoice.note.trim())
        },
    ));
    let lines: Vec<&PositionCheck> = if all {
        check
            .positions
            .iter()
            .chain(check.unchanged.iter())
            .collect()
    } else {
        check.positions.iter().collect()
    };
    out.push_str(&format!(
        "Positionen ({}): OZ | Kurztext | Einh. | Auftrag | bisher | Rechnung | geprüft | Quelle | EP | Betrag kum. | Betrag Zuwachs | Vermerk\n",
        lines.len()
    ));
    for line in lines.iter().take(LIST_LIMIT) {
        out.push_str(&format!(
            "{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {}\n",
            line.ordinal,
            line.short_text,
            line.unit,
            line.ordered
                .map(quantity)
                .unwrap_or_else(|| "nicht im Auftrag".into()),
            quantity(line.previous),
            line.claimed_quantity
                .map(quantity)
                .unwrap_or_else(|| "–".into()),
            quantity(line.checked),
            basis_text(line),
            line.unit_price
                .map(|p| format_decimal_de(p, 2))
                .unwrap_or_else(|| "–".into()),
            money(line.cumulative_total),
            money(line.increment_total),
            line.note.trim(),
        ));
        for finding in findings_text(&line.findings, hints) {
            out.push_str(&format!("    {finding}\n"));
        }
    }
    if lines.len() > LIST_LIMIT {
        out.push_str(&format!("… (+{} Positionen)\n", lines.len() - LIST_LIMIT));
    }
    let general = findings_text(&check.findings, hints);
    if !general.is_empty() {
        out.push_str("Befunde zur Rechnung:\n");
        for finding in general {
            out.push_str(&format!("- {finding}\n"));
        }
    }
    let _ = project;
    out.push_str(&format!(
        "Freigabe netto {}, brutto {} (Rechenweg: freigabe_berechnen).",
        money(check.release.release_net),
        money(check.release.release_gross)
    ));
    out
}

fn or_dash(text: &str) -> &str {
    if text.trim().is_empty() {
        "–"
    } else {
        text.trim()
    }
}

/// The release calculation of one invoice step by step.
pub(crate) fn release(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let id = invoice_id(args, project)?;
    let check = check_of(project, id)?;
    let invoice = project
        .billing()
        .invoice(id)
        .ok_or("Rechnung verschwunden.")?;
    let release = &check.release;
    let mut out = format!("Rechnungsfreigabe {}\n", name_of(&check, invoice));
    let mut line =
        |label: String, value: Money| out.push_str(&format!("{label}: {}\n", money(value)));
    if check.kind == InvoiceKind::SecurityRelease {
        line(
            "Freigabe Sicherheitseinbehalt netto".into(),
            release.release_net,
        );
    } else {
        let mode = match release.mode {
            Mode::Cumulative => "kumuliert",
            Mode::Increment => "Zuwachs dieser Rechnung",
        };
        line(
            format!("Leistung netto geprüft ({mode})"),
            release.performance,
        );
        if !release.discount.is_zero() {
            line(
                format!("− Nachlass {}", percent(release.discount_percent)),
                -release.discount,
            );
        }
        for (label, rate, amount) in &release.deductions {
            line(format!("− {label} {}", percent(*rate)), -*amount);
        }
        line("= Rechnung geprüft netto".into(), release.net);
        line(
            format!(
                "− Sicherheitseinbehalt {}",
                percent(release.security_percent)
            ),
            -release.security,
        );
        line("= Zwischensumme".into(), release.after_security);
        line("− anrechenbare Freigaben".into(), -release.credited);
        if !release.correction.is_zero() {
            let text = if release.correction_text.trim().is_empty() {
                "Korrektur"
            } else {
                release.correction_text.trim()
            };
            line(format!("± {text}"), release.correction);
        }
        line("= Freigabe netto".into(), release.release_net);
    }
    line(format!("+ {}", release.vat.label()), release.vat_amount);
    line("= Freigabe brutto".into(), release.release_gross);
    if let Some(rate) = release.cash_discount_percent {
        line(
            format!("Skonto {} (Information)", percent(rate)),
            release.cash_discount,
        );
        line(
            "Freigabebetrag abzüglich Skonto brutto".into(),
            release.release_gross - release.cash_discount,
        );
    }
    line(
        "Freigaben gesamt einschließlich dieser netto".into(),
        release.released_total,
    );
    line("Einbehalt danach netto".into(), release.withheld);
    out.push_str(&format!(
        "Auftrag mit beauftragten Nachträgen {}, Stand geprüft {} ({} %).\n",
        money(check.ordered_total),
        money(check.performed),
        format_decimal_de(check.performed_percent, 1)
    ));
    if !check.earlier.is_empty() {
        out.push_str("Bisherige Freigaben:\n");
        for earlier in &check.earlier {
            out.push_str(&format!(
                "- {} {} vom {}: Leistung {}, Freigabe netto {}, brutto {}\n",
                earlier.label,
                earlier.number,
                or_dash(&earlier.date),
                money(earlier.performance),
                money(earlier.release_net),
                money(earlier.release_gross)
            ));
        }
    }
    let findings = findings_text(&check.all_findings().cloned().collect::<Vec<_>>(), false);
    if !findings.is_empty() {
        out.push_str(&format!("Befunde ({}):\n", findings.len()));
        for finding in findings.iter().take(60) {
            out.push_str(&format!("- {finding}\n"));
        }
    }
    Ok(out.trim_end().to_string())
}

/// Takes the contract over from the awarded bid or reads it from X86/D86.
pub(crate) fn fix_contract(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let mut warnings = Vec::new();
    let had = loaded.project.contract().is_some();
    if had && !args.flag("neu")? {
        if let Some(number) = args.opt_text("auftrags_nr")? {
            if let Some(contract) = loaded.project.contract_mut() {
                contract.number = number.trim().to_string();
            }
            let answer = format!("Auftrags-Nr. gesetzt: {}.", number.trim());
            return server.save_with_answer(&loaded, answer);
        }
        return Err("Für das LV ist schon ein Auftrag festgeschrieben – neu: true schreibt ihn neu fest (Rechnungen, Aufmaß und Nachträge bleiben).".into());
    }
    match args
        .opt_text("datei")?
        .filter(|text| !text.trim().is_empty())
    {
        Some(path) => {
            let file = server.allowed(Path::new(path.trim()))?;
            if !file.is_file() {
                return Err(format!("Datei nicht gefunden: {}", file.display()));
            }
            let import = server.exchange().import_contract(&loaded.project, &file)?;
            warnings = import.warnings;
            loaded.project.set_contract(Some(import.contract));
        }
        None => {
            loaded.project.take_over_contract()?;
        }
    }
    if let Some(number) = args.opt_text("auftrags_nr")? {
        if let Some(contract) = loaded.project.contract_mut() {
            contract.number = number.trim().to_string();
        }
    }
    let contract = loaded
        .project
        .contract()
        .ok_or("Auftrag nicht übernommen.")?;
    let mut answer = format!(
        "Auftrag festgeschrieben: {} – {} Positionen, Nachlass {}, Auftragssumme netto {}.",
        contract.contractor,
        contract.items.len(),
        percent(contract.discount_percent),
        money(contract.total)
    );
    for warning in warnings.iter().take(30) {
        answer.push_str(&format!("\n- {warning}"));
    }
    server.save_with_answer(&loaded, answer)
}

/// Applies the header arguments to an invoice; returns what changed.
fn apply_header(args: &Args, invoice: &mut Invoice) -> Result<Vec<&'static str>, String> {
    let mut changed = Vec::new();
    if let Some(text) = args.opt_text("art")? {
        invoice.kind = kind_from(text)?;
        changed.push("Art");
    }
    for (key, field, label) in [
        ("nummer", &mut invoice.number, "Nummer"),
        ("datum", &mut invoice.date, "Datum"),
        ("eingang", &mut invoice.received, "Eingang"),
        ("zahlungsziel", &mut invoice.due, "Zahlungsziel"),
        (
            "leistung_von",
            &mut invoice.period_start,
            "Leistungszeitraum",
        ),
        ("leistung_bis", &mut invoice.period_end, "Leistungszeitraum"),
        ("pruefvermerk", &mut invoice.note, "Prüfvermerk"),
        ("geprueft_am", &mut invoice.checked_date, "geprüft am"),
        ("geprueft_von", &mut invoice.checked_by, "geprüft von"),
    ] {
        if let Some(text) = args.opt_text(key)? {
            *field = text.trim().to_string();
            if !changed.contains(&label) {
                changed.push(label);
            }
        }
    }
    if let Some(text) = args.opt_text("mengen")? {
        invoice.quantities = mode_from("mengen", text)?;
        changed.push("Mengen");
    }
    if let Some(text) = args.opt_text("freigabe")? {
        invoice.release = mode_from("freigabe", text)?;
        changed.push("Freigabe");
    }
    if let Some(value) = args.get("ust") {
        let text = match value {
            serde_json::Value::Number(number) => number.to_string(),
            serde_json::Value::String(text) => text.clone(),
            _ => return Err("ust: bitte Prozentsatz (19, 7) oder 13b.".into()),
        };
        invoice.vat = vat_from(&text)?;
        changed.push("USt");
    }
    for (key, field, label) in [
        ("forderung_netto", &mut invoice.claimed_net, "Forderung"),
        (
            "pauschal_gefordert",
            &mut invoice.claimed_lump,
            "Pauschalbetrag",
        ),
        (
            "pauschal_geprueft",
            &mut invoice.checked_lump,
            "Pauschalbetrag geprüft",
        ),
        (
            "angerechnet",
            &mut invoice.credited,
            "angerechnete Freigaben",
        ),
        (
            "sicherheit_freigabe",
            &mut invoice.security_release,
            "Freigabe Sicherheit",
        ),
    ] {
        if let Some(value) = args.price(key)? {
            *field = value.map(Money::from_decimal);
            changed.push(label);
        }
    }
    if let Some(value) = args.price("einbehalt_prozent")? {
        if value.is_some_and(|v| v.is_sign_negative()) {
            return Err("einbehalt_prozent darf nicht negativ sein.".into());
        }
        invoice.security_percent = value;
        changed.push("Einbehalt");
    }
    if let Some(value) = args.price("korrektur")? {
        let text = invoice
            .correction
            .as_ref()
            .map(|c| c.text.clone())
            .unwrap_or_default();
        invoice.correction = value.map(|amount| Correction {
            amount: Money::from_decimal(amount),
            text,
        });
        changed.push("Korrektur");
    }
    if let Some(text) = args.opt_text("korrektur_text")? {
        let amount = invoice
            .correction
            .as_ref()
            .map(|c| c.amount)
            .unwrap_or_default();
        invoice.correction = Some(Correction {
            amount,
            text: text.trim().to_string(),
        });
        changed.push("Korrektur");
    }
    Ok(changed)
}

fn release_line(project: &Project, id: InvoiceId) -> String {
    match check_of(project, id) {
        Ok(check) => format!(
            "Leistung geprüft {}, Freigabe netto {}, brutto {}.",
            money(check.release.performance),
            money(check.release.release_net),
            money(check.release.release_gross)
        ),
        Err(error) => error,
    }
}

/// New invoice by hand or from X89/X89B.
pub(crate) fn create_invoice(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    if loaded.project.contract().is_none() {
        return Err(
            "Für das LV ist noch kein Auftrag festgeschrieben (auftrag_festschreiben).".into(),
        );
    }
    let mut warnings = Vec::new();
    let (mut invoice, takeoff) = match args
        .opt_text("datei")?
        .filter(|text| !text.trim().is_empty())
    {
        Some(path) => {
            let file = server.allowed(Path::new(path.trim()))?;
            if !file.is_file() {
                return Err(format!("Datei nicht gefunden: {}", file.display()));
            }
            let import = server.exchange().import_invoice(&loaded.project, &file)?;
            warnings = import.warnings;
            let invoice = import
                .invoice
                .ok_or("Die Datei ist eine Mengenermittlung (X31) – mit aufmass_setzen einer Rechnung zuordnen.")?;
            (invoice, import.takeoff)
        }
        None => {
            let mut invoice = Invoice::new(InvoiceKind::Partial, "", "");
            match loaded.project.billing().last_in_sequence() {
                Some(last) => {
                    invoice.quantities = last.quantities;
                    invoice.release = last.release;
                    invoice.vat = last.vat;
                }
                None => invoice.vat = VatRule::Rate(loaded.project.lv().vat_percent()),
            }
            (invoice, Vec::new())
        }
    };
    apply_header(args, &mut invoice)?;
    let billing = loaded.project.billing_mut();
    let id = billing.add_invoice(invoice);
    let lines = billing.add_takeoff(takeoff, Some(id));
    let check = check_of(&loaded.project, id)?;
    let invoice = loaded
        .project
        .billing()
        .invoice(id)
        .ok_or("Rechnung verschwunden.")?;
    let mut answer = format!(
        "Rechnung angelegt: {} ({} Positionen{}). {}",
        name_of(&check, invoice),
        invoice.items.len(),
        if lines > 0 {
            format!(", {lines} Aufmaßzeilen")
        } else {
            String::new()
        },
        release_line(&loaded.project, id)
    );
    for warning in warnings.iter().take(30) {
        answer.push_str(&format!("\n- {warning}"));
    }
    server.save_with_answer(&loaded, answer)
}

/// Changes the header of an invoice or deletes it.
pub(crate) fn change_invoice(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let id = invoice_id(args, &loaded.project)?;
    if args.flag("loeschen")? {
        let removed = loaded
            .project
            .billing_mut()
            .remove_invoice(id)
            .ok_or("Rechnung verschwunden.")?;
        let answer = format!(
            "Rechnung #{} {} gelöscht; ihre Aufmaßzeilen bleiben (nicht zugeordnet).",
            id.0, removed.number
        );
        return server.save_with_answer(&loaded, answer);
    }
    let invoice = loaded
        .project
        .billing_mut()
        .invoice_mut(id)
        .ok_or("Rechnung verschwunden.")?;
    let changed = apply_header(args, invoice)?;
    if changed.is_empty() {
        return Err("Nichts zu ändern – bitte mindestens ein Feld angeben.".into());
    }
    let answer = format!(
        "Rechnung #{} geändert ({}). {}",
        id.0,
        changed.join(", "),
        release_line(&loaded.project, id)
    );
    server.save_with_answer(&loaded, answer)
}

/// One position of an invoice: invoice quantity, checked quantity, progress …
fn set_item(project: &mut Project, id: InvoiceId, args: &Args) -> Result<String, String> {
    let oz = args.text("oz")?.trim().to_string();
    let row = position_of(project, &oz)?;
    let invoice = project
        .billing_mut()
        .invoice_mut(id)
        .ok_or("Rechnung verschwunden.")?;
    let before = invoice.items.get(&row).cloned().unwrap_or_default();
    let mut item = before.clone();
    for (key, field) in [
        ("rechnung_menge", &mut item.claimed_quantity),
        ("rechnung_ep", &mut item.claimed_unit_price),
        ("rechnung_gp", &mut item.claimed_total),
        ("gepruefte_menge", &mut item.checked_quantity),
    ] {
        if let Some(value) = args.price(key)? {
            *field = value;
        }
    }
    if let Some(text) = args.opt_text("vermerk")? {
        item.note = text.trim().to_string();
        if let Some(progress) = &mut item.progress {
            progress.note = item.note.clone();
        }
    }
    if let Some(flag) = args.opt_flag("nachweis")? {
        item.evidence = flag;
    }
    if let Some(text) = args.opt_text("quelle")? {
        item.basis = match text.trim().to_lowercase().as_str() {
            "" | "auto" | "automatisch" => None,
            "rechnung" => Some(proava_domain::CheckBasis::Claimed),
            "geprueft" | "geprüft" | "menge" => Some(proava_domain::CheckBasis::Quantity),
            "aufmass" | "aufmaß" => Some(proava_domain::CheckBasis::Takeoff),
            "fortschritt" => Some(proava_domain::CheckBasis::Progress),
            other => {
                return Err(format!(
                    "quelle: „{other}“ – bitte auto, rechnung, geprueft, aufmass oder fortschritt."
                ))
            }
        };
    }
    if let Some(value) = args.price("fortschritt_prozent")? {
        item.progress = match value {
            None => None,
            Some(percent_value) => {
                let note = item.note.trim().to_string();
                if note.is_empty() {
                    return Err(format!(
                        "{oz}: Leistungsfortschritt braucht einen vermerk, worauf er beruht (z. B. „Absprache mit AN vom …“)."
                    ));
                }
                let progress = Progress {
                    factor: percent_value / Decimal::ONE_HUNDRED,
                    note,
                };
                if let Some(problem) = progress.problem() {
                    return Err(format!("{oz}: {problem}"));
                }
                Some(progress)
            }
        };
    }
    if item == before {
        return Ok(format!("{oz}: unverändert"));
    }
    if item == InvoiceItem::default() {
        invoice.items.remove(&row);
    } else {
        invoice.items.insert(row, item);
    }
    Ok(format!("{oz}: gesetzt"))
}

/// Sets one or many positions of an invoice.
pub(crate) fn set_position(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let id = invoice_id(args, &loaded.project)?;
    let mut lines = Vec::new();
    match args.get("positionen") {
        Some(serde_json::Value::Array(list)) => {
            for (number, entry) in list.iter().enumerate() {
                let map = entry
                    .as_object()
                    .ok_or_else(|| format!("positionen[{number}] muss ein Objekt sein."))?;
                lines.push(set_item(&mut loaded.project, id, &Args(map))?);
            }
        }
        Some(_) => return Err("positionen muss eine Liste sein.".into()),
        None => lines.push(set_item(&mut loaded.project, id, args)?),
    }
    let check = check_of(&loaded.project, id)?;
    let mut answer = format!("{}\n", lines.join("\n"));
    let rows: Vec<String> = lines
        .iter()
        .filter_map(|line| line.split(':').next().map(str::to_string))
        .collect();
    for line in check.positions.iter().filter(|line| {
        rows.iter().any(|oz| {
            proava_domain::normalize_ordinal(oz) == proava_domain::normalize_ordinal(&line.ordinal)
        })
    }) {
        let findings = findings_text(&line.findings, false);
        if !findings.is_empty() {
            answer.push_str(&format!("{}: {}\n", line.ordinal, findings.join(" ")));
        }
    }
    answer.push_str(&release_line(&loaded.project, id));
    server.save_with_answer(&loaded, answer)
}

/// Takeoff lines of one position for an invoice, or an X31 for an invoice.
pub(crate) fn set_takeoff(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    if loaded.project.contract().is_none() {
        return Err(
            "Für das LV ist noch kein Auftrag festgeschrieben (auftrag_festschreiben).".into(),
        );
    }
    let target = match args.opt_count("rechnung")? {
        Some(_) => Some(invoice_id(args, &loaded.project)?),
        None => None,
    };
    let where_to = target
        .map(|id| format!("Rechnung #{}", id.0))
        .unwrap_or_else(|| "nicht zugeordnet".into());
    if let Some(path) = args
        .opt_text("datei")?
        .filter(|text| !text.trim().is_empty())
    {
        let file = server.allowed(Path::new(path.trim()))?;
        if !file.is_file() {
            return Err(format!("Datei nicht gefunden: {}", file.display()));
        }
        let import = server.exchange().import_invoice(&loaded.project, &file)?;
        if import.invoice.is_some() {
            return Err(
                "Die Datei ist eine Rechnung (X89) – mit rechnung_anlegen einlesen.".into(),
            );
        }
        let positions = import.takeoff.len();
        let lines = loaded
            .project
            .billing_mut()
            .add_takeoff(import.takeoff, target);
        let mut answer = format!(
            "Aufmaß aus {}: {lines} Zeilen zu {positions} Positionen, {where_to}.",
            file.display()
        );
        for warning in import.warnings.iter().take(30) {
            answer.push_str(&format!("\n- {warning}"));
        }
        if let Some(id) = target {
            answer.push_str(&format!("\n{}", release_line(&loaded.project, id)));
        }
        return server.save_with_answer(&loaded, answer);
    }
    let oz = args.text("oz")?.trim().to_string();
    let row = position_of(&loaded.project, &oz)?;
    let entries = match args.get("zeilen") {
        Some(serde_json::Value::Array(list)) => list,
        _ => {
            return Err("zeilen fehlt: Liste von {ort, rechenansatz}; [] löscht die Zeilen.".into())
        }
    };
    let mut new_lines = Vec::new();
    for (number, entry) in entries.iter().enumerate() {
        let map = entry
            .as_object()
            .ok_or_else(|| format!("zeilen[{number}] muss ein Objekt sein."))?;
        let line = Args(map);
        let new = TakeoffLine {
            label: line.opt_text("ort")?.unwrap_or_default().trim().to_string(),
            formula: line
                .opt_text("rechenansatz")?
                .unwrap_or_default()
                .trim()
                .to_string(),
            invoice: target,
        };
        new.result()
            .map_err(|error| format!("zeilen[{number}] „{}“: {error}", new.formula))?;
        new_lines.push(new);
    }
    let lines = loaded.project.billing_mut().takeoff_mut(row);
    let replace = args.opt_flag("anhaengen")? != Some(true);
    if replace {
        lines.retain(|line| line.invoice != target);
    }
    let added = new_lines.len();
    lines.extend(new_lines);
    let (sum, _) = loaded.project.billing().takeoff_sum(row, target);
    let mut answer = format!(
        "{oz}: {added} Aufmaßzeilen {} ({where_to}), Summe {}.",
        if replace { "gesetzt" } else { "angehängt" },
        quantity(sum)
    );
    if let Some(id) = target {
        answer.push_str(&format!(" {}", release_line(&loaded.project, id)));
    }
    server.save_with_answer(&loaded, answer)
}
