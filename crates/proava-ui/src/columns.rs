//! Column layout of the LV table: widths the user can drag. Narrow tables
//! first narrow the number columns to a compact width and shorten the short
//! text down to [`MIN_TEXT_HARD`] (about 140 px, Anwender 18.11.); below that
//! columns are dropped one by one: LB and KG first (switched on by choice,
//! mostly inherited from the title, also in the Eigenschaftenfenster), then
//! the Einheit, then EP, GP last; Menge always stays (the planner needs
//! quantity and prices).

use std::collections::BTreeMap;

pub const MIN_WIDTH: f32 = 32.0;
pub const MAX_WIDTH: f32 = 420.0;
/// Below this room for the short text the number columns get compact.
const MIN_TEXT: f32 = 160.0;
/// The short text keeps at least this much room before columns are dropped
/// (Anwender 18.11. 16:00: about 140 px, before 80 px).
pub const MIN_TEXT_HARD: f32 = 140.0;
/// Compact widths: enough for „1.234,567“, „12.312,12“ and „2.300.000,00“.
const COMPACT_QTY: f32 = 60.0;
const COMPACT_UNIT: f32 = 40.0;
const COMPACT_EP: f32 = 88.0;
const COMPACT_GP: f32 = 104.0;
const PADDING: f32 = 12.0;
/// Fixed column „Status“ between OZ and Kurztext: „fertig“, „in Bearb.“
/// (and „Hinweis“ on hint rows) as text, no symbols.
pub const STATUS_WIDTH: f32 = 62.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Oz,
    /// „KG“: Kostengruppe nach DIN 276 (einblendbar, Vorgabe an).
    CostGroup,
    /// „LB“: Leistungsbereich, GAEB-Katalog „work category“ (Vorgabe aus).
    WorkCategory,
    Quantity,
    Unit,
    UnitPrice,
    Total,
}

impl Column {
    pub const ALL: [Column; 7] = [
        Column::Oz,
        Column::CostGroup,
        Column::WorkCategory,
        Column::Quantity,
        Column::Unit,
        Column::UnitPrice,
        Column::Total,
    ];

    /// Columns the user may show or hide (Ansicht, right click on the head).
    pub const OPTIONAL: [Column; 2] = [Column::CostGroup, Column::WorkCategory];

    pub fn key(self) -> &'static str {
        match self {
            Column::Oz => "oz",
            Column::CostGroup => "kg",
            Column::WorkCategory => "lb",
            Column::Quantity => "qty",
            Column::Unit => "unit",
            Column::UnitPrice => "ep",
            Column::Total => "gp",
        }
    }

    /// Title in the head of the table.
    pub fn title(self) -> &'static str {
        match self {
            Column::Oz => "OZ",
            Column::CostGroup => "KG",
            Column::WorkCategory => "LB",
            Column::Quantity => "Menge",
            Column::Unit => "Einheit",
            Column::UnitPrice => "EP",
            Column::Total => "GP",
        }
    }

    /// Name in the menus („Spalte KG (Kostengruppe)“).
    pub fn menu_label(self) -> &'static str {
        match self {
            Column::CostGroup => "Spalte KG (Kostengruppe)",
            Column::WorkCategory => "Spalte LB (Leistungsbereich)",
            _ => self.title(),
        }
    }

    /// Whether an optional column is shown when nothing is stored.
    pub fn shown_by_default(self) -> bool {
        !matches!(self, Column::WorkCategory)
    }

    fn default_width(self) -> f32 {
        match self {
            Column::Oz => 122.0,
            Column::CostGroup => 52.0,
            Column::WorkCategory => 64.0,
            Column::Quantity => 82.0,
            Column::Unit => 56.0,
            Column::UnitPrice => 104.0,
            Column::Total => 120.0,
        }
    }
}

