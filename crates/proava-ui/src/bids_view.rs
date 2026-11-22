//! Angebote: the bids read into the project, their totals, findings and the
//! bidder's data.

use eframe::egui::{
    self, Align, Align2, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use proava_domain::{
    format_decimal_de, Address, Bid, BidId, ComparisonOptions, Decimal, Discount, Money,
    NumberKind, PriceComparison, Project, RowId,
};

use crate::award_dialog::{self, AwardDialog};
use crate::comparison_view::{
    accent_button, big_accent_button, centered_paragraph, plain_button, segment, text_fit, Signal,
};
use crate::icons::Icon;
use crate::list_keys::{ListFocus, ListKey};
use crate::theme::Palette;
use crate::view::{ViewRequest, ViewResponse};
use crate::widgets;

mod assign;

/// Tooltip of the tool buttons without a selection.
const CHOOSE_FIRST: &str = "Erst ein Angebot wählen";

const ROW_H: f32 = 30.0;
const HEAD_H: f32 = 26.0;
const MANUAL_SOURCE: &str = "manuell";

/// Sort key of the bid list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BidSort {
    #[default]
    Rank,
    Name,
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscountKind {
    #[default]
    None,
    Percent,
    Amount,
}

/// One line of the bid list, computed from the project.
#[derive(Debug, Clone, PartialEq)]
pub struct BidRow {
    pub id: BidId,
    pub input: usize,
    pub name: String,
    pub source: String,
    pub date: String,
    pub net: Money,
    pub discount: Money,
    pub net_after_discount: Money,
    pub gross: Money,
    /// None for excluded bids.
    pub rank: Option<usize>,
    pub above_lowest_percent: Option<Decimal>,
    pub missing: usize,
    pub errors: usize,
    pub unknown: usize,
    pub excluded: bool,
    /// All positions that count in the total are priced; incomplete bids
    /// rank after the complete ones.
    pub complete: bool,
}

impl BidRow {
    /// Lowest complete bid – only that one is „günstigstes“.
    pub fn is_lowest(&self) -> bool {
        self.rank == Some(1) && self.complete
    }

    /// Short note for an incomplete bid, e.g. „unvollständig – 3 Preise fehlen“.
    pub fn incomplete_note(&self) -> Option<String> {
        (!self.complete && !self.excluded)
            .then(|| crate::comparison_view::incomplete_note(self.missing))
    }
}

/// Tooltip text explaining why an incomplete bid ranks behind the others.
pub const INCOMPLETE_HINT: &str = "Nicht alle wertungsrelevanten Positionen sind bepreist. \
     Das Angebot wird nach allen vollständigen Angeboten gereiht, zählt nicht als günstigstes \
     und nicht für die Aufgreifschwelle (fehlende Preise, § 16 VOB/A).";

#[derive(Debug, Clone, PartialEq)]
enum Dialog {
    NewBid {
        name: String,
        error: Option<String>,
        focused: bool,
    },
    Remove(BidId),
    /// Angebotspositionen zuordnen (Strukturabweichungen).
    Assign(assign::AssignDialog),
}

#[derive(Debug, Clone)]
enum Action {
    Select(BidId),
    Sort(BidSort),
    ToggleExcluded(BidId),
    NewBidDialog,
    ConfirmNewBid,
    RemoveDialog(BidId),
    ConfirmRemove(BidId),
    CloseDialog,
    ApplyDiscount,
    Award(BidId),
    Contract(BidId),
    Prices(BidId),
    AssignDialog(BidId),
    /// (OZ in the file, LV row, how, similarity).
    ApplyAssignments(BidId, Vec<(String, RowId, proava_domain::AssignedBy, u8)>),
}

pub struct BidsView {
    selected: Option<BidId>,
    sort: BidSort,
    ascending: bool,
    discount_for: Option<BidId>,
    discount_kind: DiscountKind,
    discount_text: String,
    discount_error: Option<String>,
    date_for: Option<BidId>,
    date_text: String,
    /// The typed bid date is not a valid date (the stored one stays).
    date_error: Option<String>,
    dialog: Option<Dialog>,
    award: Option<AwardDialog>,
    /// Today as TT.MM.JJJJ, set by the shell (it owns the clock).
    today: String,
    /// Bid list of the last frame with the project state it belongs to
    /// (revision, active LV); computed again only after a change.
    rows_cache: Option<((u64, usize), Vec<BidRow>)>,
    /// ↑/↓ and Entf after a click into the list.
    keys: ListFocus,
}

impl Default for BidsView {
    fn default() -> Self {
        Self::new()
    }
}

struct Column {
    label: &'static str,
    width: f32,
    right: bool,
    sort: Option<BidSort>,
}

const COLUMNS: &[Column] = &[
    Column {
        label: "Rang",
        width: 54.0,
        right: false,
        sort: Some(BidSort::Rank),
    },
    Column {
        label: "Bieter",
        width: 0.0,
        right: false,
        sort: Some(BidSort::Name),
    },
    Column {
        label: "Quelle",
        width: 156.0,
        right: false,
        sort: Some(BidSort::Input),
    },
    Column {
        label: "Datum",
        width: 84.0,
        right: false,
        sort: None,
    },
    Column {
        label: "Netto",
        width: 112.0,
        right: true,
        sort: None,
    },
    Column {
        label: "Nachlass",
        width: 96.0,
        right: true,
        sort: None,
    },
    Column {
        label: "Netto n. N.",
        width: 116.0,
        right: true,
        sort: None,
    },
    Column {
        label: "Brutto",
        width: 116.0,
        right: true,
        sort: None,
    },
    Column {
        label: "über Günst.",
        width: 92.0,
        right: true,
        sort: None,
    },
    Column {
        label: "fehlend",
        width: 64.0,
        right: true,
        sort: None,
    },
    Column {
        label: "Rechenf.",
        width: 66.0,
        right: true,
        sort: None,
    },
    Column {
        label: "unbek. OZ",
        width: 74.0,
        right: true,
        sort: None,
    },
    Column {
        label: "Ausschluss",
        width: 82.0,
        right: false,
        sort: None,
    },
];
const NAME_MIN: f32 = 210.0;

impl BidsView {
    pub fn new() -> Self {
        Self {
            selected: None,
            sort: BidSort::Rank,
            ascending: true,
            discount_for: None,
            discount_kind: DiscountKind::None,
            discount_text: String::new(),
            discount_error: None,
            date_for: None,
            date_text: String::new(),
            date_error: None,
            dialog: None,
            award: None,
            today: String::new(),
            rows_cache: None,
            keys: ListFocus::default(),
        }
    }

    /// [`bid_rows`], computed once per state of the project.
    fn rows(&mut self, project: &Project) -> Vec<BidRow> {
        let key = (project.revision(), project.active_lv());
        match &self.rows_cache {
            Some((cached, rows)) if *cached == key => rows.clone(),
            _ => {
                let rows = bid_rows(project);
                self.rows_cache = Some((key, rows.clone()));
                rows
            }
        }
    }

    /// Today from the shell's ISO time (`2026-11-17T18:00:00` → `17.11.2026`).
    pub fn set_today(&mut self, iso: &str) {
        let date = iso.get(..10).unwrap_or(iso);
        let pieces: Vec<&str> = date.split('-').collect();
        self.today = match pieces.as_slice() {
            [year, month, day] => format!("{day}.{month}.{year}"),
            _ => String::new(),
        };
    }

    /// The bid the award actions work on: the selected one, else rank 1.
    fn target(&self, project: &Project) -> Option<BidId> {
        self.selected
            .filter(|id| project.bids().iter().any(|bid| bid.id() == *id))
            .or_else(|| {
                let mut rows = bid_rows(project);
                sort_rows(&mut rows, BidSort::Rank, true);
                rows.first().map(|row| row.id)
            })
    }

    /// „Angebot von Hand anlegen …“ (also from the menu Angebote).
    pub fn open_new_bid(&mut self) {
        self.award = None;
        self.dialog = Some(Dialog::NewBid {
            name: String::new(),
            error: None,
            focused: false,
        });
    }

    /// „Auftrag erteilen …“ for the selected (or best) bid.
    pub fn open_award(&mut self, project: &Project) -> bool {
        let Some(id) = self.target(project) else {
            return false;
        };
        self.start_award(AwardDialog::award(id, &self.today));
        true
    }

    /// Opens an award dialog; a question of this view still open (e.g.
    /// „Angebot entfernen“ left behind by switching tabs) is dropped – two
    /// modals from one `ui` would share their id (Affentest).
    fn start_award(&mut self, dialog: AwardDialog) {
        self.dialog = None;
        self.award = Some(dialog);
    }

    /// „Auftrags-LV exportieren …“ for the awarded (or selected) bid.
    pub fn open_contract(&mut self, project: &Project) -> bool {
        let Some(id) = self.target(project) else {
            return false;
        };
        self.start_award(AwardDialog::contract(project, id));
        true
    }

    /// „Preise als Planerpreise übernehmen …“.
    pub fn open_prices(&mut self, project: &Project) -> bool {
        let Some(id) = self.target(project) else {
            return false;
        };
        self.start_award(AwardDialog::prices(id));
        true
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
    ) -> ViewResponse {
        let mut response = ViewResponse::default();
        let mut actions: Vec<Action> = Vec::new();
        let area = ui.available_rect_before_wrap();

        if project.bids().is_empty() {
            self.empty_state(ui, palette, &mut actions, &mut response);
        } else {
            if self
                .selected
                .is_none_or(|id| !project.bids().iter().any(|bid| bid.id() == id))
            {
                self.selected = None;
            }
            let mut rows = self.rows(project);
            sort_rows(&mut rows, self.sort, self.ascending);
            if self.selected.is_none() {
                self.selected = rows.first().map(|row| row.id);
            }

            self.toolbar(ui, palette, &rows, &mut actions, &mut response);
            ui.add_space(6.0);
            self.tool_row(ui, palette, project, &mut actions);
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("bids_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let scroll = self.keys.take_scroll();
                    self.table(ui, palette, &rows, scroll, &mut actions);
                    ui.add_space(14.0);
                    if let Some(row) = self
                        .selected
                        .and_then(|id| rows.iter().find(|row| row.id == id))
                        .cloned()
                    {
                        self.detail(ui, palette, project, &row, &mut actions, &mut response);
                    }
                    ui.add_space(10.0);
                });
            if self.dialog.is_none() && self.award.is_none() && actions.is_empty() {
                let ids: Vec<BidId> = rows.iter().map(|row| row.id).collect();
                match self.keys.key(ui) {
                    Some(key @ (ListKey::Up | ListKey::Down)) => {
                        if let Some(next) =
                            crate::list_keys::step(&ids, self.selected, key == ListKey::Down)
                        {
                            self.selected = Some(next);
                        }
                    }
                    Some(ListKey::Delete) => {
                        if let Some(id) = self.selected {
                            actions.push(Action::RemoveDialog(id));
                        }
                    }
                    Some(ListKey::Enter) | None => {}
                }
            }
            self.keys.track(ui.ctx(), area);
        }

        self.dialogs(ui, palette, project, &mut actions);
        if let Some(dialog) = self.award.as_mut() {
            if award_dialog::show(dialog, ui, palette, project, &mut response) {
                self.award = None;
            }
        }
        for action in actions {
            self.apply(action, project, &mut response);
        }
        response
    }

    fn empty_state(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        actions: &mut Vec<Action>,
        response: &mut ViewResponse,
    ) {
        ui.add_space(70.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Noch keine Angebote")
                    .size(18.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(8.0);
            centered_paragraph(
                ui,
                "Lies die Angebote der Bieter als GAEB-Datei ein (X84 oder D84). proAVA ordnet die \
                 Preise den Positionen des Leistungsverzeichnisses zu, prüft fehlende Preise und \
                 Rechenfehler und stellt alles im Preisspiegel nebeneinander.",
                540.0,
                palette.text_dim,
            );
            ui.add_space(20.0);
            if big_accent_button(ui, palette, "Angebote einlesen (X84/D84) …").clicked() {
                response.requests.push(ViewRequest::ImportBids);
            }
            ui.add_space(10.0);
            if plain_button(ui, palette, "Angebot von Hand anlegen …").clicked() {
                actions.push(Action::NewBidDialog);
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Für Bieter ohne GAEB-Datei – die Preise trägst du dann im Preisspiegel ein.",
                )
                .size(11.0)
                .color(palette.text_dim),
            );
        });
    }

    fn toolbar(
        &self,
        ui: &mut egui::Ui,
        palette: &Palette,
        rows: &[BidRow],
        actions: &mut Vec<Action>,
        response: &mut ViewResponse,
    ) {
        let lowest = rows.iter().find(|row| row.is_lowest());
        let incomplete = rows
            .iter()
            .filter(|row| row.incomplete_note().is_some())
            .count();
        let excluded = rows.iter().filter(|row| row.excluded).count();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Angebote")
                    .size(17.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(10.0);
            let mut summary = crate::format::count(rows.len(), "Angebot", "Angebote");
            if excluded > 0 {
                summary.push_str(&format!(" · {excluded} ausgeschlossen"));
            }
            if incomplete > 0 {
                summary.push_str(&format!(" · {incomplete} unvollständig"));
            }
            if let Some(lowest) = lowest {
                summary.push_str(&format!(
                    " · günstigstes: {} mit {} netto",
                    lowest.name, lowest.net_after_discount
                ));
            }
            ui.label(RichText::new(summary).size(11.5).color(palette.text_dim));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if accent_button(ui, palette, "Angebote einlesen (X84/D84) …").clicked() {
                    response.requests.push(ViewRequest::ImportBids);
                }
                ui.add_space(4.0);
                if plain_button(ui, palette, "Angebot von Hand anlegen …").clicked() {
                    actions.push(Action::NewBidDialog);
                }
            });
        });
    }

    /// Tool row above the list (Anwender 18.11.): what works on the chosen
    /// bid – greyed out without one.
    fn tool_row(
        &self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        actions: &mut Vec<Action>,
    ) {
        let selected = self
            .selected
            .and_then(|id| project.bids().iter().find(|bid| bid.id() == id));
        let chosen = selected.is_some();
        ui.horizontal(|ui| {
            if widgets::list_button(ui, palette, None, "Auftrag erteilen …", chosen, CHOOSE_FIRST)
                .on_hover_text("Dem gewählten Angebot den Auftrag erteilen")
                .clicked()
            {
                if let Some(bid) = selected {
                    actions.push(Action::Award(bid.id()));
                }
            }
            let excluded = selected.is_some_and(Bid::is_excluded);
            let label = if excluded {
                "In den Preisspiegel aufnehmen"
            } else {
                "Vom Preisspiegel ausschließen"
            };
            if widgets::list_button(ui, palette, None, label, chosen, CHOOSE_FIRST).clicked() {
                if let Some(bid) = selected {
                    actions.push(Action::ToggleExcluded(bid.id()));
                }
            }
            if widgets::list_button(ui, palette, Some(Icon::Trash), "Entfernen …", chosen, CHOOSE_FIRST)
                .on_hover_text("Gewähltes Angebot mit seinen Preisen aus dem Projekt entfernen (Entf) – mit Rückfrage")
                .clicked()
            {
                if let Some(bid) = selected {
                    actions.push(Action::RemoveDialog(bid.id()));
                }
            }
        });
    }

    fn table(
        &self,
        ui: &mut egui::Ui,
        palette: &Palette,
        rows: &[BidRow],
        scroll: bool,
        actions: &mut Vec<Action>,
    ) {
        let signal = Signal::of(ui);
        let fixed: f32 = COLUMNS.iter().map(|column| column.width).sum();
        let name_w = (ui.available_width() - fixed - 24.0).max(NAME_MIN);
        let total_w = fixed + name_w + 24.0;
        let widths: Vec<f32> = COLUMNS
            .iter()
            .map(|column| {
                if column.width == 0.0 {
                    name_w
                } else {
                    column.width
                }
            })
            .collect();

        egui::ScrollArea::horizontal()
            .id_salt("bids_table")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let height = HEAD_H + rows.len() as f32 * ROW_H + 10.0;
                let (frame, _) = ui.allocate_exact_size(Vec2::new(total_w, height), Sense::hover());
                let painter = ui.painter().clone();
                painter.rect_filled(frame, CornerRadius::same(8), palette.panel);
                painter.rect_stroke(
                    frame,
                    CornerRadius::same(8),
                    Stroke::new(1.0, palette.line),
                    StrokeKind::Inside,
                );

                let left = frame.left() + 12.0;
                let mut lefts = Vec::with_capacity(widths.len());
                let mut x = left;
                for width in &widths {
                    lefts.push(x);
                    x += width;
                }

                let head = Rect::from_min_size(
                    frame.min + Vec2::new(0.0, 4.0),
                    Vec2::new(total_w, HEAD_H),
                );
                for (index, column) in COLUMNS.iter().enumerate() {
                    let cell = Rect::from_min_size(
                        egui::pos2(lefts[index], head.top()),
                        Vec2::new(widths[index], HEAD_H),
                    );
                    let active = column.sort == Some(self.sort);
                    let color = if active {
                        palette.accent
                    } else {
                        palette.text_dim
                    };
                    let (pos, align) = text_anchor(cell, column.right);
                    let label =
                        painter.text(pos, align, column.label, FontId::proportional(10.5), color);
                    if active {
                        sort_arrow(
                            &painter,
                            egui::pos2(label.right() + 8.0, label.center().y),
                            self.ascending,
                            color,
                        );
                    }
                    if let Some(sort) = column.sort {
                        let response = ui
                            .interact(
                                cell,
                                ui.scope_id().with(("bids_head", index)),
                                Sense::click(),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text("Sortieren");
                        if response.clicked() {
                            actions.push(Action::Sort(sort));
                        }
                    }
                }
                painter.line_segment(
                    [
                        egui::pos2(frame.left() + 8.0, head.bottom()),
                        egui::pos2(frame.right() - 8.0, head.bottom()),
                    ],
                    Stroke::new(1.0, palette.line),
                );

                for (number, row) in rows.iter().enumerate() {
                    let top = head.bottom() + 3.0 + number as f32 * ROW_H;
                    let rect = Rect::from_min_size(
                        egui::pos2(frame.left() + 6.0, top),
                        Vec2::new(total_w - 12.0, ROW_H - 2.0),
                    );
                    let response = ui
                        .interact(
                            rect,
                            ui.scope_id().with(("bids_row", row.id)),
                            Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    let response = match row.incomplete_note() {
                        Some(note) => response.on_hover_text(format!("{note}. {INCOMPLETE_HINT}")),
                        None => response,
                    };
                    let selected = self.selected == Some(row.id);
                    widgets::name_row(&response, &row.name, selected);
                    let fill = if selected {
                        palette.selected
                    } else if response.hovered() {
                        palette.row_hover
                    } else if number % 2 == 1 {
                        palette.row_alt
                    } else {
                        palette.panel
                    };
                    painter.rect_filled(rect, CornerRadius::same(5), fill);
                    if selected {
                        widgets::selection_bar(&painter, palette, rect);
                        if scroll {
                            ui.scroll_to_rect(rect, None);
                        }
                    }
                    if response.clicked() || response.secondary_clicked() {
                        actions.push(Action::Select(row.id));
                    }
                    response.context_menu(|ui| {
                        if ui.button("Auftrag erteilen …").clicked() {
                            actions.push(Action::Award(row.id));
                            ui.close();
                        }
                        if ui.button("Auftrags-LV exportieren …").clicked() {
                            actions.push(Action::Contract(row.id));
                            ui.close();
                        }
                        if ui.button("Preise als Planerpreise übernehmen …").clicked() {
                            actions.push(Action::Prices(row.id));
                            ui.close();
                        }
                        let toggle = if row.excluded {
                            "In den Preisspiegel aufnehmen"
                        } else {
                            "Vom Preisspiegel ausschließen"
                        };
                        if ui.button(toggle).clicked() {
                            actions.push(Action::ToggleExcluded(row.id));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Entfernen …").clicked() {
                            actions.push(Action::RemoveDialog(row.id));
                            ui.close();
                        }
                    });

                    let middle = rect.center().y;
                    let text = if row.excluded {
                        palette.text_dim
                    } else {
                        palette.text
                    };
                    let cell = |index: usize| {
                        Rect::from_min_size(
                            egui::pos2(lefts[index], rect.top()),
                            Vec2::new(widths[index], rect.height()),
                        )
                    };
                    let write = |index: usize, value: String, color: egui::Color32| {
                        let target = cell(index);
                        let (pos, align) = text_anchor(target, COLUMNS[index].right);
                        text_fit(
                            &painter,
                            pos,
                            align,
                            &value,
                            FontId::proportional(12.0),
                            color,
                            target.width() - 12.0,
                        );
                    };

                    match row.rank {
                        Some(rank) => {
                            let badge = Rect::from_center_size(
                                egui::pos2(lefts[0] + 14.0, middle),
                                Vec2::new(24.0, 20.0),
                            );
                            let (fill, color) = if !row.complete {
                                (signal.warn_bg, signal.warn_text)
                            } else if rank == 1 {
                                (signal.good_bg, signal.good_text)
                            } else {
                                (palette.button, palette.text)
                            };
                            painter.rect_filled(badge, CornerRadius::same(10), fill);
                            painter.text(
                                badge.center(),
                                Align2::CENTER_CENTER,
                                rank.to_string(),
                                FontId::proportional(11.5),
                                color,
                            );
                        }
                        None => write(0, "—".to_string(), palette.text_dim),
                    }
                    write(1, row.name.clone(), text);
                    write(2, row.source.clone(), palette.text_dim);
                    write(
                        3,
                        crate::format::iso_to_german_date(&row.date),
                        palette.text_dim,
                    );
                    write(4, row.net.plain(), text);
                    write(
                        5,
                        if row.discount.is_zero() {
                            "—".to_string()
                        } else {
                            (-row.discount).plain()
                        },
                        palette.text_dim,
                    );
                    write(
                        6,
                        row.net_after_discount.plain(),
                        if row.is_lowest() {
                            signal.good_text
                        } else {
                            text
                        },
                    );
                    write(7, row.gross.plain(), text);
                    let (above, above_color) = match (row.rank, row.above_lowest_percent) {
                        _ if row.net_after_discount.is_zero() && !row.excluded => {
                            ("keine Preise".to_string(), signal.warn_text)
                        }
                        _ if row.incomplete_note().is_some() => {
                            ("unvollständig".to_string(), signal.warn_text)
                        }
                        _ if row.is_lowest() => ("günstigstes".to_string(), palette.text_dim),
                        (_, Some(percent)) => (
                            format!("+{} %", format_decimal_de(percent, 2)),
                            palette.text_dim,
                        ),
                        _ => ("—".to_string(), palette.text_dim),
                    };
                    write(8, above, above_color);
                    for (index, count) in [(9, row.missing), (10, row.errors), (11, row.unknown)] {
                        write(
                            index,
                            count.to_string(),
                            if count > 0 {
                                palette.warn
                            } else {
                                palette.text_dim
                            },
                        );
                    }

                    let check = Rect::from_center_size(
                        egui::pos2(lefts[12] + 12.0, middle),
                        Vec2::splat(16.0),
                    );
                    let check_response = ui
                        .interact(
                            check.expand(4.0),
                            ui.scope_id().with(("bids_excluded", row.id)),
                            Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("Vom Preisspiegel ausschließen");
                    checkbox(
                        &painter,
                        palette,
                        check,
                        row.excluded,
                        check_response.hovered(),
                    );
                    if row.excluded {
                        painter.text(
                            egui::pos2(check.right() + 6.0, middle),
                            Align2::LEFT_CENTER,
                            "aus",
                            FontId::proportional(11.0),
                            signal.bad_text,
                        );
                    }
                    if check_response.clicked() {
                        actions.push(Action::ToggleExcluded(row.id));
                    }
                }
            });
    }

    fn detail(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        row: &BidRow,
        actions: &mut Vec<Action>,
        response: &mut ViewResponse,
    ) {
        if self.discount_for != Some(row.id) {
            let discount = project
                .bids()
                .iter()
                .find(|bid| bid.id() == row.id)
                .map(Bid::discount)
                .unwrap_or_default();
            let (kind, text) = discount_fields(discount);
            self.discount_kind = kind;
            self.discount_text = text;
            self.discount_error = None;
            self.discount_for = Some(row.id);
        }
        if self.date_for != Some(row.id) {
            self.date_text = project
                .bids()
                .iter()
                .find(|bid| bid.id() == row.id)
                .and_then(|bid| bid.bid_date().map(crate::format::iso_to_german_date))
                .unwrap_or_default();
            self.date_error = None;
            self.date_for = Some(row.id);
        }

        let signal = Signal::of(ui);
        egui::Frame::new()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.line))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&row.name)
                            .size(15.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("Quelle: {}", row.source))
                            .size(11.0)
                            .color(palette.text_dim),
                    );
                    if row.excluded {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("ausgeschlossen")
                                .size(11.0)
                                .color(signal.bad_text),
                        );
                    }
                });
                ui.add_space(12.0);

                ui.columns(3, |columns| {
                    self.bidder_column(&mut columns[0], palette, project, row.id, response);
                    self.rating_column(&mut columns[1], palette, project, row, actions, response);
                    findings_column(&mut columns[2], palette, project, row, actions, response);
                });
            });
    }

    fn bidder_column(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: BidId,
        response: &mut ViewResponse,
    ) {
        section_title(ui, palette, "Bieter");
        // The fields edit a copy; the bid is written only after a change (a
        // mutable access every frame would void the cached bid list).
        let Some(bid) = project.bids().iter().find(|bid| bid.id() == id) else {
            return;
        };
        let mut address: Address = bid.bidder().clone();
        let address = &mut address;
        let stored_date = bid.bid_date().map(str::to_string);
        let mut new_date: Option<Option<String>> = None;
        let mut changed = false;
        changed |= field(ui, palette, "Name", &mut address.name1);
        if address.name1.trim().is_empty() {
            ui.label(
                RichText::new("Bitte den Namen des Bieters eintragen – er steht im Preisspiegel.")
                    .size(11.0)
                    .color(palette.error),
            );
            ui.add_space(4.0);
        }
        changed |= field(ui, palette, "Zusatz", &mut address.name2);
        changed |= field(ui, palette, "Straße", &mut address.street);
        ui.horizontal(|ui| {
            let width = ui.available_width();
            ui.vertical(|ui| {
                ui.set_width(78.0);
                changed |= field(ui, palette, "PLZ", &mut address.postal_code);
            });
            ui.vertical(|ui| {
                ui.set_width((width - 86.0).max(80.0));
                changed |= field(ui, palette, "Ort", &mut address.city);
            });
        });
        changed |= field(ui, palette, "Telefon", &mut address.phone);
        changed |= field(ui, palette, "E-Mail", &mut address.email);
        if field(ui, palette, "Angebotsdatum", &mut self.date_text) {
            let date = self.date_text.trim();
            if date.is_empty() {
                self.date_error = None;
                new_date = Some(None);
                changed = true;
            } else {
                match crate::format::parse_german_date(date) {
                    Ok(iso) => {
                        self.date_error = None;
                        if stored_date.as_deref() != Some(iso.as_str()) {
                            new_date = Some(Some(iso));
                            changed = true;
                        }
                    }
                    Err(error) => self.date_error = Some(error),
                }
            }
        }
        if let Some(error) = &self.date_error {
            ui.label(RichText::new(error).size(11.0).color(palette.error));
        }
        if changed {
            if let Some(bid) = project.bid_mut(id) {
                *bid.bidder_mut() = address.clone();
                if let Some(date) = new_date {
                    bid.set_bid_date(date);
                }
            }
            response.dirty = true;
        }
    }

    fn rating_column(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        row: &BidRow,
        actions: &mut Vec<Action>,
        response: &mut ViewResponse,
    ) {
        let signal = Signal::of(ui);
        section_title(ui, palette, "Wertung");
        let vat = project.lv().vat(row.net_after_discount);
        let vat_label = format!("USt {} %", format_decimal_de(project.lv().vat_percent(), 0));
        let lines = [
            ("Summe netto", row.net.to_string(), false),
            (
                "Nachlass",
                if row.discount.is_zero() {
                    "—".to_string()
                } else {
                    (-row.discount).to_string()
                },
                false,
            ),
            (
                "Netto nach Nachlass",
                row.net_after_discount.to_string(),
                true,
            ),
            (vat_label.as_str(), vat.to_string(), false),
            ("Brutto", row.gross.to_string(), false),
            (
                "Rang",
                match (row.rank, row.incomplete_note()) {
                    (Some(rank), Some(note)) => format!("{rank}. ({note})"),
                    (Some(rank), None) => format!("{rank}."),
                    (None, _) => "ausgeschlossen".to_string(),
                },
                true,
            ),
        ];
        egui::Grid::new(("bids_values", row.id))
            .num_columns(2)
            .spacing([18.0, 5.0])
            .show(ui, |ui| {
                for (label, value, strong) in lines {
                    ui.label(RichText::new(label).size(11.5).color(palette.text_dim));
                    let mut text = RichText::new(value)
                        .size(if strong { 12.5 } else { 12.0 })
                        .color(palette.text);
                    if strong && row.is_lowest() {
                        text = text.color(signal.good_text);
                    } else if label == "Rang" && !row.complete && !row.excluded {
                        text = text.color(signal.warn_text);
                    }
                    ui.label(text);
                    ui.end_row();
                }
            });

        ui.add_space(14.0);
        ui.label(RichText::new("Nachlass").size(11.0).color(palette.text_dim));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (kind, label) in [
                (DiscountKind::None, "keiner"),
                (DiscountKind::Percent, "in %"),
                (DiscountKind::Amount, "in €"),
            ] {
                if segment(ui, palette, label, self.discount_kind == kind).clicked() {
                    self.discount_kind = kind;
                    self.discount_error = None;
                    if kind == DiscountKind::None {
                        actions.push(Action::ApplyDiscount);
                    }
                }
            }
        });
        if self.discount_kind != DiscountKind::None {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.discount_text)
                        .desired_width(110.0)
                        .align(Align2::RIGHT_CENTER)
                        .hint_text(if self.discount_kind == DiscountKind::Percent {
                            "3,0"
                        } else {
                            "1.500,00"
                        }),
                );
                ui.label(if self.discount_kind == DiscountKind::Percent {
                    "%"
                } else {
                    "€"
                });
                let enter =
                    edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if plain_button(ui, palette, "Übernehmen").clicked() || enter {
                    actions.push(Action::ApplyDiscount);
                }
            });
        }
        if let Some(error) = &self.discount_error {
            ui.add_space(4.0);
            ui.label(RichText::new(error).size(11.0).color(palette.error));
        }

        ui.add_space(16.0);
        let mut excluded = row.excluded;
        if ui
            .checkbox(&mut excluded, "Vom Preisspiegel ausschließen")
            .on_hover_text("Das Angebot bleibt im Projekt, wird aber nicht gewertet.")
            .changed()
        {
            actions.push(Action::ToggleExcluded(row.id));
        }
        ui.add_space(10.0);
        if let Some(bid) = project.bids().iter().find(|bid| bid.id() == row.id) {
            if let Some(award) = bid.award() {
                ui.label(
                    RichText::new(format!(
                        "Beauftragt am {} – Auftragssumme netto {}",
                        award.date, award.total
                    ))
                    .size(12.5)
                    .strong()
                    .color(signal.good_text),
                );
                ui.add_space(6.0);
            }
            let texts: usize = bid.complements().values().map(Vec::len).sum();
            if texts > 0 {
                ui.label(
                    RichText::new(format!(
                        "{texts} Bieterangaben (Fabrikat/Typ) – im Preisspiegel als Tooltip"
                    ))
                    .size(11.0)
                    .color(palette.text_dim),
                );
                ui.add_space(6.0);
            }
        }
        ui.horizontal_wrapped(|ui| {
            if plain_button(ui, palette, "Auftrag erteilen …").clicked() {
                actions.push(Action::Award(row.id));
            }
            if plain_button(ui, palette, "Auftrags-LV exportieren (X86/D86/PDF) …").clicked() {
                actions.push(Action::Contract(row.id));
            }
            if plain_button(ui, palette, "Preise als Planerpreise übernehmen …").clicked() {
                actions.push(Action::Prices(row.id));
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let remove = ui.add(
                egui::Button::new(RichText::new("Angebot entfernen …").color(signal.bad_text))
                    .min_size(Vec2::new(150.0, 28.0)),
            );
            if remove.clicked() {
                actions.push(Action::RemoveDialog(row.id));
            }
        });
        let _ = response;
    }

    fn dialogs(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        actions: &mut Vec<Action>,
    ) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        match dialog {
            Dialog::NewBid {
                name,
                error,
                focused,
            } => {
                let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 420.0, |ui| {
                    widgets::title(
                        ui,
                        palette,
                        "Angebot von Hand anlegen",
                        "Für einen Bieter ohne GAEB-Datei. Die Preise trägst du danach im Preisspiegel per Doppelklick ein.",
                    );
                    ui.label(RichText::new("Bieter").size(11.0).color(palette.text_dim));
                    ui.add_space(4.0);
                    let field = ui.add_sized(
                        [ui.available_width(), 30.0],
                        egui::TextEdit::singleline(name)
                            .hint_text("Name des Bieters")
                            .align(egui::Align2::LEFT_CENTER),
                    );
                    if !*focused {
                        field.request_focus();
                        *focused = true;
                    }
                    ui.add_space(10.0);
                    widgets::error_line(ui, palette, error.as_deref());
                    widgets::dialog_buttons(ui, palette, "Anlegen")
                });
                let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                if cancelled || close {
                    actions.push(Action::CloseDialog);
                } else if confirmed || enter {
                    actions.push(Action::ConfirmNewBid);
                }
            }
            Dialog::Assign(dialog) => match dialog.show(ui, palette, project) {
                assign::AssignOutcome::Open => {}
                assign::AssignOutcome::Cancel => actions.push(Action::CloseDialog),
                assign::AssignOutcome::Apply(chosen) => {
                    actions.push(Action::ApplyAssignments(dialog.bid, chosen))
                }
            },
            Dialog::Remove(id) => {
                let id = *id;
                let name = project
                    .bids()
                    .iter()
                    .find(|bid| bid.id() == id)
                    .map(|bid| bid.name().to_string())
                    .unwrap_or_default();
                let question = remove_bid_question(project, id, &name);
                let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 480.0, |ui| {
                    widgets::title(ui, palette, "Angebot entfernen", "");
                    ui.label(RichText::new(question).size(13.0).color(palette.text));
                    ui.add_space(14.0);
                    widgets::danger_dialog_buttons(ui, palette, "Entfernen")
                });
                let enter = ui.ctx().input(|input| input.key_pressed(egui::Key::Enter));
                if cancelled || close {
                    actions.push(Action::CloseDialog);
                } else if confirmed || enter {
                    actions.push(Action::ConfirmRemove(id));
                }
            }
        }
    }

    fn apply(&mut self, action: Action, project: &mut Project, response: &mut ViewResponse) {
        match action {
            Action::Select(id) => {
                self.selected = Some(id);
                self.keys.take();
            }
            Action::Sort(sort) => {
                if self.sort == sort {
                    self.ascending = !self.ascending;
                } else {
                    self.sort = sort;
                    self.ascending = true;
                }
            }
            Action::ToggleExcluded(id) => {
                if let Some(bid) = project.bid_mut(id) {
                    let excluded = !bid.is_excluded();
                    bid.set_excluded(excluded);
                    response.dirty = true;
                    response.toast = Some(if excluded {
                        format!("{} ist vom Preisspiegel ausgeschlossen.", bid.name())
                    } else {
                        format!("{} wird wieder gewertet.", bid.name())
                    });
                }
            }
            Action::NewBidDialog => {
                self.dialog = Some(Dialog::NewBid {
                    name: String::new(),
                    error: None,
                    focused: false,
                })
            }
            Action::ConfirmNewBid => {
                let Some(Dialog::NewBid {
                    name,
                    error,
                    focused,
                }) = self.dialog.as_mut()
                else {
                    return;
                };
                let name = name.trim().to_string();
                if name.is_empty() {
                    *error = Some("Bitte den Namen des Bieters eintragen.".to_string());
                    // Enter nimmt dem Feld den Fokus – gleich weitertippen können.
                    *focused = false;
                    return;
                }
                let id = project.add_bid(Bid::new(Address::named(&name), MANUAL_SOURCE));
                self.selected = Some(id);
                self.dialog = None;
                response.dirty = true;
                response.toast = Some(format!(
                    "Angebot {name} angelegt – Preise im Preisspiegel per Doppelklick eintragen."
                ));
            }
            Action::RemoveDialog(id) => self.dialog = Some(Dialog::Remove(id)),
            Action::AssignDialog(id) => {
                self.dialog = assign::AssignDialog::of(project, id).map(Dialog::Assign);
            }
            Action::ApplyAssignments(id, chosen) => {
                self.dialog = None;
                let lv = project.lv().clone();
                let Some(bid) = project.bid_mut(id) else {
                    return;
                };
                let mut done = 0;
                let mut refused = Vec::new();
                for (ordinal, row, by, similarity) in chosen {
                    match bid.assign_item(&lv, &ordinal, row, by, similarity) {
                        Ok(()) => done += 1,
                        Err(error) => refused.push(error),
                    }
                }
                if done > 0 {
                    response.dirty = true;
                    response.label = Some("Angebotspositionen zuordnen");
                }
                let mut toast = format!(
                    "{} zugeordnet – Strg+Z nimmt es zurück.",
                    crate::format::count(done, "Position", "Positionen")
                );
                if let Some(first) = refused.first() {
                    toast.push_str(&format!(" Nicht möglich: {first}"));
                }
                response.toast = Some(toast);
            }
            Action::ConfirmRemove(id) => {
                if let Some(bid) = project.remove_bid(id) {
                    response.dirty = true;
                    response.label = Some("Angebot entfernen");
                    response.toast = Some(format!(
                        "Angebot {} entfernt – Strg+Z holt es zurück.",
                        bid.name()
                    ));
                }
                if self.selected == Some(id) {
                    self.selected = None;
                }
                self.dialog = None;
                self.keys.take();
            }
            Action::CloseDialog => {
                self.dialog = None;
                self.keys.take();
            }
            Action::Award(id) => self.award = Some(AwardDialog::award(id, &self.today)),
            Action::Contract(id) => self.award = Some(AwardDialog::contract(project, id)),
            Action::Prices(id) => self.award = Some(AwardDialog::prices(id)),
            Action::ApplyDiscount => {
                let Some(id) = self.selected else {
                    return;
                };
                match parse_discount(self.discount_kind, &self.discount_text) {
                    Ok(discount) => {
                        self.discount_error = None;
                        if let Some(bid) = project.bid_mut(id) {
                            if bid.discount() != discount {
                                bid.set_discount(discount);
                                response.dirty = true;
                                response.toast = Some(format!(
                                    "Nachlass für {}: {}",
                                    bid.name(),
                                    discount_label(discount)
                                ));
                            }
                        }
                        let (kind, text) = discount_fields(discount);
                        self.discount_kind = kind;
                        self.discount_text = text;
                    }
                    Err(message) => self.discount_error = Some(message),
                }
            }
        }
    }
}

