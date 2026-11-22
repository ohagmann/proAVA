//! Kostenverfolgung nach DIN 276, 4.4–4.6 (Reiter Kostenverfolgung):
//! lesen, Budget (Kostenvorgabe) setzen, Erläuterungen, Wahl der
//! Kostenermittlungen und Angebote, Kostenstände. Gedruckt wird mit
//! `pdf_drucken dokument: kostenverfolgung`. Gilt für das ganze Projekt.

use std::collections::BTreeMap;

use proava_domain::{
    cost_tracking, format_decimal_de, group_label, parse_cost_group, record_snapshot, stage_parts,
    BidId, BudgetKind, CostStage, Money, OfferChoice, Project, TrackDeviation, TrackOptions,
    TrackReport, TrackRow, TrackRowKind, TrackStage, TrackView,
};
use serde_json::Value;

use crate::args::{number, Args};
use crate::server::Server;

fn today() -> String {
    chrono::Local::now().format("%d.%m.%Y").to_string()
}

/// Table cell: no line breaks, no `|`.
fn cell(text: &str) -> String {
    let text = text.replace(['\n', '\r'], " ").replace('|', "/");
    if text.trim().is_empty() {
        "–".to_string()
    } else {
        text.trim().to_string()
    }
}

/// „+1.234,56 € / +3,3 %“
fn deviation_text(deviation: &proava_domain::Deviation) -> String {
    let sign = if deviation.amount.cents() > 0 {
        "+"
    } else {
        ""
    };
    match deviation.percent {
        Some(percent) => format!(
            "{sign}{} / {sign}{} %",
            deviation.amount,
            format_decimal_de(percent, 1)
        ),
        None => format!("{sign}{}", deviation.amount),
    }
}

/// The view from the arguments: ebene, ansicht, brutto, abweichung, vergleich.
pub(crate) fn options(project: &Project, args: &Args) -> Result<TrackOptions, String> {
    let mut options = TrackOptions::default();
    if let Some(level) = args.opt_count("ebene")? {
        if !(1..=3).contains(&level) {
            return Err(format!("ebene: {level} – bitte 1, 2 oder 3."));
        }
        options.level = level as u8;
    }
    if let Some(view) = args.opt_text("ansicht")? {
        options.view = TrackView::from_key(view).ok_or_else(|| {
            format!(
                "ansicht: „{}“ – bitte kostengruppen oder vergabeeinheiten.",
                view.trim()
            )
        })?;
    }
    if let Some(gross) = args.opt_flag("brutto")? {
        options.gross = gross;
    }
    if let Some(mode) = args.opt_text("abweichung")? {
        options.deviation = TrackDeviation::from_key(mode).ok_or_else(|| {
            format!(
                "abweichung: „{}“ – bitte budget (kostenvorgabe), vorstufe, stand oder keine.",
                mode.trim()
            )
        })?;
    }
    options.snapshot = snapshot_index(project, args.get("vergleich"))?;
    // With a recorded state the change since then, unless asked otherwise.
    match (options.snapshot, args.has("abweichung")) {
        (Some(_), false) => options.deviation = TrackDeviation::Snapshot,
        (None, _) if options.deviation == TrackDeviation::Snapshot => {
            return Err(
                "abweichung stand braucht vergleich (Nummer oder Datum des Kostenstands)."
                    .to_string(),
            );
        }
        _ => {}
    }
    Ok(options)
}