/// The widths of the fixed columns and which optional ones are shown; the
/// short text takes the rest.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnWidths {
    widths: [f32; 7],
    cost_group: bool,
    work_category: bool,
    /// Width the longest OZ of the LV needs (set by the editor each frame,
    /// not stored): a narrow table shrinks the OZ column to it before it
    /// drops columns.
    oz_need: Option<f32>,
}

impl Default for ColumnWidths {
    fn default() -> Self {
        Self {
            widths: Column::ALL.map(Column::default_width),
            cost_group: Column::CostGroup.shown_by_default(),
            work_category: Column::WorkCategory.shown_by_default(),
            oz_need: None,
        }
    }
}

impl ColumnWidths {
    /// From the settings; unknown keys are ignored, missing ones use the default.
    pub fn from_map(map: &BTreeMap<String, f32>) -> Self {
        let mut widths = Self::default();
        for column in Column::ALL {
            if let Some(&width) = map.get(column.key()) {
                widths.set(column, width);
            }
        }
        widths
    }

    pub fn to_map(&self) -> BTreeMap<String, f32> {
        Column::ALL
            .iter()
            .map(|&column| (column.key().to_string(), self.get(column)))
            .collect()
    }

    pub fn get(&self, column: Column) -> f32 {
        self.widths[column as usize]
    }

    pub fn set(&mut self, column: Column, width: f32) {
        self.widths[column as usize] = width.clamp(MIN_WIDTH, MAX_WIDTH);
    }

    pub fn reset(&mut self, column: Column) {
        self.set(column, column.default_width());
    }

    /// Whether the column is switched on (the fixed ones always are; a
    /// narrow window may still leave it out, see [`Columns`]).
    pub fn shown(&self, column: Column) -> bool {
        match column {
            Column::CostGroup => self.cost_group,
            Column::WorkCategory => self.work_category,
            _ => true,
        }
    }

    /// Switches an optional column on or off (the fixed ones stay).
    pub fn set_shown(&mut self, column: Column, on: bool) {
        match column {
            Column::CostGroup => self.cost_group = on,
            Column::WorkCategory => self.work_category = on,
            _ => {}
        }
    }

    /// Width the longest OZ needs (glyphs plus the gap to the next column);
    /// None = unknown, the OZ column keeps its width.
    pub fn set_oz_need(&mut self, width: Option<f32>) {
        self.oz_need = width.filter(|width| width.is_finite() && *width > 0.0);
    }

    /// Dragging the column's splitter by `dx`: OZ grows to the right, the
    /// right-aligned columns grow to the left.
    pub fn drag(&mut self, column: Column, dx: f32) {
        let width = match column {
            Column::Oz => self.get(column) + dx,
            _ => self.get(column) - dx,
        };
        self.set(column, width);
    }

