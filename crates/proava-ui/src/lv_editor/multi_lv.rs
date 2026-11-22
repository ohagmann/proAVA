//! Several LVs of a project one below the other in the same table (Anwender
//! 18.11. 22:16: „wenn ich ein zweites LV anlege, steht es einfach unter dem
//! ersten“).
//!
//! The editor keeps working on the *active* LV (`Project::active_lv`): its
//! rows are drawn and edited as before. The rows of the other LVs are drawn
//! from `Project::lv_at` without editing; a click, right click, double click
//! or a step of the keyboard into another LV hands the shell an [`LvJump`] –
//! it makes that LV the active one (its undo stack and view, see
//! `History::switch`) and the editor then selects the row. So the active LV is
//! always the LV of the Schreibmarke, and every LV command (Drucken, GAEB,
//! LV prüfen, Neu nummerieren …) means that LV.
//!
//! The table is one list of [`ViewRow`]s: a head row per LV (only when the
//! project has more than one) and the shown rows of each LV. All rows are
//! [`super::ROW_HEIGHT`] high, so the table still draws only what is in view.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Align2, FontId, Rect, Sense, Stroke, Vec2};
use proava_domain::{GroupTotal, Lv, Project, RowId, SurchargeComputation};

use crate::columns::ColumnWidths;
use crate::lv_list::LvListAction;
use crate::lv_properties::lv_label;
use crate::theme::Palette;

/// One row of the table over all LVs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewRow {
    /// Head row of the LV at this index (only with several LVs).
    Head(usize),
    /// Row `index` of the LV at `lv`.
    Row(usize, usize),
}

impl ViewRow {
    pub fn lv(self) -> usize {
        match self {
            ViewRow::Head(lv) | ViewRow::Row(lv, _) => lv,
        }
    }
}

/// What follows when the selection goes into another LV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpThen {
    /// Select the row (click, arrow keys, search hit).
    Select,
    /// Select it and open its context menu (right click).
    Menu,
    /// Select it and open the window „Eigenschaften“ (double click).
    Window,
    /// Select it and go on dragging it (a drag started on a row of another LV).
    Drag,
    /// Only make the LV active and show its head row (click on the head,
    /// „Gehe zu LV“); the selection is the one the LV had, else its first row.
    Head,
}

/// The selection moves into the LV at `lv` (made active by the shell).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LvJump {
    pub lv: usize,
    pub row: Option<RowId>,
    pub then: JumpThen,
}

/// The table rows over the LVs `shown` (in their order): per LV its head row
/// (when `heads`) and, unless the LV is folded, its shown rows (`rows(lv)`).
pub fn view_rows(
    shown: &[usize],
    heads: bool,
    mut rows: impl FnMut(usize) -> Vec<usize>,
    folded: impl Fn(usize) -> bool,
) -> Vec<ViewRow> {
    if !heads {
        return shown
            .iter()
            .flat_map(|&lv| {
                rows(lv)
                    .into_iter()
                    .map(move |index| ViewRow::Row(lv, index))
            })
            .collect();
    }
    let mut out = Vec::new();
    for &lv in shown {
        out.push(ViewRow::Head(lv));
        if !folded(lv) {
            out.extend(rows(lv).into_iter().map(|index| ViewRow::Row(lv, index)));
        }
    }
    out
}

/// The LVs of the table with the active one: all LVs of the tender (the
/// Ausschreibung) one below the other, a Kostenermittlung alone
/// ([`Project::lv_indices_shown_with`]).
pub fn shown(project: &Project) -> Vec<usize> {
    project.lv_indices_shown_with(project.active_lv())
}

/// Every LV in the table carries its head row as soon as the project has
/// more than one LV – also a Kostenermittlung shown alone, so the table
/// always says which LV it is.
pub fn heads(project: &Project) -> bool {
    project.lv_count() > 1
}

