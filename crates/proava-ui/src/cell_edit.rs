//! Editing the LV table cell by cell (like a spreadsheet), without egui:
//! which cells a row has, moving between them, the text a cell starts with
//! and taking over a typed value with the same checks as the edit panel.

use proava_domain::{parse_cost_group, parse_work_category, Lv, LvRow};

use crate::columns::Columns;
use crate::numbers;
use crate::struct_ops;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Ordinal,
    Short,
    /// Kostengruppe (own value of the row; inherited ones are shown grey).
    CostGroup,
    /// Leistungsbereich (own value of the row).
    WorkCategory,
    Quantity,
    Unit,
    Price,
}

impl Cell {
    pub fn label(self) -> &'static str {
        match self {
            Cell::Ordinal => "OZ",
            Cell::Short => "Kurztext",
            Cell::CostGroup => "KG",
            Cell::WorkCategory => "LB",
            Cell::Quantity => "Menge",
            Cell::Unit => "Einheit",
            Cell::Price => "EP",
        }
    }
}

/// Which columns the table shows (KG and LB can be switched on, narrow
/// windows hide them and Einheit and EP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub cost_group: bool,
    pub work_category: bool,
    pub unit: bool,
    pub price: bool,
}

impl Shown {
    pub const ALL: Shown = Shown {
        cost_group: true,
        work_category: true,
        unit: true,
        price: true,
    };

    pub fn of(columns: &Columns) -> Self {
        Self {
            cost_group: columns.show_kg,
            work_category: columns.show_lb,
            unit: columns.show_unit,
            price: columns.show_ep,
        }
    }
}

/// The editable cells of a row, left to right.
pub fn cells(row: &LvRow, shown: Shown) -> Vec<Cell> {
    let mut cells = vec![Cell::Ordinal, Cell::Short];
    if shown.cost_group {
        cells.push(Cell::CostGroup);
    }
    if shown.work_category {
        cells.push(Cell::WorkCategory);
    }
    match row {
        LvRow::Group(_) => cells,
        LvRow::Position(_) => {
            cells.push(Cell::Quantity);
            if shown.unit {
                cells.push(Cell::Unit);
            }
            if shown.price {
                cells.push(Cell::Price);
            }
            cells
        }
        // Hint (its heading) and Ausführungsbeschreibung (its Bezeichnung):
        // the text column only, like a position's Kurztext (Anwender 18.11.).
        LvRow::Note(_) | LvRow::Performance(_) => vec![Cell::Short],
        LvRow::Spacer(_) => Vec::new(),
    }
}

/// What the Kurztext cell edits, as stored: a title's or position's
/// Kurztext, a hint's heading, an Ausführungsbeschreibung's Bezeichnung.
pub fn short_value(row: &LvRow) -> String {
    match row {
        LvRow::Note(note) => note.heading().to_string(),
        LvRow::Performance(performance) => performance.label().to_string(),
        other => other.short_text(),
    }
}

/// Sets what the Kurztext cell edits (see [`short_value`]); false for rows
/// without one.
pub fn set_short_value(row: &mut LvRow, text: &str) -> bool {
    match row {
        LvRow::Group(group) => group.set_short_text(text),
        LvRow::Position(position) => position.set_short_text(text),
        LvRow::Note(note) => note.set_heading(text),
        LvRow::Performance(performance) => performance.set_label(text),
        LvRow::Spacer(_) => return false,
    }
    true
}

/// GAEB limit of the text in the Kurztext cell: 70 for a position's
/// Kurztext, 55 for an Ausführungsbeschreibung's Bezeichnung (PerfLbl);
/// the hint while typing, None when it fits or there is no limit.
pub fn short_limit_hint(row: &LvRow, text: &str) -> Option<String> {
    let length = text.trim().chars().count();
    match row {
        LvRow::Position(_) if length > crate::lv_check::SHORT_TEXT_LIMIT => Some(format!(
            "Kurztext hat {length} Zeichen – GAEB überträgt höchstens {} (GAEB 90 kürzt).",
            crate::lv_check::SHORT_TEXT_LIMIT
        )),
        LvRow::Performance(_) if length > crate::struct_ops::PERFORMANCE_LABEL_LIMIT => {
            Some(format!(
                "Bezeichnung hat {length} Zeichen – GAEB überträgt höchstens {}, längere kürzt GAEB.",
                crate::struct_ops::PERFORMANCE_LABEL_LIMIT
            ))
        }
        _ => None,
    }
}

