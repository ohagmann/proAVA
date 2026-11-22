//! Nachträge: lesen, anlegen, ändern (Kopf und Prüfung), Positionen setzen,
//! Status wechseln, PDF und GAEB.

use std::path::PathBuf;

use proava_application::{ChangeOrderExport, ChangeOrderPrint};
use proava_domain::{
    change_order_overview, deviation_percent, optional_date, price_hints, sums_by_status,
    ChangeBasis, ChangeOrder, ChangeOrderId, ChangeOrderStatus, ItemChange, Project,
};

use crate::args::{find_row, new_position, Args};
use crate::exchange_tools::{stem, target};
use crate::format::{money, price, quantity};
use crate::lv_tools::label;
use crate::server::Server;

fn today() -> String {
    chrono::Local::now().format("%d.%m.%Y").to_string()
}

/// „N03“, „3“ or 3 → the change order with that number.
fn find(project: &Project, args: &Args) -> Result<ChangeOrderId, String> {
    let wanted = match args.get("nachtrag") {
        Some(serde_json::Value::Number(number)) => number.as_u64().map(|n| n as u32),
        Some(serde_json::Value::String(text)) => text
            .trim()
            .trim_start_matches(['N', 'n'])
            .trim()
            .parse::<u32>()
            .ok(),
        _ => None,
    }
    .ok_or("Bitte nachtrag angeben, z. B. „N03“ oder 3.")?;
    project
        .change_orders()
        .iter()
        .find(|order| order.number == wanted)
        .map(|order| order.id)
        .ok_or_else(|| {
            let known: Vec<String> = project
                .change_orders()
                .iter()
                .map(ChangeOrder::label)
                .collect();
            format!(
                "Nachtrag N{wanted:02} gibt es im LV nicht (vorhanden: {}).",
                if known.is_empty() {
                    "keine".to_string()
                } else {
                    known.join(", ")
                }
            )
        })
}

fn basis_of(text: &str) -> Result<ChangeBasis, String> {
    let wanted = text.trim().to_lowercase();
    if wanted.is_empty() {
        return Ok(ChangeBasis::Unspecified);
    }
    let found = ChangeBasis::ALL
        .into_iter()
        .find(|basis| basis.key() == wanted || basis.label().to_lowercase() == wanted);
    found
        .or_else(|| {
            Some(if wanted.contains("geändert") || wanted.contains("geaendert") || wanted.contains("§ 2 abs. 5") {
                ChangeBasis::ChangedPerformance
            } else if wanted.contains("zusätzlich") || wanted.contains("zusaetzlich") || wanted.contains("§ 2 abs. 6") {
                ChangeBasis::AdditionalPerformance
            } else if wanted.contains("menge") || wanted.contains("§ 2 abs. 3") {
                ChangeBasis::QuantityChange
            } else if wanted.contains("ohne auftrag") || wanted.contains("§ 2 abs. 8") {
                ChangeBasis::WithoutOrder
            } else if wanted.contains("behinderung") || wanted.contains("beschleunigung") || wanted.contains("§ 6") {
                ChangeBasis::Hindrance
            } else if wanted.contains("sonst") {
                ChangeBasis::Other
            } else if wanted.contains("ohne angabe") {
                ChangeBasis::Unspecified
            } else {
                return None;
            })
        })
        .ok_or_else(|| {
            format!(
                "Anlass „{}“ – bitte geändert, zusätzlich, mengenänderung, ohne_auftrag, behinderung oder sonstig.",
                text.trim()
            )
        })
}

fn date_arg(args: &Args, key: &str) -> Result<Option<String>, String> {
    match args.opt_text(key)? {
        Some(text) => {
            optional_date(text).map_err(|error| format!("{key}: {error}"))?;
            Ok(Some(text.trim().to_string()))
        }
        None => Ok(None),
    }
}

fn line_of(project: &Project, order: &ChangeOrder) -> String {
    let contract = project.contract();
    let approved = order.approved_total(contract);
    format!(
        "{} | {} | {} | {}{} | {} | {} | {} | {}",
        order.label(),
        order.title,
        order.basis.label(),
        order.status.label(),
        match order.status_date() {
            "" => String::new(),
            date => format!(" seit {date}"),
        },
        money(order.offered_total(contract)),
        money(order.checked_total(contract)),
        money(approved - order.discount_on(approved, contract)),
        if order.status == ChangeOrderStatus::Approved
            || (order.status.is_open() && order.in_forecast)
        {
            "ja"
        } else {
            "nein"
        }
    )
}

