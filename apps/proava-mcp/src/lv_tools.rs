//! Several LVs per project and the project's master data (Projektstammdaten).

use proava_domain::{
    join_deadline, split_deadline, Address, Contact, Decimal, Deduction, Lv, OzMask, Project,
};
use serde_json::Value;

use crate::args::Args;
use crate::format::{money, percent};
use crate::server::Server;

/// The LV meant by the argument `lv`: first an LV number („2“, „481“), else
/// the position in `lvs_lesen` (1 = first). Without the argument: the active LV.
pub(crate) fn select_lv(project: &Project, value: Option<&Value>) -> Result<usize, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(project.active_lv());
    };
    let wanted = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => {
            return Err("lv: bitte LV-Nummer oder Position als Text oder Zahl angeben.".to_string())
        }
    };
    if wanted.is_empty() {
        return Ok(project.active_lv());
    }
    let wanted_loose = wanted.trim_start_matches("LV").trim();
    if let Some(index) = project
        .lvs()
        .position(|lv| lv.number().trim() == wanted || lv.number().trim() == wanted_loose)
    {
        return Ok(index);
    }
    if let Ok(position) = wanted_loose.parse::<usize>() {
        if (1..=project.lv_count()).contains(&position) {
            return Ok(position - 1);
        }
    }
    Err(format!(
        "LV „{wanted}“ gibt es im Projekt nicht. Vorhanden: {}.",
        project
            .lvs()
            .enumerate()
            .map(|(index, lv)| format!("{} = {}", index + 1, label(lv)))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// „LV 2 – Elektro“
pub(crate) fn label(lv: &Lv) -> String {
    match (lv.number().trim(), lv.title().trim()) {
        ("", title) => format!("„{title}“"),
        (number, title) => format!("LV {number} – {title}"),
    }
}

pub(crate) fn read_lvs(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let mut lines = vec![format!(
        "{} LV im Projekt „{}“ (Pos | Nr. | Bezeichnung | Art | OZ-Maske | Positionen | netto | Angebote):",
        project.lv_count(),
        project.name()
    )];
    for (index, lv) in project.lvs().enumerate() {
        lines.push(format!(
            "{} | {} | {} | {} | {} | {} | {} | {}{}",
            index + 1,
            if lv.number().trim().is_empty() {
                "–"
            } else {
                lv.number().trim()
            },
            lv.title(),
            crate::cost_lv_tools::purpose_text(lv),
            lv.oz_mask().pattern(),
            lv.position_count(),
            money(lv.net_total()),
            project.bids_of(index).len(),
            if index == loaded.stored_active() {
                " | aktiv in proAVA"
            } else {
                ""
            }
        ));
    }
    let doubles = project.duplicate_lv_numbers();
    if !doubles.is_empty() {
        lines.push(format!(
            "Achtung: LV-Nummer doppelt: {}.",
            doubles.join(", ")
        ));
    }
    lines.push(
        "Andere Werkzeuge wählen ein LV mit lv (Nummer oder Position). Kosten-LVs \
         (Kostenschätzung/Kostenberechnung nach DIN 276) gehen nie an Bieter."
            .to_string(),
    );
    Ok(lines.join("\n"))
}

pub(crate) fn create_lv(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let project = &mut loaded.project;
    let template = project.lv().clone();
    let title = args
        .opt_text("bezeichnung")?
        .unwrap_or_default()
        .trim()
        .to_string();
    // With aus the title comes from the Kostenschätzung.
    if title.is_empty() && !args.get("aus").is_some_and(|value| !value.is_null()) {
        return Err("Bitte eine Bezeichnung für das LV angeben.".to_string());
    }
    let number = match args.opt_text("nummer")? {
        Some(number) if !number.trim().is_empty() => number.trim().to_string(),
        _ => next_number(project),
    };
    if project.lvs().any(|lv| lv.number().trim() == number) {
        return Err(format!(
            "Die LV-Nummer „{number}“ gibt es im Projekt schon."
        ));
    }
    let mask = match args.opt_text("oz_maske")? {
        Some(pattern) if !pattern.trim().is_empty() => OzMask::parse(pattern.trim())?,
        _ => template.oz_mask().clone(),
    };
    let step = match args.opt_count("schrittweite")? {
        Some(step) if [1, 10, 100, 1000].contains(&step) => step as u32,
        Some(_) => return Err("Die Schrittweite ist 1, 10, 100 oder 1000.".to_string()),
        None => template.position_step(),
    };
    // Kosten-LV (art kostenschaetzung/kostenberechnung): titles after DIN 276
    // or a copy of a Kostenschätzung (aus); a given mask keeps its own outline.
    let purpose = crate::cost_lv_tools::purpose_argument(args)?;
    let cost_lv = match purpose.cost_stage() {
        Some(stage) => crate::cost_lv_tools::new_cost_lv(project, args, stage, &title, step)?,
        None if args.get("aus").is_some_and(|value| !value.is_null()) => {
            return Err("aus geht nur mit art kostenberechnung.".to_string())
        }
        None => None,
    };
    let mut lv = match cost_lv {
        Some(lv) => lv,
        None => {
            let mut lv = Lv::new(title, mask);
            lv.set_purpose(purpose);
            lv
        }
    };
    lv.set_number(number);
    lv.set_position_step(step);
    let vat = args.opt_number("mwst")?.unwrap_or(template.vat_percent());
    if vat < Decimal::ZERO || vat > Decimal::ONE_HUNDRED {
        return Err("Die Umsatzsteuer muss zwischen 0 und 100 % liegen.".to_string());
    }
    lv.set_vat_percent(vat);
    lv.set_currency(
        args.opt_text("waehrung")?
            .filter(|text| !text.trim().is_empty())
            .map(|text| text.trim().to_uppercase())
            .unwrap_or_else(|| template.currency().to_string()),
    );
    let answer = format!(
        "LV angelegt: {} – {} (Position {} von {}), OZ-Maske {}, Schrittweite {}, MwSt {}. Weitere Werkzeuge mit lv: „{}“.",
        label(&lv),
        crate::cost_lv_tools::purpose_text(&lv),
        project.lv_count() + 1,
        project.lv_count() + 1,
        lv.oz_mask().pattern(),
        lv.position_step(),
        percent(lv.vat_percent()),
        lv.number()
    );
    project.add_lv(lv);
    server.save_with_answer(&loaded, answer)
}

fn next_number(project: &Project) -> String {
    let mut next = project
        .lvs()
        .filter_map(|lv| lv.number().trim().parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    while project
        .lvs()
        .any(|lv| lv.number().trim() == next.to_string())
    {
        next += 1;
    }
    next.to_string()
}

/// Lines of `projekt_lesen` about the master data (only what is filled in).
pub(crate) fn master_data_lines(project: &Project) -> Vec<String> {
    let info = project.info();
    let mut lines = Vec::new();
    let mut add = |label: &str, value: String| {
        if !value.trim().is_empty() {
            lines.push(format!("{label}: {}", value.trim()));
        }
    };
    add("Projektbezeichnung", info.label.clone());
    add("Kurzname/Projektnummer AG", info.short_name.clone());
    add("Baumaßnahme", info.description.replace('\n', " / "));
    add("Bauherr", info.owner.details());
    match &info.own_client {
        Some(client) => add("Auftraggeber", client.details()),
        None if !info.owner.one_line().trim().is_empty() => {
            add("Auftraggeber", "wie Bauherr".to_string())
        }
        None => {}
    }
    add("Aufsteller/Planer", info.planner.details());
    add(
        "Baustelle",
        proava_domain::Place {
            address: info.site.clone(),
            number: info.site_number.clone(),
            name: info.site_name.clone(),
        }
        .details(),
    );
    if let Some(place) = &info.notification_site {
        add("Benachrichtigungsort", place.details());
    }
    for site in &info.further_sites {
        add("Weiterer Ausführungsort", site.site.details());
        if let Some(place) = &site.notification {
            add("  dessen Benachrichtigungsort", place.details());
        }
    }
    for place in &info.requesters {
        add("Bedarfsträger", place.details());
    }
    add(
        "Debitorennummer AG (AcctRecNo)",
        info.client_account.clone(),
    );
    add(
        "Staat AG",
        match info.client_country_kind.as_str() {
            "" => String::new(),
            kind => format!(
                "{}{}",
                match kind {
                    "EEA" => "EWR",
                    "Other" => "anderer Staat",
                    other => other,
                },
                if info.client_country_code.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", info.client_country_code)
                }
            ),
        },
    );
    for contact in &info.contacts {
        add(
            &format!(
                "Adresse {}",
                if contact.role.is_empty() {
                    "–"
                } else {
                    &contact.role
                }
            ),
            contact.address.details(),
        );
    }
    add("Vergabeart", info.award_procedure.clone());
    add("Vergabenummer", info.award_number.clone());
    add("Datenaustauschnummer (DPNo)", info.exchange_number.clone());
    add("Abgabetermin", info.submission_deadline.clone());
    add("Abgabeort", info.submission_place.clone());
    add("Zuschlags-/Bindefrist", info.binding_period_end.clone());
    add("Ausführungsbeginn", info.execution_start.clone());
    add("Ausführungsende", info.execution_end.clone());
    add("Weitere Fristen", info.execution_note.replace('\n', " / "));
    let terms = &info.terms;
    add(
        "Sicherheit Vertragserfüllung",
        terms
            .performance_security_percent
            .map(percent)
            .unwrap_or_default(),
    );
    add(
        "Sicherheit Mängelansprüche",
        terms
            .warranty_security_percent
            .map(percent)
            .unwrap_or_default(),
    );
    add(
        "Skonto",
        match (terms.cash_discount_percent, terms.cash_discount_days) {
            (Some(rate), Some(days)) => format!("{} in {days} Tagen", percent(rate)),
            (Some(rate), None) => percent(rate),
            _ => String::new(),
        },
    );
    if !terms.deductions.is_empty() {
        add(
            "Abzüge/Umlagen",
            format!(
                "{} (zusammen {})",
                terms
                    .deductions
                    .iter()
                    .map(|deduction| format!("{} {}", deduction.label, percent(deduction.percent)))
                    .collect::<Vec<_>>()
                    .join(", "),
                percent(terms.deduction_percent())
            ),
        );
    }
    add(
        "Zahlungsbedingungen",
        terms.payment_terms.replace('\n', " / "),
    );
    add("Weitere Vereinbarungen", terms.notes.replace('\n', " / "));
    if let Some(frame) = &info.page_frame {
        add("Eigene Kopf-/Fußzeile", crate::frame_args::describe(frame));
    }
    for warning in info.warnings() {
        lines.push(format!("Hinweis: {warning}"));
    }
    lines
}

/// The master data fields of `projektangaben_setzen` beyond the old ones.
/// Pushes the changed labels; checks dates and percentages.
pub(crate) fn set_master_data(
    project: &mut Project,
    args: &Args,
    changed: &mut Vec<&'static str>,
) -> Result<(), String> {
    let info = project.info_mut();
    let texts: [(&str, &str); 9] = [
        ("kurzname", "Kurzname"),
        ("baumassnahme", "Baumaßnahme"),
        ("vergabeart", "Vergabeart"),
        ("bindefrist", "Bindefrist"),
        ("ausfuehrung_beginn", "Ausführungsbeginn"),
        ("ausfuehrung_ende", "Ausführungsende"),
        ("weitere_fristen", "Weitere Fristen"),
        ("zahlungsbedingungen", "Zahlungsbedingungen"),
        ("weitere_vereinbarungen", "Weitere Vereinbarungen"),
    ];
    for (key, label) in texts {
        if let Some(value) = args.opt_text(key)? {
            let field = match key {
                "kurzname" => &mut info.short_name,
                "baumassnahme" => &mut info.description,
                "vergabeart" => &mut info.award_procedure,
                "bindefrist" => &mut info.binding_period_end,
                "ausfuehrung_beginn" => &mut info.execution_start,
                "ausfuehrung_ende" => &mut info.execution_end,
                "weitere_fristen" => &mut info.execution_note,
                "zahlungsbedingungen" => &mut info.terms.payment_terms,
                _ => &mut info.terms.notes,
            };
            *field = value.trim().to_string();
            changed.push(label);
        }
    }
    if let Some(time) = args.opt_text("abgabe_uhrzeit")? {
        let (date, _) = split_deadline(&info.submission_deadline);
        info.submission_deadline = join_deadline(&date, time);
        changed.push("Abgabe-Uhrzeit");
    }
    if let Some(site) = args.object("baustelle")? {
        merge_address(&mut info.site, &site)?;
        changed.push("Baustelle");
    }
    let texts: [(&str, &str); 3] = [
        ("baustelle_nummer", "Kennnummer der Baustelle"),
        ("baustelle_bezeichnung", "Bezeichnung der Baustelle"),
        ("debitorennummer", "Debitorennummer"),
    ];
    for (key, label) in texts {
        if let Some(value) = args.opt_text(key)? {
            let field = match key {
                "baustelle_nummer" => &mut info.site_number,
                "baustelle_bezeichnung" => &mut info.site_name,
                _ => &mut info.client_account,
            };
            *field = value.trim().to_string();
            changed.push(label);
        }
    }
    if let Some(kind) = args.opt_text("auftraggeber_staat")? {
        info.client_country_kind = match kind.trim().to_lowercase().as_str() {
            "" => String::new(),
            "ewr" | "eea" => "EEA".to_string(),
            "anderer" | "anderer staat" | "other" => "Other".to_string(),
            other => {
                return Err(format!(
                    "auftraggeber_staat: „{other}“ – erlaubt sind „ewr“, „anderer“ oder „“ (GAEB CntryType)."
                ))
            }
        };
        if info.client_country_kind.is_empty() {
            info.client_country_code.clear();
        }
        changed.push("Staat des Auftraggebers");
    }
    if let Some(code) = args.opt_text("auftraggeber_kfz_kennzeichen")? {
        let code = code.trim().to_uppercase();
        if code.chars().count() > 3 {
            return Err(format!(
                "auftraggeber_kfz_kennzeichen: „{code}“ – höchstens 3 Zeichen (internationales Kfz-Kennzeichen, z. B. „D“)."
            ));
        }
        info.client_country_code = code;
        changed.push("Kfz-Kennzeichen des Auftraggebers");
    }
    match args.get("benachrichtigungsort") {
        None => {}
        Some(Value::Null) => {
            info.notification_site = None;
            changed.push("Benachrichtigungsort");
        }
        Some(_) => {
            let entry = args.object("benachrichtigungsort")?.expect("given");
            let mut place = info.notification_site.clone().unwrap_or_default();
            merge_place(&mut place, &entry)?;
            info.notification_site = (!place.is_empty()).then_some(place);
            changed.push("Benachrichtigungsort");
        }
    }
    if let Some(list) = args.get("bedarfstraeger").filter(|value| !value.is_null()) {
        info.requesters = places(list, "bedarfstraeger")?;
        changed.push("Bedarfsträger");
    }
    if let Some(list) = args
        .get("weitere_ausfuehrungsorte")
        .filter(|value| !value.is_null())
    {
        let items = list
            .as_array()
            .ok_or("weitere_ausfuehrungsorte: bitte eine Liste angeben.")?;
        let mut sites = Vec::new();
        for item in items {
            let map = item
                .as_object()
                .ok_or("weitere_ausfuehrungsorte: jeder Eintrag ist ein Objekt.")?;
            let entry = Args(map);
            let mut site = proava_domain::ExecutionSite::default();
            merge_place(&mut site.site, &entry)?;
            if let Some(notification) = entry.object("benachrichtigungsort")? {
                let mut place = proava_domain::Place::default();
                merge_place(&mut place, &notification)?;
                site.notification = Some(place);
            }
            sites.push(site);
        }
        info.further_sites = sites;
        changed.push("Weitere Ausführungsorte");
    }
    if let Some(list) = args.get("adressen").filter(|value| !value.is_null()) {
        let items = list
            .as_array()
            .ok_or("adressen: bitte eine Liste angeben.")?;
        let mut contacts = Vec::new();
        for item in items {
            let map = item
                .as_object()
                .ok_or("adressen: jeder Eintrag ist ein Objekt.")?;
            let entry = Args(map);
            let mut address = Address::default();
            merge_address(&mut address, &entry)?;
            contacts.push(Contact {
                role: entry
                    .opt_text("rolle")?
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                address,
            });
        }
        info.contacts = contacts;
        changed.push("Weitere Adressen");
    }
    let percents: [(&str, &str); 3] = [
        (
            "sicherheit_vertragserfuellung",
            "Sicherheit Vertragserfüllung",
        ),
        ("sicherheit_maengel", "Sicherheit Mängelansprüche"),
        ("skonto_prozent", "Skonto"),
    ];
    for (key, label) in percents {
        if args.has(key) {
            let value = optional_number(args, key)?;
            let field = match key {
                "sicherheit_vertragserfuellung" => &mut info.terms.performance_security_percent,
                "sicherheit_maengel" => &mut info.terms.warranty_security_percent,
                _ => &mut info.terms.cash_discount_percent,
            };
            *field = value;
            changed.push(label);
        }
    }
    if args.has("skonto_tage") {
        info.terms.cash_discount_days = match args.get("skonto_tage") {
            Some(Value::Null) => None,
            _ => args.opt_count("skonto_tage")?.map(|days| days as u32),
        };
        changed.push("Skontofrist");
    }
    if let Some(list) = args.get("abzuege").filter(|value| !value.is_null()) {
        let items = list
            .as_array()
            .ok_or("abzuege: bitte eine Liste angeben.")?;
        let mut deductions = Vec::new();
        for item in items {
            let map = item
                .as_object()
                .ok_or("abzuege: jeder Eintrag ist ein Objekt {bezeichnung, prozent}.")?;
            let entry = Args(map);
            deductions.push(Deduction {
                label: entry.text("bezeichnung")?.trim().to_string(),
                percent: entry.opt_number("prozent")?.unwrap_or_default(),
            });
        }
        info.terms.deductions = deductions;
        changed.push("Abzüge");
    }
    // The plausibility of the whole (dates, order, percentages) is checked by
    // projektangaben_setzen, together with the other fields.
    Ok(())
}

/// A number or null (null/"" = no value).
fn optional_number(args: &Args, key: &str) -> Result<Option<Decimal>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        _ => args.opt_number(key),
    }
}