/// A recorded state: number in the list (1 = first) or date/text of it.
fn snapshot_index(project: &Project, value: Option<&Value>) -> Result<Option<usize>, String> {
    let snapshots = &project.cost_tracking().snapshots;
    let wanted = match value {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(text)) if text.trim().is_empty() => return Ok(None),
        Some(Value::String(text)) => text.trim().to_string(),
        Some(_) => return Err("vergleich: Nummer oder Datum des Kostenstands.".to_string()),
    };
    if snapshots.is_empty() {
        return Err("Es gibt keinen festgehaltenen Kostenstand (kostenverfolgung_aendern kostenstand_festhalten).".to_string());
    }
    if let Ok(number) = wanted.parse::<usize>() {
        if (1..=snapshots.len()).contains(&number) {
            return Ok(Some(number - 1));
        }
    }
    snapshots
        .iter()
        .position(|snapshot| snapshot.label().contains(&wanted))
        .map(Some)
        .ok_or_else(|| format!("Einen Kostenstand „{wanted}“ gibt es nicht (kostenverfolgung_lesen zeigt die Liste)."))
}

fn row_line(report: &TrackReport, row: &TrackRow, stages: &[TrackStage]) -> String {
    let indent = "  ".repeat(row.depth as usize);
    let title = match row.kind {
        TrackRowKind::Total => "**Gesamt**".to_string(),
        TrackRowKind::Group | TrackRowKind::Lv | TrackRowKind::NoLv => {
            format!("**{}**", cell(&row.title()))
        }
        _ => cell(&row.title()),
    };
    let mut line = format!("| {indent}{title} |");
    for stage in stages {
        let deviation = report.options.deviation.of(row, *stage);
        let text = match row.value(*stage) {
            // Gone since the recorded state: „–“ with the change.
            None => match &deviation {
                Some(deviation) => format!("– ({})", deviation_text(deviation).replace(" €", "")),
                None => "–".to_string(),
            },
            Some(value) => {
                let mut text = value.plain();
                if let Some(deviation) = deviation {
                    text.push_str(&format!(
                        " ({})",
                        deviation_text(&deviation).replace(" €", "")
                    ));
                }
                match report.mark(row, *stage) {
                    proava_domain::Mark::OverBudget => text.push_str(" [über Kostenvorgabe]"),
                    proava_domain::Mark::AbovePrevious => text.push_str(" [über Vorstufe]"),
                    proava_domain::Mark::None => {}
                }
                text
            }
        };
        line.push_str(&format!(" {text} |"));
    }
    line.push_str(&format!(" {} |", cell(&row.note)));
    line
}

