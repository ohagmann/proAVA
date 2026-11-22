//! Invariants of a project that every edit – in the domain, through the
//! UI operations or by the monkey test clicking around – must keep.

use std::collections::HashSet;

use proava_domain::{Lv, LvRow, Money, Project, RowId};

/// Everything that must hold for one LV.
pub fn check_lv(lv: &Lv) -> Result<(), String> {
    let rows = lv.rows();
    // Eindeutige Ids, keine unbesetzte Id, nächste Id größer als alle.
    let mut ids = HashSet::new();
    for row in rows {
        if row.id() == RowId::UNSET || !ids.insert(row.id()) {
            return Err(format!("Id {:?} doppelt oder unbesetzt", row.id()));
        }
        if row.id().0 >= lv.next_row_id().0 {
            return Err(format!("Id {:?} ≥ nächste Id", row.id()));
        }
    }
    // Keine doppelten OZ.
    let duplicates = lv.duplicate_ordinals();
    if !duplicates.is_empty() {
        return Err(format!("doppelte OZ {duplicates:?}"));
    }
    // Titelebenen innerhalb der Maske; ohne Titelstufe keine Titel.
    let levels = lv.level_count();
    for row in rows {
        if let Some(group) = row.as_group() {
            if group.level() >= levels.max(1) || (levels == 0) {
                return Err(format!(
                    "Titel {} auf Stufe {} bei Maske {}",
                    group.ordinal(),
                    group.level(),
                    lv.oz_mask().pattern()
                ));
            }
        }
    }
    // Bezüge zeigen auf vorhandene Zeilen des passenden Typs.
    for position in lv.positions() {
        if let Some(performance) = position.performance() {
            if lv.performance_by_id(performance).is_none() {
                return Err(format!(
                    "{} zeigt auf fehlende Ausführungsbeschreibung",
                    position.ordinal()
                ));
            }
        }
        if let Some(base) = position.repeats() {
            if base == position.id() || lv.position_by_id(base).is_none() {
                return Err(format!(
                    "{} wiederholt eine fehlende Position",
                    position.ordinal()
                ));
            }
        }
    }
    // Nummern von Ausführungsbeschreibungen eindeutig.
    let mut numbers = HashSet::new();
    for performance in lv.performances() {
        let key = performance
            .number()
            .trim()
            .trim_start_matches('0')
            .to_uppercase();
        if !numbers.insert(key) {
            return Err(format!(
                "Ausführungsbeschreibung {} doppelt",
                performance.number()
            ));
        }
    }
    // block_range: unabhängig nachgerechnet.
    for (index, row) in rows.iter().enumerate() {
        let range = lv.block_range(index);
        let expected_end = match row.as_group() {
            Some(group) => rows[index + 1..]
                .iter()
                .position(|other| {
                    other
                        .as_group()
                        .is_some_and(|other| other.level() <= group.level())
                })
                .map_or(rows.len(), |offset| index + 1 + offset),
            None => index + 1,
        };
        if range != (index..expected_end) {
            return Err(format!(
                "block_range({index}) = {range:?}, erwartet {index}..{expected_end}"
            ));
        }
    }
    // Summen, unabhängig nachgerechnet: Betrag je Zeile – Position: gezählter
    // Betrag; Zuschlagsposition: Bezugssumme × Prozentsatz (gerundet, die
    // Bezugssummen-Regeln prüfen die Domänentests); ein Titel mit
    // Pauschalsumme zählt mit ihr statt mit seinem Block. LV = Pauschalsumme
    // des LV oder die Summe; jeder Titel = Pauschalsumme oder Summe seines Blocks.
    let surcharges = lv.surcharge_values();
    let mut amounts = Vec::with_capacity(rows.len());
    for row in rows {
        let amount = match row.as_position() {
            Some(position) if position.is_surcharge() => {
                let value = surcharges.get(&position.id()).copied().unwrap_or_default();
                let expected = position
                    .unit_price()
                    .map(|percent| {
                        Money::total(
                            percent,
                            value.base.to_decimal() / proava_domain::Decimal::ONE_HUNDRED,
                        )
                    })
                    .unwrap_or_default();
                if value.amount != expected {
                    return Err(format!(
                        "Zuschlag {}: {:?} ≠ {:?} % von {:?}",
                        position.ordinal(),
                        value.amount,
                        position.unit_price(),
                        value.base
                    ));
                }
                if position.counts_in_total() {
                    value.amount
                } else {
                    Money::ZERO
                }
            }
            Some(position) => position.counted_total(),
            None => Money::ZERO,
        };
        amounts.push(amount);
    }
    // Sum of rows[from..to]; a title with a Pauschalsumme counts with it.
    let block_sum = |from: usize, to: usize| -> Money {
        let mut sum = Money::ZERO;
        let mut index = from;
        while index < to {
            if let Some(group) = rows[index].as_group() {
                if let Some(lump) = group.lump_sum_total() {
                    sum += lump;
                    let level = group.level();
                    index += 1;
                    while index < to
                        && !rows[index]
                            .as_group()
                            .is_some_and(|other| other.level() <= level)
                    {
                        index += 1;
                    }
                    continue;
                }
            }
            sum += amounts[index];
            index += 1;
        }
        sum
    };
    let net = lv
        .lump_sum_total()
        .unwrap_or_else(|| block_sum(0, rows.len()));
    if lv.net_total() != net {
        return Err(format!("Summe {:?} ≠ {:?}", lv.net_total(), net));
    }
    let totals = lv.group_totals();
    for total in totals {
        let start = total.group_row;
        let group = rows[start].as_group().unwrap();
        let level = group.level();
        let end = rows[start + 1..]
            .iter()
            .position(|row| row.as_group().is_some_and(|other| other.level() <= level))
            .map_or(rows.len(), |offset| start + 1 + offset);
        let sum = group
            .lump_sum_total()
            .unwrap_or_else(|| block_sum(start + 1, end));
        if sum != total.total {
            return Err(format!(
                "Titelsumme in Zeile {start}: {:?} ≠ {:?}",
                total.total, sum
            ));
        }
    }
    // Automatisch nummerierte LVs: jede OZ passt zur Maske.
    if lv.is_auto_numbered() {
        let locked = lv.locked_numbers();
        let mask = lv.oz_mask();
        for (index, row) in rows.iter().enumerate() {
            if locked.contains(&row.id()) || !row.has_ordinal() {
                continue;
            }
            // Under a title that keeps its number for a finished position,
            // the neighbours count on with that title's own prefix.
            let under_kept_title = proava_ui::lv_ops::ancestors(lv, index)
                .iter()
                .any(|id| locked.contains(id));
            if under_kept_title {
                continue;
            }
            // Neighbours of a finished position count on from its (kept)
            // number; positions without a title in a mask with title levels
            // cannot fit at all.
            let block_start = rows[..index].iter().rposition(LvRow::is_group);
            let block_end = rows[index..]
                .iter()
                .position(LvRow::is_group)
                .map_or(rows.len(), |offset| index + offset);
            let after_kept = rows[block_start.map_or(0, |start| start + 1)..block_end]
                .iter()
                .any(|other| locked.contains(&other.id()));
            if row.is_position() && (after_kept || (block_start.is_none() && lv.level_count() > 0))
            {
                continue;
            }
            let ordinal = row.ordinal();
            let fits = match row {
                LvRow::Group(_) => mask.fits_group(ordinal),
                LvRow::Position(_) => mask.fits_position(ordinal),
                _ => true,
            };
            if !fits && !ordinal.is_empty() {
                return Err(format!(
                    "OZ {ordinal} passt nicht zur Maske {}",
                    mask.pattern()
                ));
            }
        }
    }
    Ok(())
}

