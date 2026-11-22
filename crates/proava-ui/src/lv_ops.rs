//! Editing commands on the LV without any UI: what a key or menu entry does.
//! Every operation renumbers and returns the index of the row to select.

use std::collections::HashSet;

use eframe::egui::{Key, Modifiers};
use proava_domain::{Decimal, EditStatus, Lv, LvGroup, LvRow, Note, Position, RowId, Spacer};

use crate::status_view::StatusFilter;

pub const NEW_TITLE_TEXT: &str = "";

/// Everything the LV editor can do from the keyboard, menus and context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    SelectUp,
    SelectDown,
    ExtendUp,
    ExtendDown,
    PageUp,
    PageDown,
    First,
    Last,
    NewPosition,
    NewPositionBefore,
    NewTitle,
    NewNote,
    /// Abstandshalter: an empty line for editing only (no OZ, text or price, not printed).
    NewSpacer,
    /// Datenpunkt-Abschnitt (KG 480) in a Kosten-LV (GU-Fall).
    NewPointSection,
    /// Aufteilungszeilen (KG 481–489 with the split of the
    /// Datenpunktrechnung) on the positions before them – only in a Kosten-LV.
    NewSplit,
    /// The GA as Kurztext: title, Datenpunkte AI … BV and the Aufteilung.
    NewPointsTemplate,
    /// The selected Datenpunkt-Abschnitt becomes Kurztext positions with the
    /// Aufteilung (per Bauteil).
    PointSectionToKurztext,
    /// The selected hints without text become Abstandshalter.
    NoteToSpacer,
    /// Every hint without text of the LV becomes an Abstandshalter.
    AllBlankNotesToSpacers,
    /// Neue Position aus STLB-Bau: the take-over dialog, below the selected
    /// row (a selected placeholder from a project or GAEB file is filled).
    FromStlb,
    /// Take manufacturer texts from AUSSCHREIBEN.DE (GAEB export in the browser).
    FromAusschreiben,
    Delete,
    MoveUp,
    MoveDown,
    Deeper,
    Higher,
    Duplicate,
    Copy,
    Cut,
    Paste,
    /// Marks all rows of the LV (for Strg+C of everything).
    SelectAll,
    Undo,
    Redo,
    Find,
    FindNext,
    EditShort,
    EditLong,
    /// Window „Eigenschaften“ of the selected row (double click, Alt+Enter).
    Properties,
    Collapse,
    Expand,
    CollapseAll,
    ExpandAll,
    Renumber,
    Palette,
    /// Status of the selected positions (titles: all positions below).
    SetStatus(EditStatus),
    /// fertig ↔ offen for the selection.
    ToggleDone,
    /// GP-Prüfung of the import answered: the computed GP stays, the yellow
    /// mark goes (selection; titles: all positions below).
    ConfirmPriceCheck,
    FindReplace,
    /// Rechtschreibung des ganzen LV prüfen (F7).
    SpellCheck,
    /// Jumps to the next position without quantity.
    NextMissingQuantity,
    Filter(StatusFilter),
    /// Yellow marks for a missing unit or short text on/off.
    ToggleGapMarks,
    /// Window with the open work (Offene Punkte).
    OpenPoints,
    /// Window „Kosten nach Kostengruppen“.
    CostGroups,
    /// Table column „KG“ on/off.
    ToggleCostGroupColumn,
    /// Table column „LB“ (Leistungsbereich) on/off.
    ToggleWorkCategoryColumn,
}

impl Command {
    pub fn label(self) -> &'static str {
        match self {
            Command::SelectUp => "Zeile darüber",
            Command::SelectDown => "Zeile darunter",
            Command::ExtendUp => "Auswahl nach oben erweitern",
            Command::ExtendDown => "Auswahl nach unten erweitern",
            Command::PageUp => "Seite hoch",
            Command::PageDown => "Seite runter",
            Command::First => "Erste Zeile",
            Command::Last => "Letzte Zeile",
            // Same names in the menu Einfügen and the right click (Anwender 18.11.).
            Command::NewPosition => "Neue Position darunter",
            Command::NewPositionBefore => "Neue Position davor",
            Command::NewTitle => "Neuer Titel",
            Command::NewNote => "Neuer Hinweis darunter",
            Command::NewSpacer => "Neuer Abstandshalter darunter (Leerzeile)",
            Command::NewPointSection => "Datenpunkt-Abschnitt einfügen (KG 480)",
            Command::NewSplit => "Aufteilung nach KG einfügen",
            Command::NewPointsTemplate => "GA nach Datenpunkten (Kurztext-Vorlage) einfügen",
            Command::PointSectionToKurztext => {
                "Datenpunkt-Abschnitt in Kurztext-Positionen umwandeln"
            }
            Command::NoteToSpacer => "In Abstandshalter umwandeln",
            Command::AllBlankNotesToSpacers => "Leere Hinweise in Abstandshalter umwandeln",
            Command::FromStlb => "Neue Position aus STLB-Bau …",
            Command::FromAusschreiben => "Neue Position aus AUSSCHREIBEN.DE …",
            Command::Delete => "Löschen",
            Command::MoveUp => "Nach oben schieben",
            Command::MoveDown => "Nach unten schieben",
            Command::Deeper => "Titel eine Stufe tiefer",
            Command::Higher => "Titel eine Stufe höher",
            Command::Duplicate => "Duplizieren",
            Command::Copy => "Kopieren",
            Command::SelectAll => "Alle Zeilen markieren",
            Command::Cut => "Ausschneiden",
            Command::Paste => "Einfügen",
            Command::Undo => "Rückgängig",
            Command::Redo => "Wiederholen",
            Command::Find => "Suchen",
            Command::FindNext => "Weitersuchen",
            Command::EditShort => "Kurztext bearbeiten",
            Command::EditLong => "Langtext im eigenen Fenster bearbeiten …",
            Command::Properties => "Eigenschaften im eigenen Fenster …",
            Command::Collapse => "Titel zuklappen",
            Command::Expand => "Titel aufklappen",
            Command::CollapseAll => "Alle Titel zuklappen",
            Command::ExpandAll => "Alle Titel aufklappen",
            Command::Renumber => "Neu nummerieren …",
            Command::Palette => "Befehl suchen",
            Command::SetStatus(EditStatus::Done) => "Status: fertig",
            Command::SetStatus(EditStatus::InProgress) => "Status: in Bearbeitung",
            Command::SetStatus(EditStatus::Open) => "Status: offen",
            Command::ToggleDone => "Fertig/offen umschalten",
            Command::ConfirmPriceCheck => "GP-Abweichung bestätigen",
            Command::FindReplace => "Suchen und Ersetzen",
            Command::SpellCheck => "Rechtschreibung prüfen …",
            Command::NextMissingQuantity => "Leere Mengen finden",
            Command::Filter(filter) => filter.label(),
            Command::ToggleGapMarks => "Fehlende Angaben markieren (rot/orange) ein/aus",
            Command::OpenPoints => "Offene Punkte …",
            Command::CostGroups => "Kosten nach Kostengruppen …",
            Command::ToggleCostGroupColumn => "Spalte KG (Kostengruppe) ein/aus",
            Command::ToggleWorkCategoryColumn => "Spalte LB (Leistungsbereich) ein/aus",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Command::SelectUp => "↑",
            Command::SelectDown => "↓",
            Command::ExtendUp => "Umschalt+↑",
            Command::ExtendDown => "Umschalt+↓",
            Command::PageUp => "Bild↑",
            Command::PageDown => "Bild↓",
            Command::First => "Pos1",
            Command::Last => "Ende",
            // Function keys first (Anwender 18.11.): F5–F8 fetch and check
            // texts, F9–F12 new rows; the Strg keys stay as well.
            Command::NewPosition => "F9",
            Command::NewPositionBefore => "Umschalt+F9",
            Command::NewTitle => "F12",
            Command::NewNote => "F10",
            Command::NewSpacer => "Strg+L",
            Command::NewPointSection
            | Command::NewSplit
            | Command::NewPointsTemplate
            | Command::PointSectionToKurztext => "",
            Command::NoteToSpacer | Command::AllBlankNotesToSpacers => "",
            Command::FromStlb => "F5",
            Command::FromAusschreiben => "F6",
            Command::Delete => "Entf",
            Command::MoveUp => "Alt+↑",
            Command::MoveDown => "Alt+↓",
            Command::Deeper => "Tab",
            Command::Higher => "Umschalt+Tab",
            Command::Duplicate => "Strg+D",
            Command::Copy => "Strg+C",
            Command::SelectAll => "Strg+A",
            Command::Cut => "Strg+X",
            Command::Paste => "Strg+V",
            Command::Undo => "Strg+Z",
            Command::Redo => "Strg+Y",
            Command::Find => "Strg+F",
            Command::FindNext => "F3",
            Command::EditShort => "",
            Command::EditLong => "Umschalt+F2",
            Command::Properties => "Alt+Enter",
            Command::Collapse => "←",
            Command::Expand => "→",
            Command::CollapseAll => "",
            Command::ExpandAll => "",
            Command::Renumber => "",
            Command::Palette => "Strg+K",
            Command::SetStatus(EditStatus::InProgress) => "Strg+Umschalt+E",
            Command::SetStatus(_) => "",
            Command::ToggleDone => "Strg+Umschalt+F",
            Command::ConfirmPriceCheck => "",
            Command::FindReplace => "Strg+R",
            Command::SpellCheck => "F7",
            Command::NextMissingQuantity => "F4",
            Command::Filter(_) | Command::ToggleGapMarks => "",
            Command::OpenPoints => "Strg+Umschalt+P",
            Command::CostGroups
            | Command::ToggleCostGroupColumn
            | Command::ToggleWorkCategoryColumn => "",
        }
    }
}

