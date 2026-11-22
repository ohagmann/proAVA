//! Reading tool arguments with clear German messages.

use std::str::FromStr;

use proava_domain::{
    normalize_ordinal, Decimal, Lv, LvRow, NumberKind, Position, PositionKind, Project, RowId,
};
use serde_json::{Map, Value};

use proava_domain::{ComplementKind, TextComplement};

use crate::markup::{parse_markup_with_places, Places};

pub(crate) struct Args<'a>(pub &'a Map<String, Value>);

impl<'a> Args<'a> {
    pub fn get(&self, key: &str) -> Option<&'a Value> {
        self.0.get(key).filter(|value| !value.is_null())
    }

    pub fn has(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn text(&self, key: &str) -> Result<&'a str, String> {
        self.opt_text(key)?
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| format!("Angabe „{key}“ fehlt."))
    }

    pub fn opt_text(&self, key: &str) -> Result<Option<&'a str>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text.as_str())),
            Some(_) => Err(format!("„{key}“ muss ein Text sein.")),
        }
    }

    pub fn flag(&self, key: &str) -> Result<bool, String> {
        Ok(self.opt_flag(key)?.unwrap_or(false))
    }

    pub fn opt_flag(&self, key: &str) -> Result<Option<bool>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::Bool(flag)) => Ok(Some(*flag)),
            Some(Value::String(text)) if text == "true" || text == "ja" => Ok(Some(true)),
            Some(Value::String(text)) if text == "false" || text == "nein" => Ok(Some(false)),
            Some(_) => Err(format!("„{key}“ muss true oder false sein.")),
        }
    }

    pub fn opt_count(&self, key: &str) -> Result<Option<u64>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::Number(number)) => number
                .as_u64()
                .map(Some)
                .ok_or_else(|| format!("„{key}“ muss eine ganze Zahl ≥ 0 sein.")),
            Some(Value::String(text)) => text
                .trim()
                .parse()
                .map(Some)
                .map_err(|_| format!("„{key}“ muss eine ganze Zahl ≥ 0 sein.")),
            Some(_) => Err(format!("„{key}“ muss eine ganze Zahl sein.")),
        }
    }

    pub fn opt_number(&self, key: &str) -> Result<Option<Decimal>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(value) => number(value)
                .map(Some)
                .map_err(|error| format!("„{key}“: {error}")),
        }
    }

    /// Unit price: absent = leave as is, null or "" = no price, else the number.
    pub fn price(&self, key: &str) -> Result<Option<Option<Decimal>>, String> {
        match self.0.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(text)) if text.trim().is_empty() || text.trim() == "-" => {
                Ok(Some(None))
            }
            Some(value) => number(value)
                .map(|value| Some(Some(value)))
                .map_err(|error| format!("„{key}“: {error}")),
        }
    }

    /// A number of this kind with the limits of the program's input fields
    /// (`proava_domain::input_limits`); absent = None.
    pub fn number_of(&self, key: &str, kind: NumberKind) -> Result<Option<Decimal>, String> {
        self.opt_number(key)?
            .map(|value| {
                kind.check(value)
                    .map_err(|error| format!("„{key}“: {error}"))
            })
            .transpose()
    }

    /// Like [`Args::price`], with the limits of `kind`.
    pub fn price_of(&self, key: &str, kind: NumberKind) -> Result<Option<Option<Decimal>>, String> {
        match self.price(key)? {
            Some(Some(value)) => kind
                .check(value)
                .map(|value| Some(Some(value)))
                .map_err(|error| format!("„{key}“: {error}")),
            other => Ok(other),
        }
    }

    pub fn object(&self, key: &str) -> Result<Option<Args<'a>>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::Object(map)) => Ok(Some(Args(map))),
            Some(_) => Err(format!("„{key}“ muss ein Objekt sein.")),
        }
    }
}

pub(crate) fn number(value: &Value) -> Result<Decimal, String> {
    match value {
        Value::Number(number) => {
            let text = number.to_string();
            Decimal::from_str(&text)
                .or_else(|_| Decimal::from_scientific(&text))
                .map_err(|_| match number.as_f64() {
                    // 1e30 and the like: beyond any input limit.
                    Some(value) if value.is_finite() && value.abs() >= 1.0 => {
                        "die Zahl ist zu groß.".to_string()
                    }
                    _ => format!("„{text}“ ist keine Zahl."),
                })
        }
        // German like the input fields: „1.150“ = 1150, „1.234,5“, „2.5“ = 2,5.
        Value::String(text) => proava_domain::input_limits::parse_text(text),
        _ => Err("keine Zahl".to_string()),
    }
}