/// Everything that must hold for a project: every LV, the active LV, the
/// session keys, bids only pricing positions.
pub fn check_project(project: &Project) -> Result<(), String> {
    for (number, lv) in project.lvs().enumerate() {
        check_lv(lv).map_err(|error| format!("LV {number}: {error}"))?;
    }
    let active = project.active_lv();
    if active >= project.lv_count() {
        return Err(format!("aktives LV {active} gibt es nicht"));
    }
    let keys: HashSet<u64> = (0..project.lv_count())
        .filter_map(|index| project.lv_key(index))
        .collect();
    if keys.len() != project.lv_count() {
        return Err("LV-Schlüssel doppelt".to_string());
    }
    for index in 0..project.lv_count() {
        let lv = project.lv_at(index).unwrap();
        for bid in project.bids_of(index) {
            for id in bid.prices().keys() {
                // Prices of deleted positions may stay (undo); they must not
                // point to another kind of row.
                if lv.rows().iter().any(|row| row.id() == *id) && lv.position_by_id(*id).is_none() {
                    return Err(format!(
                        "Angebot bepreist Zeile {id:?}, die keine Position ist"
                    ));
                }
            }
        }
    }
    check_costs(project)?;
    check_correspondence(project)
}

/// Kostenermittlungen: eindeutige Ids (nicht 0), jede Zeile der
/// Anlagenliste mit genau so vielen Anzahlen wie DP-Arten (sonst ändert
/// Speichern und Laden das Projekt), Summen stimmen, eine Aufteilung mit
/// 100 % ergibt centgenau die Gesamtsumme.
pub fn check_costs(project: &Project) -> Result<(), String> {
    let mut ids = HashSet::new();
    for estimate in project.cost_estimates() {
        let name = estimate.display_name();
        if estimate.id.0 == 0 || !ids.insert(estimate.id) {
            return Err(format!(
                "Kostenermittlung {name}: Id {:?} doppelt oder 0",
                estimate.id
            ));
        }
        for (index, section) in estimate.sections.iter().enumerate() {
            for (number, row) in section.rows.iter().enumerate() {
                if row.counts.len() != estimate.kinds.len() {
                    return Err(format!(
                        "Kostenermittlung {name}, Bauteil {index}, Zeile {number}: {} Anzahlen bei {} DP-Arten",
                        row.counts.len(),
                        estimate.kinds.len()
                    ));
                }
            }
        }
        let result = estimate.result();
        let total: Money = result.sections.iter().map(|section| section.total).sum();
        if total != result.total {
            return Err(format!(
                "Kostenermittlung {name}: Summe der Bauteile {total:?} ≠ {:?}",
                result.total
            ));
        }
        for (index, section) in result.sections.iter().enumerate() {
            let full = estimate.stage == proava_domain::CostStage::Calculation
                && proava_domain::cost_estimate::percent_sum(estimate.shares_of(index))
                    == proava_domain::Decimal::ONE_HUNDRED;
            if full && !section.check.is_zero() {
                return Err(format!(
                    "Kostenermittlung {name}, Bauteil {index}: 100 % mit Prüfsumme {:?}",
                    section.check
                ));
            }
        }
    }
    Ok(())
}