/// Key → command while the table has the keyboard (no text field focused).
/// Copy, cut and paste arrive as their own events, not as keys.
pub fn command_for_key(key: Key, modifiers: Modifiers) -> Option<Command> {
    let command = modifiers.command && !modifiers.alt;
    let shift = modifiers.shift;
    let plain = !modifiers.command && !modifiers.alt && !modifiers.shift;
    let alt_only = modifiers.alt && !modifiers.command && !modifiers.shift;
    Some(match key {
        Key::ArrowUp if alt_only => Command::MoveUp,
        Key::ArrowDown if alt_only => Command::MoveDown,
        Key::ArrowUp if plain => Command::SelectUp,
        Key::ArrowUp if shift && !command && !modifiers.alt => Command::ExtendUp,
        Key::ArrowDown if shift && !command && !modifiers.alt => Command::ExtendDown,
        Key::ArrowDown if plain => Command::SelectDown,
        Key::ArrowLeft if plain => Command::Collapse,
        Key::ArrowRight if plain => Command::Expand,
        Key::PageUp if plain => Command::PageUp,
        Key::PageDown if plain => Command::PageDown,
        Key::Home if plain || command => Command::First,
        Key::End if plain || command => Command::Last,
        Key::Enter if command && shift => Command::NewTitle,
        Key::Enter if command => Command::NewPosition,
        Key::Enter if plain => Command::EditShort,
        Key::Enter if alt_only => Command::Properties,
        Key::Insert if plain => Command::NewPosition,
        Key::Insert if shift && !command => Command::NewPositionBefore,
        Key::T if command && !shift => Command::NewTitle,
        Key::H if command && !shift => Command::NewNote,
        Key::L if command && !shift => Command::NewSpacer,
        Key::Delete if plain => Command::Delete,
        Key::Tab if plain => Command::Deeper,
        Key::Tab if shift && !command => Command::Higher,
        Key::D if command && !shift => Command::Duplicate,
        Key::A if command && !shift => Command::SelectAll,
        Key::Z if command && shift => Command::Redo,
        Key::Z if command => Command::Undo,
        Key::Y if command && !shift => Command::Redo,
        Key::F if command && !shift => Command::Find,
        // Strg+E belongs to the window (Eigenschaftenfenster ein/aus).
        Key::F if command && shift => Command::ToggleDone,
        Key::F3 if !command => Command::FindNext,
        Key::F2 if shift && !command => Command::EditLong,
        Key::K if command && !shift => Command::Palette,
        Key::E if command && shift => Command::SetStatus(EditStatus::InProgress),
        Key::R if command && !shift => Command::FindReplace,
        Key::F7 if !command && !shift => Command::SpellCheck,
        Key::P if command && shift => Command::OpenPoints,
        Key::F4 if plain => Command::NextMissingQuantity,
        // Function keys in groups (Anwender 18.11.): F5/F6 texts from STLB-Bau and
        // AUSSCHREIBEN.DE (F7 Rechtschreibung, F8 Bibliothek), F9–F12 new rows
        // (Umschalt: before / one level deeper). Umschalt+F10 stays the
        // context menu (system-wide custom), the hint before is Strg+F10.
        // Windows sends F10 as WM_SYSKEYDOWN: winit 0.30 reports the press and
        // passes WM_SYSKEYUP to DefWindowProc only for windows with a native
        // menu bar (GetMenu), which proAVA has none of – so Windows does not
        // enter its menu mode (docs/BUILD.md, Windows).
        Key::F5 if plain => Command::FromStlb,
        Key::F6 if plain => Command::FromAusschreiben,
        Key::F9 if plain => Command::NewPosition,
        Key::F9 if shift && !command && !modifiers.alt => Command::NewPositionBefore,
        Key::F10 if plain => Command::NewNote,
        Key::F12 if plain => Command::NewTitle,
        _ => return None,
    })
}

/// Indices of the rows shown when the titles in `collapsed` are closed.
pub fn visible_rows(lv: &Lv, collapsed: &HashSet<RowId>) -> Vec<usize> {
    let rows = lv.rows();
    let mut visible = Vec::with_capacity(rows.len());
    let mut index = 0;
    while index < rows.len() {
        visible.push(index);
        if rows[index].is_group() && collapsed.contains(&rows[index].id()) {
            index = lv.block_range(index).end;
        } else {
            index += 1;
        }
    }
    visible
}

/// Ids of the titles a row lies in (outermost first).
pub fn ancestors(lv: &Lv, index: usize) -> Vec<RowId> {
    let rows = lv.rows();
    let mut result = Vec::new();
    let mut level_limit = rows
        .get(index)
        .and_then(LvRow::as_group)
        .map(LvGroup::level)
        .unwrap_or(usize::MAX);
    for row in rows[..index.min(rows.len())].iter().rev() {
        if let LvRow::Group(group) = row {
            if group.level() < level_limit {
                result.push(row.id());
                level_limit = group.level();
            }
        }
    }
    result.reverse();
    result
}

/// Next row after `after` (wrapping) whose OZ or short text contains `query`.
pub fn find(lv: &Lv, query: &str, after: Option<usize>) -> Option<usize> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() || lv.rows().is_empty() {
        return None;
    }
    let count = lv.rows().len();
    let start = after.map(|index| index + 1).unwrap_or(0);
    (0..count)
        .map(|offset| (start + offset) % count)
        .find(|&index| {
            let row = &lv.rows()[index];
            row.ordinal().to_lowercase().contains(&needle)
                || row.short_text().to_lowercase().contains(&needle)
        })
}

/// The first row in `range` whose OZ or short text contains `query`
/// (search over several LVs: one piece of one LV after the other).
pub fn find_in(lv: &Lv, query: &str, range: std::ops::Range<usize>) -> Option<usize> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let end = range.end.min(lv.rows().len());
    (range.start.min(end)..end).find(|&index| {
        let row = &lv.rows()[index];
        row.ordinal().to_lowercase().contains(&needle)
            || row.short_text().to_lowercase().contains(&needle)
    })
}

/// After a change: an automatically numbered LV (`auto`, checked before the
/// change) is renumbered; an LV with its own numbers (GAEB import) keeps them.
fn finish(lv: &mut Lv, auto: bool, index: usize) -> usize {
    if auto {
        lv.renumber();
    }
    index
}

/// Inserts rows at `index` one after the other. An automatically numbered LV
/// is renumbered; an LV with its own numbers (GAEB import) keeps them and the
/// new rows get free order numbers between their neighbours (0015 between
/// 0010 and 0020). Nothing changes when there is no free number.
fn insert_rows(lv: &mut Lv, index: usize, rows: Vec<LvRow>) -> Result<usize, String> {
    let before = lv.clone();
    for (offset, row) in rows.into_iter().enumerate() {
        if let Err(error) = lv.insert_numbered(index + offset, row) {
            *lv = before;
            return Err(error);
        }
    }
    Ok(index)
}

