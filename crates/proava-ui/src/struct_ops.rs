//! Structure of an LV without UI: Untertitel and Ausführungsbeschreibungen
//! einfügen, Bezüge setzen, Ordnungszahl-Stufen, Gliederung (OZ-Maske)
//! wechseln, Kennzeichen für die Tabelle.

use proava_domain::{
    Decimal, Lv, LvGroup, LvRow, MaskSpec, OzMask, PerformanceDescription, Position, RichText,
    RowId, TextComplement,
};

use crate::lv_ops::NEW_TITLE_TEXT;

/// New heading one level below the selected title (or the title the selected
/// row belongs to), at the end of that title's block.
pub fn insert_subtitle(lv: &mut Lv, selected: Option<usize>) -> Result<usize, String> {
    let index = selected.ok_or("Bitte erst einen Titel wählen.")?;
    let parent = lv.rows()[..=index.min(lv.rows().len().saturating_sub(1))]
        .iter()
        .rposition(LvRow::is_group)
        .ok_or("Untertitel gehören unter einen Titel – erst einen Titel anlegen.")?;
    let level = lv.rows()[parent]
        .as_group()
        .map(LvGroup::level)
        .unwrap_or(0)
        + 1;
    if level >= lv.level_count() {
        return Err(format!(
            "Die Gliederung hat keine Stufe unter „{}“ – unter LV › Gliederung und OZ-Maske … eine Stufe ergänzen.",
            lv.level_label(level - 1)
        ));
    }
    let at = lv.block_range(parent).end;
    lv.insert_numbered(at, LvRow::Group(LvGroup::new(level, "", NEW_TITLE_TEXT)))?;
    Ok(at)
}

/// New title like [`crate::lv_ops::insert_title`]; in an LV with its own
/// numbers and no free number between the neighbours (481 | 482) the title
/// gets the number after the highest one on its level, and a hint.
pub fn insert_title(
    lv: &mut Lv,
    selected: Option<usize>,
) -> Result<(usize, Option<String>), String> {
    match crate::lv_ops::insert_title(lv, selected) {
        Ok(index) => Ok((index, None)),
        Err(error) if lv.is_auto_numbered() => Err(error),
        Err(error) => {
            let level = lv.level_for_new_group(selected);
            let at = lv.insert_group_index(selected, level);
            let parent_end = lv.rows()[..at]
                .iter()
                .rposition(|row| matches!(row, LvRow::Group(group) if group.level() < level));
            let siblings = match parent_end {
                Some(parent) => lv.block_range(parent),
                None => 0..lv.rows().len(),
            };
            let highest = lv.rows()[siblings]
                .iter()
                .filter_map(LvRow::as_group)
                .filter(|group| group.level() == level)
                .filter_map(|group| group.ordinal().rsplit('.').next()?.parse::<u32>().ok())
                .max()
                .ok_or(error.clone())?;
            let before = lv.clone();
            lv.insert_row(at, LvRow::Group(LvGroup::new(level, "", NEW_TITLE_TEXT)));
            match lv.set_row_number(at, &(highest + 1).to_string()) {
                Ok(()) => Ok((
                    at,
                    Some(format!(
                        "Hier ist keine Nummer frei – vorläufig {}; die OZ lässt sich im Bearbeitungsfenster ändern.",
                        lv.rows()[at].ordinal()
                    )),
                )),
                Err(_) => {
                    *lv = before;
                    Err(error)
                }
            }
        }
    }
}

/// New Ausführungsbeschreibung after the selection with the next free number.
pub fn insert_performance(lv: &mut Lv, selected: Option<usize>) -> Result<usize, String> {
    let at = lv.insert_index(selected);
    insert_performance_at(lv, at)
}

fn insert_performance_at(lv: &mut Lv, at: usize) -> Result<usize, String> {
    // Without a number: the LV gives it (1, 2, 3 … by place while they are
    // in order, otherwise the next free one).
    lv.insert_numbered(
        at,
        LvRow::Performance(PerformanceDescription::new("", "", RichText::default())),
    )?;
    Ok(at)
}

/// New Ausführungsbeschreibung for the given positions: it goes right under
/// the title of the first one (where descriptions usually stand) and all of
/// them refer to it. Returns its index.
pub fn insert_performance_for(lv: &mut Lv, positions: &[RowId]) -> Result<usize, String> {
    let first = positions
        .iter()
        .filter_map(|id| lv.index_of(*id))
        .filter(|index| lv.rows()[*index].is_position())
        .min()
        .ok_or("Bitte erst eine oder mehrere Positionen wählen.")?;
    let at = lv.rows()[..first]
        .iter()
        .rposition(LvRow::is_group)
        .map_or(0, |title| title + 1);
    let before = lv.clone();
    let index = insert_performance_at(lv, at)?;
    let id = lv.rows()[index].id();
    if let Err(error) = link_many(lv, positions, Some(id)) {
        *lv = before;
        return Err(error);
    }
    Ok(index)
}

