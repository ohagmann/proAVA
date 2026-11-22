//! Kostenermittlung nach DIN 276 (Reiter Kosten): lesen, anlegen, ändern,
//! DP-Zeilen schreiben. Gedruckt wird mit `pdf_drucken dokument:
//! kostenermittlung`. Gilt für das ganze Projekt, nicht je LV.

use proava_domain::{
    cost_control, format_decimal_de, CostEstimate, CostEstimateId, CostSection, CostShare,
    CostStage, Decimal, Lv, LvGroup, LvRow, NumberKind, PointKind, PointRow, Project,
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

fn percent(value: Decimal) -> String {
    let value = value.round_dp(2).normalize();
    format!("{} %", format_decimal_de(value, value.scale()))
}

/// `kostenermittlung`: Nummer in der Liste (1 = erste), Name oder Teil davon;
/// ohne Angabe die neueste.
fn find(project: &Project, args: &Args) -> Result<CostEstimateId, String> {
    let wanted = match args.get("kostenermittlung") {
        None | Some(Value::Null) => String::new(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(text)) => text.trim().to_string(),
        Some(_) => return Err("kostenermittlung: Nummer oder Name.".to_string()),
    };
    if project.cost_estimates().is_empty() {
        return Err("Im Projekt gibt es keine Kostenermittlung – erst kosten_anlegen.".to_string());
    }
    project
        .find_cost_estimate(&wanted)
        .map(|estimate| estimate.id)
        .ok_or_else(|| {
            format!(
                "Eine Kostenermittlung „{wanted}“ gibt es nicht (kosten_lesen zeigt die Liste)."
            )
        })
}

fn position_of(project: &Project, id: CostEstimateId) -> usize {
    project
        .cost_estimates()
        .iter()
        .position(|estimate| estimate.id == id)
        .map(|index| index + 1)
        .unwrap_or(0)
}

/// The estimate as text: head, DP-Arten, Bauteile with sums and split,
/// Zusammenfassung, Kostenkontrolle, hints; `rows` adds the plant lists.
fn describe(project: &Project, estimate: &CostEstimate, rows: bool) -> String {
    let mut out = format!(
        "#{} {} – {} (Stand {}, Bearbeiter {}){}",
        position_of(project, estimate.id),
        estimate.display_name(),
        estimate.stage.level_label(),
        cell(&estimate.date),
        cell(&estimate.author),
        if estimate.remark.trim().is_empty() {
            String::new()
        } else {
            format!("\nBemerkung: {}", estimate.remark.trim())
        }
    );
    out.push_str(&body(estimate, rows));
    let control = cost_control(project, estimate);
    if control.has_planner || control.has_award {
        out.push_str("\n\nKostenkontrolle (Kostenvoranschlag = Planerpreise der Ausschreibungs-LVs je KG; Auftrag = beauftragte Angebote netto vor Nachlass):");
        for line in &control.lines {
            out.push_str(&format!(
                "\n  KG {}: Ermittlung {} · Kostenvoranschlag {} (Abw. {}){}",
                if line.code.is_empty() {
                    "–"
                } else {
                    &line.code
                },
                line.estimate.plain(),
                line.planner.plain(),
                line.planner_difference().plain(),
                if control.has_award {
                    format!(" · Auftrag {}", line.awarded.plain())
                } else {
                    String::new()
                }
            ));
        }
    }
    out.push_str(&hints(estimate));
    out
}

/// A Datenpunkt-Abschnitt of a Kosten-LV (GU-Fall) as text: where it is,
/// then everything of a Datenpunktrechnung but the Kostenkontrolle.
fn describe_section(lv: &Lv, index: usize, rows: bool) -> String {
    let row = &lv.rows()[index];
    let Some(points) = row.as_group().and_then(LvGroup::points) else {
        return String::new();
    };
    let mut out = format!(
        "Datenpunkt-Abschnitt {} im Kosten-LV {} – {}, zählt mit dem Kosten-LV (Bezeichnung, Stand: die des LV)",
        section_name(row),
        crate::lv_tools::label(lv),
        points.stage.level_label(),
    );
    out.push_str(&body(points, rows));
    out.push_str(&hints(points));
    out
}

/// „480 Gebäude- und Anlagenautomation“.
fn section_name(row: &LvRow) -> String {
    [row.ordinal().trim(), row.short_text().trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Hints of the estimate (split ≠ 100 %, DP-Art without price …).
fn hints(estimate: &CostEstimate) -> String {
    let result = estimate.result();
    let mut out = String::new();
    if !result.warnings.is_empty() {
        out.push_str("\n\nHinweise:");
        for warning in &result.warnings {
            out.push_str(&format!("\n- {warning}"));
        }
    }
    out
}

/// Sums, DP-Arten, Bauteile (with the plant lists when `rows`) and the
/// Zusammenfassung je KG.
fn body(estimate: &CostEstimate, rows: bool) -> String {
    let result = estimate.result();
    let mut out = String::new();
    out.push_str(&format!(
        "\nGesamt: {} netto, {} Datenpunkte",
        result.total, result.points
    ));
    if let (Some(low), Some(high)) = (result.low, result.high) {
        out.push_str(&format!(", Spanne {low} bis {high}"));
    }
    if estimate.stage == CostStage::Estimate {
        out.push_str(&if estimate.rate_per_kind {
            "\nKennwert: je DP-Art (Preise unten)".to_string()
        } else {
            format!(
                "\nKennwert: {} € je DP",
                format_decimal_de(estimate.rate, 2)
            )
        });
    } else {
        out.push_str(&format!("; Prüfsumme {}", result.check));
    }
    out.push_str("\n\nDP-Arten:\n| Kürzel | Bezeichnung | virtuell | Preis je DP | Anzahl | Betrag |\n|---|---|---|---|---|---|");
    for (kind, sum) in estimate.kinds.iter().zip(&result.kinds) {
        out.push_str(&format!(
            "\n| {} | {} | {} | {} | {} | {} |",
            cell(&kind.key),
            cell(&kind.label),
            if kind.is_virtual() { "ja" } else { "nein" },
            format_decimal_de(sum.unit_price, 2),
            sum.count,
            sum.amount.plain()
        ));
    }
    for (section, sums) in estimate.sections.iter().zip(&result.sections) {
        out.push_str(&format!(
            "\n\nBauteil „{}“: {} DP, {}{}",
            section.name.trim(),
            sums.points,
            sums.total,
            if section.shares.is_some() {
                " (eigene Aufteilung)"
            } else {
                ""
            }
        ));
        if let (Some(low), Some(high)) = (sums.low, sums.high) {
            out.push_str(&format!(" (Spanne {low} bis {high})"));
        }
        if estimate.stage == CostStage::Calculation {
            for share in &sums.shares {
                out.push_str(&format!(
                    "\n  KG {} {}: {} = {}",
                    share.code,
                    share.label,
                    percent(share.percent),
                    share.amount
                ));
            }
            out.push_str(&format!(
                "\n  Summe {} · Prüfsumme {}",
                percent(sums.percent_sum),
                sums.check
            ));
        }
        if rows && !section.rows.is_empty() {
            out.push_str("\n\n| Zeile | Ort | Gewerk | Anlage |");
            for kind in &estimate.kinds {
                out.push_str(&format!(" {} |", cell(&kind.key)));
            }
            out.push_str(" DP | Bemerkung |\n|---|---|---|---|");
            out.push_str(&"---|".repeat(estimate.kinds.len() + 2));
            for (index, row) in section.rows.iter().enumerate() {
                out.push_str(&format!(
                    "\n| {} | {} | {} | {} |",
                    index + 1,
                    cell(&row.location),
                    cell(&row.trade),
                    cell(&row.plant)
                ));
                for kind in 0..estimate.kinds.len() {
                    out.push_str(&format!(" {} |", row.count(kind)));
                }
                out.push_str(&format!(" {} | {} |", row.points(), cell(&row.remark)));
            }
        } else if !rows {
            out.push_str(&format!(
                " – {} Zeilen (zeilen: true zeigt sie)",
                section.rows.len()
            ));
        }
    }
    if estimate.stage == CostStage::Calculation {
        out.push_str("\n\nZusammenfassung je Kostengruppe:");
        for group in &result.groups {
            out.push_str(&format!(
                "\n  KG {} {}: {} ({})",
                group.code,
                group.label,
                group.amount,
                percent(group.percent)
            ));
        }
        out.push_str(&format!(
            "\n  Gesamt {} · Prüfsumme {}",
            result.total, result.check
        ));
    }
    out
}

/// With `lv` (GU-Fall): the Datenpunkt-Abschnitt of that Kosten-LV (the
/// active LV after `server.open`) – its index, or None without `lv`.
/// `abschnitt` = OZ of its title when the LV has several.
fn section_of(project: &Project, args: &Args) -> Result<Option<usize>, String> {
    if !args.has("lv") {
        return Ok(None);
    }
    let lv = project.lv();
    if lv.purpose().is_tender() {
        return Err(format!(
            "LV {} ist ein Ausschreibungs-LV – Datenpunkt-Abschnitte gibt es nur im Kosten-LV.",
            crate::lv_tools::label(lv)
        ));
    }
    let sections = lv.point_sections();
    let wanted = args
        .opt_text("abschnitt")?
        .map(str::trim)
        .filter(|text| !text.is_empty());
    let found = match wanted {
        Some(oz) => sections
            .iter()
            .copied()
            .find(|&index| lv.rows()[index].ordinal().trim() == oz)
            .ok_or_else(|| format!("Einen Datenpunkt-Abschnitt mit OZ „{oz}“ gibt es nicht."))?,
        None => match sections[..] {
            [] => return Ok(None),
            [index] => index,
            _ => {
                let names: Vec<String> = sections
                    .iter()
                    .map(|&index| section_name(&lv.rows()[index]))
                    .collect();
                return Err(format!(
                    "Das Kosten-LV hat mehrere Datenpunkt-Abschnitte ({}) – abschnitt = OZ angeben.",
                    names.join(", ")
                ));
            }
        },
    };
    Ok(Some(found))
}

/// [`section_of`], but an LV without one is an error.
fn existing_section(project: &Project, args: &Args) -> Result<Option<usize>, String> {
    match section_of(project, args)? {
        Some(index) => Ok(Some(index)),
        None if args.has("lv") => Err(format!(
            "Das Kosten-LV {} hat keinen Datenpunkt-Abschnitt – kosten_zeilen_schreiben mit lv legt ihn an.",
            crate::lv_tools::label(project.lv())
        )),
        None => Ok(None),
    }
}

/// The estimate of the section at `index` of the active LV.
fn section_mut(project: &mut Project, index: usize) -> &mut CostEstimate {
    match project.lv_mut().row_mut(index) {
        Some(LvRow::Group(group)) => group.points_mut().expect("Datenpunkt-Abschnitt"),
        _ => unreachable!("Datenpunkt-Abschnitt ist ein Titel"),
    }
}

pub(crate) fn read_costs(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    if let Some(index) = existing_section(project, args)? {
        return Ok(describe_section(project.lv(), index, args.flag("zeilen")?));
    }
    let cost_lvs = crate::cost_lv_tools::overview(project);
    if project.cost_estimates().is_empty() {
        return Ok(cost_lvs.unwrap_or_else(|| {
            "Im Projekt gibt es noch keine Kostenermittlung (Kosten-LV: lv_anlegen mit art \
             kostenschaetzung/kostenberechnung; Datenpunkte GA: kosten_anlegen)."
                .to_string()
        }));
    }
    let one = args.has("kostenermittlung");
    if !one {
        let mut out = cost_lvs
            .map(|text| format!("{text}\n\n"))
            .unwrap_or_default();
        out += &format!(
            "{} nach Datenpunkten (GA) im Projekt:\n\n| Nr. | Stufe | Bezeichnung | Stand | Bauteile | DP | Summe netto |\n|---|---|---|---|---|---|---|",
            proava_domain::wording::count(project.cost_estimates().len(), "Kostenermittlung", "Kostenermittlungen")
        );
        for (index, estimate) in project.cost_estimates().iter().enumerate() {
            let result = estimate.result();
            out.push_str(&format!(
                "\n| {} | {} ({}. Ebene) | {} | {} | {} | {} | {} |",
                index + 1,
                estimate.stage.label(),
                estimate.stage.level(),
                cell(&estimate.title),
                cell(&estimate.date),
                estimate.sections.len(),
                result.points,
                result.total.plain()
            ));
        }
        let newest = project.cost_estimates().last().expect("nicht leer");
        out.push_str("\n\nNeueste im Einzelnen:\n");
        out.push_str(&describe(project, newest, args.flag("zeilen")?));
        return Ok(out);
    }
    let id = find(project, args)?;
    let estimate = project.cost_estimate(id).expect("gefunden");
    Ok(describe(project, estimate, args.flag("zeilen")?))
}

/// One entry of a list argument as arguments.
fn entry_of<'a>(value: &'a Value, what: &str) -> Result<Args<'a>, String> {
    match value {
        Value::Object(map) => Ok(Args(map)),
        _ => Err(format!("{what}: jeder Eintrag ist ein Objekt.")),
    }
}

/// Kennwert, Spanne: the limits of the input fields (`input_limits`).
fn decimal_arg(args: &Args, key: &str) -> Result<Option<Decimal>, String> {
    args.number_of(key, NumberKind::CostPrice)
}

pub(crate) fn create_cost_estimate(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let from = args
        .opt_text("aus")?
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let date = args
        .opt_text("stand")?
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .unwrap_or_else(today);
    let author = args
        .opt_text("bearbeiter")?
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut estimate = match &from {
        Some(source) => {
            let source = loaded
                .project
                .find_cost_estimate(source)
                .ok_or_else(|| format!("aus: eine Kostenermittlung „{source}“ gibt es nicht."))?;
            source.to_calculation(&date, &author)
        }
        None => {
            let stage = CostStage::from_key(args.text("stufe")?)
                .ok_or("stufe: kostenschaetzung (2. Ebene) oder kostenberechnung (3. Ebene).")?;
            CostEstimate::template(stage, &date, &author)
        }
    };
    if let Some(title) = args.opt_text("titel")? {
        estimate.title = title.trim().to_string();
    }
    if let Some(remark) = args.opt_text("bemerkung")? {
        estimate.remark = remark.trim().to_string();
    }
    if let Some(name) = args
        .opt_text("bauteil")?
        .filter(|text| !text.trim().is_empty())
    {
        if from.is_none() {
            estimate.sections[0].name = name.trim().to_string();
        }
    }
    apply_rate(&mut estimate, args)?;
    let id = loaded.project.add_cost_estimate(estimate);
    let answer = format!(
        "Angelegt:\n{}",
        describe(
            &loaded.project,
            loaded.project.cost_estimate(id).expect("eben angelegt"),
            false
        )
    );
    server.save_with_answer(&loaded, answer)
}

fn apply_rate(estimate: &mut CostEstimate, args: &Args) -> Result<(), String> {
    if let Some(rate) = decimal_arg(args, "kennwert")? {
        estimate.rate = rate;
    }
    for (key, field) in [
        ("kennwert_von", &mut estimate.rate_low),
        ("kennwert_bis", &mut estimate.rate_high),
    ] {
        match args.get(key) {
            None => {}
            Some(Value::Null) => *field = None,
            Some(Value::String(text)) if text.trim().is_empty() => *field = None,
            Some(value) => {
                let value = number(value)
                    .and_then(|value| NumberKind::CostPrice.check(value))
                    .map_err(|error| format!("„{key}“: {error}"))?;
                *field = Some(value);
            }
        }
    }
    if let Some(per_kind) = args.opt_flag("kennwert_je_dp_art")? {
        estimate.rate_per_kind = per_kind;
    }
    Ok(())
}

/// `{"481": 40, "482": 12, …}` → split; all codes checked.
fn shares_of(value: &Value) -> Result<Vec<CostShare>, String> {
    let Value::Object(map) = value else {
        return Err("aufteilung: Objekt wie {\"481\": 40, \"482\": 12, …}.".to_string());
    };
    let mut shares = Vec::new();
    for (code, percent) in map {
        let code =
            proava_domain::parse_cost_group(code)?.ok_or("aufteilung: leere Kostengruppe.")?;
        let percent = number(percent)
            .and_then(|percent| NumberKind::CostShare.check(percent))
            .map_err(|error| format!("aufteilung KG {code}: {error}"))?;
        shares.push(CostShare::new(&code, percent));
    }
    shares.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(shares)
}

fn section_index(estimate: &mut CostEstimate, name: &str, create: bool) -> Result<usize, String> {
    let name = name.trim();
    if let Some(index) = estimate
        .sections
        .iter()
        .position(|section| section.name.trim().eq_ignore_ascii_case(name))
    {
        return Ok(index);
    }
    if !create {
        return Err(format!("Ein Bauteil „{name}“ gibt es nicht."));
    }
    estimate.sections.push(CostSection::new(name));
    Ok(estimate.sections.len() - 1)
}

pub(crate) fn change_cost_estimate(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    if let Some(index) = existing_section(&loaded.project, args)? {
        return change_section(server, loaded, args, index);
    }
    let id = find(&loaded.project, args)?;
    if args.flag("loeschen")? {
        let removed = loaded.project.remove_cost_estimate(id).expect("gefunden");
        let answer = format!("Gelöscht: {}", removed.display_name());
        return server.save_with_answer(&loaded, answer);
    }
    let estimate = loaded.project.cost_estimate_mut(id).expect("gefunden");
    for (key, field) in [
        ("titel", &mut estimate.title),
        ("stand", &mut estimate.date),
        ("bearbeiter", &mut estimate.author),
        ("bemerkung", &mut estimate.remark),
    ] {
        if let Some(text) = args.opt_text(key)? {
            *field = text.trim().to_string();
        }
    }
    apply_changes(estimate, args)?;
    let answer = format!(
        "Geändert:\n{}",
        describe(
            &loaded.project,
            loaded.project.cost_estimate(id).expect("da"),
            false
        )
    );
    server.save_with_answer(&loaded, answer)
}

/// kosten_aendern with `lv`: the Datenpunkt-Abschnitt at `index`; loeschen
/// makes its title an ordinary one again.
fn change_section(
    server: &mut Server,
    mut loaded: crate::server::Loaded,
    args: &Args,
    index: usize,
) -> Result<String, String> {
    for key in ["titel", "stand", "bearbeiter"] {
        if args.has(key) {
            return Err(format!(
                "{key}: Bezeichnung, Stand und Bearbeiter sind die des Kosten-LV (projektangaben_setzen, zeile_aendern)."
            ));
        }
    }
    if args.flag("loeschen")? {
        let name = section_name(&loaded.project.lv().rows()[index]);
        if let Some(LvRow::Group(group)) = loaded.project.lv_mut().row_mut(index) {
            group.set_points(None);
        }
        let answer =
            format!("Datenpunkte von {name} gelöscht – der Titel bleibt (ohne Positionen).");
        return server.save_with_answer(&loaded, answer);
    }
    let estimate = section_mut(&mut loaded.project, index);
    if let Some(remark) = args.opt_text("bemerkung")? {
        estimate.remark = remark.trim().to_string();
    }
    apply_changes(estimate, args)?;
    let answer = format!(
        "Geändert:\n{}",
        describe_section(loaded.project.lv(), index, false)
    );
    server.save_with_answer(&loaded, answer)
}

/// Kennwert, DP-Arten, Aufteilung, Bauteile – the part of kosten_aendern a
/// Datenpunktrechnung and a Datenpunkt-Abschnitt share.
fn apply_changes(estimate: &mut CostEstimate, args: &Args) -> Result<(), String> {
    apply_rate(estimate, args)?;
    if let Some(Value::Array(kinds)) = args.get("dp_arten") {
        for entry in kinds {
            let entry = entry_of(entry, "dp_arten")?;
            let key = entry.text("kuerzel")?.trim().to_string();
            let index = match estimate.kind_index(&key) {
                Some(index) => index,
                None => estimate.add_kind(PointKind::new(&key, "", Decimal::ZERO)),
            };
            if entry.flag("entfernen")? {
                estimate.remove_kind(index);
                continue;
            }
            let kind = &mut estimate.kinds[index];
            if let Some(label) = entry.opt_text("bezeichnung")? {
                kind.label = label.trim().to_string();
            }
            if let Some(price) = entry.number_of("preis", NumberKind::CostPrice)? {
                kind.unit_price = price;
            }
        }
    }
    if let Some(value) = args.get("aufteilung") {
        let shares = shares_of(value)?;
        match args
            .opt_text("bauteil")?
            .filter(|text| !text.trim().is_empty())
        {
            Some(name) => {
                let index = section_index(estimate, name, false)?;
                estimate.sections[index].shares = Some(shares);
            }
            None => estimate.shares = shares,
        }
    }
    if let Some(name) = args
        .opt_text("vorgabe_aufteilung_fuer")?
        .filter(|text| !text.trim().is_empty())
    {
        let index = section_index(estimate, name, false)?;
        estimate.sections[index].shares = None;
    }
    if let Some(Value::Object(renames)) = args.get("bauteile_umbenennen") {
        for (old, new) in renames {
            let index = section_index(estimate, old, false)?;
            estimate.sections[index].name = new.as_str().unwrap_or_default().trim().to_string();
        }
    }
    Ok(())
}

pub(crate) fn write_cost_rows(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let mut protocol: Vec<String> = Vec::new();
    if args.has("lv") {
        // GU-Fall: the Datenpunkt-Abschnitt of the Kosten-LV, new when it has none.
        let index = match section_of(&loaded.project, args)? {
            Some(index) => index,
            None => {
                let index = loaded.project.lv_mut().insert_point_section()?;
                protocol.push(format!(
                    "Datenpunkt-Abschnitt {} angelegt",
                    section_name(&loaded.project.lv().rows()[index])
                ));
                index
            }
        };
        let estimate = section_mut(&mut loaded.project, index);
        write_rows(estimate, args, &mut protocol)?;
        let answer = format!(
            "{}\n\n{}",
            protocol.join("\n"),
            describe_section(loaded.project.lv(), index, false)
        );
        return server.save_with_answer(&loaded, answer);
    }
    let id = find(&loaded.project, args)?;
    let estimate = loaded.project.cost_estimate_mut(id).expect("gefunden");
    write_rows(estimate, args, &mut protocol)?;
    let answer = format!(
        "{}\n\n{}",
        protocol.join("\n"),
        describe(
            &loaded.project,
            loaded.project.cost_estimate(id).expect("da"),
            false
        )
    );
    server.save_with_answer(&loaded, answer)
}

/// The rows of kosten_zeilen_schreiben into `estimate` (all or nothing).
fn write_rows(
    estimate: &mut CostEstimate,
    args: &Args,
    protocol: &mut Vec<String>,
) -> Result<(), String> {
    let before = protocol.len();
    {
        if let Some(Value::Array(rows)) = args.get("zeilen") {
            let replace = args.flag("ersetzen")?;
            let mut cleared: Vec<usize> = Vec::new();
            let mut work = estimate.clone();
            for (number, entry) in rows.iter().enumerate() {
                let entry = entry_of(entry, "zeilen")?;
                let name = entry
                    .opt_text("bauteil")?
                    .map(str::trim)
                    .filter(|text| !text.is_empty());
                let section = match name {
                    Some(name) => section_index(&mut work, name, true)?,
                    None => {
                        if work.sections.is_empty() {
                            work.sections.push(CostSection::new("Gebäude"));
                        }
                        0
                    }
                };
                if replace && !cleared.contains(&section) {
                    work.sections[section].rows.clear();
                    cleared.push(section);
                }
                // Empty template lines go away when real ones come.
                work.sections[section].rows.retain(|row| !row.is_blank());
                let mut row = PointRow {
                    location: entry
                        .opt_text("ort")?
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    trade: entry
                        .opt_text("gewerk")?
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    plant: entry
                        .opt_text("anlage")?
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    remark: entry
                        .opt_text("bemerkung")?
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    counts: vec![0; work.kinds.len()],
                };
                if let Some(Value::Object(points)) = entry.get("dp") {
                    for (key, value) in points {
                        let count = value
                            .as_u64()
                            .ok_or_else(|| {
                                format!(
                                    "Zeile {}: dp.{key} muss eine ganze Zahl ≥ 0 sein.",
                                    number + 1
                                )
                            })
                            .and_then(|count| {
                                proava_domain::input_limits::point_count(count).map_err(|error| {
                                    format!("Zeile {}: dp.{key}: {error}", number + 1)
                                })
                            })?;
                        let kind = match work.kind_index(key) {
                            Some(kind) => kind,
                            None => {
                                protocol.push(format!(
                                    "Neue DP-Art {key} ohne Preis (kosten_aendern dp_arten)"
                                ));
                                work.add_kind(PointKind::new(key, "", Decimal::ZERO))
                            }
                        };
                        row.counts.resize(work.kinds.len(), 0);
                        row.set_count(kind, count);
                    }
                }
                let count = work.kinds.len();
                for section in &mut work.sections {
                    for line in &mut section.rows {
                        line.counts.resize(count, 0);
                    }
                }
                row.counts.resize(count, 0);
                work.sections[section].rows.push(row);
            }
            protocol.push(format!(
                "{} {}",
                proava_domain::wording::count(rows.len(), "Zeile", "Zeilen"),
                if replace {
                    "geschrieben (Bauteile ersetzt)"
                } else {
                    "angehängt"
                }
            ));
            *estimate = work;
        }
        if protocol.len() == before {
            return Err(
                "Nichts zu schreiben: zeilen angeben, z. B. [{bauteil, anlage, dp: {\"AI\": 12}}]."
                    .to_string(),
            );
        }
    }
    Ok(())
}