/// A moved position in an LV with its own numbers gets a free order number at
/// its new place (or keeps its old one when there is none).
fn renumber_moved(lv: &mut Lv, auto: bool, index: usize) -> usize {
    if auto {
        lv.renumber();
    } else if lv
        .rows()
        .get(index)
        .and_then(LvRow::as_position)
        .is_some_and(|position| !position.is_locked())
    {
        // Probe on a copy: removing the row for real would drop references
        // (Wiederholung, Ausführungsbeschreibung) pointing to it.
        let mut probe = lv.clone();
        if let Some(row) = probe.remove_row(index) {
            if let Some(ordinal) = probe.free_ordinal_at(index, &row) {
                if let Some(position) = lv.row_mut(index).and_then(LvRow::as_position_mut) {
                    position.set_ordinal(ordinal);
                }
            }
        }
    }
    index
}

/// New position right after the selection (or before it). Positions belong
/// under a title of the deepest level; missing titles are added on the way.
/// Which titles come along with a new position (F9, Strg+Enter, Einfügen,
/// STLB-Bau).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewTitles {
    /// LV: a position belongs under the deepest title level, so the missing
    /// levels below the title of the Schreibmarke come along – unless the LV
    /// already has positions directly under titles of that level and this
    /// title has no subtitles (an imported LV, GAEB „unbelegte
    /// Hierarchiestufen“): then the new one joins them.
    Fill,
    /// Positionsbibliothek: the position goes into the title of the
    /// Schreibmarke, on its level – the levels of the library mask are room,
    /// not a must. Only a library without any title above the
    /// place gets one top title.
    Never,
}

/// A new position after (or before) the selection, see [`NewTitles`].
pub fn insert_position(
    lv: &mut Lv,
    selected: Option<usize>,
    before: bool,
) -> Result<usize, String> {
    insert_position_with(lv, selected, before, NewTitles::Fill)
}

pub fn insert_position_with(
    lv: &mut Lv,
    selected: Option<usize>,
    before: bool,
    titles: NewTitles,
) -> Result<usize, String> {
    let unit = selected
        .and_then(|index| lv.rows().get(index))
        .and_then(LvRow::as_position)
        .map(|position| position.unit().to_string())
        .filter(|unit| !unit.is_empty())
        .unwrap_or_else(|| "St".to_string());
    let mut index = insertion(lv, selected, before);
    let mut rows = Vec::new();
    if lv.level_count() > 0 {
        // The title the new position lands in.
        let parent = lv.rows()[..index]
            .iter()
            .rposition(LvRow::is_group)
            .map(|at| (at, lv.rows()[at].as_group().map_or(0, LvGroup::level)));
        if lv.group_count() == 0 {
            index = lv.rows().len();
        }
        let levels = match (titles, parent) {
            (NewTitles::Never, Some(_)) => 0..0,
            (NewTitles::Never, None) => 0..1,
            (NewTitles::Fill, Some((at, _))) if lv.holds_positions_directly(at) => 0..0,
            // Positions belong under the deepest title level: add the missing titles.
            (NewTitles::Fill, parent) => parent.map_or(0, |(_, level)| level + 1)..lv.level_count(),
        };
        for level in levels {
            rows.push(LvRow::Group(LvGroup::new(level, "", NEW_TITLE_TEXT)));
        }
    }
    let titles = rows.len();
    rows.push(LvRow::Position(Position::new(
        "",
        "",
        Decimal::ONE,
        unit,
        None,
    )));
    insert_rows(lv, index, rows).map(|start| start + titles)
}

pub fn insert_stlb(lv: &mut Lv, selected: Option<usize>) -> Result<usize, String> {
    let index = insert_position(lv, selected, false)?;
    if let Some(position) = lv.row_mut(index).and_then(LvRow::as_position_mut) {
        position.set_stlb_placeholder(true);
        position.set_short_text("Positionsplatzhalter aus STLB-Bau");
        position.set_quantity(Decimal::ZERO);
        position.set_unit("");
    }
    Ok(index)
}

pub fn insert_note(lv: &mut Lv, selected: Option<usize>, before: bool) -> Result<usize, String> {
    let index = insertion(lv, selected, before);
    insert_rows(lv, index, vec![LvRow::Note(Note::new(""))])
}

/// Abstandshalter right after the selection (or before it).
pub fn insert_spacer(lv: &mut Lv, selected: Option<usize>, before: bool) -> Result<usize, String> {
    let index = insertion(lv, selected, before);
    insert_rows(lv, index, vec![LvRow::Spacer(Spacer::new())])
}

/// New title on the level of the selection's title, after its whole block.
pub fn insert_title(lv: &mut Lv, selected: Option<usize>) -> Result<usize, String> {
    if lv.level_count() == 0 {
        return Err(format!(
            "Die OZ-Maske „{}“ hat keine Titelstufe – für Titel erst unter LV › Gliederung und OZ-Maske … eine \
             Stufe ergänzen (z. B. 11.PPPP).",
            lv.oz_mask().pattern()
        ));
    }
    let level = lv.level_for_new_group(selected);
    let index = lv.insert_group_index(selected, level);
    insert_rows(
        lv,
        index,
        vec![LvRow::Group(LvGroup::new(level, "", NEW_TITLE_TEXT))],
    )
}

fn insertion(lv: &Lv, selected: Option<usize>, before: bool) -> usize {
    match (selected, before) {
        (Some(index), true) => index.min(lv.rows().len()),
        (None, true) => 0,
        _ => lv.insert_index(selected),
    }
}

/// Removes a row or a title with its content; returns the row to select next.
pub fn delete(lv: &mut Lv, index: usize) -> Option<usize> {
    if index >= lv.rows().len() {
        return None;
    }
    let auto = lv.is_auto_numbered();
    lv.remove_block(index);
    finish(lv, auto, index);
    if lv.rows().is_empty() {
        None
    } else {
        Some(index.min(lv.rows().len() - 1))
    }
}

/// Moves a row or title one step. A position jumps over places where it would
/// sit directly under a higher title (e.g. between „02“ and „02.01“, OZ
/// „02.00.0010“) to the next place under a title of the deepest level.
pub fn move_step(lv: &mut Lv, index: usize, up: bool) -> Option<usize> {
    let auto = lv.is_auto_numbered();
    if !lv.rows().get(index).is_some_and(LvRow::is_position) || !well_placed(lv, index) {
        let moved = lv.move_block(index, up)?;
        return Some(renumber_moved(lv, auto, moved));
    }
    let before = lv.clone();
    let mut current = index;
    for _ in 0..lv.rows().len() {
        match lv.move_block(current, up) {
            Some(moved) => current = moved,
            None => break,
        }
        if well_placed(lv, current) {
            return Some(renumber_moved(lv, auto, current));
        }
    }
    *lv = before;
    None
}

/// A position under a title of the deepest level (or in an LV without titles).
fn well_placed(lv: &Lv, index: usize) -> bool {
    let levels = lv.level_count();
    let parent = lv.rows()[..index].iter().rev().find_map(LvRow::as_group);
    match parent {
        Some(group) => levels == 0 || group.level() + 1 == levels,
        None => lv.group_count() == 0,
    }
}

/// Title one level deeper/higher. An LV with its own numbers keeps them
/// (the caller points to „Neu nummerieren“).
pub fn shift(lv: &mut Lv, index: usize, deeper: bool) -> bool {
    let auto = lv.is_auto_numbered();
    let changed = lv.shift_level(index, deeper);
    if changed {
        if auto {
            finish(lv, auto, index);
        } else {
            number_block_in_place(lv, index);
        }
    }
    changed
}