/// Sets (or clears) the Ausführungsbeschreibung of every position among
/// `ids`; returns how many positions changed.
pub fn link_many(lv: &mut Lv, ids: &[RowId], performance: Option<RowId>) -> Result<usize, String> {
    let positions: Vec<RowId> = ids
        .iter()
        .copied()
        .filter(|id| {
            lv.position_by_id(*id)
                .is_some_and(|p| p.performance() != performance)
        })
        .collect();
    for id in &positions {
        lv.link_performance(*id, performance)?;
    }
    Ok(positions.len())
}

/// GAEB allows 1 to 9999 as number of an Ausführungsbeschreibung (PerfNo).
pub fn check_performance_number(text: &str) -> Result<String, String> {
    // „7“, „0007“, „A0007“ and „AB 0007“ all mean 7.
    let text = proava_domain::performance_number_input(text);
    match text.parse::<u32>() {
        Ok(number) if (1..=9999).contains(&number) && text.bytes().all(|b| b.is_ascii_digit()) => {
            Ok(number.to_string())
        }
        _ => Err(format!(
            "„{text}“ passt nicht: die Nummer ist eine Zahl von 1 bis 9999."
        )),
    }
}

/// GAEB: the label (PerfLbl) of an Ausführungsbeschreibung has at most 55 characters.
pub const PERFORMANCE_LABEL_LIMIT: usize = 55;

/// „A0003 – Kabeltrassen“ for every Ausführungsbeschreibung.
pub fn performance_choices(lv: &Lv) -> Vec<(RowId, String)> {
    lv.performances()
        .map(|performance| {
            let title = performance.title();
            let text = if title.is_empty() {
                performance.mark()
            } else {
                format!("{} – {title}", performance.mark())
            };
            (performance.id(), text)
        })
        .collect()
}

/// Positions a position may repeat (all others): „01.481.10.0010 – Text“.
pub fn repetition_choices(lv: &Lv, position: RowId) -> Vec<(RowId, String)> {
    lv.positions()
        .filter(|other| other.id() != position)
        .map(|other| {
            (
                other.id(),
                format!("{} – {}", other.ordinal(), other.short_text()),
            )
        })
        .collect()
}

/// The fixed part and the own part of a row's order number, e.g. a heading
/// „01.481“ → („01.“, „481“), a position „01.481.10.0010“ → („01.481.10.“, „0010“).
pub fn ordinal_stage(lv: &Lv, index: usize) -> (String, String) {
    let Some(row) = lv.rows().get(index) else {
        return (String::new(), String::new());
    };
    let ordinal = row.ordinal().trim().to_string();
    let parent = match row {
        LvRow::Group(group) => lv.rows()[..index]
            .iter()
            .rev()
            .filter_map(LvRow::as_group)
            .find(|other| other.level() < group.level())
            .map(|other| other.ordinal().trim().to_string()),
        LvRow::Position(_) => lv.rows()[..index]
            .iter()
            .rev()
            .find_map(LvRow::as_group)
            .map(|group| group.ordinal().trim().to_string()),
        _ => return (String::new(), String::new()),
    };
    match parent.filter(|parent| !parent.is_empty()) {
        Some(parent) => match ordinal.strip_prefix(&format!("{parent}.")) {
            Some(own) => (format!("{parent}."), own.to_string()),
            None => (String::new(), ordinal),
        },
        None => (String::new(), ordinal),
    }
}

/// Small marks for a position in the table: A0003, Wdh. 0010, TE, SP, Menge offen.
pub fn row_badges(lv: &Lv, position: &Position) -> Vec<String> {
    let mut badges = Vec::new();
    if let Some(performance) = lv.performance_of(position) {
        badges.push(performance.mark());
    }
    if let Some(base) = position.repeats().and_then(|id| lv.position_by_id(id)) {
        let ordinal = base.ordinal();
        let last = ordinal.rsplit('.').next().unwrap_or(ordinal);
        badges.push(format!("Wdh. {last}"));
    }
    if !position.complements().is_empty() {
        badges.push("TE".to_string());
    }
    if position.is_key_item() {
        badges.push("SP".to_string());
    }
    if position.is_quantity_open() {
        badges.push("Menge offen".to_string());
    }
    badges
}

// ---------- Weitere Angaben ----------

/// Empty → None, else a quantity with up to 3 decimals („0,75“).
pub fn parse_optional_quantity(text: &str) -> Result<Option<Decimal>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    crate::numbers::parse_quantity(text).map(Some)
}

/// Next free marker for a Textergänzung: „01“, „02“ …
pub fn next_complement_marker(complements: &[TextComplement]) -> String {
    proava_domain::complement_text::next_marker(complements)
}

/// Problems of a list of Textergänzungen: empty or doubled markers.
pub fn complement_problems(complements: &[TextComplement]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (index, complement) in complements.iter().enumerate() {
        let marker = complement.marker.trim();
        if marker.is_empty() {
            problems.push(format!("Textergänzung {}: Kennung fehlt.", index + 1));
        } else if !seen.insert(marker.trim_start_matches('0').to_string()) {
            problems.push(format!("Kennung {marker} kommt doppelt vor."));
        }
    }
    problems
}