/// Text of the question before removing a bid: what else it touches.
fn remove_bid_question(project: &Project, id: BidId, name: &str) -> String {
    let mut text = format!("„{name}“ mit allen Preisen aus dem Projekt entfernen?");
    if let Some(invitee) = project
        .invitees()
        .iter()
        .find(|invitee| invitee.bid == Some(id))
    {
        text.push_str(&format!(
            "\n\nIm Bieterverzeichnis bleibt „{}“ stehen, nur die Verknüpfung entfällt.",
            invitee.name()
        ));
    }
    let awarded = project
        .bids()
        .iter()
        .find(|bid| bid.id() == id)
        .is_some_and(Bid::is_awarded);
    if awarded && project.contract().is_some() {
        text.push_str(
            "\n\nDas Angebot ist beauftragt; der festgeschriebene Auftrag bleibt bestehen.",
        );
    } else if awarded {
        text.push_str("\n\nDas Angebot ist beauftragt – mit ihm entfällt die Auftragserteilung.");
    }
    text.push_str("\n\nStrg+Z holt es zurück.");
    text
}

fn findings_column(
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &Project,
    row: &BidRow,
    actions: &mut Vec<Action>,
    response: &mut ViewResponse,
) {
    let signal = Signal::of(ui);
    section_title(ui, palette, "Prüfbefunde");
    let Some(bid) = project.bids().iter().find(|bid| bid.id() == row.id) else {
        return;
    };
    let lv = project.lv();
    structure_box(ui, palette, project, bid, actions);
    let missing = bid.missing_positions(lv);
    let errors = bid.arithmetic_errors(lv);
    let unknown = bid.unknown_ordinals();

    if lv.position_count() == 0 {
        ui.label(
            RichText::new("Das LV hat noch keine Positionen – nichts zu prüfen.")
                .size(12.0)
                .color(palette.text_dim),
        );
        return;
    }
    if missing.is_empty() && errors.is_empty() && unknown.is_empty() {
        ui.label(
            RichText::new("Keine Befunde – alle Positionen bepreist, rechnerisch richtig.")
                .size(12.0)
                .color(signal.good_text),
        );
        return;
    }
    if bid.source() == MANUAL_SOURCE && missing.len() == lv.position_count() {
        ui.label(
            RichText::new("Angebot von Hand: Preise im Preisspiegel per Doppelklick auf die EP-Zelle eintragen.")
                .size(11.5)
                .color(palette.text_dim),
        );
        ui.add_space(8.0);
    }

    finding_header(ui, palette, "Fehlende Preise", missing.len());
    if !missing.is_empty() {
        egui::ScrollArea::vertical()
            .id_salt(("bids_missing", row.id))
            .max_height(150.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for position in &missing {
                    if link_row(
                        ui,
                        palette,
                        position.ordinal(),
                        position.short_text(),
                        position.id(),
                    ) {
                        response.requests.push(ViewRequest::ShowInLv(position.id()));
                    }
                }
            });
    }
    ui.add_space(10.0);

    finding_header(ui, palette, "Rechenfehler", errors.len());
    for (position, stated, computed) in &errors {
        let detail = format!("genannt {stated} · gerechnet {computed}");
        if link_row(ui, palette, position.ordinal(), &detail, position.id()) {
            response.requests.push(ViewRequest::ShowInLv(position.id()));
        }
    }
    if !errors.is_empty() {
        ui.label(
            RichText::new("Gewertet wird der Einheitspreis (§ 16c VOB/A).")
                .size(10.5)
                .color(palette.text_dim),
        );
    }
    ui.add_space(10.0);

    finding_header(ui, palette, "Unbekannte OZ aus der Datei", unknown.len());
    for ordinal in unknown {
        ui.label(
            RichText::new(ordinal)
                .monospace()
                .size(11.5)
                .color(palette.text),
        );
    }
}