    /// x positions for a table between `left` and `right`.
    pub fn layout(&self, left: f32, right: f32) -> Columns {
        let left = left + PADDING;
        let right = right - PADDING;
        let mut oz = self.get(Column::Oz) + STATUS_WIDTH;
        let mut kg = if self.cost_group {
            self.get(Column::CostGroup)
        } else {
            0.0
        };
        let mut lb = if self.work_category {
            self.get(Column::WorkCategory)
        } else {
            0.0
        };
        let mut qty = self.get(Column::Quantity);
        let mut unit = self.get(Column::Unit);
        let mut ep = self.get(Column::UnitPrice);
        let mut gp = self.get(Column::Total);
        let room = |oz: f32, kg: f32, lb: f32, qty: f32, unit: f32, ep: f32, gp: f32| {
            right - left - oz - kg - lb - qty - unit - ep - gp - 4.0
        };
        if room(oz, kg, lb, qty, unit, ep, gp) < MIN_TEXT {
            qty = qty.min(COMPACT_QTY);
            unit = unit.min(COMPACT_UNIT);
            ep = ep.min(COMPACT_EP);
            gp = gp.min(COMPACT_GP);
            // The OZ column only as wide as the longest OZ (no empty room
            // while columns would go).
            if let Some(need) = self.oz_need {
                let oz_column = self.get(Column::Oz).min(need.max(MIN_WIDTH));
                oz = oz_column + STATUS_WIDTH;
            }
        }
        let text_room = |kg: f32, lb: f32, qty: f32, unit: f32, ep: f32, gp: f32| {
            room(oz, kg, lb, qty, unit, ep, gp)
        };
        // Too narrow: first the optional columns go (LB, KG), then Einheit,
        // EP, GP; Menge stays. The Einheit only after KG/LB: it belongs to
        // the quantity („12 m“ or „12 St“) and is the narrowest column.
        if text_room(kg, lb, qty, unit, ep, gp) < MIN_TEXT_HARD {
            lb = 0.0;
        }
        if text_room(kg, lb, qty, unit, ep, gp) < MIN_TEXT_HARD {
            kg = 0.0;
        }
        if text_room(kg, lb, qty, unit, ep, gp) < MIN_TEXT_HARD {
            unit = 0.0;
        }
        if text_room(kg, lb, qty, unit, ep, gp) < MIN_TEXT_HARD {
            ep = 0.0;
        }
        if text_room(kg, lb, qty, unit, ep, gp) < MIN_TEXT_HARD {
            gp = 0.0;
        }
        let gp_right = right;
        let ep_right = gp_right - gp;
        let ep_left = ep_right - ep;
        let unit_left = ep_left - unit;
        let qty_right = unit_left - 4.0;
        let qty_left = qty_right - qty;
        let lb_left = qty_left - lb;
        let kg_left = lb_left - kg;
        Columns {
            left,
            status_x: left + oz - STATUS_WIDTH,
            text_x: left + oz,
            text_right: kg_left,
            kg_left,
            lb_left,
            qty_left,
            qty_right,
            unit_x: unit_left + 6.0,
            unit_left,
            ep_left,
            ep_right,
            gp_right,
            show_kg: kg > 0.0,
            show_lb: lb > 0.0,
            show_unit: unit > 0.0,
            show_ep: ep > 0.0,
            show_gp: gp > 0.0,
        }
    }
}

/// Absolute x positions of one table row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Columns {
    pub left: f32,
    /// Left edge of the column „Status“ (= right end of the OZ column).
    pub status_x: f32,
    pub text_x: f32,
    /// Right end of the Kurztext (= left edge of KG, LB or Menge).
    pub text_right: f32,
    /// Left edges of the columns „KG“ and „LB“ (equal to the next one when
    /// the column is not shown).
    pub kg_left: f32,
    pub lb_left: f32,
    pub qty_left: f32,
    pub qty_right: f32,
    pub unit_x: f32,
    pub unit_left: f32,
    pub ep_left: f32,
    pub ep_right: f32,
    pub gp_right: f32,
    pub show_kg: bool,
    pub show_lb: bool,
    pub show_unit: bool,
    pub show_ep: bool,
    pub show_gp: bool,
}

impl Columns {
    /// Splitters the user can drag: the column and the x of its splitter.
    pub fn splitters(&self) -> Vec<(Column, f32)> {
        let mut list = vec![(Column::Oz, self.status_x - 6.0)];
        if self.show_kg {
            list.push((Column::CostGroup, self.kg_left));
        }
        if self.show_lb {
            list.push((Column::WorkCategory, self.lb_left));
        }
        list.push((Column::Quantity, self.qty_left));
        if self.show_unit {
            list.push((Column::Unit, self.unit_left));
        }
        if self.show_ep {
            list.push((Column::UnitPrice, self.ep_left));
        }
        if self.show_gp {
            list.push((Column::Total, self.ep_right));
        }
        list
    }

    /// Right end of the column „KG“ (= left edge of the next one).
    pub fn kg_right(&self) -> f32 {
        self.lb_left
    }