// ---------- Ziehen ----------

/// Moves the gap-free rows `range` (e.g. several selected rows dragged with
/// the mouse) before row `target`. Finished positions refuse the move. An
/// automatically numbered LV is renumbered; otherwise moved positions take a
/// free number at their new place (finished ones keep theirs anyway).
/// Returns the range the rows now have.
pub fn move_rows_to(
    lv: &mut Lv,
    range: std::ops::Range<usize>,
    target: usize,
) -> Result<std::ops::Range<usize>, String> {
    let ids: Vec<RowId> = lv.rows()[range.clone()].iter().map(LvRow::id).collect();
    lv.ensure_editable(&ids)?;
    let before = lv.clone();
    let auto = lv.is_auto_numbered();
    let length = range.len();
    let start = lv
        .move_rows(range, target)
        .ok_or("Die Zeilen lassen sich nicht in sich selbst verschieben.")?;
    if auto {
        lv.renumber();
    } else {
        for index in start..start + length {
            if !lv.rows()[index].is_position() {
                continue;
            }
            // Probe on a copy: removing the row for real would drop
            // references (Wiederholung, Ausführungsbeschreibung) to it.
            // The moved rows after this one do not count yet (they get
            // their numbers next).
            let mut probe = lv.clone();
            for later in (index + 1..start + length).rev() {
                probe.remove_row(later);
            }
            let Some(row) = probe.remove_row(index) else {
                continue;
            };
            if let Some(ordinal) = probe.free_ordinal_at(index, &row) {
                if let Some(position) = lv.row_mut(index).and_then(LvRow::as_position_mut) {
                    position.set_ordinal(ordinal);
                }
            }
        }
        // A row moved earlier may have taken the number a later one keeps
        // for lack of a free one: then nothing moves.
        if lv.duplicate_ordinals().len() > before.duplicate_ordinals().len() {
            *lv = before;
            return Err(
                "An der neuen Stelle sind nicht genug Ordnungszahlen frei – \
                        erst neu nummerieren oder einzeln verschieben."
                    .to_string(),
            );
        }
    }
    Ok(start..start + length)
}

// ---------- Umnummerieren ----------

/// First order number of `before` (in its order) that is gone or different
/// in `after` – rows matched by id: Duplizieren, Einfügen with renumbering,
/// Löschen, Verschieben, Neu nummerieren, OZ by hand. None when every row of
/// `before` keeps its number (new rows with free numbers do not count).
pub fn first_moved_ordinal(before: &Lv, after: &Lv) -> Option<String> {
    let now: std::collections::HashMap<RowId, String> = after
        .rows()
        .iter()
        .filter(|row| row.has_ordinal())
        .map(|row| (row.id(), proava_domain::normalize_ordinal(row.ordinal())))
        .collect();
    before
        .rows()
        .iter()
        .filter(|row| row.has_ordinal() && !row.ordinal().trim().is_empty())
        .find(|row| now.get(&row.id()) != Some(&proava_domain::normalize_ordinal(row.ordinal())))
        .map(|row| row.ordinal().to_string())
}

/// Warnings before order numbers change: X83 already sent, bids read.
pub fn renumber_warnings(lv: &Lv, bids: usize) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(iso) = lv.request_exported() {
        warnings.push(format!(
            "Die Angebotsaufforderung (X83) wurde am {} {} exportiert – mit neuen Ordnungszahlen passen \
             die Rückläufer der Bieter nicht mehr zu diesem LV.",
            crate::format::iso_to_german_date(iso),
            iso.get(11..16).unwrap_or_default()
        ));
    }
    if bids > 0 {
        warnings.push(format!(
            "{bids} Angebot(e) bleiben zugeordnet, verschickte GAEB-Dateien tragen aber die alten Ordnungszahlen."
        ));
    }
    warnings
}

/// The title a renumbering of a part may be limited to: the selected title
/// or the title the selected row belongs to.
pub fn renumber_scope(lv: &Lv, selected: Option<usize>) -> Option<RowId> {
    let index = selected?;
    lv.rows()[..=index.min(lv.rows().len().checked_sub(1)?)]
        .iter()
        .rev()
        .find(|row| row.is_group())
        .map(LvRow::id)
}

// ---------- Gliederung wechseln ----------

/// What happens to the order numbers when the Gliederung changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskNumbering {
    /// Alle Titel und Positionen neu nummerieren.
    All,
    /// Titelnummern (481 …) behalten, nur auf die neuen Stellen auffüllen;
    /// Positionen neu nummerieren.
    KeepTitles,
    /// Nummern unverändert lassen (die Prüfung meldet, was nicht passt).
    Unchanged,
}