/// Strukturabweichungen of the file, conspicuous above the other findings
/// (Anwender 20.11.), with the way to assign file positions.
fn structure_box(
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &Project,
    bid: &Bid,
    actions: &mut Vec<Action>,
) {
    let lines = proava_application::protocol::structure_lines(project, bid, 500);
    if lines.is_empty() {
        return;
    }
    let signal = Signal::of(ui);
    egui::Frame::new()
        .fill(signal.warn_bg)
        .stroke(egui::Stroke::new(1.0, signal.warn_text))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut rest = lines.iter();
            if let Some(first) = rest.next() {
                ui.label(RichText::new(first).size(12.0).strong().color(signal.warn_text));
            }
            egui::ScrollArea::vertical()
                .id_salt(("bid_structure", bid.id().0))
                .max_height(160.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for line in rest {
                        ui.label(RichText::new(line).size(11.0).color(palette.text));
                    }
                });
            let open = bid.structure().suggestions.len() + bid.structure().foreign.len();
            if open > 0 {
                ui.add_space(4.0);
                if ui
                    .button("Positionen zuordnen …")
                    .on_hover_text(
                        "Positionen der Datei ohne passende OZ einer LV-Position zuweisen – Vorschläge über Reihenfolge, Menge und Text",
                    )
                    .clicked()
                {
                    actions.push(Action::AssignDialog(bid.id()));
                }
            }
        });
    ui.add_space(10.0);
}