/// The cell left or right of `cell` in the row; None at the edge.
pub fn step(row: &LvRow, shown: Shown, cell: Cell, forward: bool) -> Option<Cell> {
    let cells = cells(row, shown);
    let at = cells.iter().position(|other| *other == cell)?;
    if forward {
        cells.get(at + 1).copied()
    } else {
        at.checked_sub(1)
            .and_then(|before| cells.get(before).copied())
    }
}

/// The neighbour of `cell` in row `index` that may be edited, for
/// Tab/Umschalt+Tab: locked cells (the unit of an STLB-Bau position …) are
/// passed over; None at the end of the row.
pub fn step_editable(
    lv: &Lv,
    index: usize,
    shown: Shown,
    cell: Cell,
    forward: bool,
) -> Option<Cell> {
    let row = lv.rows().get(index)?;
    let mut at = cell;
    while let Some(next) = step(row, shown, at, forward) {
        if may_edit(lv, index, next).is_ok() {
            return Some(next);
        }
        at = next;
    }
    None
}

/// The cell F2/Enter opens in row `index` for the active cell `wanted`: the
/// cell itself, or – when it is locked – the next editable one after it,
/// else before it. None when the row has no editable cell (finished).
pub fn fit_editable(lv: &Lv, index: usize, shown: Shown, wanted: Cell) -> Option<Cell> {
    let row = lv.rows().get(index)?;
    let cell = fit(row, shown, wanted)?;
    if may_edit(lv, index, cell).is_ok() {
        return Some(cell);
    }
    step_editable(lv, index, shown, cell, true)
        .or_else(|| step_editable(lv, index, shown, cell, false))
}

/// The cell to use in `row` when coming with `wanted`: the same, else the
/// Kurztext; None for rows without cells.
pub fn fit(row: &LvRow, shown: Shown, wanted: Cell) -> Option<Cell> {
    let cells = cells(row, shown);
    if cells.contains(&wanted) {
        Some(wanted)
    } else if cells.contains(&Cell::Short) {
        Some(Cell::Short)
    } else {
        None
    }
}

/// Whether the keys of the table (Enter, F2, ←/→, typing) work on the
/// cells of this row: titles and positions. Hints and Ausführungsbeschreibungen
/// edit their text cell by mouse only; Enter there goes into the text.
pub fn keyboard_cells(row: &LvRow) -> bool {
    matches!(row, LvRow::Group(_) | LvRow::Position(_))
}

/// The next visible row below `index` that has `cell` (for Enter); hints
/// and Ausführungsbeschreibungen are passed over.
pub fn row_below(
    lv: &Lv,
    visible: &[usize],
    index: usize,
    cell: Cell,
    shown: Shown,
) -> Option<usize> {
    let at = visible.iter().position(|&row| row == index)?;
    visible[at + 1..].iter().copied().find(|&row| {
        let row = &lv.rows()[row];
        keyboard_cells(row) && cells(row, shown).contains(&cell)
    })
}

/// The text a cell starts with when editing begins. A Kurztext with several
/// lines (GAEB) comes on one line, as the table shows it; unchanged it keeps
/// its lines, typed over it becomes one line.
pub fn text(lv: &Lv, index: usize, cell: Cell) -> String {
    let Some(row) = lv.rows().get(index) else {
        return String::new();
    };
    match (cell, row) {
        (Cell::Ordinal, _) => struct_ops::ordinal_stage(lv, index).1,
        (Cell::Short, row) => crate::format::one_line(&short_value(row)).into_owned(),
        (Cell::CostGroup, LvRow::Position(position)) => {
            position.cost_group().unwrap_or_default().to_string()
        }
        (Cell::CostGroup, LvRow::Group(group)) => {
            group.cost_group().unwrap_or_default().to_string()
        }
        (Cell::WorkCategory, LvRow::Position(position)) => {
            position.work_category().unwrap_or_default().to_string()
        }
        (Cell::WorkCategory, LvRow::Group(group)) => {
            group.work_category().unwrap_or_default().to_string()
        }
        (Cell::Quantity, LvRow::Position(position)) => numbers::quantity_entry_text(position),
        (Cell::Unit, LvRow::Position(position)) => position.unit().to_string(),
        (Cell::Price, LvRow::Position(position)) => numbers::price_input(position.unit_price()),
        _ => String::new(),
    }
}