/// Effects of a new mask on an LV, for the warning in the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskImpact {
    pub same_mask: bool,
    /// Rows with an order number.
    pub numbered_rows: usize,
    /// The LV has its own numbers (not what renumbering would write).
    pub own_numbers: bool,
    /// Titles on levels the new mask does not have; they move up to its
    /// deepest level (nothing is deleted).
    pub lifted_titles: usize,
    /// Whether the title numbers fit the new mask (for [`MaskNumbering::KeepTitles`]).
    pub keep_titles: Result<(), String>,
    /// The new mask has no levels while the LV has titles.
    pub blocked: Option<String>,
}

impl MaskImpact {
    pub fn suggested(&self) -> MaskNumbering {
        if self.same_mask && !self.own_numbers {
            MaskNumbering::All
        } else if self.own_numbers && self.keep_titles.is_ok() {
            MaskNumbering::KeepTitles
        } else {
            MaskNumbering::All
        }
    }
}

pub fn mask_impact(lv: &Lv, mask: &OzMask) -> MaskImpact {
    let count = mask.level_count();
    let titles = lv.group_count();
    let lifted_titles = lv
        .rows()
        .iter()
        .filter_map(LvRow::as_group)
        .filter(|group| group.level() >= count)
        .count();
    let blocked = (count == 0 && titles > 0).then(|| {
        format!("Eine Gliederung ohne Titelstufen passt nicht zu diesem LV mit {titles} Titeln.")
    });
    MaskImpact {
        same_mask: mask.pattern() == lv.oz_mask().pattern(),
        numbered_rows: lv.rows().iter().filter(|row| row.has_ordinal()).count(),
        own_numbers: !lv.is_auto_numbered(),
        lifted_titles,
        keep_titles: refit_titles(lv, mask).map(|_| ()),
        blocked,
    }
}

/// New order numbers of all headings for `mask`, with their own pieces kept
/// (481 stays 481, „1“ becomes „01“). Fails when a piece does not fit.
fn refit_titles(lv: &Lv, mask: &OzMask) -> Result<Vec<(usize, String)>, String> {
    let count = mask.level_count();
    let mut result = Vec::new();
    for (index, row) in lv.rows().iter().enumerate() {
        let Some(group) = row.as_group() else {
            continue;
        };
        let ordinal = group.ordinal().trim();
        let level = group.level().min(count.saturating_sub(1));
        let pieces: Vec<&str> = ordinal
            .split('.')
            .filter(|piece| !piece.is_empty())
            .collect();
        if pieces.len() != level + 1 {
            return Err(format!(
                "Titel „{ordinal}“ hat nicht {} Stufe(n) – Titelnummern lassen sich nicht übernehmen.",
                level + 1
            ));
        }
        let mut fitted = Vec::new();
        for (piece_level, piece) in pieces.iter().enumerate() {
            let width = mask.level_width(piece_level);
            let letters = mask.level_is_letters(piece_level);
            let core = if letters {
                piece.to_string()
            } else {
                let trimmed = piece.trim_start_matches('0');
                if trimmed.is_empty() { "0" } else { trimmed }.to_string()
            };
            let kind_ok = if letters {
                core.chars()
                    .all(|character| character.is_ascii_alphabetic())
            } else {
                core.chars().all(|character| character.is_ascii_digit())
            };
            if !kind_ok || core.len() > width {
                return Err(format!(
                    "Titel „{ordinal}“: „{piece}“ passt nicht in die {}. Stufe ({width} {}).",
                    piece_level + 1,
                    if letters { "Buchstaben" } else { "Stellen" }
                ));
            }
            fitted.push(if letters {
                core.to_ascii_uppercase()
            } else {
                format!("{core:0>width$}")
            });
        }
        result.push((index, fitted.join(".")));
    }
    // Two titles must not end up with the same number („1“ and „01“).
    let mut seen = std::collections::HashSet::new();
    for (_, ordinal) in &result {
        if !seen.insert(ordinal.clone()) {
            return Err(format!(
                "Titelnummern lassen sich nicht übernehmen: „{ordinal}“ käme doppelt vor."
            ));
        }
    }
    Ok(result)
}