fn section_title(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).size(13.0).strong().color(palette.text));
    ui.add_space(8.0);
}

fn finding_header(ui: &mut egui::Ui, palette: &Palette, text: &str, count: usize) {
    let signal = Signal::of(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).size(11.5).strong().color(palette.text));
        let (label, color) = if count == 0 {
            ("keine".to_string(), signal.good_text)
        } else {
            (count.to_string(), palette.warn)
        };
        ui.label(RichText::new(label).size(11.5).color(color));
    });
    ui.add_space(2.0);
}

fn link_row(ui: &mut egui::Ui, palette: &Palette, ordinal: &str, text: &str, id: RowId) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Im Leistungsverzeichnis zeigen");
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.row_hover);
    }
    let painter = ui.painter();
    let ordinal_rect = painter.text(
        egui::pos2(rect.left() + 4.0, rect.center().y),
        Align2::LEFT_CENTER,
        ordinal,
        FontId::monospace(11.0),
        palette.accent,
    );
    // Lange OZ (01.481.10.0015) dürfen nicht in den Text laufen.
    let text_left = (ordinal_rect.right() + 8.0).max(rect.left() + 92.0);
    text_fit(
        painter,
        egui::pos2(text_left, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.5),
        palette.text,
        rect.right() - text_left - 4.0,
    );
    let _ = id;
    response.clicked()
}

