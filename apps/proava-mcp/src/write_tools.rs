//! Tools that change a project. Each loads the file, changes it through the
//! domain and saves it (see [`Server::save`] for backup and change check).

use std::collections::HashSet;
use std::path::Path;

use proava_domain::{
    normalize_ordinal, BidId, Decimal, Discount, EditStatus, Lv, LvGroup, LvRow, Money, Note,
    NumberKind, OzMask, PerformanceDescription, PriceComponent, PriceSource, ProjectName, RichText,
    RowId, Spacer, TextBlock,
};

use crate::args::{change_position, content_index, find_row, heading_target, new_position, Args};
use crate::format::{
    amount_columns, money, ordinals_by_id, percent, position_total, renumbered_count, row_name,
    sums_after_change,
};
use crate::markup::parse_markup;
use crate::read_tools::{bid_summary, numbering_note};
use crate::server::Server;

const STEPS: [u32; 4] = [1, 10, 100, 1000];

fn step(args: &Args) -> Result<Option<u32>, String> {
    match args.opt_count("schrittweite")? {
        None => Ok(None),
        Some(value) if STEPS.contains(&(value as u32)) && value <= 1000 => Ok(Some(value as u32)),
        Some(value) => Err(format!(
            "Schrittweite {value} geht nicht – erlaubt sind 1, 10, 100 und 1000."
        )),
    }
}

fn mask(args: &Args) -> Result<Option<OzMask>, String> {
    args.opt_text("oz_maske")?
        .filter(|text| !text.trim().is_empty())
        .map(OzMask::parse)
        .transpose()
}

/// The lock message names the keys of the program; tools set the status.
/// The Langtext and the unit of an STLB-Bau position are write-protected:
/// Err with what the assistant can do instead.
pub(crate) fn text_editable(lv: &Lv, id: RowId) -> Result<(), String> {
    lv.ensure_text_editable(&[id])
        .map_err(|_| stlb_locked(lv, id))
}

/// Answer for a change of the Langtext or the unit of an STLB-Bau position.
pub(crate) fn stlb_locked(lv: &Lv, id: RowId) -> String {
    let name = lv
        .position_by_id(id)
        .map(|position| format!("Position {}", position.ordinal()))
        .unwrap_or_else(|| "Die Position".to_string());
    format!(
        "{name}: Langtext und Einheit stammen aus STLB-Bau und sind schreibgeschützt (Kurztext, Menge, EP, KG, LB, \
         Status, Art, Bezug und die Werte der Textergänzungen sind frei). Ändern: über STLB-Bau (stlb_pruefdatei, \
         stlb_aktualisieren) oder zeile_aendern mit in_freie_position: true – das entfernt die STLB-Bau-Kennung. \
         Nichts gespeichert."
    )
}

fn locked(error: String) -> String {
    error.replace(
        proava_domain::LOCKED_HINT,
        "zum Ändern erst mit zeile_aendern status „offen“ oder „in Bearbeitung“ setzen",
    )
}

/// `ausfuehrungsbeschreibung` of a position: None = not given, Some(None) =
/// clear the reference („“), Some(Some(id)) = the description with that number.
/// `wiederholung_von`: OZ of the position whose description this one repeats
/// (Wiederholungsbeschreibung, GAEB RefDescr „Rep“); „“ = none. None when
/// not given.
fn repetition_ref(lv: &Lv, args: &Args) -> Result<Option<Option<RowId>>, String> {
    let Some(oz) = args.opt_text("wiederholung_von")? else {
        return Ok(None);
    };
    let oz = oz.trim();
    if oz.is_empty() {
        return Ok(Some(None));
    }
    let index = crate::args::find_by_ordinal(lv, oz)?;
    match &lv.rows()[index] {
        LvRow::Position(position) => Ok(Some(Some(position.id()))),
        _ => Err(format!(
            "wiederholung_von: {oz} ist keine Position (Bezug nur auf Positionen)."
        )),
    }
}

fn performance_ref(lv: &Lv, args: &Args) -> Result<Option<Option<RowId>>, String> {
    let Some(number) = args.opt_text("ausfuehrungsbeschreibung")? else {
        return Ok(None);
    };
    let number = number.trim();
    if number.is_empty() {
        return Ok(Some(None));
    }
    // „A0002“, „AB 0002“, „0002“ and „2“ all mean 2.
    let wanted = proava_domain::performance_number_input(number);
    match lv.performance_by_number(wanted) {
        Some(performance) => Ok(Some(Some(performance.id()))),
        None => {
            let known: Vec<String> = lv
                .performances()
                .map(|performance| performance.mark())
                .collect();
            Err(format!(
                "Eine Ausführungsbeschreibung {} gibt es im LV nicht{}. Erst mit ausfuehrungsbeschreibung_einfuegen anlegen.",
                proava_domain::performance_mark(wanted),
                if known.is_empty() {
                    String::new()
                } else {
                    format!(" (vorhanden: {})", known.join(", "))
                }
            ))
        }
    }
}

/// A hint from `text` (markup) and `kurztext` (heading); one of them must be
/// given. `text: ""` without heading is an empty line (Leerzeile), as GAEB
/// files of other programs carry them between positions.
fn new_note(
    args: &Args,
    text: Option<&str>,
    heading: Option<&str>,
    known_image: impl Fn(&str) -> bool,
) -> Result<Note, String> {
    let heading = heading.unwrap_or_default().trim();
    if heading.is_empty() && text.is_none() {
        return Err(
            "Der Hinweis braucht text (auch „“ für eine Leerzeile) oder kurztext.".to_string(),
        );
    }
    let text = text.unwrap_or_default();
    let (mut rich, complements) = crate::args::text_with_complements(args, text, known_image)?;
    if rich.is_empty() {
        // As a GAEB import holds it: one empty paragraph (<p/> in the file).
        rich = RichText::new(vec![TextBlock::text("")]);
    }
    let mut note = Note::rich(rich);
    note.set_heading(heading);
    note.set_complements(complements);
    Ok(note)
}

/// GAEB type of an EP share from its usual German label.
fn component_kind(label: &str) -> &'static str {
    match label.trim().to_lowercase().as_str() {
        "lohn" | "löhne" | "lohnanteil" => "Wages",
        "material" | "stoffe" | "stoff" | "materialanteil" => "Materials",
        "gerät" | "geräte" | "geraet" | "geraete" => "Plant",
        "sonstiges" | "sonstige" => "Miscellaneous",
        _ => "",
    }
}

/// `ebenen`, `ep_anteile` and `zeiteinheit` of projektangaben_setzen; the
/// problems of all three.
fn set_lv_labels(lv: &mut Lv, args: &Args, changed: &mut Vec<&'static str>) -> Vec<String> {
    let mut problems = Vec::new();
    for result in [
        level_labels(lv, args, changed),
        price_component_labels(lv, args, changed),
        time_label(lv, args, changed),
    ] {
        if let Err(problem) = result {
            problems.push(problem);
        }
    }
    problems
}

/// A list of texts (None when not given or null).
fn text_list(args: &Args, key: &str) -> Result<Option<Vec<String>>, String> {
    match args.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(|text| text.trim().to_string())
                    .ok_or_else(|| format!("{key}: bitte eine Liste von Texten angeben."))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(format!("{key}: bitte eine Liste von Texten angeben.")),
    }
}

fn level_labels(lv: &mut Lv, args: &Args, changed: &mut Vec<&'static str>) -> Result<(), String> {
    if let Some(labels) = text_list(args, "ebenen")? {
        if labels.len() > lv.level_count() {
            return Err(format!(
                "ebenen: die OZ-Maske {} hat nur {} Titelebene(n).",
                lv.oz_mask().pattern(),
                lv.level_count()
            ));
        }
        if let Some(long) = labels.iter().find(|label| label.chars().count() > 40) {
            return Err(format!(
                "ebenen: „{long}“ ist länger als 40 Zeichen (GAEB-Grenze)."
            ));
        }
        lv.set_level_labels(labels);
        changed.push("Ebenen");
    }
    Ok(())
}

fn price_component_labels(
    lv: &mut Lv,
    args: &Args,
    changed: &mut Vec<&'static str>,
) -> Result<(), String> {
    if let Some(labels) = text_list(args, "ep_anteile")? {
        if labels.len() > 6 {
            return Err("ep_anteile: GAEB kennt höchstens 6 EP-Anteile.".to_string());
        }
        if labels.iter().any(String::is_empty) {
            return Err("ep_anteile: leere Bezeichnung.".to_string());
        }
        let used = lv
            .positions()
            .any(|position| position.price_components().iter().any(Option::is_some));
        if used && labels.len() != lv.price_components().len() {
            return Err(format!(
                "ep_anteile: Positionen haben schon Werte für {} EP-Anteile – die Anzahl bleibt, nur die Bezeichnungen lassen sich ändern.",
                lv.price_components().len()
            ));
        }
        lv.set_price_components(
            labels
                .into_iter()
                .map(|label| PriceComponent {
                    kind: component_kind(&label).to_string(),
                    label,
                })
                .collect(),
        );
        changed.push("EP-Anteile");
    }
    Ok(())
}

fn time_label(lv: &mut Lv, args: &Args, changed: &mut Vec<&'static str>) -> Result<(), String> {
    if let Some(label) = args.opt_text("zeiteinheit")? {
        let label = label.trim();
        if label.chars().count() > 20 {
            return Err(format!(
                "zeiteinheit: „{label}“ ist länger als 20 Zeichen (GAEB-Grenze)."
            ));
        }
        lv.set_time_label(label);
        changed.push("Zeiteinheit");
    }
    Ok(())
}

/// Refusal when a title's cost group would change finished positions.
fn cost_group_heirs_message(lv: &Lv, heirs: &[RowId], code: Option<&str>) -> String {
    heirs_message(
        lv,
        heirs,
        proava_domain::cost_group_heirs_question(heirs.len(), code),
    )
}