/// Whether editing a cell of this row is allowed (finished positions are
/// locked; a title's number moves its block, its text does not; a title's
/// KG/LB asks first when finished positions would inherit it).
pub fn may_edit(lv: &Lv, index: usize, cell: Cell) -> Result<(), String> {
    let Some(row) = lv.rows().get(index) else {
        return Err("Diese Zeile gibt es nicht.".to_string());
    };
    if row.is_group() && matches!(cell, Cell::Short | Cell::CostGroup | Cell::WorkCategory) {
        return Ok(());
    }
    lv.ensure_editable(&[row.id()])?;
    // The unit belongs to the STLB-Bau text (write-protected).
    if cell == Cell::Unit {
        lv.ensure_text_editable(&[row.id()])?;
    }
    Ok(())
}

/// Takes over the typed `text`; Ok(true) when the LV changed. Nothing changes
/// on an error.
pub fn commit(lv: &mut Lv, index: usize, cell: Cell, text: &str) -> Result<bool, String> {
    may_edit(lv, index, cell)?;
    let before = self::text(lv, index, cell);
    if text == before {
        return Ok(false);
    }
    if let Some(code) = parsed_code(cell, text)? {
        // „70“ for an LB of „070“, „481 Automation…“ from the list.
        if code.as_deref().unwrap_or_default() == before {
            return Ok(false);
        }
    }
    match cell {
        Cell::Ordinal => {
            if text.trim() == before.trim() {
                return Ok(false);
            }
            lv.set_row_number(index, text)?;
        }
        Cell::Short => {
            if !lv
                .row_mut(index)
                .is_some_and(|row| set_short_value(row, text))
            {
                return Err("Diese Zeile hat keinen Kurztext.".to_string());
            }
        }
        Cell::CostGroup => {
            let code = parse_cost_group(text)?;
            match lv.row_mut(index) {
                Some(LvRow::Group(group)) => group.set_cost_group(code),
                Some(LvRow::Position(position)) => position.set_cost_group(code),
                _ => return Err("Diese Zeile hat keine Kostengruppe.".to_string()),
            }
        }
        Cell::WorkCategory => {
            let code = parse_work_category(text)?;
            match lv.row_mut(index) {
                Some(LvRow::Group(group)) => group.set_work_category(code),
                Some(LvRow::Position(position)) => position.set_work_category(code),
                _ => return Err("Diese Zeile hat keinen Leistungsbereich.".to_string()),
            }
        }
        Cell::Quantity => {
            let (value, formula) =
                numbers::parse_quantity_entry(text).map_err(|error| format!("Menge: {error}"))?;
            let position = position_mut(lv, index)?;
            if position.quantity() == value && position.quantity_formula() == formula.as_deref() {
                return Ok(false);
            }
            position.set_quantity(value);
            position.set_quantity_formula(formula);
        }
        Cell::Unit => {
            let position = position_mut(lv, index)?;
            if position.unit() == text.trim() {
                return Ok(false);
            }
            position.set_unit(text.trim());
        }
        Cell::Price => {
            let value = numbers::parse_price(text).map_err(|error| format!("EP: {error}"))?;
            let position = position_mut(lv, index)?;
            if position.unit_price() == value {
                return Ok(false);
            }
            position.set_unit_price(value);
        }
    }
    Ok(true)
}

/// The value a KG or LB cell gives (None for the other cells): the code or
/// None = keine eigene. Errors as in the edit panel.
pub fn parsed_code(cell: Cell, text: &str) -> Result<Option<Option<String>>, String> {
    match cell {
        Cell::CostGroup => parse_cost_group(text).map(Some),
        Cell::WorkCategory => parse_work_category(text).map(Some),
        _ => Ok(None),
    }
}