fn field(ui: &mut egui::Ui, palette: &Palette, label: &str, value: &mut String) -> bool {
    ui.label(RichText::new(label).size(10.5).color(palette.text_dim));
    ui.add_space(2.0);
    let changed = ui
        .add_sized(
            [ui.available_width(), 26.0],
            egui::TextEdit::singleline(value).align(egui::Align2::LEFT_CENTER),
        )
        .changed();
    ui.add_space(6.0);
    changed
}

fn checkbox(painter: &egui::Painter, palette: &Palette, rect: Rect, checked: bool, hovered: bool) {
    let (fill, stroke) = if checked {
        (palette.accent, palette.accent)
    } else if hovered {
        (palette.button_hover, palette.accent)
    } else {
        (palette.input, palette.line)
    };
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.2, stroke),
        StrokeKind::Inside,
    );
    if checked {
        let stroke = Stroke::new(2.0, palette.on_accent);
        let a = rect.left_top() + Vec2::new(3.5, 8.5);
        let b = rect.left_top() + Vec2::new(7.0, 12.0);
        let c = rect.left_top() + Vec2::new(12.5, 4.5);
        painter.line_segment([a, b], stroke);
        painter.line_segment([b, c], stroke);
    }
}

fn sort_arrow(painter: &egui::Painter, center: egui::Pos2, up: bool, color: egui::Color32) {
    let (tip, base) = if up { (-3.0, 2.5) } else { (3.0, -2.5) };
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(0.0, tip),
            center + Vec2::new(4.0, base),
            center + Vec2::new(-4.0, base),
        ],
        color,
        Stroke::NONE,
    ));
}