/// Refusal when a title's Leistungsbereich would change finished positions.
fn work_category_heirs_message(lv: &Lv, heirs: &[RowId], code: Option<&str>) -> String {
    heirs_message(
        lv,
        heirs,
        proava_domain::work_category_heirs_question(heirs.len(), code),
    )
}

fn heirs_message(lv: &Lv, heirs: &[RowId], question: String) -> String {
    let ordinals: Vec<&str> = heirs
        .iter()
        .filter_map(|id| lv.row_by_id(*id))
        .map(|row| row.ordinal())
        .collect();
    let shown = ordinals
        .iter()
        .take(3)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let more = ordinals.len().saturating_sub(3);
    let list = if more > 0 {
        format!("{shown} und {more} weitere")
    } else {
        shown
    };
    format!(
        "Nicht geändert: {question} Betroffen: {list}. Ihr Status bleibt „fertig“. Zum Ändern mit fertige_erben: true bestätigen."
    )
}

/// Inserts `row` at `index`: with `oz` under exactly this order number
/// (checked against the mask, taken numbers and the title above; written
/// with the widths of the mask), else numbered as usual. Other rows keep
/// their numbers.
fn insert_at(lv: &mut Lv, index: usize, mut row: LvRow, oz: Option<&str>) -> Result<RowId, String> {
    let Some(oz) = oz.map(str::trim).filter(|oz| !oz.is_empty()) else {
        return lv.insert_numbered(index, row);
    };
    let index = index.min(lv.rows().len());
    let ordinal = checked_ordinal(lv, index, &row, oz)?;
    match &mut row {
        LvRow::Group(group) => group.set_ordinal(ordinal),
        LvRow::Position(position) => position.set_ordinal(ordinal),
        LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => {}
    }
    Ok(lv.insert_row(index, row))
}

/// The order number `oz` for `row` at `index`, as the mask writes it.
fn checked_ordinal(lv: &Lv, index: usize, row: &LvRow, oz: &str) -> Result<String, String> {
    let mask = lv.oz_mask();
    let wrong = |why: String| {
        format!(
            "OZ „{oz}“ passt nicht zur OZ-Maske {}: {why}",
            mask.pattern()
        )
    };
    let too_big = |levels: &[u32]| {
        levels
            .iter()
            .enumerate()
            .find(|(level, value)| **value > mask.level_max(*level))
            .map(|(level, value)| {
                format!(
                    "Ebene {} geht bis {}, nicht {value}.",
                    level + 1,
                    mask.level_max(level)
                )
            })
    };
    let parent_of = |level: usize| {
        lv.rows()[..index]
            .iter()
            .rev()
            .filter_map(LvRow::as_group)
            .find(|group| group.level() < level)
    };
    let ordinal = match row {
        LvRow::Group(group) => {
            let level = group.level();
            let example = mask.format_group(&vec![1; level + 1]);
            let levels = mask.read_group(oz).ok_or_else(|| {
                wrong(format!(
                    "ein Titel der Ebene {} schreibt sich wie „{example}“.",
                    level + 1
                ))
            })?;
            if levels.len() != level + 1 {
                return Err(wrong(format!(
                    "das ist eine OZ der Ebene {}, der Titel steht auf Ebene {} (z. B. „{example}“).",
                    levels.len(),
                    level + 1
                )));
            }
            if let Some(why) = too_big(&levels) {
                return Err(wrong(why));
            }
            if let Some(parent) = parent_of(level).filter(|parent| parent.level() + 1 == level) {
                if let Some(parent_levels) = mask.read_group(parent.ordinal()) {
                    if levels[..level] != parent_levels[..] {
                        return Err(format!(
                            "OZ „{oz}“ passt nicht unter Titel {}: ein Untertitel dort beginnt mit {}.",
                            parent.ordinal(),
                            parent.ordinal()
                        ));
                    }
                }
            }
            mask.format_group(&levels)
        }
        LvRow::Position(_) => {
            let parent = parent_of(usize::MAX);
            // A position right under a title above the deepest level (as
            // imported LVs hold them: 01.0001 in an 11.22.PPPP mask).
            if mask.read_position(oz).is_none() {
                if let Some(ordinal) = parent.and_then(|parent| position_below(mask, parent, oz)) {
                    return free_ordinal(lv, ordinal);
                }
                // Positionsnummer with letters („3A“), where the LV allows them
                // (GAEB breakdown of the positions alphanumeric, as imported).
                if let Some((ordinal, levels)) = mask.read_lettered_position(oz) {
                    if !lv.has_alphanumeric_positions() {
                        return Err(wrong(
                            "Positionsnummern mit Buchstaben erlaubt das LV nicht (projektangaben_setzen positionen_mit_buchstaben: true schaltet sie ein)."
                                .to_string(),
                        ));
                    }
                    if let Some(parent) = parent {
                        if let Some(parent_levels) = mask.read_group(parent.ordinal()) {
                            // Under its own title: same levels (or deeper,
                            // when the position sits on the deepest level).
                            let fits = levels == parent_levels
                                || (levels.len() == mask.level_count()
                                    && levels.starts_with(&parent_levels));
                            if !fits {
                                return Err(format!(
                                    "OZ „{oz}“ passt nicht unter Titel {}: Positionen dort beginnen mit {}.",
                                    parent.ordinal(),
                                    parent.ordinal()
                                ));
                            }
                        }
                    }
                    return free_ordinal(lv, ordinal);
                }
            }
            let example = mask.format_position(&vec![1; mask.level_count()], 1, "");
            let (levels, number) = mask
                .read_position(oz)
                .ok_or_else(|| wrong(format!("eine Position schreibt sich wie „{example}“.")))?;
            if let Some(why) = too_big(&levels) {
                return Err(wrong(why));
            }
            if number > mask.position_max() {
                return Err(wrong(format!(
                    "Positionen gehen bis {}.",
                    mask.position_max()
                )));
            }
            let base = mask.format_position(&levels, number, "");
            let pieces: Vec<&str> = oz.split('.').map(str::trim).collect();
            let index_part = pieces
                .get(base.split('.').count()..)
                .map(|rest| rest.concat().to_uppercase())
                .unwrap_or_default();
            if index_part.chars().count() > mask.index_width() {
                return Err(wrong(format!(
                    "der Index hat höchstens {} Stelle(n).",
                    mask.index_width()
                )));
            }
            if let Some(parent) = parent_of(usize::MAX) {
                if let Some(parent_levels) = mask.read_group(parent.ordinal()) {
                    if !levels.starts_with(&parent_levels) {
                        return Err(format!(
                            "OZ „{oz}“ passt nicht unter Titel {}: Positionen dort beginnen mit {}.",
                            parent.ordinal(),
                            parent.ordinal()
                        ));
                    }
                }
            }
            mask.format_position(&levels, number, &index_part)
        }
        LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => {
            return Err(
                "Hinweise, Ausführungsbeschreibungen und Abstandshalter haben keine OZ."
                    .to_string(),
            )
        }
    };
    free_ordinal(lv, ordinal)
}

/// The ordinal if no other row has it yet.
fn free_ordinal(lv: &Lv, ordinal: String) -> Result<String, String> {
    let key = normalize_ordinal(&ordinal);
    if let Some(taken) = lv
        .rows()
        .iter()
        .find(|other| other.has_ordinal() && normalize_ordinal(other.ordinal()) == key)
    {
        return Err(format!(
            "Die OZ {ordinal} ist schon vergeben: {}.",
            row_name(taken)
        ));
    }
    Ok(ordinal)
}

/// OZ of a position directly under `parent`, a title above the deepest
/// level of the mask: the title's OZ, the position number (padded to the
/// mask) and an optional index – "01.1" → "01.0001" in 11.22.PPPP.I, as the
/// GAEB import keeps such positions. None if `oz` is not of that form.
fn position_below(mask: &OzMask, parent: &LvGroup, oz: &str) -> Option<String> {
    let parent_levels = mask.read_group(parent.ordinal())?;
    let depth = parent_levels.len();
    if depth >= mask.level_count() {
        return None;
    }
    let pieces: Vec<&str> = oz.split('.').map(str::trim).collect();
    if pieces.len() <= depth || mask.read_group(&pieces[..depth].join("."))? != parent_levels {
        return None;
    }
    let (number, index) = match &pieces[depth..] {
        [number] => (*number, ""),
        [number, index] if mask.index_width() > 0 => (*number, *index),
        _ => return None,
    };
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value: u32 = number.parse().ok()?;
    if value == 0
        || value > mask.position_max()
        || index.chars().count() > mask.index_width()
        || !index
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return None;
    }
    let mut ordinal = format!(
        "{}.{:0width$}",
        parent.ordinal(),
        value,
        width = mask.position_width().max(1)
    );
    if !index.is_empty() {
        ordinal.push('.');
        ordinal.push_str(&index.to_uppercase());
    }
    Some(ordinal)
}

fn renumber_note(count: usize) -> String {
    match count {
        0 => String::new(),
        1 => "\n1 weitere Zeile hat eine neue OZ.".to_string(),
        count => format!("\n{count} weitere Zeilen haben neue OZ."),
    }
}

fn position_line(lv: &Lv, id: RowId) -> String {
    let Some(row) = lv.row_by_id(id) else {
        return String::new();
    };
    match row.as_position() {
        Some(position) => format!(
            "{} · {} × {} = {}{}",
            row_name(row),
            amount_columns(lv, position).0,
            amount_columns(lv, position).1,
            position_total(lv, position),
            match position.status() {
                EditStatus::Open => String::new(),
                status => format!(" · {}", status.label()),
            }
        ),
        None => row_name(row),
    }
}