pub(crate) fn read(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    if args.has("nachtrag") {
        let id = find(project, args)?;
        return Ok(detail(project, id));
    }
    let contract = project.contract();
    let mut lines = vec![format!(
        "Nachträge {} ({}){}",
        label(project.lv()),
        project.change_orders().len(),
        match contract {
            Some(contract) => format!(
                " – Auftrag {}{} vom {}, Auftragssumme {}",
                contract.contractor,
                if contract.number.is_empty() {
                    String::new()
                } else {
                    format!(" Nr. {}", contract.number)
                },
                contract.date,
                money(contract.total)
            ),
            None =>
                " – noch kein Auftrag übernommen (auftrag_erteilen; Mengenänderungen brauchen ihn)"
                    .to_string(),
        }
    )];
    let mut orders: Vec<&ChangeOrder> = project.change_orders().iter().collect();
    orders.sort_by_key(|order| order.number);
    if !orders.is_empty() {
        lines.push("Nr. | Bezeichnung | Anlass | Status | angeboten | geprüft | beauftragt (nach Nachlass) | Prognose".into());
        lines.extend(orders.iter().map(|order| line_of(project, order)));
        lines.push("Summen je Status:".into());
        for sum in sums_by_status(contract, project.change_orders())
            .into_iter()
            .filter(|sum| sum.count > 0)
        {
            lines.push(format!(
                "  {}: {} – angeboten {}, geprüft {}, beauftragt {}",
                sum.status.label(),
                sum.count,
                money(sum.offered),
                money(sum.checked),
                money(sum.approved)
            ));
        }
    }
    let overview = change_order_overview(contract, project.change_orders());
    lines.push(format!(
        "Auftragssumme alt {} + beauftragte Nachträge {} = neu {}; offene Nachträge {}; Prognose {}.",
        money(overview.contract_total),
        money(overview.approved),
        money(overview.total_with_change_orders),
        money(overview.open),
        money(overview.forecast)
    ));
    Ok(lines.join("\n"))
}