fn text_anchor(cell: Rect, right: bool) -> (egui::Pos2, Align2) {
    if right {
        (
            egui::pos2(cell.right() - 8.0, cell.center().y),
            Align2::RIGHT_CENTER,
        )
    } else {
        (
            egui::pos2(cell.left() + 4.0, cell.center().y),
            Align2::LEFT_CENTER,
        )
    }
}

// ---------------------------------------------------------------------------
// Logic without egui.

/// The bid list with totals; ranks as in the Preisspiegel (default options).
pub fn bid_rows(project: &Project) -> Vec<BidRow> {
    let lv = project.lv();
    let comparison = PriceComparison::build(lv, project.bids(), ComparisonOptions::default());
    project
        .bids()
        .iter()
        .enumerate()
        .map(|(input, bid)| {
            let summary = comparison
                .bids
                .iter()
                .find(|summary| summary.bid == bid.id());
            let net = bid.net_total(lv);
            let discount = bid.discount_amount(net);
            let net_after_discount = net - discount;
            BidRow {
                id: bid.id(),
                input,
                name: if bid.name().trim().is_empty() {
                    "(ohne Namen)".to_string()
                } else {
                    bid.name().to_string()
                },
                source: bid.source().to_string(),
                date: bid.bid_date().unwrap_or_default().to_string(),
                net,
                discount,
                net_after_discount,
                gross: net_after_discount + lv.vat(net_after_discount),
                rank: summary.map(|summary| summary.rank),
                above_lowest_percent: summary.map(|summary| summary.above_lowest_percent),
                missing: bid.missing_positions(lv).len(),
                errors: bid.arithmetic_errors(lv).len(),
                unknown: bid.unknown_ordinals().len(),
                excluded: bid.is_excluded(),
                complete: summary.is_none_or(|summary| summary.complete),
            }
        })
        .collect()
}