/// Finds a row by `oz` or `id` (row id from lv_lesen).
pub(crate) fn find_row(lv: &Lv, args: &Args, oz_key: &str, id_key: &str) -> Result<usize, String> {
    if let Some(id) = args.opt_count(id_key)? {
        return lv
            .index_of(RowId(id))
            .ok_or_else(|| format!("Eine Zeile #{id} gibt es nicht."));
    }
    let oz = args
        .opt_text(oz_key)?
        .filter(|oz| !oz.trim().is_empty())
        .ok_or_else(|| format!("Bitte „{oz_key}“ (OZ) oder „{id_key}“ (Zeilen-Id) angeben."))?;
    find_by_ordinal(lv, oz)
}

pub(crate) fn find_by_ordinal(lv: &Lv, oz: &str) -> Result<usize, String> {
    let exact = normalize_ordinal(oz);
    let mut hits: Vec<usize> = matching(lv, |ordinal| normalize_ordinal(ordinal) == exact);
    if hits.is_empty() {
        let loose = loose_key(oz);
        hits = matching(lv, |ordinal| loose_key(ordinal) == loose);
    }
    match hits.as_slice() {
        [index] => Ok(*index),
        [] => Err(format!("Die OZ „{}“ gibt es im LV nicht.", oz.trim())),
        several => Err(format!(
            "Die OZ „{}“ gibt es mehrfach ({}) – bitte die Zeilen-Id angeben.",
            oz.trim(),
            several
                .iter()
                .map(|index| format!("#{}", lv.rows()[*index].id().0))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn matching(lv: &Lv, wanted: impl Fn(&str) -> bool) -> Vec<usize> {
    lv.rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.ordinal().is_empty() && wanted(row.ordinal()))
        .map(|(index, _)| index)
        .collect()
}

/// "1.2.10" and "01.02.0010" are the same.
fn loose_key(ordinal: &str) -> String {
    normalize_ordinal(ordinal)
        .split('.')
        .map(|piece| {
            let trimmed = piece.trim_start_matches('0');
            if trimmed.is_empty() && !piece.is_empty() {
                "0"
            } else {
                trimmed
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Where a new position or hint goes: after `nach_oz`/`nach_id`, into
/// `unter_titel` (end of its own rows, or right after it with `am_anfang`),
/// else at the end of the LV.
pub(crate) fn content_index(lv: &Lv, args: &Args) -> Result<usize, String> {
    if args.get("nach_oz").is_some() || args.get("nach_id").is_some() {
        let index = find_row(lv, args, "nach_oz", "nach_id")?;
        return Ok(index + 1);
    }
    if let Some(title) = args
        .opt_text("unter_titel")?
        .filter(|text| !text.trim().is_empty())
    {
        let index = find_by_ordinal(lv, title)?;
        if !lv.rows()[index].is_group() {
            return Err(format!("„{title}“ ist kein Titel."));
        }
        if args.flag("am_anfang")? {
            return Ok(index + 1);
        }
        let end = lv.rows()[index + 1..]
            .iter()
            .position(LvRow::is_group)
            .map(|offset| index + 1 + offset)
            .unwrap_or(lv.rows().len());
        return Ok(end);
    }
    Ok(lv.rows().len())
}

/// Where a new heading goes and on which level (0-based).
pub(crate) fn heading_target(lv: &Lv, args: &Args) -> Result<(usize, usize), String> {
    let levels = lv.level_count().max(1);
    let wanted_level = match args.opt_count("ebene")? {
        Some(0) => return Err("„ebene“ beginnt bei 1 (Titel).".to_string()),
        Some(level) if level as usize > levels => {
            return Err(format!(
                "Die OZ-Maske {} hat nur {levels} Titelebene(n).",
                lv.oz_mask().pattern()
            ))
        }
        Some(level) => Some(level as usize - 1),
        None => None,
    };

    if let Some(title) = args
        .opt_text("unter_titel")?
        .filter(|text| !text.trim().is_empty())
    {
        let index = find_by_ordinal(lv, title)?;
        let Some(parent) = lv.rows()[index].as_group() else {
            return Err(format!("„{title}“ ist kein Titel."));
        };
        let level = wanted_level.unwrap_or(parent.level() + 1);
        if level <= parent.level() || level >= levels {
            return Err(format!(
                "Unter „{title}“ (Ebene {}) passt keine Ebene {} – die Maske hat {levels} Ebene(n).",
                parent.level() + 1,
                level + 1
            ));
        }
        return Ok((lv.block_range(index).end, level));
    }

    if args.get("nach_oz").is_some() || args.get("nach_id").is_some() {
        let index = find_row(lv, args, "nach_oz", "nach_id")?;
        let level = wanted_level.unwrap_or_else(|| lv.level_for_new_group(Some(index)));
        return Ok((lv.insert_group_index(Some(index), level), level));
    }
    Ok((lv.rows().len(), wanted_level.unwrap_or(0)))
}

pub(crate) fn kind(args: &Args, current: PositionKind) -> Result<PositionKind, String> {
    let group = args.opt_count("wahlgruppe")?.map(|value| value as u32);
    let serial = args.opt_count("wahl_nr")?.map(|value| value as u32);
    let Some(key) = args.opt_text("art")? else {
        return Ok(match current {
            PositionKind::Base { group: old } => PositionKind::Base {
                group: group.unwrap_or(old),
            },
            PositionKind::Alternative {
                group: old,
                serial: old_serial,
            } => PositionKind::Alternative {
                group: group.unwrap_or(old),
                serial: serial.unwrap_or(old_serial),
            },
            other => other,
        });
    };
    Ok(match key.trim().to_lowercase().as_str() {
        "" | "normal" | "normalposition" => PositionKind::Normal,
        "bedarf" | "bedarf_ohne_gb" | "bedarfsposition" => {
            PositionKind::Provisional { with_total: false }
        }
        "bedarf_mit_gb" => PositionKind::Provisional { with_total: true },
        "grund" | "grundposition" => PositionKind::Base {
            group: group.unwrap_or(1),
        },
        "wahl" | "wahlposition" | "alternative" => PositionKind::Alternative {
            group: group.unwrap_or(1),
            serial: serial.unwrap_or(1),
        },
        "zulage" | "zulageposition" => PositionKind::Supplement,
        other => {
            return Err(format!(
                "Unbekannte Positionsart „{other}“ (normal, bedarf, bedarf_mit_gb, grund, wahl, zulage)."
            ))
        }
    })
}

/// A new position from the arguments.
pub(crate) fn new_position(args: &Args, project: &Project) -> Result<Position, String> {
    let short_text = args.text("kurztext")?.trim().to_string();
    let lump_sum = args.flag("pauschal")?;
    let quantity = args
        .number_of("menge", NumberKind::Quantity)?
        .unwrap_or(if lump_sum {
            Decimal::ONE
        } else {
            Decimal::ZERO
        });
    let mut unit = args.opt_text("einheit")?.unwrap_or("").trim().to_string();
    if unit.is_empty() && lump_sum {
        unit = "psch".to_string();
    }
    let price = args.price_of("ep", NumberKind::UnitPrice)?.flatten();
    let mut position = Position::new("", short_text, quantity, unit, price);
    position.set_kind(kind(args, PositionKind::Normal)?);
    position.set_lump_sum(lump_sum);
    position.set_hourly(args.flag("stundenlohn")?);
    position.set_stlb_placeholder(args.flag("stlb")?);
    if let Some(text) = args.opt_text("kg")? {
        position.set_cost_group(proava_domain::parse_cost_group(text)?);
    }
    if let Some(text) = args.opt_text("lb")? {
        position.set_work_category(proava_domain::parse_work_category(text)?);
    }
    let places = match args.opt_text("langtext")? {
        Some(long_text) => {
            let (text, places) =
                parse_markup_with_places(long_text, |file| project.image(file).is_some())?;
            position.set_long_text(text);
            places
        }
        None => Vec::new(),
    };
    let complements = complements(args, &[])?.unwrap_or_default();
    position.set_complements(place_complements(complements, &places)?);
    price_breakdown(&mut position, args, project.lv().price_components().len())?;
    if let Some(lines) = takeoff(args)? {
        position.set_takeoff(lines);
        if !args.has("menge") {
            position.set_quantity(takeoff_sum(&position)?);
        }
    }
    let surcharge = surcharge_arg(project.lv(), args, None)?;
    import_fields(&mut position, args, surcharge)?;
    Ok(position)
}

/// `zuschlag` of a position, resolved against the LV: None = not given,
/// Some(None) = no Zuschlagsposition (false, null, „“), Some(Some(..)) = a
/// Zuschlagsposition with its kind and listed positions. `own` = the row
/// itself (no reference to itself). `aufteilung` (or `zuschlag` with
/// `aufteilung: true`) makes it an Aufteilungszeile – only in a Kosten-LV;
/// `aufteilung: 40` = 40 % on the positions before it.
pub(crate) fn surcharge_arg(
    lv: &Lv,
    args: &Args,
    own: Option<RowId>,
) -> Result<Option<Option<proava_domain::Surcharge>>, String> {
    use proava_domain::{Surcharge, SurchargeKind, SurchargeReference};
    const KINDS: &str = "alle (alle Positionen der Liste davor), gekennzeichnet (Positionen mit zu_bezuschlagen), liste (die Positionen aus bezug)";
    let (key, split_field) = match args.get("aufteilung") {
        Some(_) => ("aufteilung", true),
        None => ("zuschlag", false),
    };
    let (kind, references, split) = match args.get(key) {
        None => return Ok(None),
        Some(Value::Null) | Some(Value::Bool(false)) => return Ok(Some(None)),
        Some(Value::String(text)) if text.trim().is_empty() => return Ok(Some(None)),
        Some(Value::String(text)) => (text.trim().to_string(), None, split_field),
        Some(Value::Bool(true)) => ("alle".to_string(), None, split_field),
        // aufteilung: 40 – the percentage on the positions before it.
        Some(Value::Number(_)) if split_field => ("alle".to_string(), None, true),
        Some(Value::Object(map)) => {
            let entry = Args(map);
            (
                entry.opt_text("art")?.unwrap_or("alle").trim().to_string(),
                entry.get("bezug"),
                split_field || entry.opt_flag("aufteilung")?.unwrap_or(false),
            )
        }
        Some(_) => {
            return Err(format!(
                "{key}: bitte ein Objekt {{art, prozent, bezug}} oder die Art als Text ({KINDS})."
            ))
        }
    };
    if split && lv.purpose().is_tender() {
        return Err(format!(
            "{key}: Aufteilungszeilen gibt es nur in einer Kostenermittlung (Kosten-LV) – im \
             Ausschreibungs-LV zählten ihre Positionen sonst nicht."
        ));
    }
    let kind = SurchargeKind::from_key(&kind)
        .ok_or_else(|| format!("{key}: unbekannte Art „{kind}“ – {KINDS}."))?;
    let mut listed = Vec::new();
    if let Some(list) = references {
        let items = list.as_array().ok_or_else(|| {
            format!("{key}.bezug: bitte eine Liste, je Eintrag OZ als Text oder {{oz, teilmenge}}.")
        })?;
        for item in items {
            let (oz, quantity) = match item {
                Value::String(oz) => (oz.as_str(), None),
                Value::Object(map) => {
                    let entry = Args(map);
                    (
                        entry.text("oz")?,
                        entry.number_of("teilmenge", NumberKind::Quantity)?,
                    )
                }
                _ => {
                    return Err(format!(
                        "{key}.bezug: je Eintrag OZ als Text oder {{oz, teilmenge}}."
                    ))
                }
            };
            let index = find_by_ordinal(lv, oz)?;
            let Some(position) = lv.rows()[index].as_position() else {
                return Err(format!("{key}.bezug: {} ist keine Position.", oz.trim()));
            };
            if Some(position.id()) == own {
                return Err(format!(
                    "{key}.bezug: eine Zuschlagsposition kann sich nicht auf sich selbst beziehen."
                ));
            }
            if position.is_surcharge() {
                return Err(format!(
                    "{key}.bezug: {} ist selbst eine Zuschlagsposition (Bezug nur auf Positionen mit Menge und EP).",
                    position.ordinal()
                ));
            }
            listed.push(SurchargeReference {
                position: position.id(),
                quantity,
            });
        }
    }
    match kind {
        SurchargeKind::Listed if listed.is_empty() => Err(format!(
            "{key}: Art „liste“ braucht bezug mit mindestens einer Position (GAEB ListInSubQty)."
        )),
        SurchargeKind::Listed => Ok(Some(Some(Surcharge {
            split,
            kind,
            references: listed,
        }))),
        _ if !listed.is_empty() => Err(format!(
            "{key}: bezug gilt nur für die Art „liste“ (hier „{}“).",
            kind.key()
        )),
        _ => Ok(Some(Some(Surcharge {
            split,
            kind,
            references: Vec::new(),
        }))),
    }
}

/// Fields the GAEB import knows, also by hand: `zuschlag` (resolved with
/// [`surcharge_arg`], percentage from `zuschlag.prozent` or `ep`),
/// `zu_bezuschlagen`, `entfaellt` (GAEB NotAppl) and `unterbeschreibungen`.
pub(crate) fn import_fields(
    position: &mut Position,
    args: &Args,
    surcharge: Option<Option<proava_domain::Surcharge>>,
) -> Result<Vec<&'static str>, String> {
    let mut changed = Vec::new();
    if let Some(surcharge) = surcharge {
        let percent = match args.get("aufteilung").or_else(|| args.get("zuschlag")) {
            Some(Value::Object(map)) => Args(map).price_of("prozent", NumberKind::UnitPrice)?,
            // aufteilung: 40
            Some(value @ Value::Number(_)) if args.get("aufteilung").is_some() => Some(Some(
                number(value)
                    .and_then(|percent| NumberKind::UnitPrice.check(percent))
                    .map_err(|error| format!("aufteilung: {error}"))?,
            )),
            _ => None,
        };
        if surcharge.is_some() {
            position.set_marked_for_surcharge(false);
        }
        position.set_surcharge(surcharge);
        if let Some(percent) = percent {
            position.set_unit_price(percent);
        }
        changed.push("Zuschlag");
    }
    if let Some(flag) = args.opt_flag("zu_bezuschlagen")? {
        if flag && position.is_surcharge() {
            return Err("zu_bezuschlagen gilt für Positionen, auf die ein Zuschlag „gekennzeichnet“ wirkt – nicht für die Zuschlagsposition selbst.".to_string());
        }
        position.set_marked_for_surcharge(flag);
        changed.push("zu bezuschlagen");
    }
    if let Some(flag) = args.opt_flag("entfaellt")? {
        position.set_not_applicable(flag);
        changed.push("entfällt");
    }
    if let Some(list) = args.get("unterbeschreibungen") {
        position.set_sub_descriptions(sub_descriptions(list)?);
        changed.push("Unterbeschreibungen");
    }
    if position.is_surcharge() {
        let conflict = ["menge", "einheit", "mengenermittlung"]
            .into_iter()
            .find(|key| args.has(key))
            .or_else(|| {
                ["pauschal", "stundenlohn"]
                    .into_iter()
                    .find(|key| args.flag(key).unwrap_or(false))
            });
        if let Some(key) = conflict {
            return Err(format!(
                "Eine Zuschlagsposition hat keine Menge und Einheit (GP = Bezugssumme × Prozentsatz im EP) – „{key}“ bitte weglassen."
            ));
        }
    }
    Ok(changed)
}

/// `unterbeschreibungen`: [{nummer, kurztext, langtext, menge, einheit}] –
/// the Unterbeschreibungen (GAEB SubDescr) of a position; [] = none.
fn sub_descriptions(list: &Value) -> Result<Vec<proava_domain::SubDescription>, String> {
    let items = list.as_array().ok_or(
        "unterbeschreibungen: bitte eine Liste von Objekten {nummer, kurztext, langtext, menge, einheit}.",
    )?;
    let mut result = Vec::new();
    for (number, item) in items.iter().enumerate() {
        let fail = |error: String| format!("unterbeschreibungen Eintrag {}: {error}", number + 1);
        let map = item.as_object().ok_or_else(|| {
            fail("kein Objekt {nummer, kurztext, langtext, menge, einheit}.".to_string())
        })?;
        let entry = Args(map);
        let long_text = match entry.opt_text("langtext").map_err(fail)? {
            Some(text) => {
                crate::markup::parse_markup_with_places(text, |_| false)
                    .map_err(fail)?
                    .0
            }
            None => proava_domain::RichText::default(),
        };
        let sub = proava_domain::SubDescription {
            number: entry
                .opt_text("nummer")
                .map_err(fail)?
                .unwrap_or("")
                .trim()
                .to_string(),
            short_text: entry
                .opt_text("kurztext")
                .map_err(fail)?
                .unwrap_or("")
                .trim()
                .to_string(),
            long_text,
            quantity: entry
                .number_of("menge", NumberKind::Quantity)
                .map_err(fail)?,
            unit: entry
                .opt_text("einheit")
                .map_err(fail)?
                .unwrap_or("")
                .trim()
                .to_string(),
            catalog: Default::default(),
        };
        if sub.short_text.is_empty() && sub.long_text.is_empty() {
            return Err(fail("ohne kurztext und langtext.".to_string()));
        }
        result.push(sub);
    }
    Ok(result)
}

/// `mengenermittlung`: [{ort, rechenansatz}] – the lines of the
/// Mengenermittlung (same lines as the Aufmaß of the Abrechnung); None when
/// not given, [] clears them.
fn takeoff(args: &Args) -> Result<Option<Vec<proava_domain::TakeoffLine>>, String> {
    let Some(list) = args.get("mengenermittlung") else {
        return Ok(None);
    };
    let items = list
        .as_array()
        .ok_or("mengenermittlung: bitte eine Liste von Objekten {ort, rechenansatz}.")?;
    let mut lines = Vec::new();
    for item in items {
        let map = item
            .as_object()
            .ok_or("mengenermittlung: jeder Eintrag ist ein Objekt {ort, rechenansatz}.")?;
        let entry = Args(map);
        let line = proava_domain::TakeoffLine {
            label: entry.opt_text("ort")?.unwrap_or("").trim().to_string(),
            formula: match entry.get("rechenansatz") {
                Some(Value::Number(number)) => number.to_string().replace('.', ","),
                Some(Value::String(text)) => text.trim().to_string(),
                _ => String::new(),
            },
            invoice: None,
        };
        if let Err(error) = line.result() {
            return Err(format!(
                "mengenermittlung Zeile {} „{}“: {error}",
                lines.len() + 1,
                line.formula
            ));
        }
        lines.push(line);
    }
    Ok(Some(lines))
}

/// Sum of the Mengenermittlung as the quantity (all lines are valid here).
fn takeoff_sum(position: &Position) -> Result<Decimal, String> {
    let (sum, errors) = position.takeoff_total();
    match errors.first() {
        Some(error) => Err(format!("mengenermittlung {error}")),
        None => proava_domain::input_limits::check_size(sum)
            .map_err(|error| format!("mengenermittlung: Summe {error}")),
    }
}

/// `textergaenzungen`: [{kennung, art, vortext, inhalt, nachtext}] – None
/// when not given. Fields left out keep the value of the complement with the
/// same marker in `existing`.
fn complements(
    args: &Args,
    existing: &[TextComplement],
) -> Result<Option<Vec<TextComplement>>, String> {
    let Some(list) = args.get("textergaenzungen") else {
        return Ok(None);
    };
    let items = list
        .as_array()
        .ok_or("textergaenzungen: bitte eine Liste von Objekten {kennung, art, vortext, inhalt, nachtext}.")?;
    let mut out: Vec<TextComplement> = Vec::new();
    for item in items {
        let map = item
            .as_object()
            .ok_or("textergaenzungen: jeder Eintrag ist ein Objekt {kennung, art, vortext, inhalt, nachtext}.")?;
        let entry = Args(map);
        let marker = match entry.get("kennung") {
            Some(Value::Number(number)) => number.to_string(),
            Some(Value::String(text)) => text.trim().to_string(),
            _ => {
                return Err(
                    "textergaenzungen: jede braucht eine kennung (Zahl, z. B. 11).".to_string(),
                )
            }
        };
        if marker.is_empty() || !marker.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(format!(
                "textergaenzungen: Kennung „{marker}“ – GAEB erlaubt nur Ziffern."
            ));
        }
        if out.iter().any(|complement| complement.marker == marker) {
            return Err(format!("textergaenzungen: Kennung {marker} doppelt."));
        }
        let mut complement = existing
            .iter()
            .find(|complement| complement.marker == marker)
            .cloned()
            .unwrap_or_default();
        complement.marker = marker;
        if let Some(kind) = entry.opt_text("art")? {
            complement.kind = match kind.trim().to_lowercase().as_str() {
                "ag" | "auftraggeber" | "planer" | "owner" => ComplementKind::Owner,
                "bieter" | "an" | "bidder" => ComplementKind::Bidder,
                other => {
                    return Err(format!(
                        "textergaenzungen: Art „{other}“ – AG (Auftraggeber) oder Bieter."
                    ))
                }
            };
        }
        // Vortext und Nachtext bleiben, wie sie geschrieben sind (Leerzeichen,
        // Zeilenumbrüche): GAEB verlangt die Vorgaben des Ausschreibenden
        // unverändert; nur Leerraum allein zählt als leer.
        for (key, field) in [
            ("vortext", &mut complement.caption),
            ("nachtext", &mut complement.tail),
        ] {
            if let Some(text) = entry.opt_text(key)? {
                let text = text.replace("\r\n", "\n");
                *field = if text.trim().is_empty() {
                    String::new()
                } else {
                    text
                };
            }
        }
        if let Some(text) = entry.opt_text("inhalt")? {
            complement.body = text.trim().to_string();
        }
        out.push(complement);
    }
    Ok(Some(out))
}

/// Text of a hint or an Ausführungsbeschreibung with `{TE …}` places and the
/// `textergaenzungen` of the call, as for positions (GAEB allows
/// TextComplement in every DetailTxt).
pub(crate) fn text_with_complements(
    args: &Args,
    text: &str,
    known_image: impl Fn(&str) -> bool,
) -> Result<(proava_domain::RichText, Vec<TextComplement>), String> {
    let (rich, places) = crate::markup::parse_markup_with_places(text, known_image)?;
    let complements = complements(args, &[])?.unwrap_or_default();
    Ok((rich, place_complements(complements, &places)?))
}

/// Puts each complement at its `{TE …}` place (none = after the text);
/// every place needs a complement with that marker.
fn place_complements(
    mut complements: Vec<TextComplement>,
    places: &Places,
) -> Result<Vec<TextComplement>, String> {
    for (marker, _) in places {
        if !complements
            .iter()
            .any(|complement| &complement.marker == marker)
        {
            return Err(format!(
                "{{TE {marker}}} im Langtext, aber keine Textergänzung mit Kennung {marker} in textergaenzungen."
            ));
        }
    }
    for complement in &mut complements {
        complement.anchor = places
            .iter()
            .find(|(marker, _)| *marker == complement.marker)
            .map(|(_, anchor)| *anchor);
    }
    Ok(complements)
}

/// `ep_aufgliederung` (the bidder breaks the EP down) and
/// `ep_anteile_werte` (one value per EP share of the LV).
fn price_breakdown(
    position: &mut Position,
    args: &Args,
    shares: usize,
) -> Result<Vec<&'static str>, String> {
    let mut changed = Vec::new();
    if let Some(flag) = args.opt_flag("ep_aufgliederung")? {
        position.set_price_breakdown(flag);
        changed.push("EP-Aufgliederung");
    }
    if let Some(list) = args.get("ep_anteile_werte") {
        let items = list
            .as_array()
            .ok_or("ep_anteile_werte: bitte eine Liste von Zahlen (null = ohne Wert).")?;
        if shares == 0 {
            return Err(
                "ep_anteile_werte: das LV hat keine EP-Anteile (projektangaben_setzen ep_anteile)."
                    .to_string(),
            );
        }
        if items.len() != shares {
            return Err(format!(
                "ep_anteile_werte: das LV hat {shares} EP-Anteile, angegeben sind {}.",
                items.len()
            ));
        }
        let values = items
            .iter()
            .map(|value| match value {
                Value::Null => Ok(None),
                other => number(other)
                    .and_then(|value| NumberKind::UnitPrice.check(value))
                    .map(Some)
                    .map_err(|error| format!("ep_anteile_werte: {error}")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        position.set_price_components(values);
        changed.push("EP-Anteile");
    }
    match args.get("zeitansatz") {
        None => {}
        Some(Value::Null) => {
            position.set_time_quantity(None);
            changed.push("Zeitansatz");
        }
        Some(Value::String(text)) if text.trim().is_empty() => {
            position.set_time_quantity(None);
            changed.push("Zeitansatz");
        }
        Some(value) => {
            let hours = number(value).map_err(|error| format!("zeitansatz: {error}"))?;
            if hours.is_sign_negative()
                || hours.normalize().scale() > 4
                || hours >= Decimal::from(100_000_000)
            {
                return Err(
                    "„zeitansatz“: Zahl ab 0 mit höchstens vier Nachkommastellen (GAEB TimeQu)."
                        .to_string(),
                );
            }
            position.set_time_quantity(Some(hours));
            changed.push("Zeitansatz");
        }
    }
    Ok(changed)
}

/// Changes only the given fields of a position; returns their names.
pub(crate) fn change_position(
    position: &mut Position,
    args: &Args,
    project_images: impl Fn(&str) -> bool,
    shares: usize,
    surcharge: Option<Option<proava_domain::Surcharge>>,
) -> Result<Vec<&'static str>, String> {
    let mut changed = Vec::new();
    // STLB-Bau text: Langtext and unit are write-protected; the values of the
    // Textergänzungen stay free (checked below).
    if position.is_text_locked() {
        let unit_changes = args
            .opt_text("einheit")?
            .is_some_and(|unit| unit.trim() != position.unit());
        if args.has("langtext") || unit_changes {
            let lv_name = format!("Position {}", position.ordinal());
            return Err(format!(
                "{lv_name}: Langtext und Einheit stammen aus STLB-Bau und sind schreibgeschützt (Kurztext, Menge, EP, KG, LB, \
                 Status, Art, Bezug und die Werte der Textergänzungen sind frei). Ändern: über STLB-Bau (stlb_pruefdatei, \
                 stlb_aktualisieren) oder zeile_aendern mit in_freie_position: true – das entfernt die STLB-Bau-Kennung. \
                 Nichts gespeichert."
            ));
        }
    }
    if let Some(text) = args.opt_text("kurztext")? {
        if text.trim().is_empty() {
            return Err("Der Kurztext darf nicht leer sein.".to_string());
        }
        position.set_short_text(text.trim());
        changed.push("Kurztext");
    }
    let places = match args.opt_text("langtext")? {
        Some(text) => {
            let (rich, places) = parse_markup_with_places(text, &project_images)?;
            position.set_long_text(rich);
            changed.push("Langtext");
            Some(places)
        }
        None => None,
    };
    if let Some(lines) = takeoff(args)? {
        position.set_takeoff(lines);
        changed.push("Mengenermittlung");
        if !args.has("menge") {
            position.set_quantity(takeoff_sum(position)?);
            position.set_quantity_formula(None);
            changed.push("Menge");
        }
    }
    let given = complements(args, position.complements())?;
    if position.is_text_locked() {
        if let Some(list) = given {
            // Only the named fields get new values; the others stay.
            let mut edited = position.complements().to_vec();
            for given in list {
                match edited.iter_mut().find(|known| known.marker == given.marker) {
                    Some(known) => *known = given,
                    None => edited.push(given),
                }
            }
            let values = position
                .locked_complement_values(&edited)
                .map_err(|error| format!("{error} Nichts gespeichert."))?;
            position.set_complements(values);
            changed.push("Werte der Textergänzungen");
        }
    } else if given.is_some() || places.is_some() {
        let list = given.unwrap_or_else(|| position.complements().to_vec());
        let placed = match &places {
            Some(places) => place_complements(list, places)?,
            // Only the list changed: the places stay with their markers.
            None => {
                let old = position.complements().to_vec();
                list.into_iter()
                    .map(|mut complement| {
                        complement.anchor = old
                            .iter()
                            .find(|before| before.marker == complement.marker)
                            .and_then(|before| before.anchor);
                        complement
                    })
                    .collect()
            }
        };
        if args.has("textergaenzungen") {
            changed.push("Textergänzungen");
        }
        position.set_complements(placed);
    }
    changed.extend(price_breakdown(position, args, shares)?);
    if let Some(quantity) = args.number_of("menge", NumberKind::Quantity)? {
        position.set_quantity(quantity);
        changed.push("Menge");
    }
    if let Some(unit) = args.opt_text("einheit")? {
        position.set_unit(unit.trim());
        changed.push("Einheit");
    }
    if let Some(price) = args.price_of("ep", NumberKind::UnitPrice)? {
        position.set_unit_price(price);
        changed.push("EP");
    }
    if args.has("art") || args.has("wahlgruppe") || args.has("wahl_nr") {
        position.set_kind(kind(args, position.kind())?);
        changed.push("Art");
    }
    if let Some(flag) = args.opt_flag("pauschal")? {
        position.set_lump_sum(flag);
        changed.push("Pauschal");
    }
    if let Some(text) = args.opt_text("kg")? {
        position.set_cost_group(proava_domain::parse_cost_group(text)?);
        changed.push("KG");
    }
    if let Some(text) = args.opt_text("lb")? {
        position.set_work_category(proava_domain::parse_work_category(text)?);
        changed.push("Leistungsbereich");
    }
    if let Some(flag) = args.opt_flag("stundenlohn")? {
        position.set_hourly(flag);
        changed.push("Stundenlohn");
    }
    if let Some(flag) = args.opt_flag("stlb")? {
        position.set_stlb_placeholder(flag);
        changed.push("STLB");
    }
    changed.extend(import_fields(position, args, surcharge)?);
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{number, Args};

    #[test]
    fn numbers_from_json_and_german_text() {
        assert_eq!(number(&json!(12.5)).unwrap().to_string(), "12.5");
        assert_eq!(number(&json!("1.234,5")).unwrap().to_string(), "1234.5");
        assert!(number(&json!(true)).is_err());
        let map = json!({"ep": null, "menge": "3"});
        let args = Args(map.as_object().unwrap());
        assert_eq!(args.price("ep").unwrap(), Some(None));
        assert_eq!(args.price("fehlt").unwrap(), None);
        assert_eq!(args.opt_number("menge").unwrap().unwrap().to_string(), "3");
    }
}