/// Schriftverkehr im Projekt: eindeutige Ids und Dateinamen.
pub fn check_correspondence(project: &Project) -> Result<(), String> {
    let mut ids = HashSet::new();
    let mut files = HashSet::new();
    for entry in project.correspondence() {
        if !ids.insert(entry.id) {
            return Err(format!("Schriftverkehr: Id {:?} doppelt", entry.id));
        }
        if entry.file.trim().is_empty() || !files.insert(entry.file.clone()) {
            return Err(format!(
                "Schriftverkehr: Datei „{}“ leer oder doppelt",
                entry.file
            ));
        }
    }
    Ok(())
}

/// The LV as text for a failure message: mask, step, one line per row.
pub fn dump(lv: &Lv) -> String {
    let mut text = format!(
        "  Maske {} Schritt {} auto={}\n",
        lv.oz_mask().pattern(),
        lv.position_step(),
        lv.is_auto_numbered()
    );
    for (index, row) in lv.rows().iter().enumerate() {
        let kind = match row {
            LvRow::Group(group) => format!("Titel{}", group.level()),
            LvRow::Position(position) => format!("Pos {:?}", position.status()),
            _ => "Text".to_string(),
        };
        text.push_str(&format!("  {index:3} {kind:<16} {}\n", row.ordinal()));
    }
    text
}