/// Excluded bids (no rank) stay at the end when sorting by rank.
pub fn sort_rows(rows: &mut [BidRow], sort: BidSort, ascending: bool) {
    match sort {
        BidSort::Rank => rows.sort_by(|a, b| {
            let order = a
                .rank
                .unwrap_or(usize::MAX)
                .cmp(&b.rank.unwrap_or(usize::MAX));
            match (a.rank, b.rank) {
                (Some(_), Some(_)) if !ascending => order.reverse(),
                _ => order,
            }
        }),
        BidSort::Name => {
            rows.sort_by_key(|row| row.name.to_lowercase());
            if !ascending {
                rows.reverse();
            }
        }
        BidSort::Input => {
            rows.sort_by_key(|row| row.input);
            if !ascending {
                rows.reverse();
            }
        }
    }
}

/// Checks the discount input: percent between 0 and 100, amount above 0.
pub fn parse_discount(kind: DiscountKind, text: &str) -> Result<Discount, String> {
    match kind {
        DiscountKind::None => Ok(Discount::None),
        DiscountKind::Percent => {
            if text.trim().is_empty() {
                return Err("Bitte einen Prozentsatz eintragen.".to_string());
            }
            // Same limits as the MCP server (`input_limits`).
            let value = NumberKind::DiscountPercent.parse(text.trim().trim_end_matches('%'))?;
            Ok(Discount::Percent(value))
        }
        DiscountKind::Amount => {
            if text.trim().is_empty() {
                return Err("Bitte einen Betrag eintragen.".to_string());
            }
            let value = NumberKind::Amount
                .parse(text.trim().trim_end_matches('€'))
                .map_err(|error| format!("Nachlass: {error}"))?;
            if value <= Decimal::ZERO {
                return Err("Der Nachlass muss größer als 0 € sein.".to_string());
            }
            Ok(Discount::Amount(Money::from_decimal(value)))
        }
    }
}