    /// Right end of the column „LB“ (= left edge of „Menge“).
    pub fn lb_right(&self) -> f32 {
        self.qty_left
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Column, ColumnWidths};

    #[test]
    fn widths_round_trip_through_settings_and_clamp() {
        let mut widths = ColumnWidths::default();
        widths.drag(Column::Oz, 30.0);
        widths.drag(Column::Total, -40.0);
        widths.drag(Column::WorkCategory, -16.0);
        widths.set(Column::Unit, 5.0);
        let map = widths.to_map();
        assert_eq!(map["oz"], 152.0);
        assert_eq!(map["gp"], 160.0);
        assert_eq!(map["lb"], 80.0);
        assert_eq!(map["unit"], super::MIN_WIDTH);
        let mut foreign = BTreeMap::new();
        foreign.insert("qty".to_string(), 90.0);
        foreign.insert("fremd".to_string(), 1.0);
        let loaded = ColumnWidths::from_map(&foreign);
        assert_eq!(loaded.get(Column::Quantity), 90.0);
        assert_eq!(
            loaded.get(Column::Oz),
            ColumnWidths::default().get(Column::Oz)
        );
        assert_eq!(ColumnWidths::from_map(&map), widths);
    }

    #[test]
    fn narrow_tables_drop_columns_and_keep_the_text() {
        let mut widths = ColumnWidths::default();
        widths.set_shown(Column::CostGroup, false);
        let wide = widths.layout(0.0, 1000.0);
        assert!(wide.show_ep && wide.show_gp && wide.show_unit);
        assert!(!wide.show_kg && !wide.show_lb);
        assert_eq!(wide.gp_right, 988.0);
        assert_eq!(wide.ep_right, 868.0);
        assert_eq!(wide.text_right, wide.qty_left, "ohne KG/LB");
        assert!(wide.text_right - wide.text_x >= 160.0);
        assert_eq!(wide.text_x - wide.status_x, super::STATUS_WIDTH);
        // Schmal: Zahlenspalten kompakt, alle bleiben, Kurztext ≥ 140 px.
        let text = |columns: &super::Columns| columns.text_right - columns.text_x;
        let narrow = widths.layout(0.0, 650.0);
        assert!(narrow.show_ep && narrow.show_gp && narrow.show_unit);
        assert!(text(&narrow) >= super::MIN_TEXT_HARD, "{}", text(&narrow));
        assert_eq!(narrow.ep_right - narrow.ep_left, 88.0);
        // Halbe Bildschirmbreite (Tabelle etwa 610 px): erst die Einheit.
        let half = widths.layout(0.0, 610.0);
        assert!(!half.show_unit && half.show_ep && half.show_gp);
        assert!(text(&half) >= super::MIN_TEXT_HARD, "{}", text(&half));
        // Schmaler: dann EP, GP bleibt am längsten, Menge immer.
        let small = widths.layout(0.0, 560.0);
        assert!(!small.show_ep && small.show_gp);
        assert!(text(&small) >= super::MIN_TEXT_HARD);
        let tiny = widths.layout(0.0, 450.0);
        assert!(!tiny.show_gp && !tiny.show_ep && !tiny.show_unit);
        assert!(text(&tiny) >= super::MIN_TEXT_HARD);
        assert!(tiny.qty_right - tiny.qty_left > 0.0, "Menge bleibt");
        // Mit KG (Vorgabe an): bei 610 px geht zuerst KG, dann die Einheit.
        let mut with_kg = ColumnWidths::default();
        with_kg.set_shown(Column::WorkCategory, true);
        let roomy = with_kg.layout(0.0, 820.0);
        assert!(roomy.show_kg && roomy.show_lb && roomy.show_unit);
        let medium = with_kg.layout(0.0, 700.0);
        assert!(
            !medium.show_lb && medium.show_kg && medium.show_unit,
            "LB zuerst"
        );
        let half = with_kg.layout(0.0, 610.0);
        assert!(!half.show_kg && !half.show_lb && !half.show_unit);
        assert!(half.show_ep && half.show_gp);
        assert!(text(&half) >= super::MIN_TEXT_HARD, "{}", text(&half));
        let splitters = wide.splitters();
        assert_eq!(splitters.len(), 5);
        assert_eq!(splitters[4], (Column::Total, 868.0));
    }