/// Own numbers: the block at `index` gets free order numbers at its place –
/// the title one of its new level (01.481.02 one level up becomes 01.482),
/// the rows below the new prefix. Ids stay. Unchanged when a finished
/// position is in the block or a number is not free.
fn number_block_in_place(lv: &mut Lv, index: usize) {
    let range = lv.block_range(index);
    let finished = lv.rows()[range.clone()].iter().any(|row| {
        row.as_position()
            .is_some_and(|position| position.status() == proava_domain::EditStatus::Done)
    });
    if finished {
        return;
    }
    let before = lv.clone();
    let block = lv.remove_block(index);
    for (offset, row) in block.into_iter().enumerate() {
        if lv.insert_with_free_ordinal(index + offset, row).is_err() {
            *lv = before;
            return;
        }
    }
}

/// Copy of a row or title block right after it; returns the first copied row.
pub fn duplicate(lv: &mut Lv, index: usize) -> Result<usize, String> {
    if index >= lv.rows().len() {
        return Err("Keine Zeile gewählt.".to_string());
    }
    let range = lv.block_range(index);
    let rows: Vec<LvRow> = lv.rows()[range.clone()]
        .iter()
        .cloned()
        .map(fresh)
        .collect();
    insert_rows(lv, range.end, rows)
}

/// Moves a row or block so that it starts before row `target` (or at the end
/// when `target` is the row count). Ids stay the same.
pub fn move_to(lv: &mut Lv, from: usize, target: usize) -> Option<usize> {
    if from >= lv.rows().len() || target > lv.rows().len() {
        return None;
    }
    let range = lv.block_range(from);
    if target >= range.start && target <= range.end {
        return None;
    }
    let auto = lv.is_auto_numbered();
    // move_rows keeps ids and references (taking the rows out and in again
    // would drop references to a moved Ausführungsbeschreibung).
    let start = lv.move_rows(range, target)?;
    Some(renumber_moved(lv, auto, start))
}

/// The rows Strg+C takes: the selected row or the title with its content.
pub fn copy_block(lv: &Lv, index: usize) -> Vec<LvRow> {
    if index >= lv.rows().len() {
        return Vec::new();
    }
    lv.rows()[lv.block_range(index)].to_vec()
}

/// Inserts copied rows after the selection; titles are moved to the level a
/// new title would get there. Returns the index of the first inserted row.
pub fn paste(lv: &mut Lv, selected: Option<usize>, rows: &[LvRow]) -> Result<usize, String> {
    // Without a title level in the mask, titles cannot be numbered: only
    // their content is pasted.
    let without_titles: Vec<LvRow>;
    let rows = if lv.level_count() == 0 && rows.iter().any(LvRow::is_group) {
        without_titles = rows.iter().filter(|row| !row.is_group()).cloned().collect();
        &without_titles[..]
    } else {
        rows
    };
    if rows.is_empty() {
        return Err("Nichts zum Einfügen.".to_string());
    }
    let max_level = lv.level_count().saturating_sub(1);
    let lowest = rows
        .iter()
        .filter_map(LvRow::as_group)
        .map(LvGroup::level)
        .min();
    let deepest = rows
        .iter()
        .filter_map(LvRow::as_group)
        .map(LvGroup::level)
        .max();
    let has_positions = rows.iter().any(|row| matches!(row, LvRow::Position(_)));
    let (index, delta) = match lowest {
        Some(lowest) => {
            let mut target = lv.level_for_new_group(selected);
            // Positions belong under the deepest title level: a block whose
            // deepest title holds positions keeps that depth – pasted after a
            // higher title it goes into that title (481 ▸ 01 ▸ positions
            // stays 481 ▸ 01, never 485 ▸ 00 ▸ positions).
            if let (true, Some(deepest)) = (has_positions, deepest) {
                let span = deepest - lowest;
                if span <= max_level {
                    target = max_level - span;
                }
            }
            let index = if rows[0].is_group() {
                lv.insert_group_index(selected, target)
            } else {
                lv.insert_index(selected)
            };
            (index, target as isize - lowest as isize)
        }
        None => (lv.insert_index(selected), 0),
    };
    let old_ids: Vec<RowId> = rows.iter().map(LvRow::id).collect();
    let rows = rows
        .iter()
        .map(|row| {
            let mut row = row.clone();
            if let LvRow::Group(group) = &mut row {
                let level = (group.level() as isize + delta).clamp(0, max_level as isize);
                let cost_group = group.cost_group().map(str::to_string);
                let work_category = group.work_category().map(str::to_string);
                let lump = group.lump_sum_total();
                let points = group.points().cloned();
                *group = LvGroup::new(level as usize, "", group.short_text());
                group.set_cost_group(cost_group);
                group.set_work_category(work_category);
                group.set_lump_sum_total(lump);
                group.set_points(points);
            }
            fresh(row)
        })
        .collect::<Vec<_>>();
    let count = rows.len();
    let index = insert_rows(lv, index, rows)?;
    let renamed: std::collections::HashMap<RowId, RowId> = old_ids
        .into_iter()
        .zip(lv.rows()[index..index + count].iter().map(LvRow::id))
        .collect();
    // Pasted positions refer to the pasted Ausführungsbeschreibung; references
    // to descriptions that are gone (cut, other project) are dropped.
    for position in renamed.values().copied().collect::<Vec<_>>() {
        let target = lv
            .position_by_id(position)
            .and_then(|position| position.performance())
            .and_then(|old| renamed.get(&old).copied());
        if let Some(target) = target {
            let _ = lv.link_performance(position, Some(target));
        }
        let base = lv
            .position_by_id(position)
            .and_then(|position| position.repeats())
            .and_then(|old| renamed.get(&old).copied());
        if let Some(base) = base {
            let _ = lv.link_repetition(position, Some(base));
        }
        // A pasted Zuschlag on listed positions refers to the pasted copies
        // of them (others stay, gone ones are dropped below).
        if let Some(row) = lv
            .index_of(position)
            .and_then(|index| lv.row_mut(index))
            .and_then(LvRow::as_position_mut)
        {
            if let Some(mut surcharge) = row.surcharge().cloned() {
                for reference in &mut surcharge.references {
                    if let Some(new) = renamed.get(&reference.position) {
                        reference.position = *new;
                    }
                }
                row.set_surcharge(Some(surcharge));
            }
        }
    }
    lv.unlink_missing_references();
    // Datenpunkt-Abschnitte take the stage of this LV (none in a tender LV).
    lv.fit_point_sections();
    Ok(index)
}

/// Same row with an unset id, so the LV hands out a new one.
fn fresh(row: LvRow) -> LvRow {
    match row {
        LvRow::Group(group) => {
            let mut copy = LvGroup::new(group.level(), "", group.short_text());
            copy.set_cost_group(group.cost_group().map(str::to_string));
            copy.set_work_category(group.work_category().map(str::to_string));
            copy.set_lump_sum_total(group.lump_sum_total());
            copy.set_points(group.points().cloned());
            LvRow::Group(copy)
        }
        LvRow::Position(position) => {
            let mut copy = Position::new(
                "",
                position.short_text(),
                position.quantity(),
                position.unit(),
                position.unit_price(),
            );
            copy.set_long_text(position.long_text().clone());
            copy.set_kind(position.kind());
            copy.set_lump_sum(position.is_lump_sum());
            copy.set_hourly(position.is_hourly());
            copy.set_stlb_placeholder(position.is_stlb_placeholder());
            copy.set_performance(position.performance());
            copy.set_repeats(position.repeats());
            copy.set_complements(position.complements().to_vec());
            copy.set_quantity_open(position.is_quantity_open());
            copy.set_quantity_formula(position.quantity_formula().map(str::to_string));
            copy.set_price_breakdown(position.has_price_breakdown());
            copy.set_not_applicable(position.is_not_applicable());
            copy.set_key_item(position.is_key_item());
            // Zuschlag (listed positions are mapped after pasting) and the
            // mark „zu bezuschlagen“.
            copy.set_marked_for_surcharge(position.is_marked_for_surcharge());
            if let Some(surcharge) = position.surcharge() {
                copy.set_surcharge(Some(surcharge.clone()));
                copy.set_unit_price(position.unit_price());
            }
            copy.set_price_components(position.price_components().to_vec());
            copy.set_time_quantity(position.time_quantity());
            copy.set_sub_descriptions(position.sub_descriptions().to_vec());
            copy.set_cost_group(position.cost_group().map(str::to_string));
            copy.set_work_category(position.work_category().map(str::to_string));
            copy.set_takeoff(position.takeoff().to_vec());
            // Text source (AUSSCHREIBEN.DE …), STLB-Bau key and the link to a
            // library entry travel with the copy – it is the same text.
            copy.copy_catalog_keys_from(&position);
            copy.set_origin(position.origin().cloned());
            copy.set_library_info(position.library_info().clone());
            LvRow::Position(copy)
        }
        LvRow::Note(note) => {
            let mut copy = Note::rich(note.text().clone());
            copy.set_heading(note.heading());
            copy.set_preliminary(note.is_preliminary());
            copy.set_closing(note.is_closing());
            LvRow::Note(copy)
        }
        LvRow::Performance(performance) => {
            let mut copy = proava_domain::PerformanceDescription::new(
                performance.number(),
                performance.label(),
                performance.long_text().clone(),
            );
            copy.set_short_text(performance.short_text());
            LvRow::Performance(copy)
        }
        LvRow::Spacer(_) => LvRow::Spacer(Spacer::new()),
    }
}