/// Input fields for a stored discount.
pub fn discount_fields(discount: Discount) -> (DiscountKind, String) {
    match discount {
        Discount::None => (DiscountKind::None, String::new()),
        Discount::Percent(percent) => (DiscountKind::Percent, decimal_input(percent)),
        Discount::Amount(amount) => (DiscountKind::Amount, amount.plain()),
    }
}

fn decimal_input(value: Decimal) -> String {
    let text = format_decimal_de(value, 3);
    text.trim_end_matches('0').trim_end_matches(',').to_string()
}

pub fn discount_label(discount: Discount) -> String {
    match discount {
        Discount::None => "kein Nachlass".to_string(),
        Discount::Percent(percent) => format!("{} %", decimal_input(percent)),
        Discount::Amount(amount) => amount.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use proava_domain::{Decimal, Discount, Money};

    use super::{bid_rows, discount_fields, parse_discount, sort_rows, BidSort, DiscountKind};
    use crate::demo::demo_project;

    #[test]
    fn discount_input_is_checked() {
        assert_eq!(
            parse_discount(DiscountKind::None, "abc").unwrap(),
            Discount::None
        );
        assert_eq!(
            parse_discount(DiscountKind::Percent, "3,5 %").unwrap(),
            Discount::Percent("3.5".parse().unwrap())
        );
        assert!(parse_discount(DiscountKind::Percent, "").is_err());
        assert!(parse_discount(DiscountKind::Percent, "100").is_err());
        assert!(parse_discount(DiscountKind::Percent, "-1").is_err());
        assert_eq!(
            parse_discount(DiscountKind::Amount, "1.500,5").unwrap(),
            Discount::Amount(Money::from_cents(150050))
        );
        assert!(parse_discount(DiscountKind::Amount, "0").is_err());
        assert!(parse_discount(DiscountKind::Amount, "viel").is_err());

        assert_eq!(
            discount_fields(Discount::Percent(Decimal::from(3))),
            (DiscountKind::Percent, "3".to_string())
        );
        assert_eq!(
            discount_fields(Discount::Amount(Money::from_cents(123456))),
            (DiscountKind::Amount, "1.234,56".to_string())
        );
    }

    #[test]
    fn rows_and_sorting() {
        let mut project = demo_project();
        let excluded = project.bids()[0].id();
        project.bid_mut(excluded).unwrap().set_excluded(true);
        let mut rows = bid_rows(&project);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].rank, None);
        assert_eq!(rows[2].missing, 3);
        assert_eq!(rows[2].errors, 1);
        assert_eq!(rows[2].unknown, 2);
        assert!(!rows[1].discount.is_zero());

        // Das Angebot mit fehlenden Preisen ist unvollständig und nie „günstigstes“.
        assert!(!rows[2].complete);
        assert!(!rows[2].is_lowest());
        assert_eq!(
            rows[2].incomplete_note().as_deref(),
            Some("unvollständig – 3 Preise fehlen")
        );
        assert!(rows[0].incomplete_note().is_none());
        let lowest: Vec<_> = rows.iter().filter(|row| row.is_lowest()).collect();
        assert!(lowest.len() <= 1);
        assert!(lowest.iter().all(|row| row.complete));

        sort_rows(&mut rows, BidSort::Rank, true);
        assert_eq!(rows[0].rank, Some(1));
        assert_eq!(rows[3].rank, None);
        sort_rows(&mut rows, BidSort::Rank, false);
        assert_eq!(rows[0].rank, Some(3));
        assert_eq!(rows[3].rank, None);

        sort_rows(&mut rows, BidSort::Name, true);
        assert!(rows[0].name.starts_with("Automation"));
        sort_rows(&mut rows, BidSort::Input, false);
        assert_eq!(rows[0].input, 3);
    }
}