/// Where Suchen / Weitersuchen looks, in this order: the active LV after
/// `start`, the LVs of the table below it, those above, the active LV up to
/// `start` (a full round over the table).
pub fn search_pieces(project: &Project, start: usize) -> Vec<(usize, std::ops::Range<usize>)> {
    let active = project.active_lv();
    let shown = shown(project);
    let at = shown.iter().position(|lv| *lv == active).unwrap_or(0);
    let mut pieces = vec![(active, start..usize::MAX)];
    pieces.extend(
        (1..shown.len())
            .map(|step| shown[(at + step) % shown.len()])
            .filter(|lv| *lv != active)
            .map(|lv| (lv, 0..usize::MAX)),
    );
    pieces.push((active, 0..start));
    pieces
}

/// Sums and depths of a not active LV, computed once per project revision.
#[derive(Default)]
pub struct OtherLvs {
    revision: u64,
    lvs: HashMap<usize, OtherLv>,
}

pub struct OtherLv {
    pub depths: Vec<usize>,
    pub totals: Vec<GroupTotal>,
    /// Zuschläge, Rechengrundlage and Aufteilungsblöcke.
    pub surcharges: SurchargeComputation,
}

impl OtherLvs {
    pub fn of(&mut self, project: &Project, lv: usize) -> Option<&OtherLv> {
        if self.revision != project.revision() {
            self.revision = project.revision();
            self.lvs.clear();
        }
        let data = project.lv_at(lv)?;
        Some(self.lvs.entry(lv).or_insert_with(|| OtherLv {
            depths: super::depths(data),
            totals: data.group_totals(),
            surcharges: data.split_computation(),
        }))
    }
}

/// Folded titles of the LVs that are not active, by session key (the active
/// LV's are the editor's `collapsed`), and the folded LV head rows.
#[derive(Default, Clone)]
pub struct Folding {
    pub titles: HashMap<u64, HashSet<RowId>>,
    pub lvs: HashSet<u64>,
}

/// What a click on a head row asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadClick {
    Select,
    Toggle,
    Properties,
}

/// Name of a head row for screen readers and UI tests.
pub fn head_name(lv: &Lv) -> String {
    match lv.purpose().cost_stage() {
        Some(stage) => format!("LV-Kopf {} · {}", lv_label(lv), stage.label()),
        None => format!("LV-Kopf {}", lv_label(lv)),
    }
}

/// The head row of an LV: a strong band with „LV 01 – Bezeichnung“, a
/// disclosure triangle and the net sum of the LV.
pub fn head_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    lv: &Lv,
    folded: bool,
    active: bool,
    widths: &ColumnWidths,
) -> (egui::Response, Option<HeadClick>) {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, super::ROW_HEIGHT), Sense::click());
    let name = head_name(lv);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Row, true, active, &name));
    let painter = ui.painter_at(rect);
    let fill = if response.hovered() {
        palette.lv_head_hover
    } else {
        palette.lv_head
    };
    painter.rect_filled(rect, egui::CornerRadius::ZERO, fill);
    if active {
        // The LV of the Schreibmarke: a light edge like the selected row.
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(4.0, rect.height())),
            egui::CornerRadius::ZERO,
            palette.on_lv_head,
        );
    }
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.bottom() - 0.5),
            egui::pos2(rect.right(), rect.bottom() - 0.5),
        ],
        Stroke::new(1.0, palette.line),
    );
    let columns = widths.layout(rect.left(), rect.right());
    let middle = rect.center().y;
    let chevron = Rect::from_center_size(egui::pos2(columns.left + 9.0, middle), Vec2::splat(14.0));
    crate::icons::disclosure(&painter, chevron.center(), folded, palette.on_lv_head);
    let total = if lv.currency() == "EUR" {
        lv.net_total().to_string()
    } else {
        format!("{} {}", lv.net_total().plain(), lv.currency())
    };
    let total_galley = painter.layout_no_wrap(total, crate::fonts::bold(13.5), palette.on_lv_head);
    let total_left = rect.right() - 12.0 - total_galley.size().x;
    painter.galley(
        egui::pos2(total_left, middle - total_galley.size().y / 2.0),
        total_galley,
        palette.on_lv_head,
    );
    let text_left = columns.left + 24.0;
    let mut positions = crate::format::count(lv.position_count(), "Position", "Positionen");
    // A Kostenermittlung says so (it stands alone, never among the LVs of
    // the tender).
    if let Some(stage) = lv.purpose().cost_stage() {
        positions = format!("{} · {positions}", stage.label());
    }
    let note = painter.layout_no_wrap(
        positions,
        FontId::proportional(12.0),
        palette.on_lv_head_dim,
    );
    let note_width = note.size().x;
    let (label, _) = crate::widgets::fit_line(
        &painter,
        &lv_label(lv),
        &crate::fonts::bold(14.5),
        (total_left - text_left - note_width - 32.0).max(40.0),
    );
    let label_rect = crate::widgets::bold_text(
        &painter,
        egui::pos2(text_left, middle),
        Align2::LEFT_CENTER,
        &label,
        FontId::proportional(14.5),
        palette.on_lv_head,
    );
    painter.galley(
        egui::pos2(label_rect.right() + 14.0, middle - note.size().y / 2.0),
        note,
        palette.on_lv_head_dim,
    );
    let mut click = None;
    if response.double_clicked() {
        click = Some(HeadClick::Properties);
    } else if response.clicked() {
        let on_chevron = response
            .interact_pointer_pos()
            .is_some_and(|pos| chevron.expand(6.0).contains(pos));
        click = Some(if on_chevron {
            HeadClick::Toggle
        } else {
            HeadClick::Select
        });
    }
    let response = response.on_hover_text(format!(
        "{} – Klick: zu diesem LV, Dreieck: auf-/zuklappen, Rechtsklick: LV-Befehle",
        lv_label(lv)
    ));
    (response, click)
}