pub(crate) fn create_project(server: &mut Server, args: &Args) -> Result<String, String> {
    let folder = server.allowed(Path::new(args.text("ordner")?.trim()))?;
    if !folder.is_dir() {
        return Err(format!("Ordner nicht gefunden: {}", folder.display()));
    }
    let name = ProjectName::parse(args.text("name")?).map_err(|error| error.to_string())?;
    let mask = mask(args)?;
    let step = step(args)?;
    let title = args
        .opt_text("lv_titel")?
        .filter(|text| !text.trim().is_empty());

    if let Some(wanted) = args
        .opt_text("vorlage")?
        .map(str::trim)
        .filter(|text| !text.is_empty() && !text.eq_ignore_ascii_case("leer"))
    {
        let keep_positions = args.opt_flag("positionen")?.unwrap_or(true);
        return crate::template_tools::create_from_template(
            server,
            &folder,
            name,
            wanted,
            keep_positions,
            (mask, step, title),
        );
    }

    // Ohne Angabe die Vorgaben aus den proAVA-Einstellungen (Neue Projekte),
    // wie „Neues Projekt“ mit „leer“ in proAVA.
    let preferences = server.template_defaults().preferences.clone();
    let mask = mask.unwrap_or_else(|| {
        OzMask::parse(&preferences.new_lv_mask)
            .unwrap_or_else(|_| OzMask::parse(OzMask::NEW_LV_PATTERN).expect("gültig"))
    });
    let path = server.repository().create_project(&folder, &name)?;
    let mut loaded = server.open_path(&path)?;
    let lv = loaded.project.lv_mut();
    lv.set_oz_mask(mask);
    lv.set_position_step(step.unwrap_or(preferences.new_lv_step));
    lv.set_vat_percent(preferences.new_lv_vat);
    if let Some(title) = title {
        lv.set_title(title.trim());
    }
    server.save_new(&loaded.path, &loaded.project)?;
    let lv = loaded.project.lv();
    Ok(format!(
        "Projekt angelegt: {}\nLV „{}“ · OZ-Maske {} · Schrittweite {} (erste Position z. B. {}).",
        loaded.path.display(),
        lv.title(),
        lv.oz_mask().pattern(),
        lv.position_step(),
        lv.oz_mask().example_with_step(lv.position_step())
    ))
}

use crate::lv_tools::merge_address;