/// The Kostenverfolgung as text.
fn describe(project: &Project, report: &TrackReport) -> String {
    let tracking = project.cost_tracking();
    let stages = report.shown_stages();
    let mut out = format!(
        "Kostenverfolgung (DIN 276, 4.4–4.6), Kostenstand {} · Beträge {} · Ebene {} · {} · Abweichung {}",
        today(),
        report.amounts_text(),
        report.options.level,
        match report.options.view {
            TrackView::ByGroup => "nach Kostengruppen",
            TrackView::ByLv => "nach Vergabeeinheiten",
        },
        report.options.deviation.label()
    );
    if let Some(snapshot) = &report.snapshot {
        out.push_str(&format!(" · Vergleich mit Stand {snapshot}"));
    }
    let total = &report.total;
    match total.value(TrackStage::Budget) {
        Some(budget) => out.push_str(&format!(
            "\nKostenvorgabe {budget} ({}{}{}); Markierung über der Vorstufe ab {} %",
            report.budget_kind.label(),
            if report.budget_date.is_empty() {
                String::new()
            } else {
                format!(", Stand {}", report.budget_date)
            },
            if report.budget_note.is_empty() {
                String::new()
            } else {
                format!(", {}", report.budget_note)
            },
            format_decimal_de(report.threshold, 1).trim_end_matches(",0")
        )),
        None => out.push_str("\nKeine Kostenvorgabe (kostenvorgabe_setzen)."),
    }
    if let Some(forecast) = total.value(TrackStage::Forecast) {
        out.push_str(&format!("\nPrognose {forecast}"));
        if let Some(deviation) = total.to_budget(TrackStage::Forecast) {
            out.push_str(&format!(
                ", zur Kostenvorgabe {}",
                deviation_text(&deviation)
            ));
        }
    }
    if let Some(contract) = total.value(TrackStage::Contract) {
        out.push_str(&format!("\nAuftrag mit Nachträgen {contract}"));
    }
    if let Some(billed) = total.value(TrackStage::Billed) {
        let released = if report.options.gross {
            report.released_gross
        } else {
            report.released_net
        };
        out.push_str(&format!(
            "\nAbrechnung (geprüft) {billed}, Freigaben {released}"
        ));
    }
    if report.contract_lvs > 0 {
        out.push_str(&format!("\n{}", report.final_text()));
    }
    if report.options.gross {
        for detail in report.vat_details() {
            out.push_str(&format!("\nUSt {detail}"));
        }
    }

    out.push_str("\n\n| Kostengruppe |");
    for stage in &stages {
        out.push_str(&format!(" {} |", stage.label()));
    }
    out.push_str(" Erläuterung / Maßnahme |\n|---|");
    for _ in 0..=stages.len() {
        out.push_str("---|");
    }
    for row in report.rows.iter().chain(std::iter::once(&report.total)) {
        out.push('\n');
        out.push_str(&row_line(report, row, &stages));
    }
    out.push_str(&format!(
        "\n(Klammer: Abweichung {} in € und %; [über Kostenvorgabe] = über der Kostenvorgabe, [über Vorstufe] = mehr als die Schwelle über der Vorstufe)",
        report.options.deviation.label()
    ));

    if !report.lvs.is_empty() {
        out.push_str("\n\nVergabeeinheiten:\n| LV | Auftragnehmer | Angebot | Prognose aus | Freigaben netto | Schlussrechnung |\n|---|---|---|---|---|---|");
        for lv in &report.lvs {
            out.push_str(&format!(
                "\n| {} | {} | {} | {} | {} | {} |",
                cell(&lv.label),
                cell(&lv.contractor),
                cell(lv.offer.as_deref().unwrap_or("")),
                cell(lv.forecast_basis),
                lv.released_net.plain(),
                if lv.final_invoice { "geprüft" } else { "–" }
            ));
        }
    }
    let parts = stage_parts(project);
    if !parts.is_empty() {
        out.push_str("\n\nKostenermittlungen (Schlüssel für kostenverfolgung_aendern kostenermittlungen):\n| Stufe | Name | Art | Stand | Summe | zählt | Vorgabe | Schlüssel |\n|---|---|---|---|---|---|---|---|");
        for part in &parts {
            out.push_str(&format!(
                "\n| {} | {} | {} | {} | {} | {} | {} | {} |",
                part.stage.label(),
                cell(&part.name),
                cell(part.method),
                cell(&part.date),
                part.total().plain(),
                if part.counts { "ja" } else { "nein" },
                if part.by_default { "ja" } else { "nein" },
                part.key
            ));
        }
    }
    let lists = [
        ("Grundlagen", &report.sources),
        ("Prognose – Annahmen", &report.assumptions),
        ("Hinweise", &report.warnings),
    ];
    for (title, items) in lists {
        if !items.is_empty() {
            out.push_str(&format!("\n\n{title}:"));
            for item in items {
                out.push_str(&format!("\n- {item}"));
            }
        }
    }
    let conflicts = tracking.budget_conflicts();
    if !conflicts.is_empty() {
        out.push_str("\n\nKostenvorgabe prüfen:");
        for conflict in conflicts {
            out.push_str(&format!("\n- {conflict}"));
        }
    }
    if !report.without_group.is_empty() {
        out.push_str(
            "\n\nPositionen ohne Kostengruppe:\n| LV | OZ | Kurztext | Betrag |\n|---|---|---|---|",
        );
        for entry in &report.without_group {
            out.push_str(&format!(
                "\n| {} | {} | {} | {} |",
                cell(&entry.lv_label),
                cell(&entry.ordinal),
                cell(&entry.short_text),
                entry.amount.plain()
            ));
        }
    }
    if !tracking.snapshots.is_empty() {
        out.push_str("\n\nFestgehaltene Kostenstände (vergleich = Nummer):");
        for (index, snapshot) in tracking.snapshots.iter().enumerate() {
            out.push_str(&format!("\n{}. {}", index + 1, snapshot.label()));
        }
    }
    out
}