fn detail(project: &Project, id: ChangeOrderId) -> String {
    let order = project.change_order(id).expect("found");
    let contract = project.contract();
    let mut lines = vec![format!(
        "{} {} – {}{}",
        order.label(),
        order.title,
        order.status.label(),
        match order.status_date() {
            "" => String::new(),
            date => format!(" seit {date}"),
        }
    )];
    let mut fact = |label: &str, value: &str| {
        if !value.trim().is_empty() {
            lines.push(format!("{label}: {}", value.trim().replace('\n', " / ")));
        }
    };
    fact("Firma", order.firm(contract));
    fact("Anlass", order.basis.label());
    fact("Begründung", &order.reason);
    fact("Angekündigt am", &order.announced_date);
    fact("Angebotsfrist", &order.offer_due);
    fact("Angebots-Nr.", &order.offer_number);
    fact("Angebot vom", &order.offer_date);
    fact("Entscheidung bis", &order.decision_due);
    fact("Beauftragt/entschieden am", &order.decision_date);
    fact("Geprüft am", &order.checked_date);
    fact("Geprüft von", &order.checked_by);
    fact("Prüfvermerk", &order.check_note);
    fact("Empfehlung", &order.recommendation);
    lines.push(format!(
        "Nachlass des Auftrags gilt: {}; geht offen in die Prognose ein: {}",
        if order.discount_applies { "ja" } else { "nein" },
        if order.in_forecast { "ja" } else { "nein" }
    ));
    match project.change_order_proposal(id) {
        Ok(proposal) => lines.push(format!(
            "Nachtragsbereich im LV: {} – nächste neue Position bekäme OZ {}",
            proposal.title, proposal.ordinal
        )),
        Err(error) => lines.push(format!("Nachtragsbereich: {error}")),
    }
    if order.items.is_empty() {
        lines.push("Noch keine Positionen (nachtragsposition_setzen).".into());
    } else {
        lines.push("Positionen – OZ | Kurztext | Art | Einheit | Menge ang. | EP ang. | GP ang. | Menge gepr. | EP gepr. | GP gepr. | Abw. % | Status | Vermerk:".into());
        for item in &order.items {
            let (ordinal, short, unit) = match project.lv().position_by_id(item.position) {
                Some(position) => (
                    position.ordinal().to_string(),
                    position.short_text().to_string(),
                    position.unit().to_string(),
                ),
                None => contract
                    .and_then(|contract| contract.item(item.position))
                    .map(|c| (c.ordinal.clone(), c.short_text.clone(), c.unit.clone()))
                    .unwrap_or_else(|| ("?".into(), "(nicht mehr im LV)".into(), String::new())),
            };
            let offered = item.offered_price(contract);
            let checked = item.checked_price(contract);
            lines.push(format!(
                "{ordinal} | {short} | {} | {unit} | {} | {} | {} | {} | {} | {} | {} | {} | {}",
                match item.change {
                    ItemChange::NewPosition => "neu",
                    ItemChange::Quantity if item.offered_quantity.is_sign_negative() =>
                        "Mindermenge",
                    ItemChange::Quantity => "Mehrmenge",
                },
                quantity(item.offered_quantity),
                price(offered),
                money(item.offered_total(contract)),
                quantity(item.checked_quantity()),
                price(checked),
                money(item.checked_total(contract)),
                match (checked, offered) {
                    (Some(checked), Some(offered)) => deviation_percent(checked, offered)
                        .map(|value| proava_domain::format_decimal_de(value, 1))
                        .unwrap_or_default(),
                    _ => String::new(),
                },
                item.status
                    .map(|status| status.label())
                    .unwrap_or("wie Nachtrag"),
                item.note.replace('\n', " / ")
            ));
            let hints = price_hints(project, item);
            if !hints.is_empty() {
                lines.push(format!(
                    "  Prüfhilfe: {}",
                    hints
                        .iter()
                        .map(|hint| format!(
                            "{} {}{}",
                            hint.label,
                            price(Some(hint.price)),
                            hint.deviation
                                .map(|value| format!(
                                    " (angeboten {} %)",
                                    proava_domain::format_decimal_de(value, 1)
                                ))
                                .unwrap_or_default()
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }
    }
    let approved = order.approved_total(contract);
    lines.push(format!(
        "Summen: angeboten {}, geprüft {}, beauftragt {} (nach Nachlass), Gutschrift {}",
        money(order.offered_total(contract)),
        money(order.checked_total(contract)),
        money(approved - order.discount_on(approved, contract)),
        money(order.credit_total(contract))
    ));
    if !order.history.is_empty() {
        lines.push(format!(
            "Statusverlauf: {}",
            order
                .history
                .iter()
                .map(|step| {
                    let mut text = format!("{} {}", step.date, step.status.label())
                        .trim()
                        .to_string();
                    if !step.note.is_empty() {
                        text.push_str(&format!(" – {}", step.note));
                    }
                    text
                })
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    for problem in order.problems(contract, project.lv()) {
        lines.push(format!("Hinweis: {problem}"));
    }
    lines.join("\n")
}

/// Head fields shared by nachtrag_anlegen and nachtrag_aendern.
fn apply_head(order: &mut ChangeOrder, args: &Args) -> Result<Vec<&'static str>, String> {
    let mut changed = Vec::new();
    if let Some(text) = args.opt_text("bezeichnung")? {
        order.title = text.trim().to_string();
        changed.push("Bezeichnung");
    }
    if let Some(text) = args.opt_text("anlass")? {
        order.basis = basis_of(text)?;
        changed.push("Anlass");
    }
    if let Some(text) = args.opt_text("begruendung")? {
        order.reason = text.trim().to_string();
        changed.push("Begründung");
    }
    if let Some(text) = args.opt_text("firma")? {
        order.contractor = text.trim().to_string();
        changed.push("Firma");
    }
    for (key, field, name) in [
        ("angekuendigt", &mut order.announced_date, "angekündigt"),
        ("angebotsfrist", &mut order.offer_due, "Angebotsfrist"),
        ("angebot_vom", &mut order.offer_date, "Angebot vom"),
        (
            "entscheidung_bis",
            &mut order.decision_due,
            "Entscheidung bis",
        ),
        ("entschieden_am", &mut order.decision_date, "entschieden am"),
        ("geprueft_am", &mut order.checked_date, "geprüft am"),
    ] {
        if let Some(date) = date_arg(args, key)? {
            *field = date;
            changed.push(name);
        }
    }
    for (key, field, name) in [
        ("angebot_nr", &mut order.offer_number, "Angebots-Nr."),
        ("geprueft_von", &mut order.checked_by, "geprüft von"),
        ("pruefvermerk", &mut order.check_note, "Prüfvermerk"),
        ("empfehlung", &mut order.recommendation, "Empfehlung"),
    ] {
        if let Some(text) = args.opt_text(key)? {
            *field = text.trim().to_string();
            changed.push(name);
        }
    }
    if let Some(flag) = args.opt_flag("nachlass_gilt")? {
        order.discount_applies = flag;
        changed.push("Nachlass gilt");
    }
    if let Some(flag) = args.opt_flag("prognose")? {
        order.in_forecast = flag;
        changed.push("Prognose");
    }
    if let Some(number) = args.opt_count("nummer")? {
        if !(1..=999).contains(&number) {
            return Err("nummer: 1 bis 999 (GAEB CONo).".into());
        }
        order.number = number as u32;
        changed.push("Nummer");
    }
    Ok(changed)
}

pub(crate) fn create(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let title = args.text("bezeichnung")?.trim().to_string();
    if title.is_empty() {
        return Err("Bitte eine Bezeichnung angeben.".into());
    }
    let mut order = ChangeOrder::new(ChangeOrderId(0), 0, title);
    order.announced_date = today();
    apply_head(&mut order, args)?;
    if loaded
        .project
        .change_orders()
        .iter()
        .any(|other| other.number == order.number && order.number != 0)
    {
        return Err(format!("Nummer N{:02} ist schon vergeben.", order.number));
    }
    // Nachträge gehören zum Auftrag: liegt noch keiner vor, aber ein
    // beauftragtes Angebot, wird es wie in proAVA als Auftrag übernommen.
    let mut contract_note = String::new();
    if loaded.project.contract().is_none() && loaded.project.awarded_bid().is_some() {
        let contract = loaded.project.take_over_contract()?;
        contract_note = format!(
            "\nAuftrag aus dem beauftragten Angebot übernommen: {} – Auftragssumme {}.",
            contract.contractor,
            money(contract.total)
        );
    }
    let id = loaded.project.add_change_order(order);
    let label = loaded
        .project
        .change_order(id)
        .map(ChangeOrder::label)
        .unwrap_or_default();
    let answer = format!(
        "{label} angelegt (angekündigt). Positionen mit nachtragsposition_setzen, Status mit nachtrag_status.{contract_note}"
    );
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn change(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let id = find(&loaded.project, args)?;
    if args.flag("loeschen")? {
        let delete_rows = args.opt_flag("im_lv_loeschen")?.unwrap_or(true);
        let order = loaded
            .project
            .delete_change_order(id, delete_rows)
            .expect("found");
        let answer = format!(
            "{} {} gelöscht{}.",
            order.label(),
            order.title,
            if delete_rows {
                " (neue Positionen auch im LV)"
            } else {
                ""
            }
        );
        return server.save_with_answer(&loaded, answer);
    }
    let mut order = loaded.project.change_order(id).cloned().expect("found");
    let changed = apply_head(&mut order, args)?;
    if changed.is_empty() {
        return Err("Nichts zu ändern – Felder siehe Beschreibung von nachtrag_aendern.".into());
    }
    if loaded
        .project
        .change_orders()
        .iter()
        .any(|other| other.id != id && other.number == order.number)
    {
        return Err(format!("Nummer N{:02} ist schon vergeben.", order.number));
    }
    let label = order.label();
    *loaded.project.change_order_mut(id).expect("found") = order;
    server.save_with_answer(
        &loaded,
        format!("{label}: {} geändert.", changed.join(", ")),
    )
}

pub(crate) fn set_status(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let id = find(&loaded.project, args)?;
    let status = ChangeOrderStatus::parse(args.text("status")?)?;
    let date = date_arg(args, "datum")?.unwrap_or_else(today);
    let note = args.opt_text("vermerk")?.unwrap_or("").to_string();
    let order = loaded.project.change_order_mut(id).expect("found");
    order.set_status(status, &date, &note);
    let label = order.label();
    let mut answer = format!("{label}: {} am {date}.", status.label());
    if status == ChangeOrderStatus::Approved {
        answer.push_str(
            " Die geprüften Positionen sind jetzt abrechenbar und erhöhen die Auftragssumme.",
        );
    }
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn set_item(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let id = find(&loaded.project, args)?;
    let project = &mut loaded.project;
    let mut notes = Vec::new();
    // Which position: a new one, or an LV row (assigned when not yet in it).
    let row = if args.flag("neu")? {
        let mut position = new_position(args, project)?;
        if let Some(text) = args.opt_text("oz_neu")? {
            position.set_ordinal(text.trim());
        }
        let offered = args
            .price_of("angeboten_ep", proava_domain::NumberKind::UnitPrice)?
            .flatten();
        let row = project.add_change_order_position(id, position, offered)?;
        let ordinal = project
            .lv()
            .position_by_id(row)
            .map(|p| p.ordinal().to_string())
            .unwrap_or_default();
        notes.push(format!("neue Position {ordinal} im Nachtragsbereich"));
        row
    } else {
        let index = find_row(project.lv(), args, "oz", "id")?;
        let row = project.lv().rows()[index].id();
        let in_order = project
            .change_order(id)
            .is_some_and(|order| order.item(row).is_some());
        if !in_order {
            let count = project.assign_to_change_order(id, row)?;
            notes.push(format!(
                "{count} {} zugeordnet",
                if count == 1 { "Position" } else { "Positionen" }
            ));
        }
        if project.lv().rows()[index].is_group() {
            let answer = format!(
                "{}: {}.",
                project
                    .change_order(id)
                    .map(ChangeOrder::label)
                    .unwrap_or_default(),
                notes.join(", ")
            );
            return server.save_with_answer(&loaded, answer);
        }
        row
    };
    if args.flag("entfernen")? {
        let delete = args.opt_flag("im_lv_loeschen")?.unwrap_or(false);
        project.remove_change_order_item(id, row, delete)?;
        let label = project
            .change_order(id)
            .map(ChangeOrder::label)
            .unwrap_or_default();
        return server.save_with_answer(
            &loaded,
            format!(
                "{label}: Position entfernt{}.",
                if delete { " und im LV gelöscht" } else { "" }
            ),
        );
    }
    let is_new = args.flag("neu")?;
    let order = project.change_order_mut(id).expect("found");
    let item = order
        .item_mut(row)
        .ok_or("Die Position gehört nicht zum Nachtrag.")?;
    if let Some(value) = args.number_of("angeboten_menge", proava_domain::NumberKind::Quantity)? {
        item.offered_quantity = value;
        notes.push("angebotene Menge".into());
    } else if !is_new {
        if let Some(value) = args.number_of("menge", proava_domain::NumberKind::Quantity)? {
            item.offered_quantity = value;
            notes.push("angebotene Menge".into());
        }
    }
    if !is_new {
        if let Some(value) = args.price_of("angeboten_ep", proava_domain::NumberKind::UnitPrice)? {
            item.offered_unit_price = value;
            notes.push("angebotener EP".into());
        }
    }
    match args.get("geprueft_menge") {
        None => {}
        Some(serde_json::Value::Null) => {
            item.checked_quantity = None;
            notes.push("geprüfte Menge = angeboten".into());
        }
        Some(_) => {
            item.checked_quantity =
                args.number_of("geprueft_menge", proava_domain::NumberKind::Quantity)?;
            notes.push("geprüfte Menge".into());
        }
    }
    if let Some(value) = args.price_of("geprueft_ep", proava_domain::NumberKind::UnitPrice)? {
        item.checked_unit_price = value;
        notes.push(
            if value.is_some() {
                "geprüfter EP"
            } else {
                "geprüfter EP = angeboten"
            }
            .into(),
        );
    }
    if let Some(text) = args.opt_text("vermerk")? {
        item.note = text.trim().to_string();
        notes.push("Vermerk".into());
    }
    if let Some(text) = args.opt_text("status")? {
        let wanted = text.trim().to_lowercase();
        item.status = if wanted.is_empty() || wanted == "wie nachtrag" || wanted == "nachtrag" {
            None
        } else {
            Some(ChangeOrderStatus::parse(text)?)
        };
        notes.push("Status".into());
    }
    let order = project.change_order(id).expect("found");
    let contract = project.contract();
    let item = order.item(row).expect("set");
    let ordinal = project
        .lv()
        .position_by_id(row)
        .map(|p| p.ordinal().to_string())
        .unwrap_or_default();
    let answer = format!(
        "{} {ordinal}: {} – angeboten {} × {} = {}, geprüft {} × {} = {}. Nachtrag geprüft gesamt {}.",
        order.label(),
        if notes.is_empty() { "unverändert".to_string() } else { notes.join(", ") },
        quantity(item.offered_quantity),
        price(item.offered_price(contract)),
        money(item.offered_total(contract)),
        quantity(item.checked_quantity()),
        price(item.checked_price(contract)),
        money(item.checked_total(contract)),
        money(order.checked_total(contract))
    );
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn print(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let print = match args.text("art")?.trim().to_lowercase().as_str() {
        "pruefung" | "prüfung" => ChangeOrderPrint::Check(find(&loaded.project, args)?),
        "uebersicht" | "übersicht" => ChangeOrderPrint::Overview,
        other => return Err(format!("art „{other}“ – bitte pruefung oder uebersicht.")),
    };
    let word = match print {
        ChangeOrderPrint::Check(id) => format!(
            "{}_{}",
            print.file_label(),
            loaded
                .project
                .change_order(id)
                .map(ChangeOrder::label)
                .unwrap_or_default()
        ),
        ChangeOrderPrint::Overview => print.file_label().to_string(),
    };
    let name = format!(
        "{}_{word}_{}.pdf",
        stem(&loaded.path),
        chrono::Local::now().format("%Y-%m-%d")
    );
    let file = target(server, args, &loaded.path, name)?;
    let logo = server.pdf_logo(args)?;
    let pages = server.exchange().export_change_order_pdf(
        &loaded.project,
        print,
        logo.as_ref(),
        &server.print.look,
        &server.print.page_frame,
        &file,
    )?;
    Ok(format!(
        "{} geschrieben: {} ({pages} {})",
        print.label(),
        file.display(),
        if pages == 1 { "Seite" } else { "Seiten" }
    ))
}

pub(crate) fn export(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let id = find(&loaded.project, args)?;
    let kind = match args.text("art")?.trim().to_uppercase().as_str() {
        "X83" | "83" => ChangeOrderExport::Request,
        "X86" | "86" => ChangeOrderExport::Order,
        other => {
            return Err(format!(
                "art „{other}“ – bitte X83 (Angebotsaufforderung) oder X86 (Nachtragsauftrag)."
            ))
        }
    };
    let order_label = loaded
        .project
        .change_order(id)
        .map(ChangeOrder::label)
        .unwrap_or_default();
    let name = format!(
        "{}_Nachtrag_{order_label}_{}.{}",
        stem(&loaded.path),
        chrono::Local::now().format("%Y-%m-%d"),
        kind.extension()
    );
    let file = target(server, args, &loaded.path, name)?;
    let warnings = server
        .exchange()
        .export_change_order_gaeb(&loaded.project, id, kind, &file)?;
    let mut answer = format!("{} geschrieben: {}", kind.label(), file.display());
    for warning in warnings {
        answer.push_str(&format!("\nHinweis: {warning}"));
    }
    Ok(answer)
}

pub(crate) fn import_offer(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let id = find(&loaded.project, args)?;
    let file = server.allowed(&PathBuf::from(args.text("datei")?.trim()))?;
    let (offer, mut warnings) = server.exchange().read_change_order_offer(&file)?;
    let result = loaded
        .project
        .take_change_order_offer(id, &offer, &today())?;
    warnings.extend(result.warnings);
    let order = loaded.project.change_order(id).expect("found");
    let contract = loaded.project.contract();
    let mut answer = format!(
        "{}: {} {} übernommen, {} neu im Nachtragsbereich; Status {}, angeboten {}.",
        order.label(),
        result.updated,
        if result.updated == 1 {
            "Position"
        } else {
            "Positionen"
        },
        result.created,
        order.status.label(),
        money(order.offered_total(contract))
    );
    for warning in warnings {
        answer.push_str(&format!("\nHinweis: {warning}"));
    }
    server.save_with_answer(&loaded, answer)
}