/// Applies a new Gliederung (mask, labels, step) and numbers the LV as chosen.
/// Nothing changes when it fails. Returns a short message.
pub fn apply_mask(
    lv: &mut Lv,
    spec: &MaskSpec,
    step: u32,
    numbering: MaskNumbering,
) -> Result<String, String> {
    let mask = spec.to_mask()?;
    if let Some(problem) = mask.step_problem(step) {
        return Err(problem);
    }
    let impact = mask_impact(lv, &mask);
    if let Some(blocked) = impact.blocked {
        return Err(blocked);
    }
    let titles = match numbering {
        MaskNumbering::KeepTitles => Some(refit_titles(lv, &mask)?),
        _ => None,
    };
    let deepest = mask.level_count().saturating_sub(1);
    let pattern = mask.pattern().to_string();
    let mut trial = lv.clone();
    trial.set_oz_mask(mask);
    trial.set_level_labels(spec.labels());
    trial.set_position_step(step);
    for index in 0..trial.rows().len() {
        if let Some(LvRow::Group(group)) = trial.row_mut(index) {
            if group.level() > deepest {
                group.set_level(deepest);
            }
        }
    }
    let lifted = if impact.lifted_titles > 0 {
        format!(
            " {} Titel sind auf die unterste Stufe gerückt.",
            impact.lifted_titles
        )
    } else {
        String::new()
    };
    let message = match (numbering, titles) {
        (MaskNumbering::KeepTitles, Some(titles)) => {
            for (index, ordinal) in titles {
                if let Some(LvRow::Group(group)) = trial.row_mut(index) {
                    group.set_ordinal(ordinal);
                }
            }
            trial.renumber_positions();
            format!(
                "Gliederung übernommen, Titelnummern behalten, Positionen neu nummeriert.{lifted}"
            )
        }
        (MaskNumbering::Unchanged, _) => {
            format!("Gliederung übernommen, Nummern unverändert – Prüfen › LV prüfen … zeigt, was nicht passt.{lifted}")
        }
        _ => {
            trial.renumber();
            format!("Gliederung übernommen, alles neu nummeriert.{lifted}")
        }
    };
    // Renumbering into a mask with too few digits (ten titles on a level
    // with one digit, 100 positions with „PP“) would leave rows without a
    // number: then nothing changes.
    let missing = unnumbered_rows(&trial).saturating_sub(unnumbered_rows(lv));
    if numbering != MaskNumbering::Unchanged && missing > 0 {
        return Err(format!(
            "Die Gliederung „{pattern}“ hat zu wenige Stellen für dieses LV – {} keine Ordnungszahl. \
             Bitte mehr Stellen vorsehen.",
            if missing == 1 {
                "1 Zeile bekäme".to_string()
            } else {
                format!("{missing} Zeilen bekämen")
            }
        ));
    }
    *lv = trial;
    Ok(message)
}