    #[test]
    fn cost_group_and_work_category_sit_between_text_and_quantity() {
        let mut widths = ColumnWidths::default();
        assert!(widths.shown(Column::CostGroup), "KG: Vorgabe an");
        assert!(!widths.shown(Column::WorkCategory), "LB: Vorgabe aus");
        widths.set_shown(Column::WorkCategory, true);
        let columns = widths.layout(0.0, 1200.0);
        assert!(columns.show_kg && columns.show_lb);
        assert_eq!(columns.text_right, columns.kg_left);
        assert_eq!(columns.kg_right() - columns.kg_left, 52.0);
        assert_eq!(columns.lb_right() - columns.lb_left, 64.0);
        assert_eq!(columns.lb_right(), columns.qty_left);
        assert_eq!(columns.qty_right - columns.qty_left, 82.0);
        let keys: Vec<Column> = columns
            .splitters()
            .into_iter()
            .map(|(column, _)| column)
            .collect();
        assert_eq!(
            keys,
            [
                Column::Oz,
                Column::CostGroup,
                Column::WorkCategory,
                Column::Quantity,
                Column::Unit,
                Column::UnitPrice,
                Column::Total
            ]
        );
        // Ziehen an der linken Kante macht die Spalte breiter, der Kurztext
        // wird schmaler.
        let before = columns.text_right;
        widths.drag(Column::CostGroup, -20.0);
        let wider = widths.layout(0.0, 1200.0);
        assert_eq!(wider.kg_right() - wider.kg_left, 72.0);
        assert_eq!(wider.text_right, before - 20.0);
        // Schmal: LB und KG gehen vor Einheit, EP und GP.
        let narrow = widths.layout(0.0, 500.0 + super::STATUS_WIDTH);
        assert!(!narrow.show_lb && !narrow.show_kg);
        assert!(narrow.show_gp);
        assert_eq!(narrow.text_right, narrow.qty_left);
        widths.set_shown(Column::Oz, false);
        assert!(widths.shown(Column::Oz), "feste Spalten bleiben");
    }

    #[test]
    fn a_narrow_table_shrinks_the_oz_column_to_the_longest_oz_first() {
        let mut widths = ColumnWidths::default();
        widths.set_shown(Column::CostGroup, false);
        let text = |columns: &super::Columns| columns.text_right - columns.text_x;
        // Halbe Bildschirmbreite (Tabelle 574 px): ohne Angabe fiele der EP weg.
        let unknown = widths.layout(0.0, 574.0);
        assert!(!unknown.show_ep && unknown.show_gp);
        // „01.01.0010“ braucht etwa 85 px: der EP bleibt.
        widths.set_oz_need(Some(85.0));
        let fitted = widths.layout(0.0, 574.0);
        assert_eq!(fitted.status_x - fitted.left, 85.0);
        assert!(fitted.show_ep && fitted.show_gp && !fitted.show_unit);
        assert!(text(&fitted) >= super::MIN_TEXT_HARD, "{}", text(&fitted));
        // Breit genug: die OZ-Spalte behält ihre Breite.
        let wide = widths.layout(0.0, 1200.0);
        assert_eq!(wide.status_x - wide.left, 122.0);
        // Nie breiter als eingestellt, Unsinn wird ignoriert.
        widths.set_oz_need(Some(400.0));
        assert_eq!(widths.layout(0.0, 574.0).status_x, unknown.status_x);
        widths.set_oz_need(Some(f32::NAN));
        assert_eq!(widths.layout(0.0, 574.0), unknown);
    }
}