pub(crate) fn set_project_info(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let project = &mut loaded.project;
    let mut changed = Vec::new();
    // All or nothing, but every implausible value at once: each part runs,
    // its problem is collected; nothing is saved while there is one.
    let mut problems: Vec<String> = Vec::new();
    let mut part = |result: Result<(), String>| {
        if let Err(problem) = result {
            problems.push(problem);
        }
    };

    part((|| {
        let texts: [(&str, &str); 5] = [
            ("projektbezeichnung", "Projektbezeichnung"),
            ("vergabenummer", "Vergabenummer"),
            ("datenaustauschnummer", "Datenaustauschnummer"),
            ("abgabetermin", "Abgabetermin"),
            ("abgabeort", "Abgabeort"),
        ];
        for (key, label) in texts {
            if let Some(value) = args.opt_text(key)? {
                let info = project.info_mut();
                let field = match key {
                    "projektbezeichnung" => &mut info.label,
                    "vergabenummer" => &mut info.award_number,
                    "datenaustauschnummer" => &mut info.exchange_number,
                    "abgabetermin" => &mut info.submission_deadline,
                    _ => &mut info.submission_place,
                };
                *field = value.trim().to_string();
                changed.push(label);
            }
        }
        Ok(())
    })());
    part((|| {
        if let Some(owner) = args.object("bauherr")? {
            merge_address(&mut project.info_mut().owner, &owner)?;
            changed.push("Bauherr");
        }
        Ok(())
    })());
    part((|| {
        if let Some(client) = args.object("auftraggeber")? {
            // An own Auftraggeber, starting from the one in force (the Bauherr
            // while it is „wie Bauherr“); GAEB OWN takes it from now on.
            let info = project.info_mut();
            let mut own = info.client().clone();
            merge_address(&mut own, &client)?;
            info.own_client = Some(own);
            changed.push("Auftraggeber");
        }
        if args.opt_flag("auftraggeber_wie_bauherr")? == Some(true) {
            project.info_mut().own_client = None;
            changed.push("Auftraggeber wie Bauherr");
        }
        Ok(())
    })());
    part((|| {
        if let Some(planner) = args.object("aufsteller")? {
            merge_address(&mut project.info_mut().planner, &planner)?;
            changed.push("Aufsteller");
        }
        Ok(())
    })());
    part(crate::lv_tools::set_master_data(
        project,
        args,
        &mut changed,
    ));
    part((|| {
        if args.opt_flag("kopf_fusszeile_standard")? == Some(true) {
            project.info_mut().page_frame = None;
            changed.push("Kopf-/Fußzeile (wieder aus den Einstellungen)");
        }
        let mut frame = project
            .info()
            .page_frame
            .clone()
            .unwrap_or_else(|| server.print.page_frame.clone());
        if crate::frame_args::apply(&mut frame, args)? {
            project.info_mut().page_frame = Some(frame);
            changed.push("Kopf-/Fußzeile");
        }
        Ok(())
    })());
    // Plausibility of the master data as a whole (dates, order, percentages).
    problems.extend(project.info().problems());
    let warnings = project.info().warnings();
    let mut part = |result: Result<(), String>| {
        if let Err(problem) = result {
            problems.push(problem);
        }
    };

    part((|| {
        if let Some(number) = args.opt_text("lv_nummer")? {
            let number = number.trim();
            let index = project.active_lv();
            if project.lvs().enumerate().any(|(other, lv)| {
                other != index && !number.is_empty() && lv.number().trim() == number
            }) {
                return Err(format!(
                    "Die LV-Nummer „{number}“ gibt es im Projekt schon."
                ));
            }
            project.lv_mut().set_number(number);
            changed.push("LV-Nummer");
        }
        Ok(())
    })());
    let lv = project.lv_mut();
    part((|| {
        if let Some(title) = args.opt_text("lv_titel")? {
            lv.set_title(title.trim());
            changed.push("LV-Titel");
        }
        Ok(())
    })());
    part((|| {
        if let Some(vat) = args.opt_number("mwst")? {
            if vat < Decimal::ZERO || vat > Decimal::ONE_HUNDRED {
                return Err("Die Umsatzsteuer muss zwischen 0 und 100 % liegen.".to_string());
            }
            lv.set_vat_percent(vat);
            changed.push("MwSt");
        }
        if let Some(currency) = args
            .opt_text("waehrung")?
            .filter(|text| !text.trim().is_empty())
        {
            lv.set_currency(currency.trim());
            changed.push("Währung");
        }
        Ok(())
    })());
    part((|| {
        // Pauschalsumme of the LV (GAEB TotalLSUM); null or „“ = none.
        if let Some(value) = args.price_of("pauschalsumme", NumberKind::UnitPrice)? {
            if value.is_some_and(|value| value.is_sign_negative()) {
                return Err("pauschalsumme: der Betrag darf nicht negativ sein.".to_string());
            }
            lv.set_lump_sum_total(value.map(Money::from_decimal));
            changed.push("Pauschalsumme");
        }
        Ok(())
    })());
    for problem in set_lv_labels(lv, args, &mut changed) {
        part(Err(problem));
    }
    part((|| {
        if let Some(flag) = args.opt_flag("positionen_mit_buchstaben")? {
            if !flag
                && lv.positions().any(|position| {
                    lv.oz_mask()
                        .read_lettered_position(position.ordinal())
                        .is_some()
                })
            {
                return Err(
                    "positionen_mit_buchstaben: das LV hat schon Positionsnummern mit Buchstaben."
                        .to_string(),
                );
            }
            lv.set_alphanumeric_positions(flag);
            changed.push("Positionsnummern mit Buchstaben");
        }
        Ok(())
    })());
    let mut renumbered = 0;
    part((|| {
        let mask = mask(args);
        let step = step(args);
        let (mask, step) = match (mask, step) {
            (Ok(mask), Ok(step)) => (mask, step),
            (Err(problem), Ok(_)) | (Ok(_), Err(problem)) => return Err(problem),
            (Err(mask), Err(step)) => return Err(format!("{mask} {step}")),
        };
        if mask.is_some() || step.is_some() {
            if !lv.is_auto_numbered() {
                return Err(
                    "Das LV hat eigene OZ – OZ-Maske und Schrittweite nur nach neu_nummerieren ändern."
                        .to_string(),
                );
            }
            let before = ordinals_by_id(lv);
            if let Some(mask) = mask {
                if mask.level_count()
                    < lv.rows()
                        .iter()
                        .filter_map(LvRow::as_group)
                        .map(|group| group.level() + 1)
                        .max()
                        .unwrap_or(0)
                {
                    return Err(format!(
                        "Die Maske {} hat weniger Titelebenen, als das LV benutzt.",
                        mask.pattern()
                    ));
                }
                lv.set_oz_mask(mask);
                changed.push("OZ-Maske");
            }
            if let Some(step) = step {
                lv.set_position_step(step);
                changed.push("Schrittweite");
            }
            lv.renumber();
            renumbered = renumbered_count(&before, lv, &[]);
        }
        Ok(())
    })());
    if !problems.is_empty() {
        let count = problems.len();
        return Err(format!(
            "Nichts gespeichert – {}: {}",
            if count == 1 {
                "eine Angabe passt nicht".to_string()
            } else {
                format!("{count} Angaben passen nicht")
            },
            problems
                .iter()
                .enumerate()
                .map(|(number, problem)| format!("({}) {problem}", number + 1))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    if changed.is_empty() {
        return Err("Keine Angabe zum Ändern.".to_string());
    }
    let mut answer = format!(
        "Geändert: {}.{}",
        changed.join(", "),
        if renumbered > 0 {
            format!(" {renumbered} OZ neu vergeben.")
        } else {
            String::new()
        }
    );
    for warning in warnings {
        answer.push_str(&format!("\nHinweis: {warning}"));
    }
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn insert_heading(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let text = args.text("kurztext")?.trim().to_string();
    let lv = loaded.project.lv_mut();
    let (index, level) = heading_target(lv, args)?;
    let before = ordinals_by_id(lv);
    let id = insert_at(
        lv,
        index,
        LvRow::Group(LvGroup::new(level, "", text)),
        args.opt_text("oz")?,
    )?;
    let new_index = lv.index_of(id).unwrap_or(index);
    let captured = lv.block_range(new_index).len() - 1;
    let mut answer = format!(
        "Titel eingefügt: {} auf Ebene {}.",
        row_name(&lv.rows()[new_index]),
        level + 1
    );
    if captured > 0 {
        answer.push_str(&format!(
            " Die {captured} folgenden Zeilen gehören jetzt zu diesem Titel."
        ));
    }
    answer.push_str(&renumber_note(renumbered_count(&before, lv, &[id])));
    answer.push('\n');
    answer.push_str(&sums_after_change(lv, Some(id)));
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn insert_position(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let position = new_position(args, &loaded.project)?;
    let lv = loaded.project.lv_mut();
    let performance = performance_ref(lv, args)?;
    let repetition = repetition_ref(lv, args)?;
    let index = content_index(lv, args)?;
    let before = ordinals_by_id(lv);
    let id = insert_at(lv, index, LvRow::Position(position), args.opt_text("oz")?)?;
    if let Some(performance) = performance {
        lv.link_performance(id, performance)?;
    }
    if let Some(base) = repetition {
        lv.link_repetition(id, base)?;
    }
    let answer = format!(
        "Position eingefügt: {}{}{}\n{}",
        position_line(lv, id),
        renumber_note(renumbered_count(&before, lv, &[id])),
        text_hints(lv, id),
        sums_after_change(lv, Some(id))
    );
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn insert_rows(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let items = match args.get("zeilen") {
        Some(serde_json::Value::Array(items)) if !items.is_empty() => items,
        _ => {
            return Err(
                "„zeilen“ fehlt oder ist leer (Liste von Positionen, Titeln, Hinweisen)."
                    .to_string(),
            )
        }
    };
    let images: HashSet<String> = loaded
        .project
        .images()
        .iter()
        .map(|image| image.file.clone())
        .collect();
    let mut cursor = rows_start(loaded.project.lv(), args, items)?;
    let before = ordinals_by_id(loaded.project.lv());
    let mut inserted = Vec::new();
    let mut previous_was_heading: Option<usize> = None;

    for (number, item) in items.iter().enumerate() {
        let fail = |error: String| {
            format!(
                "Zeile {} der Liste: {error} Nichts gespeichert.",
                number + 1
            )
        };
        let serde_json::Value::Object(map) = item else {
            return Err(fail("kein Objekt.".to_string()));
        };
        let item = Args(map);
        let row = if let Some(title) = item.opt_text("titel").map_err(fail)? {
            if title.trim().is_empty() {
                return Err(fail("leerer Titel.".to_string()));
            }
            let lv = loaded.project.lv();
            let levels = lv.level_count().max(1);
            let level = match item.opt_count("ebene").map_err(fail)? {
                Some(level) if level >= 1 && (level as usize) <= levels => level as usize - 1,
                Some(level) => {
                    return Err(fail(format!(
                        "Ebene {level} gibt es nicht (1 bis {levels})."
                    )))
                }
                None => match previous_was_heading {
                    Some(level) => (level + 1).min(levels - 1),
                    None => lv.level_for_new_group(cursor.checked_sub(1)),
                },
            };
            previous_was_heading = Some(level);
            LvRow::Group(LvGroup::new(level, "", title.trim()))
        } else if let Some(text) = item.opt_text("hinweis").map_err(fail)? {
            previous_was_heading = None;
            let mut note = new_note(
                &item,
                Some(text),
                item.opt_text("kurztext").map_err(fail)?,
                |file| images.contains(file),
            )
            .map_err(fail)?;
            set_note_kind(
                &mut note,
                item.flag("vorbemerkung").map_err(fail)?,
                item.flag("schlussbemerkung").map_err(fail)?,
            )
            .map_err(fail)?;
            LvRow::Note(note)
        } else {
            previous_was_heading = None;
            LvRow::Position(new_position(&item, &loaded.project).map_err(fail)?)
        };
        let (performance, repetition) = if row.is_position() {
            (
                performance_ref(loaded.project.lv(), &item).map_err(fail)?,
                repetition_ref(loaded.project.lv(), &item).map_err(fail)?,
            )
        } else {
            (None, None)
        };
        let lv = loaded.project.lv_mut();
        let oz = item.opt_text("oz").map_err(fail)?;
        let id = insert_at(lv, cursor, row, oz).map_err(fail)?;
        if let Some(performance) = performance {
            lv.link_performance(id, performance).map_err(fail)?;
        }
        if let Some(base) = repetition {
            lv.link_repetition(id, base).map_err(fail)?;
        }
        cursor = lv.index_of(id).unwrap_or(cursor) + 1;
        inserted.push(id);
    }

    let lv = loaded.project.lv();
    let mut lines = vec![format!("{} Zeilen eingefügt:", inserted.len())];
    lines.extend(
        inserted
            .iter()
            .map(|id| format!("- {}", position_line(lv, *id))),
    );
    let moved = renumbered_count(&before, lv, &inserted);
    let mut answer = lines.join("\n");
    answer.push_str(&renumber_note(moved));
    answer.push('\n');
    answer.push_str(&sums_after_change(lv, inserted.last().copied()));
    server.save_with_answer(&loaded, answer)
}

const BOTH_NOTE_KINDS: &str =
    "Ein Hinweis ist Vorbemerkung (vor dem LV) oder Schlussbemerkung (nach dem LV), nicht beides.";

/// Vorbemerkung or Schlussbemerkung of a new hint – never both.
fn set_note_kind(note: &mut Note, preliminary: bool, closing: bool) -> Result<(), String> {
    if preliminary && closing {
        return Err(BOTH_NOTE_KINDS.to_string());
    }
    note.set_preliminary(preliminary);
    note.set_closing(closing);
    Ok(())
}

pub(crate) fn insert_note(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let mut note = new_note(
        args,
        args.opt_text("text")?,
        args.opt_text("kurztext")?,
        |file| loaded.project.image(file).is_some(),
    )?;
    let preliminary = args.flag("vorbemerkung")?;
    let closing = args.flag("schlussbemerkung")?;
    set_note_kind(&mut note, preliminary, closing)?;
    let lv = loaded.project.lv_mut();
    let index = content_index(lv, args)?;
    let id = lv.insert_numbered(index, LvRow::Note(note))?;
    let new_index = lv.index_of(id).unwrap_or(index);
    let after = new_index
        .checked_sub(1)
        .map(|previous| format!(" hinter {}", row_name(&lv.rows()[previous])))
        .unwrap_or_else(|| " am Anfang des LV".to_string());
    let mut answer = format!(
        "{} eingefügt{after}: {}",
        if preliminary {
            "Vorbemerkung"
        } else if closing {
            "Schlussbemerkung"
        } else {
            "Hinweis"
        },
        row_name(&lv.rows()[new_index])
    );
    if lv.rows()[new_index].is_blank_note() {
        answer.push_str(
            " – ohne Text meldet die Prüfung ihn; für eine Leerzeile in der Bearbeitung abstand_einfuegen verwenden.",
        );
    }
    server.save_with_answer(&loaded, answer)
}

/// Abstandshalter: an empty line for editing only (no print, GAEB, CSV),
/// at the place like a hint.
pub(crate) fn insert_spacer(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let lv = loaded.project.lv_mut();
    let index = content_index(lv, args)?;
    let id = lv.insert_numbered(index, LvRow::Spacer(Spacer::new()))?;
    let new_index = lv.index_of(id).unwrap_or(index);
    let after = new_index
        .checked_sub(1)
        .map(|previous| format!(" hinter {}", row_name(&lv.rows()[previous])))
        .unwrap_or_else(|| " am Anfang des LV".to_string());
    let answer = format!(
        "Abstandshalter eingefügt{after}: {} – Leerzeile nur für die Bearbeitung, erscheint weder im Druck noch in GAEB oder CSV.",
        row_name(&lv.rows()[new_index])
    );
    server.save_with_answer(&loaded, answer)
}

/// Where positionen_einfuegen starts: like the other insert tools, but a
/// list that begins with a sub-title under `unter_titel` goes to the end of
/// the whole title (behind its existing sub-titles), as with titel_einfuegen.
fn rows_start(lv: &Lv, args: &Args, items: &[serde_json::Value]) -> Result<usize, String> {
    let starts_with_title = items
        .first()
        .and_then(serde_json::Value::as_object)
        .is_some_and(|first| first.get("titel").is_some_and(|title| !title.is_null()));
    let parent = args
        .opt_text("unter_titel")?
        .filter(|text| !text.trim().is_empty());
    if let (true, Some(parent)) = (starts_with_title, parent) {
        if !args.flag("am_anfang")?
            && args.get("nach_oz").is_none()
            && args.get("nach_id").is_none()
        {
            let index = crate::args::find_by_ordinal(lv, parent)?;
            if !lv.rows()[index].is_group() {
                return Err(format!("„{parent}“ ist kein Titel."));
            }
            return Ok(lv.block_range(index).end);
        }
    }
    content_index(lv, args)
}

pub(crate) fn insert_performance(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let (text, complements) = crate::args::text_with_complements(
        args,
        args.opt_text("langtext")?
            .or(args.opt_text("text")?)
            .unwrap_or_default(),
        |file| loaded.project.image(file).is_some(),
    )?;
    let label = args
        .opt_text("bezeichnung")?
        .unwrap_or_default()
        .trim()
        .to_string();
    let short = args
        .opt_text("kurztext")?
        .unwrap_or_default()
        .trim()
        .to_string();
    if label.is_empty() && short.is_empty() && text.is_empty() {
        return Err(
            "Die Ausführungsbeschreibung braucht eine Bezeichnung, einen Kurztext oder einen Text."
                .to_string(),
        );
    }
    let lv = loaded.project.lv_mut();
    let typed = args.opt_text("nummer")?.unwrap_or_default();
    let number = proava_domain::performance_number_input(typed).to_string();
    if !number.is_empty() {
        if lv.performance_by_number(&number).is_some() {
            return Err(format!(
                "Die Nummer {} hat schon eine andere Ausführungsbeschreibung.",
                proava_domain::performance_mark(&number)
            ));
        }
        if !number
            .parse::<u32>()
            .is_ok_and(|value| (1..=9999).contains(&value))
        {
            return Err(format!(
                "Nummer „{}“: GAEB erlaubt 1 bis 9999 (A0001 bis A9999).",
                typed.trim()
            ));
        }
    }
    // The number comes automatically (1, 2, 3 … by place while the LV's
    // descriptions are in order); `nummer` sets it by hand afterwards.
    let mut performance = PerformanceDescription::new("", label, text);
    performance.set_short_text(short);
    performance.set_complements(complements);
    let index = content_index(lv, args)?;
    let id = lv.insert_numbered(index, LvRow::Performance(performance))?;
    if !number.is_empty() {
        lv.set_performance_number(id, &number)?;
    }
    let number = lv
        .performance_by_id(id)
        .map(|performance| performance.mark())
        .unwrap_or_default();
    let answer = format!(
        "Ausführungsbeschreibung {number} eingefügt: {}. Positionen beziehen sich darauf mit ausfuehrungsbeschreibung: „{number}“.",
        row_name(lv.row_by_id(id).ok_or("Zeile nicht gefunden.")?)
    );
    server.save_with_answer(&loaded, answer)
}

/// PNG or JPEG by the first bytes (what proAVA prints and GAEB carries).
fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else {
        None
    }
}

const IMAGE_MAX_BYTES: usize = 20 * 1024 * 1024;

/// bieterangabe_einfuegen: a Textergänzung (vom Bieter or vom Auftraggeber)
/// as a field in the long text of a position or hint.
pub(crate) fn insert_bidder_field(server: &mut Server, args: &Args) -> Result<String, String> {
    use proava_domain::complement_text::{insert, FieldPlace};
    use proava_domain::{ComplementKind, TextComplement};
    let mut loaded = server.open(args)?;
    let caption = args.opt_text("vortext")?.unwrap_or_default().to_string();
    let kind = match args
        .opt_text("art")?
        .map(|art| art.trim().to_lowercase())
        .as_deref()
    {
        None | Some("") | Some("bieter") | Some("vom bieter") => ComplementKind::Bidder,
        Some("ag") | Some("auftraggeber") | Some("vom auftraggeber") => ComplementKind::Owner,
        Some(other) => return Err(format!("art „{other}“: „Bieter“ (Standard) oder „AG“.")),
    };
    let body = args.opt_text("inhalt")?.unwrap_or_default().to_string();
    let tail = args.opt_text("nachtext")?.unwrap_or_default().to_string();
    if caption.trim().is_empty() && body.trim().is_empty() {
        return Err("vortext fehlt, z. B. „Fabrikat:“ – sonst weiß der Bieter nicht, was er eintragen soll.".to_string());
    }
    let after = args
        .opt_text("nach")?
        .map(str::trim)
        .filter(|text| !text.is_empty());
    let block = args.opt_count("absatz")?;
    let place = match (after, block) {
        (Some(_), Some(_)) => {
            return Err("Entweder nach (Text) oder absatz – nicht beides.".to_string())
        }
        (Some(text), None) => FieldPlace::After(text.to_string()),
        (None, Some(count)) => FieldPlace::AfterBlock(count as usize),
        (None, None) => FieldPlace::End,
    };
    let lv = loaded.project.lv();
    let index = find_row(lv, args, "oz", "id")?;
    let row = &lv.rows()[index];
    let id = row.id();
    let (text, complements) = match row {
        LvRow::Position(position) => {
            lv.ensure_editable(&[id]).map_err(locked)?;
            text_editable(lv, id)?;
            (
                position.long_text().clone(),
                position.complements().to_vec(),
            )
        }
        LvRow::Note(note) => (note.text().clone(), note.complements().to_vec()),
        LvRow::Performance(performance) => (
            performance.long_text().clone(),
            performance.complements().to_vec(),
        ),
        _ => {
            return Err(
                "Bieterangaben stehen im Langtext von Positionen, Hinweisen und \
                 Ausführungsbeschreibungen."
                    .to_string(),
            );
        }
    };
    let marker = args
        .opt_text("kennung")?
        .unwrap_or_default()
        .trim()
        .to_string();
    if !marker.is_empty()
        && complements.iter().any(|known| {
            known.marker.trim().trim_start_matches('0') == marker.trim_start_matches('0')
        })
    {
        return Err(format!(
            "Die Kennung {marker} hat schon eine andere Textergänzung dieser Zeile."
        ));
    }
    let complement = TextComplement {
        marker,
        kind,
        caption,
        body,
        tail,
        anchor: None,
    }
    .with_spaces();
    let (text, complements) = insert(&text, &complements, complement.clone(), &place)?;
    let added = complements
        .iter()
        .find(|known| {
            known.caption == complement.caption
                && known.tail == complement.tail
                && known.kind == complement.kind
                && (complement.marker.is_empty() || known.marker == complement.marker)
        })
        .cloned()
        .unwrap_or(complement);
    let lv = loaded.project.lv_mut();
    match lv.row_mut(index) {
        Some(LvRow::Position(position)) => {
            position.set_long_text(text);
            position.set_complements(complements);
        }
        Some(LvRow::Note(note)) => {
            note.set_text(text);
            note.set_complements(complements);
        }
        Some(LvRow::Performance(performance)) => {
            performance.set_long_text(text);
            performance.set_complements(complements);
        }
        _ => {}
    }
    let lv = loaded.project.lv();
    let spot = match &place {
        FieldPlace::End => "unter dem Langtext".to_string(),
        FieldPlace::AfterBlock(count) => format!("auf eigener Zeile nach Absatz {count}"),
        FieldPlace::After(text) => format!("hinter „{text}“"),
    };
    let answer = format!(
        "{} {} in {} eingefügt ({spot}). Im Druck: {}",
        added.kind_name(),
        added.marker,
        row_name(&lv.rows()[index]),
        added.print_text().replace('\n', " ⏎ ")
    );
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn insert_image(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let path = server.allowed(Path::new(args.text("datei")?.trim()))?;
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("„{}“ lässt sich nicht lesen: {error}", path.display()))?;
    let kind = image_kind(&bytes).ok_or("Nur PNG- oder JPEG-Bilder lassen sich einfügen.")?;
    if bytes.len() > IMAGE_MAX_BYTES {
        return Err(format!(
            "Das Bild hat {} – höchstens {} (die GAEB-Datei wird sonst unhandlich).",
            proava_domain::size_text(bytes.len()),
            proava_domain::size_text(IMAGE_MAX_BYTES)
        ));
    }
    let width_mm = match args.opt_number("breite_mm")? {
        None => None,
        Some(width) => {
            let width: f32 = width.to_string().parse().unwrap_or(0.0);
            if !(5.0..=400.0).contains(&width) {
                return Err("breite_mm: 5 bis 400 mm.".to_string());
            }
            Some(width)
        }
    };
    let lv = loaded.project.lv();
    let index = find_row(lv, args, "oz", "id")?;
    let row = &lv.rows()[index];
    let id = row.id();
    if row.is_group() || row.is_spacer() {
        return Err("Titel und Abstandshalter haben keinen Langtext – Bilder gehen in Positionen, Hinweise und Ausführungsbeschreibungen.".to_string());
    }
    if row.is_position() {
        lv.ensure_editable(&[id]).map_err(locked)?;
        text_editable(lv, id)?;
    }
    let blocks = match row {
        LvRow::Position(position) => position.long_text().blocks().len(),
        LvRow::Note(note) => note.text().blocks().len(),
        LvRow::Performance(performance) => performance.long_text().blocks().len(),
        LvRow::Group(_) | LvRow::Spacer(_) => 0,
    };
    let at = match args.opt_count("absatz")? {
        None => blocks,
        Some(at) if (at as usize) <= blocks => at as usize,
        Some(at) => {
            return Err(format!(
                "absatz {at}: der Langtext hat nur {blocks} Absätze/Blöcke (0 = ganz vorne, ohne Angabe = ans Ende)."
            ))
        }
    };
    let wanted = match args
        .opt_text("name")?
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(name) => name.to_string(),
        None => path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("bild.{kind}")),
    };
    let wanted = if wanted.to_lowercase().ends_with(&format!(".{kind}"))
        || (kind == "jpg" && wanted.to_lowercase().ends_with(".jpeg"))
    {
        wanted
    } else {
        format!("{wanted}.{kind}")
    };
    let size = bytes.len();
    let file = loaded.project.add_image(&wanted, bytes);
    let lv = loaded.project.lv_mut();
    let index = lv.index_of(id).ok_or("Zeile nicht gefunden.")?;
    let image = TextBlock::Image {
        file: file.clone(),
        width_mm,
    };
    // Textergänzungen keep their place in the text (anchors move along).
    match lv.row_mut(index) {
        Some(LvRow::Position(position)) => position.insert_long_text_block(at, image),
        Some(LvRow::Note(note)) => note.insert_text_block(at, image),
        Some(LvRow::Performance(performance)) => {
            performance.long_text_mut().blocks_mut().insert(at, image)
        }
        _ => return Err("Zeile nicht gefunden.".to_string()),
    }
    let place = match at {
        0 => "ganz vorne".to_string(),
        at if at == blocks => "am Ende".to_string(),
        at => format!("nach Block {at}"),
    };
    let answer = format!(
        "Bild „{file}“ ({}) {place} eingefügt in {}; im Langtext steht es als [Bild: {file}{}]. GAEB DA XML gibt es mit (gaeb_exportieren bilder), GAEB 90 nicht.",
        proava_domain::size_text(size),
        row_name(&lv.rows()[index]),
        width_mm
            .map(|width| format!(", {} mm", format!("{width:.1}").trim_end_matches(".0").replace('.', ",")))
            .unwrap_or_default()
    );
    server.save_with_answer(&loaded, answer)
}

/// Biggest PDF taken for a clip.
const PDF_MAX_BYTES: usize = 100 * 1024 * 1024;

fn clip_number(args: &Args, object: &str, key: &str) -> Result<f32, String> {
    let value = args
        .opt_number(key)?
        .ok_or_else(|| format!("„{object}“: „{key}“ fehlt."))?;
    value
        .to_string()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("„{object}.{key}“ ist keine Zahl."))
}

/// „Ausschnitt aus PDF einfügen“: page and rectangle of a PDF into a long
/// text; printed as vector graphics, in GAEB as PNG (300 dpi).
pub(crate) fn insert_pdf_clip(server: &mut Server, args: &Args) -> Result<String, String> {
    use proava_domain::{ClipRect, PT_PER_MM};
    let mut loaded = server.open(args)?;
    let path = server.allowed(Path::new(args.text("datei")?.trim()))?;
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("„{}“ lässt sich nicht lesen: {error}", path.display()))?;
    if bytes.len() > PDF_MAX_BYTES {
        return Err(format!(
            "Die PDF hat {} – höchstens {}.",
            proava_domain::size_text(bytes.len()),
            proava_domain::size_text(PDF_MAX_BYTES)
        ));
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "ausschnitt.pdf".to_string());
    let source = proava_application::PdfSource::read(server.exchange(), &name, bytes)?;
    let page = match args.opt_count("seite")? {
        None => 1,
        Some(page) if page >= 1 && (page as usize) <= source.pages.len() => page as u32,
        Some(page) => {
            return Err(format!(
                "seite {page}: die PDF hat {} Seite(n).",
                source.pages.len()
            ))
        }
    };
    let (page_w, page_h) = source.page_size(page).ok_or("Seite nicht gefunden.")?;
    let rect = match (
        args.object("ausschnitt_mm")?,
        args.object("ausschnitt_anteile")?,
    ) {
        (Some(_), Some(_)) => {
            return Err("Bitte nur ausschnitt_mm oder ausschnitt_anteile angeben.".to_string())
        }
        (Some(mm), None) => ClipRect::new(
            clip_number(&mm, "ausschnitt_mm", "links")? * PT_PER_MM,
            clip_number(&mm, "ausschnitt_mm", "oben")? * PT_PER_MM,
            clip_number(&mm, "ausschnitt_mm", "breite")? * PT_PER_MM,
            clip_number(&mm, "ausschnitt_mm", "hoehe")? * PT_PER_MM,
        ),
        (None, Some(shares)) => {
            let share = |key: &str| -> Result<f32, String> {
                let value = clip_number(&shares, "ausschnitt_anteile", key)?;
                if (0.0..=1.0).contains(&value) {
                    Ok(value)
                } else {
                    Err(format!("„ausschnitt_anteile.{key}“: 0 bis 1."))
                }
            };
            ClipRect::from_shares(
                page_w,
                page_h,
                share("links")?,
                share("oben")?,
                share("rechts")?,
                share("unten")?,
            )
        }
        (None, None) => ClipRect::page(page_w, page_h),
    };
    let rect_inside = rect.clamped(page_w, page_h);
    if !rect_inside.is_valid() {
        return Err(format!(
            "Der Ausschnitt ist zu klein oder liegt neben der Seite ({}; Seite {:.0} × {:.0} mm).",
            rect_inside.size_text(),
            page_w / PT_PER_MM,
            page_h / PT_PER_MM
        ));
    }
    let width_mm = match args.opt_number("breite_mm")? {
        None => proava_application::pdf_clip::DEFAULT_CLIP_WIDTH_MM
            .min(rect_inside.width_mm())
            .max(10.0),
        Some(width) => {
            let width: f32 = width.to_string().parse().unwrap_or(0.0);
            if !(10.0..=proava_application::pdf_clip::MAX_CLIP_WIDTH_MM).contains(&width) {
                return Err("breite_mm: 10 bis 180 mm.".to_string());
            }
            width
        }
    };
    let lv = loaded.project.lv();
    let index = find_row(lv, args, "oz", "id")?;
    let row = &lv.rows()[index];
    let id = row.id();
    if row.is_group() || row.is_spacer() {
        return Err("Titel und Abstandshalter haben keinen Langtext – PDF-Ausschnitte gehen in Positionen, Hinweise und Ausführungsbeschreibungen.".to_string());
    }
    if row.is_position() {
        lv.ensure_editable(&[id]).map_err(locked)?;
        text_editable(lv, id)?;
    }
    let blocks = match row {
        LvRow::Position(position) => position.long_text().blocks().len(),
        LvRow::Note(note) => note.text().blocks().len(),
        LvRow::Performance(performance) => performance.long_text().blocks().len(),
        LvRow::Group(_) | LvRow::Spacer(_) => 0,
    };
    let at = match args.opt_count("absatz")? {
        None => blocks,
        Some(at) if (at as usize) <= blocks => at as usize,
        Some(at) => {
            return Err(format!(
                "absatz {at}: der Langtext hat nur {blocks} Absätze/Blöcke (0 = ganz vorne, ohne Angabe = ans Ende)."
            ))
        }
    };
    let prepared = proava_application::prepare_clip(server.exchange(), &source, page, rect_inside)?;
    let summary = prepared.summary_for(&loaded.project);
    let file = loaded
        .project
        .add_pdf_clip(&prepared.name, prepared.bytes, prepared.clip);
    let lv = loaded.project.lv_mut();
    let index = lv.index_of(id).ok_or("Zeile nicht gefunden.")?;
    let image = TextBlock::Image {
        file: file.clone(),
        width_mm: Some(width_mm.round()),
    };
    // Textergänzungen keep their place in the text (anchors move along).
    match lv.row_mut(index) {
        Some(LvRow::Position(position)) => position.insert_long_text_block(at, image),
        Some(LvRow::Note(note)) => note.insert_text_block(at, image),
        Some(LvRow::Performance(performance)) => {
            performance.long_text_mut().blocks_mut().insert(at, image)
        }
        _ => return Err("Zeile nicht gefunden.".to_string()),
    }
    let place = match at {
        0 => "ganz vorne".to_string(),
        at if at == blocks => "am Ende".to_string(),
        at => format!("nach Block {at}"),
    };
    let answer = format!(
        "PDF-Ausschnitt „{file}“ {place} eingefügt in {}, {:.0} mm breit. {summary} Im proAVA-PDF bleibt er Vektorgrafik; GAEB DA XML bekommt ihn als PNG mit 300 dpi (gaeb_exportieren bilder), GAEB 90 nicht.",
        row_name(&lv.rows()[index]),
        width_mm.round(),
    );
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn change_row(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let images: HashSet<String> = loaded
        .project
        .images()
        .iter()
        .map(|image| image.file.clone())
        .collect();
    let lv = loaded.project.lv_mut();
    let index = find_row(lv, args, "oz", "id")?;
    let id = lv.rows()[index].id();
    let auto = lv.is_auto_numbered();
    let before = ordinals_by_id(lv);
    let mut changed: Vec<&str> = Vec::new();

    // Bearbeitungsstatus first: „in Bearbeitung“ unlocks a finished
    // position for the other fields of the same call.
    let mut status_note = String::new();
    if let Some(text) = args.opt_text("status")? {
        let status = EditStatus::parse(text)?;
        let count = lv.set_status(&[id], status);
        let positions = lv.status_counts_in(lv.block_range(index)).total();
        if positions == 0 {
            return Err(
                "Nur Positionen (oder Titel mit Positionen) haben einen Bearbeitungsstatus."
                    .to_string(),
            );
        }
        if lv.rows()[index].is_group() {
            status_note = format!(
                " {count} von {positions} Positionen auf „{}“ gesetzt.",
                status.label()
            );
        }
        changed.push("Status");
    }
    // „In freie Position umwandeln“ first: the other fields of the same
    // call may then change the Langtext and the unit.
    if args.opt_flag("in_freie_position")? == Some(true) {
        if !lv.rows()[index].is_position() {
            return Err("in_freie_position gilt für Positionen mit STLB-Bau-Kennung.".to_string());
        }
        lv.ensure_editable(&[id]).map_err(locked)?;
        let today = chrono::Local::now().format("%d.%m.%Y").to_string();
        if lv.convert_to_free(&[id], &today) == 0 {
            return Err(
                "Die Position hat keine STLB-Bau-Kennung – sie ist schon frei änderbar."
                    .to_string(),
            );
        }
        changed.push("in freie Position umgewandelt (STLB-Bau-Kennung entfernt)");
    }
    let other_fields = args.0.keys().any(|key| {
        !matches!(
            key.as_str(),
            "projekt" | "oz" | "id" | "status" | "in_freie_position"
        )
    });
    // Titel selbst sind nicht gesperrt; ihre Ebene zieht aber die Positionen mit.
    let touches_positions =
        lv.rows()[index].is_position() || args.has("ebene") || args.has("neue_oz");
    if other_fields && touches_positions {
        lv.ensure_editable(&[id]).map_err(locked)?;
    }

    match lv.rows()[index].clone() {
        LvRow::Spacer(_) => {
            if other_fields {
                return Err(
                    "Ein Abstandshalter hat weder OZ noch Text – nichts zu ändern (verschieben: zeile_verschieben, entfernen: zeile_loeschen)."
                        .to_string(),
                );
            }
        }
        LvRow::Position(_) => {
            let performance = performance_ref(lv, args)?;
            let repetition = repetition_ref(lv, args)?;
            let surcharge = crate::args::surcharge_arg(lv, args, Some(id))?;
            let shares = lv.price_components().len();
            let position = lv
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
                .ok_or("Zeile nicht gefunden.")?;
            changed.extend(change_position(
                position,
                args,
                |file| images.contains(file),
                shares,
                surcharge,
            )?);
            if let Some(performance) = performance {
                lv.link_performance(id, performance)?;
                changed.push("Ausführungsbeschreibung");
            }
            if let Some(base) = repetition {
                lv.link_repetition(id, base)?;
                changed.push("Wiederholung");
            }
        }
        LvRow::Group(group) => {
            if let Some(text) = args.opt_text("kg")? {
                let code = proava_domain::parse_cost_group(text)?;
                let heirs = lv.locked_cost_group_heirs(&[index], code.as_deref());
                if !heirs.is_empty() && !args.flag("fertige_erben")? {
                    return Err(cost_group_heirs_message(lv, &heirs, code.as_deref()));
                }
                if let Some(LvRow::Group(group)) = lv.row_mut(index) {
                    group.set_cost_group(code);
                }
                changed.push("KG");
            }
            if let Some(text) = args.opt_text("lb")? {
                let code = proava_domain::parse_work_category(text)?;
                let heirs = lv.locked_work_category_heirs(&[index], code.as_deref());
                if !heirs.is_empty() && !args.flag("fertige_erben")? {
                    return Err(work_category_heirs_message(lv, &heirs, code.as_deref()));
                }
                if let Some(LvRow::Group(group)) = lv.row_mut(index) {
                    group.set_work_category(code);
                }
                changed.push("Leistungsbereich");
            }
            if let Some(text) = args.opt_text("kurztext")? {
                if text.trim().is_empty() {
                    return Err("Der Titeltext darf nicht leer sein.".to_string());
                }
                if let Some(LvRow::Group(group)) = lv.row_mut(index) {
                    group.set_short_text(text.trim());
                }
                changed.push("Kurztext");
            }
            // Pauschalsumme of the title (GAEB TotalLSUM); null or „“ = none.
            if let Some(value) = args.price_of("pauschalsumme", NumberKind::UnitPrice)? {
                if value.is_some_and(|value| value.is_sign_negative()) {
                    return Err("pauschalsumme: der Betrag darf nicht negativ sein.".to_string());
                }
                if let Some(LvRow::Group(group)) = lv.row_mut(index) {
                    group.set_lump_sum_total(value.map(Money::from_decimal));
                }
                changed.push("Pauschalsumme");
            }
            if let Some(level) = args.opt_count("ebene")? {
                let levels = lv.level_count().max(1);
                if level == 0 || level as usize > levels {
                    return Err(format!("Ebene {level} gibt es nicht (1 bis {levels})."));
                }
                let target = level as usize - 1;
                let mut current = group.level();
                while current != target {
                    let deeper = target > current;
                    if !lv.shift_level(index, deeper) {
                        return Err(format!(
                            "Ebene {level} geht nicht: ein Untertitel würde die {levels} Ebenen der OZ-Maske verlassen."
                        ));
                    }
                    current = if deeper { current + 1 } else { current - 1 };
                }
                changed.push("Ebene");
            }
        }
        LvRow::Note(_) => {
            if let Some(text) = args.opt_text("text")? {
                let rich = parse_markup(text, |file| images.contains(file))?;
                if let Some(LvRow::Note(note)) = lv.row_mut(index) {
                    note.set_text(rich);
                }
                changed.push("Text");
            }
            if let Some(heading) = args.opt_text("kurztext")? {
                if let Some(LvRow::Note(note)) = lv.row_mut(index) {
                    note.set_heading(heading.trim());
                }
                changed.push("Kurztext");
            }
            if args.opt_flag("vorbemerkung")? == Some(true)
                && args.opt_flag("schlussbemerkung")? == Some(true)
            {
                return Err(BOTH_NOTE_KINDS.to_string());
            }
            if let Some(preliminary) = args.opt_flag("vorbemerkung")? {
                if let Some(LvRow::Note(note)) = lv.row_mut(index) {
                    note.set_preliminary(preliminary);
                }
                changed.push("Vorbemerkung");
            }
            if let Some(closing) = args.opt_flag("schlussbemerkung")? {
                if let Some(LvRow::Note(note)) = lv.row_mut(index) {
                    note.set_closing(closing);
                }
                changed.push("Schlussbemerkung");
            }
        }
        LvRow::Performance(_) => {
            let text = args
                .opt_text("text")?
                .or(args.opt_text("langtext")?)
                .map(|text| parse_markup(text, |file| images.contains(file)))
                .transpose()?;
            let label = args.opt_text("bezeichnung")?;
            let short = args.opt_text("kurztext")?;
            if let Some(LvRow::Performance(performance)) = lv.row_mut(index) {
                if let Some(rich) = text {
                    performance.set_long_text(rich);
                    changed.push("Text");
                }
                if let Some(label) = label {
                    performance.set_label(label.trim());
                    changed.push("Bezeichnung");
                }
                if let Some(short) = short {
                    performance.set_short_text(short.trim());
                    changed.push("Kurztext");
                }
            }
            if let Some(number) = args.opt_text("nummer")? {
                lv.set_performance_number(id, number)?;
                changed.push("Nummer");
            }
        }
    }

    if let Some(ordinal) = args
        .opt_text("neue_oz")?
        .filter(|text| !text.trim().is_empty())
    {
        if lv.rows()[index].is_note() || lv.rows()[index].is_spacer() {
            return Err("Hinweise und Abstandshalter haben keine OZ.".to_string());
        }
        let key = normalize_ordinal(ordinal);
        if lv.rows().iter().any(|row| {
            row.id() != id && !row.ordinal().is_empty() && normalize_ordinal(row.ordinal()) == key
        }) {
            return Err(format!("Die OZ „{}“ ist schon vergeben.", ordinal.trim()));
        }
        match lv.row_mut(index) {
            Some(LvRow::Group(group)) => group.set_ordinal(ordinal.trim()),
            Some(LvRow::Position(position)) => position.set_ordinal(ordinal.trim()),
            _ => {}
        }
        changed.push("OZ");
    } else if auto && changed.contains(&"Ebene") {
        lv.renumber();
    }

    if changed.is_empty() {
        return Err("Keine passende Angabe zum Ändern (Positionen: kurztext, langtext, menge, einheit, ep, art, ausfuehrungsbeschreibung, status, kg, lb …; Titel: kurztext, ebene, status, kg, lb; Hinweise: text, kurztext; Ausführungsbeschreibungen: bezeichnung, kurztext, text, nummer).".to_string());
    }
    let answer = format!(
        "Geändert ({}): {}{status_note}{}{}\n{}",
        changed.join(", "),
        position_line(lv, id),
        renumber_note(renumbered_count(&before, lv, &[id])),
        text_hints(lv, id),
        sums_after_change(lv, Some(id))
    );
    server.save_with_answer(&loaded, answer)
}

/// „Hinweis: Kurztext hat 85 Zeichen …“ lines (GAEB limits, as the program
/// shows them at the field), empty when the texts fit.
fn text_hints(lv: &Lv, id: RowId) -> String {
    let Some(row) = lv.index_of(id).map(|index| &lv.rows()[index]) else {
        return String::new();
    };
    let unit = row
        .as_position()
        .map(|position| position.unit())
        .unwrap_or("");
    proava_domain::input_limits::text_hints(&row.short_text(), unit)
        .into_iter()
        .map(|hint| format!("\nHinweis: {hint}"))
        .collect()
}

pub(crate) fn delete_row(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let project = &mut loaded.project;
    let lv = project.lv();
    let index = find_row(lv, args, "oz", "id")?;
    lv.ensure_editable_rows(&[index]).map_err(locked)?;
    let range = lv.block_range(index);
    let content = range.len() - 1;
    if content > 0 && !args.flag("mit_inhalt")? {
        return Err(format!(
            "Der Titel {} hat {content} Zeilen Inhalt – zum Löschen samt Inhalt mit_inhalt: true angeben.",
            row_name(&lv.rows()[index])
        ));
    }
    let name = row_name(&lv.rows()[index]);
    let parent = crate::format::headings_above(lv, index)
        .last()
        .map(|heading| lv.rows()[*heading].id());
    let auto = lv.is_auto_numbered();
    let before = ordinals_by_id(lv);

    let removed = project.lv_mut().remove_block(index);
    let removed_ids: Vec<RowId> = removed.iter().map(LvRow::id).collect();
    let priced = removed
        .iter()
        .filter_map(LvRow::as_position)
        .filter(|position| {
            project
                .bids()
                .iter()
                .any(|bid| bid.price(position.id()).is_some())
        })
        .count();
    if auto {
        project.lv_mut().renumber();
    }
    project.remove_unused_images();
    let lv = project.lv();

    let mut answer = format!("Gelöscht: {name}");
    if content > 0 {
        answer.push_str(&format!(" samt {content} Zeilen Inhalt"));
    }
    answer.push('.');
    if priced > 0 {
        answer.push_str(&format!(
            " Achtung: {priced} gelöschte Position(en) hatten Bieterpreise."
        ));
    }
    answer.push_str(&renumber_note(renumbered_count(&before, lv, &removed_ids)));
    answer.push('\n');
    answer.push_str(&sums_after_change(lv, parent));
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn move_row(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let up = match args.text("richtung")?.trim().to_lowercase().as_str() {
        "hoch" | "oben" | "rauf" => true,
        "runter" | "unten" => false,
        other => return Err(format!("Richtung „{other}“ – bitte hoch oder runter.")),
    };
    let steps = args.opt_count("schritte")?.unwrap_or(1).clamp(1, 10_000);
    let lv = loaded.project.lv_mut();
    let mut index = find_row(lv, args, "oz", "id")?;
    let id = lv.rows()[index].id();
    lv.ensure_editable(&[id]).map_err(locked)?;
    let auto = lv.is_auto_numbered();
    let before = ordinals_by_id(lv);

    let mut moved = 0;
    while moved < steps {
        match lv.move_block(index, up) {
            Some(new_index) => {
                index = new_index;
                moved += 1;
            }
            None => break,
        }
    }
    if moved == 0 {
        return Err(format!(
            "{} steht schon ganz {}.",
            row_name(&lv.rows()[index]),
            if up { "oben" } else { "unten" }
        ));
    }

    let mut note = String::new();
    if auto {
        lv.renumber();
    } else if lv.rows()[index].is_position() {
        if let Some(mut row) = lv.remove_row(index) {
            let old = row.ordinal().to_string();
            match lv.free_ordinal_at(index, &row) {
                Some(ordinal) => {
                    if let Some(position) = row.as_position_mut() {
                        position.set_ordinal(ordinal);
                    }
                }
                None => {
                    note = format!(" Am neuen Platz ist keine OZ frei – sie bleibt {old}; neu_nummerieren erwägen.");
                }
            }
            lv.insert_row(index, row);
        }
    } else if lv.rows()[index].is_group() {
        note = " Die OZ bleiben (eigene Nummerierung) – neu_nummerieren erwägen.".to_string();
    }
    let answer = format!(
        "Verschoben um {moved} {} {}: {}.{note}{}\n{}",
        if moved == 1 { "Platz" } else { "Plätze" },
        if up { "nach oben" } else { "nach unten" },
        row_name(&lv.rows()[index]),
        renumber_note(renumbered_count(&before, lv, &[id])),
        sums_after_change(lv, Some(id))
    );
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn renumber(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let has_bids = !loaded.project.bids().is_empty();
    let lv = loaded.project.lv_mut();
    let before = ordinals_by_id(lv);
    lv.renumber();
    let changed = renumbered_count(&before, lv, &[]);
    // Ausführungsbeschreibungen count 1, 2, 3 … again (references by id stay).
    let performances = lv.renumber_performances();
    if changed == 0 && performances == 0 {
        return Ok("Alle OZ stimmen schon – nichts geändert, nichts gespeichert.".to_string());
    }
    let mut answer = format!(
        "{changed} OZ neu vergeben (Maske {}, Schrittweite {}). Nummerierung jetzt: {}.",
        lv.oz_mask().pattern(),
        lv.position_step(),
        numbering_note(lv)
    );
    if performances > 0 {
        answer.push_str(&format!(
            " {performances} Ausführungsbeschreibung(en) neu nummeriert (A0001, A0002, A0003 …)."
        ));
    }
    if has_bids {
        answer.push_str(" Angebote bleiben zugeordnet (über Zeilen-Ids); schon verschickte GAEB-Dateien tragen aber die alten OZ.");
    }
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn set_discount(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let id = BidId(
        args.opt_count("angebot")?
            .ok_or("Angabe „angebot“ (Id aus angebote_lesen) fehlt.")?,
    );
    let percent_value = args.opt_number("prozent")?;
    let amount = args.opt_number("betrag")?;
    // 0 removes the discount; otherwise the limits of the input field
    // (`input_limits`): above 0 and below 100 %, an amount above 0 €.
    let checked = |key: &str, kind: NumberKind, value: Decimal| {
        kind.check(value)
            .map_err(|error| format!("„{key}“: {error}"))
    };
    let discount = match (percent_value, amount) {
        (Some(_), Some(_)) => return Err("Bitte nur prozent oder betrag angeben.".to_string()),
        (Some(value), None) if value.is_zero() => Discount::None,
        (Some(value), None) => {
            Discount::Percent(checked("prozent", NumberKind::DiscountPercent, value)?)
        }
        (None, Some(value)) if value.is_zero() => Discount::None,
        (None, Some(value)) => Discount::Amount(Money::from_decimal(checked(
            "betrag",
            NumberKind::Amount,
            value,
        )?)),
        (None, None) => Discount::None,
    };
    let excluded = args.opt_flag("ausgeschlossen")?;
    let bid = loaded
        .project
        .bid_mut(id)
        .ok_or_else(|| format!("Ein Angebot #{} gibt es nicht.", id.0))?;
    bid.set_discount(discount);
    if let Some(excluded) = excluded {
        bid.set_excluded(excluded);
    }
    let what = match discount {
        Discount::None => "kein Nachlass".to_string(),
        Discount::Percent(value) => format!("Nachlass {}", percent(value)),
        Discount::Amount(value) => format!("Nachlass {}", money(value)),
    };
    let bid = loaded
        .project
        .bids()
        .iter()
        .find(|bid| bid.id() == id)
        .ok_or("Angebot verschwunden.")?;
    let answer = format!("Gesetzt: {what}.\n{}", bid_summary(&loaded.project, bid));
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn award(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    if args.opt_flag("aufheben")?.unwrap_or(false) {
        let id = loaded
            .project
            .revoke_award()
            .ok_or("Kein Angebot ist beauftragt.")?;
        let name = loaded
            .project
            .bids()
            .iter()
            .find(|bid| bid.id() == id)
            .map(|bid| bid.name().to_string())
            .unwrap_or_default();
        let answer = format!("Auftrag an {name} (#{}) aufgehoben.", id.0);
        return server.save_with_answer(&loaded, answer);
    }
    let id = BidId(
        args.opt_count("angebot")?
            .ok_or("Angabe „angebot“ (Id aus angebote_lesen) fehlt.")?,
    );
    let date = args
        .opt_text("datum")?
        .map(str::trim)
        .filter(|date| !date.is_empty())
        .ok_or("Angabe „datum“ (TT.MM.JJJJ) fehlt.")?
        .to_string();
    let total = loaded.project.award_bid(id, &date)?;
    let bid = loaded
        .project
        .bids()
        .iter()
        .find(|bid| bid.id() == id)
        .ok_or("Angebot verschwunden.")?;
    let answer = format!(
        "Auftrag an {} am {date} erteilt – Auftragssumme netto {}.\n{}",
        bid.name(),
        money(total),
        bid_summary(&loaded.project, bid)
    );
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn adopt_prices(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let source = match args.text("quelle")?.trim().to_lowercase().as_str() {
        "angebot" => PriceSource::Bid(BidId(
            args.opt_count("angebot")?
                .ok_or("Bei quelle angebot fehlt „angebot“ (Id aus angebote_lesen).")?,
        )),
        "mittelwert" => PriceSource::Mean,
        "median" => PriceSource::Median,
        "minimum" => PriceSource::Minimum,
        other => {
            return Err(format!(
                "Quelle „{other}“ – bitte angebot, mittelwert, median oder minimum."
            ))
        }
    };
    let overwrite = args.opt_flag("ueberschreiben")?.unwrap_or(false);
    let result = loaded.project.adopt_prices(source, overwrite)?;
    let mut answer = format!(
        "Planerpreise ({}): {} übernommen, {} vorhandene behalten.",
        source.label(),
        result.changed,
        result.kept
    );
    if !result.without_price.is_empty() {
        answer.push_str(&format!(
            "\nOhne Preis in der Quelle ({}): {}",
            result.without_price.len(),
            result.without_price.join(", ")
        ));
    }
    answer.push_str(&format!(
        "\nNetto des LV jetzt {}.",
        money(loaded.project.lv().net_total())
    ));
    server.save_with_answer(&loaded, answer)
}

/// `zeile_umwandeln`: Zeilenart ändern – Position, Hinweis (Text) oder
/// Ausführungsbeschreibung. Text, Bilder, Bieterangaben, STLB-Bau-Kennung und
/// Textquelle bleiben; was die neue Art nicht trägt, nennt die Antwort. Geht
/// dabei etwas verloren, erst mit `bestaetigen: true` (vorher nur Vorschau).
pub(crate) fn convert_row(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let wanted = args.text("zeilenart")?;
    let to = proava_domain::RowKind::parse(wanted).ok_or_else(|| {
        format!("Zeilenart „{wanted}“ – bitte position, hinweis oder ausfuehrungsbeschreibung.")
    })?;
    let project = &mut loaded.project;
    let lv = project.lv();
    let index = find_row(lv, args, "oz", "id")?;
    let id = lv.rows()[index].id();
    let name = row_name(&lv.rows()[index]);
    let conversion = lv.conversion(index, to)?;
    let unlinked: Vec<String> = conversion
        .unlinked
        .iter()
        .filter_map(|id| lv.position_by_id(*id))
        .map(|position| position.ordinal().to_string())
        .collect();
    let priced = if conversion.from == proava_domain::RowKind::Position {
        project
            .bids()
            .iter()
            .filter(|bid| bid.price(id).is_some())
            .count()
    } else {
        0
    };
    let mut losses = Vec::new();
    if !conversion.dropped.is_empty() {
        losses.push(format!("Fällt weg: {}.", conversion.dropped.join(", ")));
    }
    if !unlinked.is_empty() {
        losses.push(format!(
            "Bezug wird gelöst bei {} Position(en): {}.",
            unlinked.len(),
            unlinked.join(", ")
        ));
    }
    if priced > 0 {
        losses.push(format!(
            "{priced} Angebot(e) enthalten Preise für diese Position – sie zählen nicht mehr, solange die Zeile keine Position ist."
        ));
    }
    if let Some(iso) = lv.request_exported() {
        losses.push(format!(
            "Die Angebotsaufforderung (X83) wurde am {} exportiert – das LV muss danach neu versendet werden.",
            iso.get(..10).unwrap_or(iso)
        ));
    }
    if !losses.is_empty() && !args.flag("bestaetigen")? {
        return Ok(format!(
            "Vorschau – {name} in {} umwandeln:\n{}\nNichts gespeichert. Mit bestaetigen: true umwandeln.",
            to.name(),
            losses.join("\n")
        ));
    }
    let auto = lv.is_auto_numbered();
    let before = ordinals_by_id(lv);
    project.lv_mut().convert_row(index, to)?;
    let lv = project.lv();
    let now = lv
        .row_by_id(id)
        .map(row_name)
        .unwrap_or_else(|| format!("#{}", id.0));
    let mut answer = format!("Umgewandelt in {}: {name} → {now}.", to.name());
    for line in &losses {
        answer.push('\n');
        answer.push_str(line);
    }
    if auto || to == proava_domain::RowKind::Position {
        answer.push_str(&renumber_note(renumbered_count(&before, lv, &[id])));
    }
    answer.push('\n');
    answer.push_str(&sums_after_change(lv, Some(id)));
    server.save_with_answer(&loaded, answer)
}