/// Context menu of an LV head row: the LV commands of the menu LV.
pub fn head_menu(
    ui: &mut egui::Ui,
    index: usize,
    lv_count: usize,
    can_paste: bool,
) -> Option<LvListAction> {
    let mut chosen = None;
    let mut entry = |ui: &mut egui::Ui, label: &str, enabled: bool, action: LvListAction| {
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            chosen = Some(action);
        }
    };
    entry(
        ui,
        "LV-Eigenschaften …",
        true,
        LvListAction::Properties(index),
    );
    entry(ui, "Neues LV …", true, LvListAction::New);
    entry(
        ui,
        "LV duplizieren (ohne Angebote)",
        true,
        LvListAction::Duplicate(index),
    );
    ui.separator();
    entry(ui, "Ganzes LV kopieren", true, LvListAction::Copy(index));
    entry(
        ui,
        "LV aus Zwischenablage einfügen …",
        can_paste,
        LvListAction::Paste,
    );
    ui.separator();
    entry(
        ui,
        "LV nach oben",
        index > 0,
        LvListAction::Move { index, up: true },
    );
    entry(
        ui,
        "LV nach unten",
        index + 1 < lv_count,
        LvListAction::Move { index, up: false },
    );
    ui.separator();
    entry(
        ui,
        "LV löschen …",
        lv_count > 1,
        LvListAction::Delete(index),
    );
    if chosen.is_some() {
        ui.close();
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_lv_has_no_head_row_several_have_one_each() {
        let rows = |lv: usize| if lv == 0 { vec![0, 1, 2] } else { vec![0, 3] };
        assert_eq!(
            view_rows(&[0], false, rows, |_| false),
            [ViewRow::Row(0, 0), ViewRow::Row(0, 1), ViewRow::Row(0, 2)]
        );
        assert_eq!(
            view_rows(&[0, 1], true, rows, |_| false),
            [
                ViewRow::Head(0),
                ViewRow::Row(0, 0),
                ViewRow::Row(0, 1),
                ViewRow::Row(0, 2),
                ViewRow::Head(1),
                ViewRow::Row(1, 0),
                ViewRow::Row(1, 3),
            ]
        );
        // A folded LV shows only its head row.
        assert_eq!(
            view_rows(&[0, 1], true, rows, |lv| lv == 0),
            [
                ViewRow::Head(0),
                ViewRow::Head(1),
                ViewRow::Row(1, 0),
                ViewRow::Row(1, 3)
            ]
        );
        // A Kostenermittlung alone (the project has more LVs): its head row.
        assert_eq!(
            view_rows(&[1], true, rows, |_| false),
            [ViewRow::Head(1), ViewRow::Row(1, 0), ViewRow::Row(1, 3)]
        );
        assert_eq!(ViewRow::Row(1, 3).lv(), 1);
    }
}