/// Choices of a KG or LB cell: (text taken over, line shown in the list).
/// The first line is „keine eigene“ (empty text). LB: the own names in use
/// in the LV, then the Leistungsbereiche of STLB-Bau.
pub fn choices(lv: &Lv, cell: Cell) -> Vec<(String, String)> {
    let mut list = vec![(String::new(), "– keine eigene –".to_string())];
    match cell {
        Cell::CostGroup => list.extend(
            proava_domain::DIN276_KG400
                .iter()
                .map(|(code, label)| (code.to_string(), format!("{code}  {label}"))),
        ),
        Cell::WorkCategory => {
            list.extend(
                lv.work_categories_in_use()
                    .into_iter()
                    .filter(|code| proava_domain::work_category_label(code).is_none())
                    .map(|code| (code.clone(), format!("{code}  (im LV)"))),
            );
            list.extend(
                proava_domain::STLB_WORK_CATEGORIES
                    .iter()
                    .map(|(code, name)| (code.to_string(), format!("{code}  {name}"))),
            );
        }
        _ => return Vec::new(),
    }
    list
}

/// The choices matching what was typed: digits match the start of the code,
/// other text the code or a part of the name; nothing typed = all.
pub fn matching_choices<'a>(
    choices: &'a [(String, String)],
    typed: &str,
) -> Vec<&'a (String, String)> {
    let needle = typed.trim().to_lowercase();
    if needle.is_empty() {
        return choices.iter().collect();
    }
    let digits = needle.chars().all(|character| character.is_ascii_digit());
    choices
        .iter()
        .filter(|(code, line)| {
            let code = code.to_lowercase();
            // „70“ finds the LB „070“ (as the field takes it).
            !code.is_empty()
                && (code.starts_with(&needle)
                    || code.trim_start_matches('0').starts_with(&needle)
                    || (!digits && line.to_lowercase().contains(&needle)))
        })
        .collect()
}

fn position_mut(lv: &mut Lv, index: usize) -> Result<&mut proava_domain::Position, String> {
    lv.row_mut(index)
        .and_then(LvRow::as_position_mut)
        .ok_or_else(|| "Nur Positionen haben Menge, Einheit und EP.".to_string())
}

/// Left and right x of a cell in a row of the table.
pub fn span(columns: &Columns, cell: Cell, indent: f32, group: bool) -> (f32, f32) {
    match cell {
        Cell::Ordinal => (columns.left - 4.0, columns.status_x - 8.0),
        Cell::Short => {
            let left = columns.text_x + indent + if group { 18.0 } else { -4.0 };
            (left, columns.text_right - 4.0)
        }
        Cell::CostGroup => (columns.kg_left + 2.0, columns.kg_right() - 2.0),
        Cell::WorkCategory => (columns.lb_left + 2.0, columns.lb_right() - 2.0),
        Cell::Quantity => (columns.qty_left, columns.qty_right + 4.0),
        Cell::Unit => (columns.unit_left + 2.0, columns.ep_left - 4.0),
        Cell::Price => (columns.ep_left, columns.ep_right + 4.0),
    }
}

/// The cell under the pointer: the column it lies in, if that column is an
/// editable cell of the row. Status, GP, the gaps between columns and rows
/// without cells give None – a click there edits nothing (no other cell
/// takes its place).
pub fn at_x(columns: &Columns, row: &LvRow, shown: Shown, x: f32, _indent: f32) -> Option<Cell> {
    // Hints and Ausführungsbeschreibungen write their text over the whole
    // row: a click anywhere right of the OZ and Status means the text.
    if matches!(row, LvRow::Note(_) | LvRow::Performance(_)) {
        return (x >= columns.text_x - 4.0).then_some(Cell::Short);
    }
    let column = if x < columns.status_x - 6.0 {
        Cell::Ordinal
    } else if x < columns.text_x - 4.0 {
        return None; // Status
    } else if x < columns.text_right {
        Cell::Short
    } else if x < columns.kg_right() && columns.show_kg {
        Cell::CostGroup
    } else if x < columns.lb_right() && columns.show_lb {
        Cell::WorkCategory
    } else if x < columns.qty_right + 4.0 {
        Cell::Quantity
    } else if x < columns.ep_left && columns.show_unit {
        Cell::Unit
    } else if x <= columns.ep_right + 4.0 && columns.show_ep {
        Cell::Price
    } else {
        return None; // GP
    };
    // A title shows its GP-Summe over Menge … EP: no cell there.
    cells(row, shown).contains(&column).then_some(column)
}