/// Plain text for the system clipboard: OZ, short text, quantity, unit, EP – tab separated.
pub fn rows_as_text(rows: &[LvRow]) -> String {
    rows.iter()
        .map(|row| match row {
            LvRow::Group(group) => format!("{}\t{}", group.ordinal(), group.short_text()),
            LvRow::Note(note) => format!("\t{}", note.text().to_plain().replace('\n', " ")),
            LvRow::Performance(performance) => format!("\t{}", performance.heading()),
            // Abstandshalter: an empty line in Excel/Word too.
            LvRow::Spacer(_) => String::new(),
            LvRow::Position(position) => format!(
                "{}\t{}\t{}\t{}\t{}",
                position.ordinal(),
                position.short_text(),
                crate::numbers::quantity_text(position.quantity()),
                position.unit(),
                position
                    .unit_price()
                    .map(|price| proava_domain::format_decimal_de(price, 2))
                    .unwrap_or_default()
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The same rows as an HTML table (Excel/Word paste it as a table).
pub fn rows_as_html(rows: &[LvRow]) -> String {
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let mut html = String::from("<html><body><table>");
    for line in rows_as_text(rows).lines() {
        html.push_str("<tr>");
        for cell in line.split('\t') {
            html.push_str("<td>");
            html.push_str(&escape(cell));
            html.push_str("</td>");
        }
        html.push_str("</tr>");
    }
    html.push_str("</table>");
    html
}

/// Indices of the selected rows that are not already inside a selected
/// title's block, in document order – the "roots" an operation works on.
pub fn selection_roots(lv: &Lv, ids: &HashSet<RowId>) -> Vec<usize> {
    let mut roots = Vec::new();
    let mut covered_until = 0;
    for (index, row) in lv.rows().iter().enumerate() {
        if index < covered_until || !ids.contains(&row.id()) {
            continue;
        }
        roots.push(index);
        covered_until = lv.block_range(index).end;
    }
    roots
}

/// The rows the roots span when they form one gap-free range.
pub fn contiguous_range(lv: &Lv, roots: &[usize]) -> Option<std::ops::Range<usize>> {
    let first = *roots.first()?;
    let mut end = first;
    for &root in roots {
        if root != end {
            return None;
        }
        end = lv.block_range(root).end;
    }
    Some(first..end)
}

/// Rows of all roots with their blocks, for the clipboard.
pub fn copy_roots(lv: &Lv, roots: &[usize]) -> Vec<LvRow> {
    roots
        .iter()
        .flat_map(|&root| lv.rows()[lv.block_range(root)].to_vec())
        .collect()
}

/// Deletes all roots with their blocks; returns the row to select next.
pub fn delete_roots(lv: &mut Lv, roots: &[usize]) -> Option<usize> {
    let first = *roots.first()?;
    let auto = lv.is_auto_numbered();
    for &root in roots.iter().rev() {
        lv.remove_block(root);
    }
    finish(lv, auto, first);
    if lv.rows().is_empty() {
        None
    } else {
        Some(first.min(lv.rows().len() - 1))
    }
}

/// Number of rows the roots cover.
pub fn roots_len(lv: &Lv, roots: &[usize]) -> usize {
    roots.iter().map(|&root| lv.block_range(root).len()).sum()
}

/// Moves a gap-free range one row up or down; returns the new range.
pub fn move_range(
    lv: &mut Lv,
    range: std::ops::Range<usize>,
    up: bool,
) -> Option<std::ops::Range<usize>> {
    let length = range.len();
    if length == 0 || (up && range.start == 0) || (!up && range.end >= lv.rows().len()) {
        return None;
    }
    let auto = lv.is_auto_numbered();
    let start = if up { range.start - 1 } else { range.start + 1 };
    let target = if up { range.start - 1 } else { range.end + 1 };
    lv.move_rows(range, target)?;
    if auto {
        lv.renumber();
    } else if length == 1 {
        renumber_moved(lv, auto, start);
    }
    Some(start..start + length)
}

/// Ids of the visible rows between two rows (both included), for Umschalt+Klick.
pub fn span_ids(lv: &Lv, visible: &[usize], from: usize, to: usize) -> Vec<RowId> {
    let position = |index: usize| visible.iter().position(|&row| row == index);
    let (Some(a), Some(b)) = (position(from), position(to)) else {
        return Vec::new();
    };
    let (low, high) = (a.min(b), a.max(b));
    visible[low..=high]
        .iter()
        .map(|&index| lv.rows()[index].id())
        .collect()
}

/// Number of rows a delete of `index` removes (the title and its content).
pub fn block_len(lv: &Lv, index: usize) -> usize {
    if index >= lv.rows().len() {
        0
    } else {
        lv.block_range(index).len()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use eframe::egui::{Key, Modifiers};
    use proava_domain::{Decimal, Lv, LvGroup, LvRow, OzMask, Position};

    use super::*;

    fn demo() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.push_group(LvGroup::new(0, "", "Automation"));
        lv.push_group(LvGroup::new(1, "", "ASP01"));
        lv.push_position(Position::new("", "DDC", Decimal::ONE, "St", None));
        lv.push_position(Position::new("", "Modul", Decimal::TWO, "DP", None));
        lv.push_group(LvGroup::new(0, "", "Feld"));
        lv.push_position(Position::new("", "Fühler", Decimal::TEN, "St", None));
        lv.renumber();
        lv
    }

    /// GU-Fall: a Datenpunkt-Abschnitt copied into another Kosten-LV keeps
    /// its plant list (in the stage of the target), pasted into a tender LV
    /// only its title comes.
    #[test]
    fn paste_takes_a_point_section_into_cost_lvs_only() {
        use proava_domain::{cost_lv_template, CostStage, LvPurpose};
        let mut source = cost_lv_template(CostStage::Calculation, "KB", &["480"], 10);
        let index = source.insert_point_section().unwrap();
        if let Some(LvRow::Group(group)) = source.row_mut(index) {
            group.points_mut().unwrap().sections[0].rows[0].counts[0] = 10;
        }
        let rows = vec![source.rows()[index].clone()];

        let mut estimate = cost_lv_template(CostStage::Estimate, "KS", &["420"], 10);
        let last = estimate.rows().len() - 1;
        let at = paste(&mut estimate, Some(last), &rows).unwrap();
        let points = estimate.rows()[at].as_group().unwrap().points().unwrap();
        assert_eq!(points.stage, CostStage::Estimate, "Stufe des Ziels");
        assert_eq!(points.result().points, 10);

        let mut tender = demo();
        let at = paste(&mut tender, Some(0), &rows).unwrap();
        assert!(tender.purpose() == LvPurpose::Tender && tender.point_sections().is_empty());
        assert_eq!(
            tender.rows()[at].short_text(),
            source.rows()[index].short_text()
        );
    }

    /// Duplizieren und Einfügen nehmen Textquelle (AUSSCHREIBEN.DE) und
    /// Bibliotheksverknüpfung mit – beides gehört zum Text.
    #[test]
    fn copies_keep_text_source_and_library_link() {
        use proava_domain::library::LibraryOrigin;
        use proava_domain::{RowId, TextOrigin};

        let mut lv = demo();
        if let Some(LvRow::Position(position)) = lv.row_mut(2) {
            position.set_origin(Some(LibraryOrigin {
                library: "Büro".into(),
                entry: RowId(7),
                as_of: "01.11.2026".into(),
            }));
            position.set_text_origin(Some(TextOrigin {
                source: "AUSSCHREIBEN.DE".into(),
                supplier: "Muster GmbH".into(),
                ..TextOrigin::default()
            }));
        }
        let copied = duplicate(&mut lv, 2).unwrap();
        let block = copy_block(&lv, 2);
        let pasted = paste(&mut lv, Some(copied), &block).unwrap();
        for index in [copied, pasted] {
            let position = lv.rows()[index].as_position().unwrap();
            assert_eq!(position.short_text(), "DDC");
            assert_eq!(position.origin().unwrap().entry, RowId(7));
            assert_eq!(position.text_origin().unwrap().supplier, "Muster GmbH");
        }
    }

    #[test]
    fn a_position_skips_the_gap_between_title_and_subtitle() {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.push_group(LvGroup::new(0, "", "Automation"));
        lv.push_group(LvGroup::new(1, "", "ASP01"));
        lv.push_position(Position::new("", "DDC", Decimal::ONE, "St", None));
        lv.push_group(LvGroup::new(0, "", "Feld"));
        lv.push_group(LvGroup::new(1, "", "ASP02"));
        lv.push_position(Position::new("", "Fühler", Decimal::TEN, "St", None));
        lv.renumber();

        let moved = move_step(&mut lv, 2, false).unwrap();
        assert_eq!(lv.rows()[moved].short_text(), "DDC");
        assert_eq!(lv.rows()[moved].ordinal(), "02.01.0010");
        let back = move_step(&mut lv, moved, true).unwrap();
        assert_eq!(lv.rows()[back].ordinal(), "01.01.0010");
        // Nach oben gibt es keinen Platz unter einem Untertitel mehr: bleibt stehen.
        let before = lv.clone();
        assert_eq!(move_step(&mut lv, back, true), None);
        assert_eq!(lv, before);
    }

    /// LV wie aus GAEB: eigene OZ mit Sprüngen (481, 10), die bleiben müssen.
    fn imported() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.222.22.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Raumautomation"));
        lv.push_group(LvGroup::new(1, "01.481", "Automationseinrichtungen"));
        lv.push_group(LvGroup::new(2, "01.481.10", "ASP"));
        lv.push_position(Position::new(
            "01.481.10.0010",
            "A",
            Decimal::ONE,
            "St",
            None,
        ));
        lv.push_position(Position::new(
            "01.481.10.0020",
            "B",
            Decimal::ONE,
            "St",
            None,
        ));
        lv.push_position(Position::new(
            "01.481.10.0030",
            "C",
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

    #[test]
    fn imported_order_numbers_stay_when_editing() {
        let mut lv = imported();
        assert!(!lv.is_auto_numbered());

        let new = insert_position(&mut lv, Some(3), false).unwrap();
        assert_eq!(lv.rows()[new].ordinal(), "01.481.10.0015");
        assert_eq!(lv.rows()[2].ordinal(), "01.481.10");
        assert_eq!(lv.rows()[5].ordinal(), "01.481.10.0020");

        let kept = ordinals(&lv);
        delete(&mut lv, new).unwrap();
        assert_eq!(
            ordinals(&lv),
            kept.iter()
                .filter(|ordinal| *ordinal != "01.481.10.0015")
                .cloned()
                .collect::<Vec<_>>()
        );

        let copy = duplicate(&mut lv, 3).unwrap();
        assert_eq!(lv.rows()[copy].ordinal(), "01.481.10.0015");
        assert_eq!(lv.rows()[copy].short_text(), "A");

        let rows = copy_block(&lv, 5);
        let pasted = paste(&mut lv, Some(6), &rows).unwrap();
        assert_eq!(lv.rows()[pasted].ordinal(), "01.481.10.0040");

        // Verschieben: die Position bekommt am neuen Platz eine freie OZ, die anderen bleiben.
        let moved = move_step(&mut lv, 3, false).unwrap();
        assert_eq!(lv.rows()[moved].short_text(), "A");
        assert_eq!(lv.rows()[moved].ordinal(), "01.481.10.0017");
        assert_eq!(lv.rows()[3].ordinal(), "01.481.10.0015");
        assert_eq!(lv.rows()[2].ordinal(), "01.481.10");
    }

    #[test]
    fn no_titles_without_a_title_level() {
        let mut lv = Lv::new("LV", OzMask::parse("PPPP").unwrap());
        let first = insert_position(&mut lv, None, false).unwrap();
        assert_eq!(lv.rows()[first].ordinal(), "0010");
        let before = lv.clone();
        assert!(insert_title(&mut lv, Some(first))
            .unwrap_err()
            .contains("keine Titelstufe"));
        assert_eq!(lv, before);

        let block = copy_block(&demo(), 0);
        let pasted = paste(&mut lv, Some(first), &block).unwrap();
        assert!(lv.rows().iter().all(|row| !row.is_group()));
        assert_eq!(lv.rows()[pasted].short_text(), "DDC");
    }

    fn texts(lv: &Lv) -> Vec<String> {
        lv.rows().iter().map(LvRow::short_text).collect()
    }

    #[test]
    fn keys_map_to_commands() {
        let ctrl = Modifiers::COMMAND;
        assert_eq!(
            command_for_key(Key::Enter, ctrl),
            Some(Command::NewPosition)
        );
        assert_eq!(
            command_for_key(Key::Enter, ctrl | Modifiers::SHIFT),
            Some(Command::NewTitle)
        );
        assert_eq!(
            command_for_key(Key::Enter, Modifiers::NONE),
            Some(Command::EditShort)
        );
        assert_eq!(
            command_for_key(Key::ArrowUp, Modifiers::ALT),
            Some(Command::MoveUp)
        );
        assert_eq!(
            command_for_key(Key::ArrowUp, Modifiers::NONE),
            Some(Command::SelectUp)
        );
        assert_eq!(
            command_for_key(Key::Tab, Modifiers::SHIFT),
            Some(Command::Higher)
        );
        assert_eq!(command_for_key(Key::Z, ctrl), Some(Command::Undo));
        assert_eq!(
            command_for_key(Key::Z, ctrl | Modifiers::SHIFT),
            Some(Command::Redo)
        );
        assert_eq!(command_for_key(Key::Y, ctrl), Some(Command::Redo));
        assert_eq!(
            command_for_key(Key::Insert, Modifiers::NONE),
            Some(Command::NewPosition)
        );
        assert_eq!(command_for_key(Key::H, ctrl), Some(Command::NewNote));
        assert_eq!(command_for_key(Key::L, ctrl), Some(Command::NewSpacer));
        assert_eq!(command_for_key(Key::A, Modifiers::NONE), None);
        assert_eq!(
            command_for_key(Key::F, ctrl | Modifiers::SHIFT),
            Some(Command::ToggleDone)
        );
        assert_eq!(
            command_for_key(Key::E, ctrl),
            None,
            "Strg+E: Eigenschaftenfenster (Fenster)"
        );
        assert_eq!(
            command_for_key(Key::E, ctrl | Modifiers::SHIFT),
            Some(Command::SetStatus(proava_domain::EditStatus::InProgress))
        );
        assert_eq!(command_for_key(Key::R, ctrl), Some(Command::FindReplace));
        assert_eq!(
            command_for_key(Key::P, ctrl | Modifiers::SHIFT),
            Some(Command::OpenPoints)
        );
        assert_eq!(
            command_for_key(Key::F4, Modifiers::NONE),
            Some(Command::NextMissingQuantity)
        );
        assert_eq!(
            command_for_key(Key::ArrowDown, Modifiers::SHIFT),
            Some(Command::ExtendDown)
        );
    }

    #[test]
    fn new_position_goes_below_selection_and_takes_its_unit() {
        let mut lv = demo();
        let index = insert_position(&mut lv, Some(3), false).unwrap();
        assert_eq!(index, 4);
        let position = lv.rows()[4].as_position().unwrap();
        assert_eq!(position.unit(), "DP");
        assert_eq!(position.ordinal(), "01.01.0030");
        let before = insert_position(&mut lv, Some(2), true).unwrap();
        assert_eq!(before, 2);
        assert_eq!(lv.rows()[2].ordinal(), "01.01.0010");
    }

    #[test]
    fn first_position_in_empty_lv_gets_a_title() {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        let index = insert_position(&mut lv, None, false).unwrap();
        assert_eq!(index, 2);
        assert!(lv.rows()[0].is_group() && lv.rows()[1].is_group());
        assert_eq!(lv.rows()[2].ordinal(), "01.01.0010");
    }

    #[test]
    fn position_under_an_outer_title_gets_a_subtitle() {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        let title = insert_title(&mut lv, None).unwrap();
        assert_eq!(lv.rows()[title].ordinal(), "01");
        let index = insert_position(&mut lv, Some(title), false).unwrap();
        assert_eq!(index, 2);
        assert_eq!(lv.rows()[1].ordinal(), "01.01");
        assert_eq!(lv.rows()[2].ordinal(), "01.01.0010");
        let next = insert_position(&mut lv, Some(index), false).unwrap();
        assert_eq!(next, 3);
        assert_eq!(lv.rows().len(), 4);
    }

    /// Mask with four title levels, positions under titles of level 2 (like
    /// a library): 01 › 01.001 (two positions), 01.002 (one).
    fn shallow() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.222.33.44.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Automation"));
        lv.push_group(LvGroup::new(1, "", "Hardware"));
        lv.push_position(Position::new("", "A", Decimal::ONE, "St", None));
        lv.push_position(Position::new("", "B", Decimal::ONE, "St", None));
        lv.push_group(LvGroup::new(1, "", "Software"));
        lv.push_position(Position::new("", "C", Decimal::ONE, "St", None));
        lv.renumber();
        lv
    }

    #[test]
    fn a_new_position_joins_positions_above_the_deepest_level() {
        let mut lv = shallow();
        let index = insert_position(&mut lv, Some(3), false).unwrap();
        assert_eq!(lv.group_count(), 3, "keine Zwischentitel");
        assert_eq!(lv.rows()[index].ordinal(), "01.001.00.00.0030");
        // A title with subtitles: positions do not mix with them in an LV.
        let mut lv = shallow();
        let index = insert_position(&mut lv, Some(0), false).unwrap();
        assert_eq!(lv.group_count(), 6, "Ebenen 2–4 wie bisher");
        assert_eq!(lv.rows()[index].ordinal(), "01.001.01.01.0010");
    }

    #[test]
    fn in_a_library_a_new_position_never_brings_titles() {
        let mut lv = shallow();
        let index = insert_position_with(&mut lv, Some(0), false, NewTitles::Never).unwrap();
        assert_eq!(lv.group_count(), 3);
        assert_eq!(index, 1, "direkt unter dem Titel 01");
        assert_eq!(lv.rows()[1].ordinal(), "01.000.00.00.0010");
        // An empty library: one top title, not four.
        let mut empty = Lv::new("LV", OzMask::parse("11.222.33.44.PPPP").unwrap());
        let index = insert_position_with(&mut empty, None, false, NewTitles::Never).unwrap();
        assert_eq!(empty.group_count(), 1);
        assert_eq!(empty.rows()[index].ordinal(), "01.000.00.00.0010");
    }

    #[test]
    fn titles_notes_and_stlb() {
        let mut lv = demo();
        let title = insert_title(&mut lv, Some(2)).unwrap();
        assert_eq!(title, 4);
        assert_eq!(lv.rows()[4].as_group().unwrap().level(), 1);
        let note = insert_note(&mut lv, Some(0), true).unwrap();
        assert_eq!(note, 0);
        assert!(lv.rows()[0].is_note());
        let stlb = insert_stlb(&mut lv, Some(3)).unwrap();
        assert!(lv.rows()[stlb].as_position().unwrap().is_stlb_placeholder());
    }

    #[test]
    fn spacers_are_inserted_moved_copied_and_deleted_like_rows() {
        let mut lv = demo();
        let ordinals: Vec<String> = lv
            .rows()
            .iter()
            .map(|row| row.ordinal().to_string())
            .collect();
        let at = insert_spacer(&mut lv, Some(2), false).unwrap();
        assert_eq!(at, 3);
        assert!(lv.rows()[3].is_spacer());
        let before = insert_spacer(&mut lv, Some(0), true).unwrap();
        assert_eq!(before, 0);
        let kept: Vec<String> = lv
            .rows()
            .iter()
            .filter(|row| !row.is_spacer())
            .map(|row| row.ordinal().to_string())
            .collect();
        assert_eq!(kept, ordinals, "keine OZ verschoben");
        let id = lv.rows()[4].id();
        let moved = move_step(&mut lv, 4, true).unwrap();
        assert_eq!(lv.rows()[moved].id(), id);
        assert_eq!(rows_as_text(&lv.rows()[moved..=moved]), "");
        assert_eq!(fresh(lv.rows()[moved].clone()).id(), RowId::UNSET);
        let count = lv.rows().len();
        delete(&mut lv, moved);
        assert_eq!(lv.rows().len(), count - 1);
        assert_eq!(lv.spacer_count(), 1);
    }

    #[test]
    fn delete_selects_the_neighbour() {
        let mut lv = demo();
        assert_eq!(block_len(&lv, 1), 3);
        assert_eq!(delete(&mut lv, 1), Some(1));
        assert_eq!(texts(&lv), vec!["Automation", "Feld", "Fühler"]);
        assert_eq!(delete(&mut lv, 2), Some(1));
    }

    #[test]
    fn collapsed_titles_hide_their_rows() {
        let lv = demo();
        let mut collapsed = HashSet::new();
        collapsed.insert(lv.rows()[1].id());
        assert_eq!(visible_rows(&lv, &collapsed), vec![0, 1, 4, 5]);
        collapsed.insert(lv.rows()[0].id());
        assert_eq!(visible_rows(&lv, &collapsed), vec![0, 4, 5]);
        assert_eq!(
            ancestors(&lv, 3),
            vec![lv.rows()[0].id(), lv.rows()[1].id()]
        );
        assert_eq!(ancestors(&lv, 1), vec![lv.rows()[0].id()]);
    }

    #[test]
    fn find_wraps_and_matches_ordinal() {
        let lv = demo();
        assert_eq!(find(&lv, "modul", None), Some(3));
        assert_eq!(find(&lv, "02.00.0010", None), Some(5));
        assert_eq!(find(&lv, "ü", Some(5)), Some(5));
        assert_eq!(find(&lv, "DDC", Some(4)), Some(2));
        assert_eq!(find(&lv, "", None), None);
    }

    #[test]
    fn copy_paste_gives_new_ids_and_fitting_levels() {
        let mut lv = demo();
        let block = copy_block(&lv, 1);
        assert_eq!(block.len(), 3);
        let first = paste(&mut lv, Some(5), &block).unwrap();
        assert_eq!(first, 6);
        // ASP01 holds positions: it stays a subtitle (now under „Feld“).
        assert_eq!(lv.rows()[6].as_group().unwrap().level(), 1);
        assert_eq!(lv.rows()[6].ordinal(), "02.01");
        assert_eq!(lv.rows()[7].ordinal(), "02.01.0010");
        let mut ids: Vec<_> = lv.rows().iter().map(LvRow::id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), lv.rows().len());
        let single = copy_block(&lv, 2);
        let pasted = paste(&mut lv, Some(2), &single).unwrap();
        assert_eq!(pasted, 3);
        assert_eq!(lv.rows()[3].short_text(), "DDC");
        assert!(rows_as_text(&single).contains("DDC\t1\tSt"));
        let html = rows_as_html(&single);
        assert!(html.contains("<td>DDC</td><td>1</td><td>St</td>"), "{html}");
    }

    #[test]
    fn a_cut_subtitle_with_positions_goes_into_the_selected_title() {
        let mut lv = Lv::new("LV", OzMask::parse("11.222.33.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Gebäudeautomation"));
        lv.push_group(LvGroup::new(1, "", "Automation"));
        lv.push_group(LvGroup::new(2, "", "Leitungen ASP01"));
        lv.push_position(Position::new("", "Busleitung", Decimal::TEN, "m", None));
        lv.push_group(LvGroup::new(1, "", "Kabel und Leitungen"));
        lv.renumber();
        let block = copy_block(&lv, 2);
        lv.remove_block(2);
        let selected = lv
            .rows()
            .iter()
            .position(|row| row.short_text() == "Kabel und Leitungen");
        let first = paste(&mut lv, selected, &block).unwrap();
        assert_eq!(lv.rows()[first].as_group().unwrap().level(), 2);
        assert_eq!(lv.rows()[first].ordinal(), "01.002.01");
        assert_eq!(lv.rows()[first + 1].ordinal(), "01.002.01.0010");
        assert!(well_placed(&lv, first + 1));
    }

    #[test]
    fn a_title_one_level_up_gets_a_number_of_that_level() {
        let mut lv = Lv::new("LV", OzMask::parse("11.222.33.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Gebäudeautomation"));
        lv.push_group(LvGroup::new(1, "01.481", "Automation"));
        lv.push_group(LvGroup::new(2, "01.481.01", "ASP01"));
        lv.push_position(Position::new(
            "01.481.01.0010",
            "DDC",
            Decimal::ONE,
            "St",
            None,
        ));
        lv.push_group(LvGroup::new(2, "01.481.02", "Schaltschränke"));
        assert!(!lv.is_auto_numbered(), "eigene Nummern (481)");
        let ids: Vec<_> = lv.rows().iter().map(LvRow::id).collect();
        assert!(shift(&mut lv, 4, false));
        assert_eq!(lv.rows()[4].as_group().unwrap().level(), 1);
        assert_eq!(lv.rows()[4].ordinal(), "01.482", "{:?}", ordinals(&lv));
        assert_eq!(lv.rows()[1].ordinal(), "01.481", "andere bleiben");
        assert_eq!(
            lv.rows().iter().map(LvRow::id).collect::<Vec<_>>(),
            ids,
            "Ids bleiben"
        );
    }

    #[test]
    fn move_to_drops_blocks_before_a_row() {
        let mut lv = demo();
        let id = lv.rows()[4].id();
        let moved = move_to(&mut lv, 4, 0).unwrap();
        assert_eq!(moved, 0);
        assert_eq!(lv.rows()[0].id(), id);
        assert_eq!(texts(&lv)[0..2], ["Feld".to_string(), "Fühler".to_string()]);
        assert!(move_to(&mut lv, 0, 1).is_none());
        let moved = move_to(&mut lv, 4, 7).unwrap_or(99);
        assert_eq!(moved, 99);
        let moved = move_to(&mut lv, 4, 6).unwrap();
        assert_eq!(moved, 5);
        assert_eq!(lv.rows()[5].short_text(), "DDC");
    }

    #[test]
    fn multi_selection_roots_ranges_and_delete() {
        let lv = demo();
        let ids = |indices: &[usize]| -> HashSet<_> {
            indices.iter().map(|&i| lv.rows()[i].id()).collect()
        };
        // Title 1 (rows 1..4) covers its positions 2 and 3.
        assert_eq!(selection_roots(&lv, &ids(&[1, 2, 3, 5])), vec![1, 5]);
        assert_eq!(contiguous_range(&lv, &[2, 3]), Some(2..4));
        assert_eq!(contiguous_range(&lv, &[1, 4]), Some(1..6));
        assert_eq!(contiguous_range(&lv, &[2, 5]), None);
        assert_eq!(roots_len(&lv, &[1, 5]), 4);
        assert_eq!(copy_roots(&lv, &[2, 5]).len(), 2);
        let mut copy = lv.clone();
        assert_eq!(delete_roots(&mut copy, &[2, 5]), Some(2));
        assert_eq!(texts(&copy), vec!["Automation", "ASP01", "Modul", "Feld"]);
        let visible: Vec<usize> = (0..lv.rows().len()).collect();
        assert_eq!(span_ids(&lv, &visible, 3, 1).len(), 3);
    }

    #[test]
    fn ranges_move_one_row() {
        let mut lv = demo();
        let moved = move_range(&mut lv, 2..4, true).unwrap();
        assert_eq!(moved, 1..3);
        assert_eq!(
            texts(&lv)[1..4],
            ["DDC".to_string(), "Modul".to_string(), "ASP01".to_string()]
        );
        assert!(move_range(&mut lv, 0..1, true).is_none());
        let len = lv.rows().len();
        assert!(move_range(&mut lv, len - 1..len, false).is_none());
        let ids: Vec<_> = lv.rows().iter().map(LvRow::id).collect();
        move_range(&mut lv, 1..3, false).unwrap();
        assert_eq!(lv.rows()[2].id(), ids[1]);
    }

    #[test]
    fn move_and_shift_renumber() {
        let mut lv = demo();
        assert_eq!(move_step(&mut lv, 3, true), Some(2));
        assert_eq!(lv.rows()[2].short_text(), "Modul");
        assert_eq!(lv.rows()[2].ordinal(), "01.01.0010");
        assert!(shift(&mut lv, 1, false));
        assert_eq!(lv.rows()[1].ordinal(), "02");
        assert_eq!(duplicate(&mut lv, 5), Ok(6));
    }

    #[test]
    fn pasted_zuschlag_keeps_its_kind_and_refers_to_the_pasted_positions() {
        use proava_domain::{Surcharge, SurchargeKind, SurchargeReference};
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "A"));
        let mut marked = Position::new("01.0010", "a", Decimal::TWO, "St", Some(Decimal::TEN));
        marked.set_marked_for_surcharge(true);
        let base = lv.push_position(marked);
        let mut zuschlag =
            Position::new("01.0020", "Zuschlag", Decimal::ZERO, "", Some(Decimal::TEN));
        zuschlag.set_surcharge(Some(Surcharge {
            split: false,
            kind: SurchargeKind::Listed,
            references: vec![SurchargeReference {
                position: base,
                quantity: None,
            }],
        }));
        lv.push_position(zuschlag);
        let rows: Vec<LvRow> = lv.rows()[1..3].to_vec();
        let index = paste(&mut lv, Some(2), &rows).unwrap();
        let pasted_base = lv.rows()[index].as_position().unwrap();
        assert!(pasted_base.is_marked_for_surcharge());
        let pasted = lv.rows()[index + 1].as_position().unwrap();
        let surcharge = pasted.surcharge().expect("Zuschlag bleibt Zuschlag");
        assert_eq!(surcharge.references[0].position, pasted_base.id());
        assert_eq!(pasted.unit_price(), Some(Decimal::TEN));
        // 2 × 10 = 20, 10 % = 2,00 for both Zuschläge
        assert_eq!(
            lv.net_total(),
            proava_domain::Money::from_cents(2 * (2000 + 200))
        );
    }

    #[test]
    fn moving_keeps_finished_numbers_and_references() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "A"));
        let base = lv.push_position(Position::new("01.0010", "a", Decimal::ONE, "St", None));
        let copy = lv.push_position(Position::new("01.0020", "b", Decimal::ONE, "St", None));
        lv.push_position(Position::new("01.0030", "c", Decimal::ONE, "St", None));
        lv.set_row_number(3, "0035").unwrap(); // own numbers
        lv.link_repetition(copy, Some(base)).unwrap();
        // own numbering: a moved position gets a free number, references stay
        let moved = move_step(&mut lv, 1, false).unwrap();
        assert_eq!(lv.rows()[moved].ordinal(), "01.0030");
        assert_eq!(lv.position_by_id(copy).unwrap().repeats(), Some(base));
        // a finished one keeps its number
        lv.set_status(&[base], proava_domain::EditStatus::Done);
        let index = lv.index_of(base).unwrap();
        let moved = move_step(&mut lv, index, false).unwrap();
        assert_eq!(lv.rows()[moved].ordinal(), "01.0030");
        // auto numbering counts around a finished position
        let mut lv = demo();
        let first = lv.rows()[2].id();
        lv.set_status(&[first], proava_domain::EditStatus::Done);
        let index = insert_position(&mut lv, Some(1), false).unwrap();
        assert_eq!(lv.rows()[index].ordinal(), "01.01.0005");
        assert_eq!(lv.row_by_id(first).unwrap().ordinal(), "01.01.0010");
    }
}