/// Address fields plus `nummer` (Kennnummer) and `bezeichnung` of a place
/// (GAEB Requester, CnstSite, NotifSite).
fn merge_place(place: &mut proava_domain::Place, args: &Args) -> Result<(), String> {
    merge_address(&mut place.address, args)?;
    if let Some(number) = args.opt_text("nummer")? {
        place.number = number.trim().to_string();
    }
    if let Some(name) = args.opt_text("bezeichnung")? {
        place.name = name.trim().to_string();
    }
    Ok(())
}

/// A list of places (replaces the list).
fn places(list: &Value, key: &str) -> Result<Vec<proava_domain::Place>, String> {
    let items = list
        .as_array()
        .ok_or_else(|| format!("{key}: bitte eine Liste angeben."))?;
    let mut places = Vec::new();
    for item in items {
        let map = item
            .as_object()
            .ok_or_else(|| format!("{key}: jeder Eintrag ist ein Objekt."))?;
        let mut place = proava_domain::Place::default();
        merge_place(&mut place, &Args(map))?;
        places.push(place);
    }
    Ok(places)
}

pub(crate) fn merge_address(address: &mut Address, args: &Args) -> Result<(), String> {
    let fields: [(&str, &mut String); 14] = [
        ("name1", &mut address.name1),
        ("name2", &mut address.name2),
        ("name3", &mut address.name3),
        ("name4", &mut address.name4),
        ("strasse", &mut address.street),
        ("plz", &mut address.postal_code),
        ("ort", &mut address.city),
        ("land", &mut address.country),
        ("iln", &mut address.iln),
        ("ansprechpartner", &mut address.contact),
        ("telefon", &mut address.phone),
        ("fax", &mut address.fax),
        ("email", &mut address.email),
        ("ust_id", &mut address.vat_id),
    ];
    for (key, field) in fields {
        if let Some(value) = args.opt_text(key)? {
            *field = value.trim().to_string();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use proava_domain::{Lv, OzMask, Project, ProjectName};
    use serde_json::json;

    use super::select_lv;

    #[test]
    fn lv_by_number_or_position() {
        let mask = || OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        let mut first = Lv::new("GA", mask());
        first.set_number("481");
        let mut project = Project::new(ProjectName::parse("P").unwrap(), "2026", first);
        let mut second = Lv::new("Elektro", mask());
        second.set_number("2");
        project.add_lv(second);
        assert_eq!(select_lv(&project, None).unwrap(), 0);
        assert_eq!(select_lv(&project, Some(&json!("481"))).unwrap(), 0);
        assert_eq!(select_lv(&project, Some(&json!("LV 481"))).unwrap(), 0);
        assert_eq!(
            select_lv(&project, Some(&json!(2))).unwrap(),
            1,
            "Nummer 2 vor Position 2"
        );
        assert_eq!(
            select_lv(&project, Some(&json!(1))).unwrap(),
            0,
            "Position 1"
        );
        let error = select_lv(&project, Some(&json!("9"))).unwrap_err();
        assert!(error.contains("1 = LV 481 – GA"), "{error}");
    }
}