pub(crate) fn read_tracking(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let options = options(&loaded.project, args)?;
    let report = cost_tracking(&loaded.project, &options);
    Ok(describe(&loaded.project, &report))
}

/// Sums per KG of a stage for the take-over: `kostenschaetzung`,
/// `kostenberechnung` (all that count) or the key of one Kostenermittlung.
fn take_over_sums(
    project: &Project,
    from: &str,
) -> Result<(String, BTreeMap<String, Money>), String> {
    let parts = stage_parts(project);
    let stage = match from.trim().to_lowercase().as_str() {
        "kostenschaetzung" | "kostenschätzung" => Some(CostStage::Estimate),
        "kostenberechnung" => Some(CostStage::Calculation),
        _ => None,
    };
    let chosen: Vec<_> = match stage {
        Some(stage) => parts
            .iter()
            .filter(|part| part.stage == stage && part.counts)
            .collect(),
        None => parts
            .iter()
            .filter(|part| part.key == from.trim())
            .collect(),
    };
    if chosen.is_empty() {
        return Err(format!(
            "aus: „{}“ – keine passende Kostenermittlung (kostenschaetzung, kostenberechnung oder ein Schlüssel aus kostenverfolgung_lesen).",
            from.trim()
        ));
    }
    let mut sums: BTreeMap<String, Money> = BTreeMap::new();
    for part in &chosen {
        for (code, amount) in &part.sums {
            if let Some(code) = code {
                *sums.entry(code.clone()).or_default() += *amount;
            }
        }
    }
    let name = match (stage, chosen.as_slice()) {
        (Some(stage), [_, _, ..]) => format!("{} (alle geltenden)", stage.label()),
        (_, [part]) => format!("{}: {}", part.stage.label(), part.name),
        (Some(stage), _) => stage.label().to_string(),
        (None, _) => from.trim().to_string(),
    };
    Ok((name, sums))
}

fn amount_of(value: &Value, code: &str) -> Result<Money, String> {
    let amount = number(value).map_err(|error| format!("budget {code}: {error}"))?;
    if amount < proava_domain::Decimal::ZERO {
        return Err(format!("budget {code}: keine negative Kostenvorgabe."));
    }
    if amount >= proava_domain::Decimal::from(100_000_000_000_i64) {
        return Err(format!("budget {code}: zu groß."));
    }
    Ok(Money::from_decimal(amount))
}