/// Rows that should have an order number but have none.
fn unnumbered_rows(lv: &Lv) -> usize {
    lv.rows()
        .iter()
        .filter(|row| row.has_ordinal() && row.ordinal().trim().is_empty())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{ComplementKind, LevelSpec};

    fn sample_lv() -> Lv {
        let mut lv = Lv::new("GA", OzMask::parse("11.222.33.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Raumautomation"));
        lv.push_group(LvGroup::new(1, "01.481", "Automationseinrichtungen"));
        lv.push_group(LvGroup::new(2, "01.481.10", "AS"));
        lv.push_position(Position::new(
            "01.481.10.0010",
            "DDC",
            Decimal::ONE,
            "St",
            None,
        ));
        lv.push_position(Position::new(
            "01.481.10.0020",
            "Modul",
            Decimal::ONE,
            "St",
            None,
        ));
        lv.push_group(LvGroup::new(1, "01.482", "Schaltschränke"));
        lv.push_group(LvGroup::new(2, "01.482.10", "ISP"));
        lv.push_position(Position::new(
            "01.482.10.0010",
            "Schrank",
            Decimal::ONE,
            "St",
            None,
        ));
        lv
    }

    fn ordinals(lv: &Lv) -> Vec<String> {
        lv.rows()
            .iter()
            .map(|row| row.ordinal().to_string())
            .collect()
    }

    fn spec(lv: &Lv) -> MaskSpec {
        MaskSpec::of(lv.oz_mask(), lv.level_labels())
    }

    #[test]
    fn subtitle_goes_below_the_title_with_the_next_free_number() {
        let mut lv = sample_lv();
        let at = insert_subtitle(&mut lv, Some(1)).unwrap();
        assert_eq!(at, 5);
        assert_eq!(lv.rows()[at].as_group().unwrap().level(), 2);
        assert_eq!(lv.rows()[at].ordinal(), "01.481.11");
        // from a position below a title of the top level: below that title
        let at = insert_subtitle(&mut lv, Some(0)).unwrap();
        assert_eq!(lv.rows()[at].ordinal(), "01.483", "{:?}", ordinals(&lv));
        assert!(insert_subtitle(&mut lv, None).is_err());
        // below the deepest level (title 01.481.10 or one of its positions)
        let error = insert_subtitle(&mut lv, Some(3)).unwrap_err();
        assert!(error.contains("keine Stufe unter „Abschnitt“"), "{error}");
        let mut flat = Lv::new("x", OzMask::parse("11.PPPP").unwrap());
        flat.push_position(Position::new("", "p", Decimal::ONE, "St", None));
        assert!(insert_subtitle(&mut flat, Some(0)).is_err());
    }

    #[test]
    fn new_title_in_own_numbering_gets_the_next_free_number() {
        let mut lv = sample_lv();
        // after 482 (last on its level): 483
        let (at, hint) = insert_title(&mut lv, Some(5)).unwrap();
        assert_eq!(lv.rows()[at].ordinal(), "01.483", "{:?}", ordinals(&lv));
        assert!(hint.is_none());
        lv.set_row_number(at, "489").unwrap();
        // between 482 and 489: 483 again
        let (at, _) = insert_title(&mut lv, Some(5)).unwrap();
        assert_eq!(lv.rows()[at].ordinal(), "01.483");
        // between 481 and 482 nothing is free: after the highest, with a hint
        let (at, hint) = insert_title(&mut lv, Some(1)).unwrap();
        assert_eq!(lv.rows()[at].ordinal(), "01.490");
        assert!(hint.unwrap().contains("01.490"));
        assert_eq!(lv.rows()[at - 1].ordinal(), "01.481.10.0020");
        assert!(lv.duplicate_ordinals().is_empty());
    }

    #[test]
    fn performance_descriptions_are_inserted_numbered_and_linked() {
        let mut lv = sample_lv();
        let at = insert_performance(&mut lv, Some(2)).unwrap();
        assert_eq!(at, 3);
        let first = lv.rows()[at].id();
        assert_eq!(lv.performance_by_id(first).unwrap().number(), "1");
        let at = insert_performance(&mut lv, Some(at)).unwrap();
        assert_eq!(lv.rows()[at].as_performance().unwrap().number(), "2");
        assert_eq!(lv.rows()[5].ordinal(), "01.481.10.0010", "numbers stay");
        let positions: Vec<RowId> = lv.positions().map(Position::id).collect();
        let mut ids = positions.clone();
        ids.push(lv.rows()[0].id());
        assert_eq!(link_many(&mut lv, &ids, Some(first)).unwrap(), 3);
        assert_eq!(link_many(&mut lv, &ids, Some(first)).unwrap(), 0);
        assert_eq!(lv.positions_using(first), positions);
        let position = lv.position_by_id(positions[0]).unwrap();
        assert_eq!(row_badges(&lv, position), vec!["A0001"]);
        let choices = performance_choices(&lv);
        assert_eq!(choices[0].1, "A0001");
        lv.performance_mut(first).unwrap().set_label("Trassen");
        assert_eq!(performance_choices(&lv)[0].1, "A0001 – Trassen");
        assert_eq!(link_many(&mut lv, &positions[..1], None).unwrap(), 1);
        let (_, cleared) = lv.remove_performance(first).unwrap();
        assert_eq!(cleared, 2);
    }

    #[test]
    fn new_description_for_selected_positions_goes_under_their_title() {
        let mut lv = sample_lv();
        let ids: Vec<RowId> = lv.positions().map(Position::id).collect();
        let index = insert_performance_for(&mut lv, &ids[..2]).unwrap();
        assert_eq!(index, 3, "right under 01.481.10");
        let id = lv.rows()[index].id();
        assert_eq!(lv.positions_using(id), ids[..2].to_vec());
        let title = lv.rows()[0].id();
        assert!(insert_performance_for(&mut lv, &[title]).is_err());
        let mut flat = Lv::new("x", OzMask::parse("PPPP").unwrap());
        let only = flat.push_position(Position::new("0010", "p", Decimal::ONE, "St", None));
        assert_eq!(insert_performance_for(&mut flat, &[only]).unwrap(), 0);
    }

    #[test]
    fn renumber_warnings_name_the_x83_and_bids() {
        let mut lv = sample_lv();
        assert!(renumber_warnings(&lv, 0).is_empty());
        lv.set_request_exported(Some("2026-11-17T20:15:00".into()));
        let warnings = renumber_warnings(&lv, 2);
        assert!(warnings[0].contains("17.11.2026 20:15"), "{warnings:?}");
        assert!(warnings[1].contains("2 Angebot"));
        assert_eq!(renumber_scope(&lv, Some(4)), Some(lv.rows()[2].id()));
        assert_eq!(renumber_scope(&lv, Some(1)), Some(lv.rows()[1].id()));
        assert_eq!(renumber_scope(&lv, None), None);
    }

    #[test]
    fn several_rows_move_together() {
        let mut lv = sample_lv();
        // own numbers: 01.481.10.0010/0020 move below 01.482.10.0010
        let base = lv.rows()[3].id();
        let copy = lv.rows()[4].id();
        lv.link_repetition(copy, Some(base)).unwrap();
        let end = lv.rows().len();
        let moved = move_rows_to(&mut lv, 3..5, end).unwrap();
        assert_eq!(moved, 6..8);
        assert_eq!(lv.rows()[6].ordinal(), "01.482.10.0020");
        assert_eq!(lv.rows()[7].ordinal(), "01.482.10.0030");
        assert_eq!(
            lv.position_by_id(copy).unwrap().repeats(),
            Some(base),
            "references stay"
        );
        assert!(lv.duplicate_ordinals().is_empty());
        assert!(move_rows_to(&mut lv, 6..8, 7).is_err(), "not into itself");
        // finished rows refuse
        lv.set_status(&[base], proava_domain::EditStatus::Done);
        let before = lv.clone();
        assert!(move_rows_to(&mut lv, 6..8, 3)
            .unwrap_err()
            .contains("fertig"));
        assert_eq!(lv, before);
        // auto numbered: renumbered
        let mut auto = Lv::new("A", OzMask::parse("11.PPPP").unwrap());
        auto.push_group(LvGroup::new(0, "", "A"));
        for text in ["a", "b", "c"] {
            auto.push_position(Position::new("", text, Decimal::ONE, "St", None));
        }
        auto.renumber();
        move_rows_to(&mut auto, 1..3, 4).unwrap();
        let texts: Vec<String> = auto.rows().iter().map(|row| row.short_text()).collect();
        assert_eq!(texts, ["A", "c", "a", "b"]);
        assert_eq!(auto.rows()[1].ordinal(), "01.0010");
    }

    #[test]
    fn performance_numbers_follow_gaeb() {
        assert_eq!(check_performance_number(" 007 ").unwrap(), "7");
        assert!(check_performance_number("0").is_err());
        assert!(check_performance_number("10000").is_err());
        assert!(check_performance_number("3a").is_err());
        assert!(check_performance_number("-1").is_err());
    }

    #[test]
    fn ordinal_stage_splits_the_fixed_part() {
        let lv = sample_lv();
        assert_eq!(ordinal_stage(&lv, 0), ("".into(), "01".into()));
        assert_eq!(ordinal_stage(&lv, 1), ("01.".into(), "481".into()));
        assert_eq!(ordinal_stage(&lv, 3), ("01.481.10.".into(), "0010".into()));
    }

    #[test]
    fn badges_show_repetition_complements_and_key_items() {
        let mut lv = sample_lv();
        let ids: Vec<RowId> = lv.positions().map(Position::id).collect();
        lv.link_repetition(ids[1], Some(ids[0])).unwrap();
        let index = lv.index_of(ids[1]).unwrap();
        let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
        position.set_key_item(true);
        position.set_quantity_open(true);
        position.set_complements(vec![TextComplement {
            marker: "01".into(),
            kind: ComplementKind::Bidder,
            caption: "Fabrikat:".into(),
            ..TextComplement::default()
        }]);
        let position = lv.position_by_id(ids[1]).unwrap();
        assert_eq!(
            row_badges(&lv, position),
            vec!["Wdh. 0010", "TE", "SP", "Menge offen"]
        );
        assert_eq!(repetition_choices(&lv, ids[0]).len(), 2);
        assert_eq!(
            repetition_choices(&lv, ids[0])[0].1,
            "01.481.10.0020 – Modul"
        );
    }

    #[test]
    fn complements_get_free_markers_and_are_checked() {
        let mut list = vec![TextComplement {
            marker: "01".into(),
            ..TextComplement::default()
        }];
        assert_eq!(next_complement_marker(&list), "02");
        assert_eq!(next_complement_marker(&[]), "01");
        assert!(complement_problems(&list).is_empty());
        list.push(TextComplement {
            marker: "1".into(),
            ..TextComplement::default()
        });
        list.push(TextComplement::default());
        let problems = complement_problems(&list);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert_eq!(parse_optional_quantity(" ").unwrap(), None);
        assert_eq!(
            parse_optional_quantity("0,75").unwrap(),
            Some("0.75".parse().unwrap())
        );
        assert!(parse_optional_quantity("x").is_err());
    }

    #[test]
    fn wider_mask_keeps_title_numbers() {
        let mut lv = sample_lv();
        let mut spec = spec(&lv);
        spec.levels[1].label = "Kostengruppe".into();
        spec.levels[2].width = 3;
        let impact = mask_impact(&lv, &spec.to_mask().unwrap());
        assert!(impact.own_numbers);
        assert!(impact.keep_titles.is_ok());
        assert_eq!(impact.suggested(), MaskNumbering::KeepTitles);
        apply_mask(&mut lv, &spec, 10, MaskNumbering::KeepTitles).unwrap();
        assert_eq!(lv.oz_mask().pattern(), "11.222.333.PPPP");
        assert_eq!(lv.level_label(1), "Kostengruppe");
        assert_eq!(lv.rows()[1].ordinal(), "01.481");
        assert_eq!(lv.rows()[3].ordinal(), "01.481.010.0010");
    }

    #[test]
    fn narrower_mask_cannot_keep_481_and_changes_nothing_on_error() {
        let mut lv = sample_lv();
        let mut spec = spec(&lv);
        spec.levels[1].width = 2;
        let impact = mask_impact(&lv, &spec.to_mask().unwrap());
        assert!(impact.keep_titles.is_err());
        assert_eq!(impact.suggested(), MaskNumbering::All);
        let before = lv.clone();
        assert!(apply_mask(&mut lv, &spec, 10, MaskNumbering::KeepTitles).is_err());
        assert_eq!(lv, before);
        apply_mask(&mut lv, &spec, 10, MaskNumbering::All).unwrap();
        assert_eq!(lv.rows()[1].ordinal(), "01.01");
    }

    #[test]
    fn fewer_levels_lift_titles_instead_of_deleting_them() {
        let mut lv = sample_lv();
        let mut spec = spec(&lv);
        spec.levels.pop();
        let impact = mask_impact(&lv, &spec.to_mask().unwrap());
        assert_eq!(impact.lifted_titles, 2);
        let rows = lv.rows().len();
        let message = apply_mask(&mut lv, &spec, 10, MaskNumbering::All).unwrap();
        assert!(message.contains("2 Titel"), "{message}");
        assert_eq!(lv.rows().len(), rows);
        assert_eq!(lv.rows()[2].as_group().unwrap().level(), 1);
        assert!(lv.is_auto_numbered());
        assert!(lv.duplicate_ordinals().is_empty());

        let mut lv = sample_lv();
        let none = MaskSpec {
            levels: Vec::new(),
            position_width: 4,
            index_width: 0,
        };
        assert!(mask_impact(&lv, &none.to_mask().unwrap()).blocked.is_some());
        assert!(apply_mask(&mut lv, &none, 10, MaskNumbering::All).is_err());
        let bad = MaskSpec {
            levels: vec![LevelSpec::digits("Titel", 9)],
            position_width: 4,
            index_width: 0,
        };
        assert!(apply_mask(&mut lv, &bad, 10, MaskNumbering::All).is_err());
    }

    #[test]
    fn only_labels_and_step_keep_the_numbers_when_unchanged() {
        let mut lv = sample_lv();
        let mut spec = spec(&lv);
        spec.levels[0].label = "Los".into();
        let before = ordinals(&lv);
        apply_mask(&mut lv, &spec, 100, MaskNumbering::Unchanged).unwrap();
        assert_eq!(ordinals(&lv), before);
        assert!(lv.has_lot());
        assert_eq!(lv.position_step(), 100);
    }

    /// Gefunden mit den Eigenschaftstests: die Gliederung „1.PPP“ mit
    /// Schrittweite 1000 hinterließ OZ, die nicht zur Maske passen.
    #[test]
    fn mask_with_a_step_that_does_not_fit_is_refused() {
        let mut lv = sample_lv();
        let before = lv.clone();
        let spec = MaskSpec::of(&OzMask::parse("1.PPP").unwrap(), &[]);
        let error = apply_mask(&mut lv, &spec, 1000, MaskNumbering::All).unwrap_err();
        assert!(error.contains("höchstens 100"), "{error}");
        assert_eq!(lv, before, "nichts geändert");
        assert!(apply_mask(&mut lv, &spec, 100, MaskNumbering::All).is_ok());
    }

    /// Gefunden mit den Eigenschaftstests: beim Ziehen mehrerer Zeilen in
    /// einem LV mit eigenen OZ nahm die erste die Nummer, die die zweite
    /// mangels freier Nummer behielt – doppelte OZ.
    #[test]
    fn moving_rows_never_leaves_duplicate_numbers() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "A"));
        for ordinal in ["01.0001", "01.0002", "01.0003", "01.0004"] {
            lv.push_position(Position::new(ordinal, "P", Decimal::ONE, "St", None));
        }
        assert!(!lv.is_auto_numbered());
        // 0003 and 0004 before 0002: between 0001 and 0002 nothing is free.
        match move_rows_to(&mut lv, 3..5, 2) {
            Ok(_) => assert!(
                lv.duplicate_ordinals().is_empty(),
                "{:?}",
                lv.duplicate_ordinals()
            ),
            Err(error) => assert!(error.contains("nicht genug Ordnungszahlen"), "{error}"),
        }
    }

    /// Gefunden mit den Eigenschaftstests (Saat 7): der Wechsel auf eine
    /// Gliederung mit einer Titelstelle bei mehr als neun Titeln ließ Titel
    /// „10“ und alte Nummern stehen; jetzt wird er abgelehnt. Ein zehnter
    /// Titel lässt sich in einer vollen Stufe nicht einfügen.
    #[test]
    fn mask_with_too_few_digits_for_the_titles_is_refused() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        for _ in 0..10 {
            insert_title(&mut lv, None).unwrap();
        }
        let before = lv.clone();
        let spec = MaskSpec::of(&OzMask::parse("1.PPP").unwrap(), &[]);
        let error = apply_mask(&mut lv, &spec, 1, MaskNumbering::All).unwrap_err();
        assert!(error.contains("zu wenige Stellen"), "{error}");
        assert_eq!(lv, before, "nichts geändert");

        let mut lv = Lv::new("LV", OzMask::parse("1.PPP").unwrap());
        for _ in 0..9 {
            insert_title(&mut lv, None).unwrap();
        }
        assert!(insert_title(&mut lv, None).is_err());
        assert_eq!(lv.rows().last().unwrap().ordinal(), "9");
    }

    /// Gefunden mit den Eigenschaftstests: „Titelnummern behalten“ machte aus
    /// „1“ und „01“ zweimal „01“.
    #[test]
    fn keeping_titles_refuses_numbers_that_would_clash() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "1", "alt"));
        lv.push_group(LvGroup::new(0, "01", "neu"));
        let spec = MaskSpec::of(&OzMask::parse("11.22.PPPP").unwrap(), &[]);
        assert!(mask_impact(&lv, &spec.to_mask().unwrap())
            .keep_titles
            .is_err());
        let before = lv.clone();
        assert!(apply_mask(&mut lv, &spec, 10, MaskNumbering::KeepTitles).is_err());
        assert_eq!(lv, before);
    }
}