/// Why a click on a column that is no cell changes nothing (status line);
/// None where nothing needs saying.
pub fn fixed_hint(columns: &Columns, row: &LvRow, x: f32) -> Option<&'static str> {
    if !columns.show_gp || x <= columns.ep_right + 4.0 {
        return None;
    }
    match row {
        LvRow::Position(_) => Some("GP = Menge × EP – nicht direkt bearbeitbar."),
        LvRow::Group(_) => {
            Some("Titelsumme = Summe der Positionen darunter – nicht direkt bearbeitbar.")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{Decimal, EditStatus, LvGroup, Note, OzMask, Position};

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Titel"));
        lv.push_position(Position::new("", "DDC", Decimal::ONE, "St", None));
        lv.push_note(Note::new("Hinweis"));
        lv.push_position(Position::new("", "Modul", Decimal::TWO, "St", None));
        lv.renumber();
        lv
    }

    #[test]
    fn a_short_text_with_lines_is_edited_on_one_line() {
        let mut lv = lv();
        if let Some(LvRow::Position(position)) = lv.row_mut(1) {
            position.set_short_text("Ventil DN 20\n- V1, V2");
        }
        assert_eq!(
            text(&lv, 1, Cell::Short),
            "Ventil DN 20 - V1, V2",
            "ganzer Text, eine Zeile"
        );
        assert_eq!(
            commit(&mut lv, 1, Cell::Short, "Ventil DN 20 - V1, V2"),
            Ok(false)
        );
        assert_eq!(
            lv.rows()[1].short_text(),
            "Ventil DN 20\n- V1, V2",
            "unverändert: Zeilen bleiben"
        );
        assert_eq!(
            commit(&mut lv, 1, Cell::Short, "Ventil DN 25 - V1, V2"),
            Ok(true)
        );
        assert_eq!(lv.rows()[1].short_text(), "Ventil DN 25 - V1, V2");
    }

    /// Anwender 18.11. 23:34: on an STLB-Bau position Tab goes from the Menge
    /// past the locked unit to the EP and back; F2 on the unit opens the EP;
    /// a finished position has no cell to open.
    #[test]
    fn tab_passes_locked_cells_over() {
        let mut lv = lv();
        if let Some(LvRow::Position(position)) = lv.row_mut(1) {
            position.attach_stlb(proava_domain::stlb::StlbReference {
                catalog: "STLB-Bau".into(),
                ..Default::default()
            });
            assert!(position.is_text_locked());
        }
        let shown = Shown::ALL;
        assert!(may_edit(&lv, 1, Cell::Unit).is_err(), "Einheit gesperrt");
        assert_eq!(
            step_editable(&lv, 1, shown, Cell::Quantity, true),
            Some(Cell::Price)
        );
        assert_eq!(
            step_editable(&lv, 1, shown, Cell::Price, false),
            Some(Cell::Quantity)
        );
        assert_eq!(fit_editable(&lv, 1, shown, Cell::Unit), Some(Cell::Price));
        assert_eq!(
            step_editable(&lv, 1, shown, Cell::Price, true),
            None,
            "Zeilenende"
        );
        // A free position keeps every cell.
        assert_eq!(
            step_editable(&lv, 3, shown, Cell::Quantity, true),
            Some(Cell::Unit)
        );
        assert_eq!(fit_editable(&lv, 3, shown, Cell::Unit), Some(Cell::Unit));
        // Finished: nothing to open, no endless walk.
        let id = lv.rows()[3].id();
        lv.set_status(&[id], EditStatus::Done);
        assert_eq!(fit_editable(&lv, 3, shown, Cell::Short), None);
        assert_eq!(step_editable(&lv, 3, shown, Cell::Short, true), None);
    }

    #[test]
    fn rows_have_their_cells_and_tab_walks_them() {
        let lv = lv();
        let rows = lv.rows();
        assert_eq!(
            cells(&rows[0], Shown::ALL),
            [
                Cell::Ordinal,
                Cell::Short,
                Cell::CostGroup,
                Cell::WorkCategory
            ]
        );
        assert_eq!(cells(&rows[1], Shown::ALL).len(), 7);
        assert_eq!(
            cells(&rows[2], Shown::ALL),
            [Cell::Short],
            "Hinweis: Textzelle"
        );
        let narrow = Shown {
            cost_group: false,
            work_category: false,
            unit: false,
            price: false,
        };
        assert_eq!(
            cells(&rows[1], narrow),
            [Cell::Ordinal, Cell::Short, Cell::Quantity]
        );
        assert_eq!(cells(&rows[0], narrow), [Cell::Ordinal, Cell::Short]);
        assert_eq!(
            step(&rows[1], narrow, Cell::Short, true),
            Some(Cell::Quantity)
        );
        assert_eq!(
            step(&rows[1], Shown::ALL, Cell::Short, true),
            Some(Cell::CostGroup)
        );
        assert_eq!(
            step(&rows[1], Shown::ALL, Cell::WorkCategory, true),
            Some(Cell::Quantity)
        );
        assert_eq!(
            step(&rows[1], Shown::ALL, Cell::Quantity, true),
            Some(Cell::Unit)
        );
        assert_eq!(step(&rows[1], Shown::ALL, Cell::Price, true), None);
        assert_eq!(step(&rows[1], Shown::ALL, Cell::Ordinal, false), None);
        assert_eq!(step(&rows[0], narrow, Cell::Short, true), None);
        assert_eq!(step(&rows[0], Shown::ALL, Cell::WorkCategory, true), None);
        assert_eq!(fit(&rows[0], Shown::ALL, Cell::Price), Some(Cell::Short));
        assert_eq!(fit(&rows[2], Shown::ALL, Cell::Price), Some(Cell::Short));
    }

    #[test]
    fn enter_goes_to_the_next_row_with_that_cell() {
        let lv = lv();
        let visible: Vec<usize> = (0..lv.rows().len()).collect();
        assert_eq!(
            row_below(&lv, &visible, 1, Cell::Quantity, Shown::ALL),
            Some(3),
            "skips the hint"
        );
        assert_eq!(
            row_below(&lv, &visible, 0, Cell::Short, Shown::ALL),
            Some(1)
        );
        assert_eq!(row_below(&lv, &visible, 3, Cell::Short, Shown::ALL), None);
        assert_eq!(
            row_below(&lv, &visible, 1, Cell::Short, Shown::ALL),
            Some(3),
            "Kurztext: Enter überspringt den Hinweis"
        );
        assert!(!keyboard_cells(&lv.rows()[2]) && keyboard_cells(&lv.rows()[1]));
        assert_eq!(
            row_below(&lv, &[0, 3], 0, Cell::Quantity, Shown::ALL),
            Some(3),
            "only visible rows"
        );
    }

    #[test]
    fn commit_checks_like_the_edit_panel() {
        let mut lv = lv();
        assert_eq!(text(&lv, 1, Cell::Quantity), "1");
        assert!(commit(&mut lv, 1, Cell::Quantity, "1").unwrap() == false);
        assert!(commit(&mut lv, 1, Cell::Quantity, "3*12,5+4").unwrap());
        let position = lv.rows()[1].as_position().unwrap();
        assert_eq!(position.quantity(), "41.5".parse().unwrap());
        assert_eq!(text(&lv, 1, Cell::Quantity), "3*12,5+4");
        assert!(commit(&mut lv, 1, Cell::Quantity, "1.234,5").unwrap());
        assert_eq!(lv.rows()[1].as_position().unwrap().quantity_formula(), None);
        let before = lv.clone();
        assert!(commit(&mut lv, 1, Cell::Quantity, "2*(3")
            .unwrap_err()
            .contains("Klammer fehlt"));
        assert!(commit(&mut lv, 1, Cell::Price, "abc")
            .unwrap_err()
            .starts_with("EP:"));
        assert_eq!(lv, before, "nothing changes on an error");
        assert!(commit(&mut lv, 1, Cell::Price, "1.250,50").unwrap());
        assert_eq!(text(&lv, 1, Cell::Price), "1250,50");
        assert!(commit(&mut lv, 1, Cell::Unit, " m ").unwrap());
        assert_eq!(lv.rows()[1].as_position().unwrap().unit(), "m");
        assert!(commit(&mut lv, 1, Cell::Short, "DDC-Station").unwrap());
        assert!(commit(&mut lv, 0, Cell::Short, "Automation").unwrap());
        assert_eq!(text(&lv, 1, Cell::Ordinal), "0010");
        assert!(commit(&mut lv, 1, Cell::Ordinal, "0015").unwrap());
        assert_eq!(lv.rows()[1].ordinal(), "01.0015");
        assert!(commit(&mut lv, 1, Cell::Ordinal, "0020")
            .unwrap_err()
            .contains("gibt es schon"));
        assert!(
            commit(&mut lv, 2, Cell::Price, "1").is_err(),
            "a hint has no price"
        );
    }

    #[test]
    fn finished_positions_are_locked_but_titles_keep_their_text() {
        let mut lv = lv();
        let id = lv.rows()[1].id();
        lv.set_status(&[id], EditStatus::Done);
        assert!(commit(&mut lv, 1, Cell::Quantity, "5")
            .unwrap_err()
            .contains("fertig"));
        assert!(may_edit(&lv, 1, Cell::Short).is_err());
        assert!(commit(&mut lv, 0, Cell::Short, "neu").unwrap());
        assert!(
            commit(&mut lv, 0, Cell::Ordinal, "02").is_err(),
            "the number would move a finished position"
        );
    }

    #[test]
    fn the_pointer_picks_a_cell() {
        let columns = crate::columns::ColumnWidths::default().layout(0.0, 1200.0);
        let lv = lv();
        let row = &lv.rows()[1];
        let at = |x: f32| at_x(&columns, row, Shown::of(&columns), x, 12.0);
        assert_eq!(at(columns.left + 2.0), Some(Cell::Ordinal));
        assert_eq!(at(columns.text_x + 40.0), Some(Cell::Short));
        assert_eq!(at(columns.kg_left + 10.0), Some(Cell::CostGroup));
        assert_eq!(at(columns.qty_right - 5.0), Some(Cell::Quantity));
        assert_eq!(at(columns.unit_left + 10.0), Some(Cell::Unit));
        assert_eq!(at(columns.ep_right - 5.0), Some(Cell::Price));
        assert_eq!(at(columns.gp_right - 5.0), None, "GP is not editable");
        assert_eq!(at(columns.status_x + 10.0), None, "Status neither");
        assert_eq!(
            at(columns.text_x + 2.0),
            Some(Cell::Short),
            "Einrückung gehört zum Kurztext"
        );
        let title = &lv.rows()[0];
        let at_title = |x: f32| at_x(&columns, title, Shown::of(&columns), x, 0.0);
        assert_eq!(at_title(columns.kg_left + 10.0), Some(Cell::CostGroup));
        assert_eq!(
            at_title(columns.qty_right - 5.0),
            None,
            "Titel: keine Menge"
        );
        assert_eq!(at_title(columns.gp_right - 5.0), None, "Titelsumme");
        let hint = &lv.rows()[2];
        let at_hint = |x: f32| at_x(&columns, hint, Shown::of(&columns), x, 0.0);
        assert_eq!(
            at_hint(columns.text_x + 40.0),
            Some(Cell::Short),
            "Hinweis: Text"
        );
        assert_eq!(
            at_hint(columns.gp_right - 5.0),
            Some(Cell::Short),
            "der Hinweistext läuft über die ganze Zeile"
        );
        assert_eq!(at_hint(columns.left + 2.0), None, "keine OZ");
        assert!(fixed_hint(&columns, row, columns.gp_right - 5.0)
            .unwrap()
            .starts_with("GP = Menge × EP"));
        assert!(fixed_hint(&columns, title, columns.gp_right - 5.0)
            .unwrap()
            .starts_with("Titelsumme"));
        assert_eq!(fixed_hint(&columns, row, columns.ep_right - 5.0), None);
    }

    #[test]
    fn cost_group_and_work_category_cells_take_codes() {
        let mut lv = lv();
        assert_eq!(text(&lv, 1, Cell::CostGroup), "");
        assert!(commit(&mut lv, 1, Cell::CostGroup, "481").unwrap());
        assert_eq!(
            lv.rows()[1].as_position().unwrap().cost_group(),
            Some("481")
        );
        assert_eq!(
            commit(&mut lv, 1, Cell::CostGroup, "481  Automationseinrichtungen"),
            Ok(false),
            "Zeile der Liste = gleicher Code"
        );
        let before = lv.clone();
        assert!(commit(&mut lv, 1, Cell::CostGroup, "48")
            .unwrap_err()
            .contains("keine Kostengruppe"));
        assert_eq!(lv, before, "nothing changes on an error");
        assert!(commit(&mut lv, 1, Cell::CostGroup, "").unwrap());
        assert_eq!(lv.rows()[1].as_position().unwrap().cost_group(), None);

        assert!(commit(&mut lv, 0, Cell::WorkCategory, "70").unwrap());
        assert_eq!(text(&lv, 0, Cell::WorkCategory), "070");
        assert_eq!(commit(&mut lv, 0, Cell::WorkCategory, "70"), Ok(false));
        assert_eq!(lv.effective_work_category(1), Some(("070".into(), true)));
        assert!(commit(&mut lv, 1, Cell::WorkCategory, "MSR").unwrap());
        assert_eq!(text(&lv, 1, Cell::WorkCategory), "MSR");
        assert!(
            commit(&mut lv, 2, Cell::CostGroup, "481").is_err(),
            "a hint has no KG"
        );

        let id = lv.rows()[1].id();
        lv.set_status(&[id], EditStatus::Done);
        assert!(commit(&mut lv, 1, Cell::CostGroup, "482")
            .unwrap_err()
            .contains("fertig"));
        assert!(
            may_edit(&lv, 0, Cell::CostGroup).is_ok(),
            "Titel fragt erst"
        );
    }

    #[test]
    fn choices_offer_the_lists_and_filter_by_typing() {
        let mut lv = lv();
        if let Some(LvRow::Position(position)) = lv.row_mut(1) {
            position.set_work_category(Some("MSR".into()));
        }
        let kg = choices(&lv, Cell::CostGroup);
        assert_eq!(kg[0], (String::new(), "– keine eigene –".to_string()));
        assert!(kg
            .iter()
            .any(|(code, line)| code == "481" && line == "481  Automationseinrichtungen"));
        let lb = choices(&lv, Cell::WorkCategory);
        assert_eq!(lb[1].0, "MSR", "eigene Namen im LV zuerst");
        assert!(lb.iter().any(|(code, _)| code == "070"));
        assert!(choices(&lv, Cell::Price).is_empty());
        let typed = matching_choices(&kg, "48");
        assert!(!typed.is_empty() && typed.iter().all(|(code, _)| code.starts_with("48")));
        let named = matching_choices(&lb, "gebäudeauto");
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].0, "070");
        assert_eq!(matching_choices(&kg, " ").len(), kg.len());
    }

    #[test]
    fn hints_and_performance_descriptions_edit_heading_and_label() {
        let mut lv = lv();
        // A hint without heading shows its text; the cell edits the heading.
        assert_eq!(text(&lv, 2, Cell::Short), "", "Überschrift leer");
        assert!(commit(&mut lv, 2, Cell::Short, "Vorbemerkung").unwrap());
        match &lv.rows()[2] {
            LvRow::Note(note) => {
                assert_eq!(note.heading(), "Vorbemerkung");
                assert_eq!(note.text().to_plain(), "Hinweis", "Text bleibt");
            }
            _ => panic!("kein Hinweis"),
        }
        let performance = proava_domain::PerformanceDescription::new(
            "1",
            "Kabeltrassen",
            proava_domain::RichText::default(),
        );
        lv.insert_numbered(4, LvRow::Performance(performance))
            .unwrap();
        assert_eq!(cells(&lv.rows()[4], Shown::ALL), [Cell::Short]);
        assert_eq!(
            text(&lv, 4, Cell::Short),
            "Kabeltrassen",
            "Bezeichnung, nicht „Ausführungsbeschreibung A0001 – …“"
        );
        assert!(commit(&mut lv, 4, Cell::Short, "Kabelwege").unwrap());
        assert_eq!(short_value(&lv.rows()[4]), "Kabelwege");
        // GAEB limits as hints: Kurztext 70, Bezeichnung 55, others none.
        let long = "x".repeat(56);
        assert!(short_limit_hint(&lv.rows()[4], &long)
            .unwrap()
            .contains("höchstens 55"));
        assert_eq!(short_limit_hint(&lv.rows()[1], &long), None, "Position: 70");
        assert!(short_limit_hint(&lv.rows()[1], &"x".repeat(71))
            .unwrap()
            .contains("höchstens 70"));
        assert_eq!(short_limit_hint(&lv.rows()[2], &"x".repeat(200)), None);
    }
}