pub(crate) fn set_budget(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let mut changes = Vec::new();
    if let Some(from) = args.opt_text("aus")? {
        let (name, sums) = take_over_sums(&loaded.project, from)?;
        let date = args
            .opt_text("stand")?
            .map(|text| text.trim().to_string())
            .unwrap_or_else(today);
        loaded
            .project
            .cost_tracking_mut()
            .take_budget(&sums, &name, &date);
        changes.push(format!("Kostenvorgabe aus {name} übernommen"));
    } else if args.flag("ersetzen")? {
        loaded.project.cost_tracking_mut().budget.clear();
        changes.push("bisherige Kostenvorgabe geleert".to_string());
    }
    match args.get("budget") {
        None | Some(Value::Null) => {}
        Some(Value::Object(map)) => {
            for (code, value) in map {
                let code = parse_cost_group(code)?
                    .ok_or_else(|| "budget: Kostengruppe fehlt.".to_string())?;
                let amount = match value {
                    Value::Null => Money::ZERO,
                    other => amount_of(other, &code)?,
                };
                loaded.project.cost_tracking_mut().set_budget(&code, amount);
                changes.push(if amount.is_zero() {
                    format!("KG {code} ohne Kostenvorgabe")
                } else {
                    format!("KG {code} {amount}")
                });
            }
        }
        Some(_) => return Err("budget: Objekt {\"420\": 185000, …}.".to_string()),
    }
    let tracking = loaded.project.cost_tracking_mut();
    if let Some(kind) = args.opt_text("art")? {
        tracking.budget_kind = match kind.trim().to_lowercase().as_str() {
            "obergrenze" => BudgetKind::Ceiling,
            "zielgroesse" | "zielgröße" => BudgetKind::Target,
            other => {
                return Err(format!(
                    "art: „{other}“ – bitte obergrenze oder zielgroesse."
                ))
            }
        };
        changes.push(format!("Art {}", tracking.budget_kind.label()));
    }
    if let Some(date) = args.opt_text("stand")? {
        tracking.budget_date = date.trim().to_string();
    }
    if let Some(note) = args.opt_text("grundlage")? {
        tracking.budget_note = note.trim().to_string();
    }
    if let Some(threshold) = args.opt_number("schwelle")? {
        if threshold < proava_domain::Decimal::ZERO
            || threshold > proava_domain::Decimal::ONE_HUNDRED
        {
            return Err("schwelle: 0 bis 100 %.".to_string());
        }
        tracking.threshold_percent = threshold;
        changes.push(format!(
            "Markierung über der Vorstufe ab {} %",
            format_decimal_de(threshold, 1).trim_end_matches(",0")
        ));
    }
    if changes.is_empty() && !args.has("stand") && !args.has("grundlage") {
        return Err(
            "Nichts zu ändern: budget, aus, art, stand, grundlage oder schwelle angeben."
                .to_string(),
        );
    }
    let mut answer = format!("Kostenvorgabe gesetzt: {}", changes.join("; "));
    match tracking.budget_total() {
        Some(total) => answer.push_str(&format!(
            "\nKostenvorgabe gesamt {total} ({}{})",
            tracking.budget_kind.label(),
            if tracking.budget_date.is_empty() {
                String::new()
            } else {
                format!(", Stand {}", tracking.budget_date)
            }
        )),
        None => answer.push_str("\nKeine Kostenvorgabe mehr eingetragen."),
    }
    for conflict in tracking.budget_conflicts() {
        answer.push_str(&format!("\nHinweis: {conflict}"));
    }
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn change_tracking(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let mut changes = Vec::new();
    match args.get("erlaeuterungen") {
        None | Some(Value::Null) => {}
        Some(Value::Object(map)) => {
            for (code, value) in map {
                let code = parse_cost_group(code)?
                    .ok_or_else(|| "erlaeuterungen: Kostengruppe fehlt.".to_string())?;
                let text = match value {
                    Value::Null => String::new(),
                    Value::String(text) => text.trim().to_string(),
                    _ => return Err(format!("erlaeuterungen {code}: Text.")),
                };
                loaded.project.cost_tracking_mut().set_note(&code, &text);
                changes.push(if text.is_empty() {
                    format!("Erläuterung KG {code} entfernt")
                } else {
                    format!("Erläuterung KG {code} {}", group_label(&code))
                });
            }
        }
        Some(_) => return Err("erlaeuterungen: Objekt {\"420\": \"Text\"}.".to_string()),
    }
    match args.get("kostenermittlungen") {
        None | Some(Value::Null) => {}
        Some(Value::Object(map)) => {
            let parts = stage_parts(&loaded.project);
            for (key, value) in map {
                let part = parts.iter().find(|part| part.key == *key).ok_or_else(|| {
                    format!(
                        "kostenermittlungen: Schlüssel „{key}“ unbekannt (kostenverfolgung_lesen)."
                    )
                })?;
                let choice = &mut loaded.project.cost_tracking_mut().source_choice;
                match value {
                    Value::Null => {
                        choice.remove(key);
                        changes.push(format!("{} wieder nach Vorgabe", part.name));
                    }
                    Value::Bool(counts) => {
                        if *counts == part.by_default {
                            choice.remove(key);
                        } else {
                            choice.insert(key.clone(), *counts);
                        }
                        changes.push(format!(
                            "{} {}",
                            part.name,
                            if *counts { "zählt" } else { "zählt nicht" }
                        ));
                    }
                    _ => return Err(format!("kostenermittlungen {key}: true, false oder null.")),
                }
            }
        }
        Some(_) => {
            return Err("kostenermittlungen: Objekt {Schlüssel: true/false/null}.".to_string())
        }
    }
    if let Some(offer) = args.object("angebot_wahl")? {
        let index = loaded.project.active_lv();
        if loaded.project.contract_of(index).is_some() {
            return Err("Dieses LV ist beauftragt – es zählt der Auftrag; ein Angebot wählen geht nur vor der Vergabe.".to_string());
        }
        let choice = match offer.get("angebot") {
            None | Some(Value::Null) => None,
            Some(_) => {
                let id = BidId(
                    offer
                        .opt_count("angebot")?
                        .ok_or("angebot_wahl.angebot: Id aus angebote_lesen.")?,
                );
                let bid = loaded
                    .project
                    .bids_of(index)
                    .iter()
                    .find(|bid| bid.id() == id)
                    .ok_or_else(|| format!("Ein Angebot #{} gibt es in diesem LV nicht.", id.0))?;
                if bid.is_excluded() {
                    return Err(format!(
                        "{} ist vom Preisspiegel ausgeschlossen.",
                        bid.name()
                    ));
                }
                Some(OfferChoice {
                    bid: id,
                    reason: offer
                        .opt_text("begruendung")?
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                })
            }
        };
        let label = match &choice {
            Some(choice) => loaded
                .project
                .bids_of(index)
                .iter()
                .find(|bid| bid.id() == choice.bid)
                .map(|bid| format!("Angebot {} gewählt", bid.name()))
                .unwrap_or_default(),
            None => "wieder der Mindestbieter".to_string(),
        };
        loaded.project.set_offer_choice_of(index, choice);
        changes.push(label);
    }
    if let Some(record) = args.object("kostenstand_festhalten")? {
        let date = record
            .opt_text("datum")?
            .map(|text| text.trim().to_string())
            .unwrap_or_else(today);
        let note = record
            .opt_text("anlass")?
            .unwrap_or_default()
            .trim()
            .to_string();
        let snapshot = record_snapshot(&loaded.project, &date, &note);
        let label = snapshot.label();
        loaded.project.cost_tracking_mut().snapshots.push(snapshot);
        changes.push(format!("Kostenstand {label} festgehalten"));
    }
    if let Some(number) = args.opt_count("kostenstand_loeschen")? {
        let snapshots = &mut loaded.project.cost_tracking_mut().snapshots;
        let index = (number as usize)
            .checked_sub(1)
            .filter(|index| *index < snapshots.len())
            .ok_or_else(|| format!("Einen Kostenstand {number} gibt es nicht."))?;
        let removed = snapshots.remove(index);
        changes.push(format!("Kostenstand {} gelöscht", removed.label()));
    }
    if changes.is_empty() {
        return Err("Nichts zu ändern: erlaeuterungen, kostenermittlungen, angebot_wahl, kostenstand_festhalten oder kostenstand_loeschen angeben.".to_string());
    }
    let report = cost_tracking(&loaded.project, &TrackOptions::default());
    let mut answer = format!("Geändert: {}", changes.join("; "));
    if let Some(forecast) = report.total.value(TrackStage::Forecast) {
        answer.push_str(&format!("\nPrognose jetzt {forecast}"));
    }
    server.save_with_answer(&loaded, answer)
}
