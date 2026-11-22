//! Reiter „Nachträge“ des aktiven LV: Nachtragsliste mit Filter und
//! Summen, Nachtragsübersicht (Wirkung auf die Auftragssumme, Prognose) und
//! das Bearbeiten eines Nachtrags (Kopf, Nachtrags-LV mit angebotenen und
//! geprüften Preisen, Prüfhilfe, Langtext, Statusverlauf).
//!
//! Every change is one undo step of the workspace (Auftrag, Nachträge,
//! Abrechnung – `ContractSnapshot`); steps that add or edit positions keep
//! the LV as well ([`ChangeOrdersOutput::before`]).

use std::collections::HashMap;

use eframe::egui::{self, RichText};
use proava_domain::{
    change_order_overview, format_decimal_de, sums_by_status, ChangeBasis, ChangeOrder,
    ChangeOrderId, ChangeOrderStatus, ContractSnapshot, Decimal, Money, Project, RowId,
};

use crate::format::count;
use crate::numbers;
use crate::theme::Palette;
use crate::view::ViewResponse;

mod editor;

pub use editor::ChangeOrderExport;

/// What the shell does for the Nachträge (files, print).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOrderRequest {
    /// PDF „Prüfung Nachtragsangebot“ of one change order.
    PrintCheck(ChangeOrderId),
    /// PDF „Nachtragsübersicht“ of the LV.
    PrintOverview,
    /// Nachtrags-LV as GAEB file.
    Export(ChangeOrderId, ChangeOrderExport),
    /// Read the contractor's offer (X84/X86/X82/X83, D8x) into the change order.
    ImportOffer(ChangeOrderId),
}

/// Result of one frame of the Nachträge.
#[derive(Default)]
pub struct ChangeOrdersOutput {
    pub response: ViewResponse,
    /// Name of the change for the undo history.
    pub label: Option<&'static str>,
    /// State with the LV before a change that also changed the LV (new
    /// positions, texts); `join` when only text was typed.
    pub before: Option<(ContractSnapshot, bool)>,
    /// The LV was changed (new positions, texts): the editor reloads.
    pub lv_changed: bool,
    pub requests: Vec<ChangeOrderRequest>,
}

/// Status filter of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListFilter {
    #[default]
    All,
    /// angekündigt … geprüft.
    Open,
    Status(ChangeOrderStatus),
    /// abgelehnt oder zurückgezogen.
    Gone,
}

impl ListFilter {
    pub fn label(self) -> String {
        match self {
            ListFilter::All => "alle".into(),
            ListFilter::Open => "offen (angekündigt bis geprüft)".into(),
            ListFilter::Status(status) => status.label().into(),
            ListFilter::Gone => "abgelehnt oder zurückgezogen".into(),
        }
    }

    pub fn matches(self, order: &ChangeOrder) -> bool {
        match self {
            ListFilter::All => true,
            ListFilter::Open => order.status.is_open(),
            ListFilter::Status(status) => order.status == status,
            ListFilter::Gone => matches!(
                order.status,
                ChangeOrderStatus::Rejected | ChangeOrderStatus::Withdrawn
            ),
        }
    }

    fn all() -> Vec<ListFilter> {
        let mut filters = vec![ListFilter::All, ListFilter::Open];
        filters.extend(ChangeOrderStatus::ALL.map(ListFilter::Status));
        filters.push(ListFilter::Gone);
        filters
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Page {
    #[default]
    List,
    Overview,
    Edit(ChangeOrderId),
}

/// Lines of the list with the sums (also for tests).
#[derive(Debug, Clone, PartialEq)]
pub struct ListLine {
    pub id: ChangeOrderId,
    pub label: String,
    pub title: String,
    pub basis: ChangeBasis,
    pub status: ChangeOrderStatus,
    pub date: String,
    pub offered: Money,
    pub checked: Money,
    pub approved: Money,
    /// Checked against offered in percent.
    pub deviation: Option<Decimal>,
    pub forecast: bool,
}

/// The lines the list shows for `filter` and `search` (number, title,
/// reason, firm), sorted by number.
pub fn list_lines(project: &Project, filter: ListFilter, search: &str) -> Vec<ListLine> {
    let contract = project.contract();
    let words: Vec<String> = search.split_whitespace().map(str::to_lowercase).collect();
    let mut orders: Vec<&ChangeOrder> = project
        .change_orders()
        .iter()
        .filter(|order| filter.matches(order))
        .filter(|order| {
            let haystack = format!(
                "{} {} {} {} {}",
                order.label(),
                order.title,
                order.reason,
                order.firm(contract),
                order.offer_number
            )
            .to_lowercase();
            words.iter().all(|word| haystack.contains(word))
        })
        .collect();
    orders.sort_by_key(|order| (order.number, order.id));
    orders
        .into_iter()
        .map(|order| {
            let offered = order.offered_total(contract);
            let checked = order.checked_total(contract);
            let approved_net = order.approved_total(contract);
            ListLine {
                id: order.id,
                label: order.label(),
                title: order.title.clone(),
                basis: order.basis,
                status: order.status,
                date: order.status_date().to_string(),
                offered,
                checked,
                approved: approved_net - order.discount_on(approved_net, contract),
                deviation: (!offered.is_zero())
                    .then(|| {
                        proava_domain::deviation_percent(checked.to_decimal(), offered.to_decimal())
                    })
                    .flatten(),
                forecast: order.status == ChangeOrderStatus::Approved
                    || (order.status.is_open() && order.in_forecast),
            }
        })
        .collect()
}

pub struct ChangeOrdersView {
    page: Page,
    filter: ListFilter,
    search: String,
    today: String,
    editor: editor::EditorState,
    /// Typed numbers of the table, by (row, column), while a field has focus.
    pub(crate) typed: HashMap<(RowId, u8), String>,
    /// Ask before deleting this change order (and whether to delete its
    /// positions in the LV).
    delete: Option<(ChangeOrderId, bool)>,
}

impl Default for ChangeOrdersView {
    fn default() -> Self {
        Self::new()
    }
}

impl ChangeOrdersView {
    pub fn new() -> Self {
        Self {
            page: Page::List,
            filter: ListFilter::All,
            search: String::new(),
            today: String::new(),
            editor: editor::EditorState::default(),
            typed: HashMap::new(),
            delete: None,
        }
    }

    /// Today as ISO date (from the launcher), for status dates.
    pub fn set_today(&mut self, iso: &str) {
        self.today = crate::format::iso_to_german_date(iso);
    }

    /// The change order being edited, if any.
    pub fn editing(&self) -> Option<ChangeOrderId> {
        match self.page {
            Page::Edit(id) => Some(id),
            _ => None,
        }
    }

    /// Opens a change order for editing.
    pub fn open(&mut self, id: ChangeOrderId) {
        self.page = Page::Edit(id);
        self.editor = editor::EditorState::default();
        self.typed.clear();
    }

    /// Shows the list (e.g. after switching the LV).
    pub fn show_list(&mut self) {
        self.page = Page::List;
    }

    pub fn set_filter(&mut self, filter: ListFilter) {
        self.filter = filter;
    }

    /// Text for the status line.
    pub fn status_text(&self, project: &Project) -> String {
        match self.page {
            Page::Edit(id) => project
                .change_order(id)
                .map(|order| {
                    format!(
                        "{} {} – {}",
                        order.label(),
                        order.title,
                        count(order.items.len(), "Position", "Positionen")
                    )
                })
                .unwrap_or_default(),
            _ => count(project.change_orders().len(), "Nachtrag", "Nachträge"),
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
    ) -> ChangeOrdersOutput {
        let mut output = ChangeOrdersOutput::default();
        if let Page::Edit(id) = self.page {
            if project.change_order(id).is_none() {
                self.page = Page::List;
            }
        }
        match self.page {
            Page::Edit(id) => {
                let back = editor::show(self, ui, palette, project, id, &mut output);
                if back {
                    self.page = Page::List;
                }
            }
            Page::List | Page::Overview => {
                self.head(ui, palette, project, &mut output);
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt("change_orders_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.page == Page::List {
                            self.list(ui, palette, project);
                        } else {
                            overview(ui, palette, project);
                        }
                    });
            }
        }
        self.delete_dialog(ui, palette, project, &mut output);
        output
    }

    fn head(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut ChangeOrdersOutput,
    ) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Nachträge ({})", project.change_orders().len()))
                    .size(15.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(12.0);
            let mut page = self.page;
            for (which, label) in [(Page::List, "Nachtragsliste"), (Page::Overview, "Nachtragsübersicht")] {
                if ui.selectable_label(page == which, label).clicked() {
                    page = which;
                }
            }
            self.page = page;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Übersicht drucken …")
                    .on_hover_text("Nachtragsübersicht mit Summen je Status, Auftragssumme alt/neu und Prognose")
                    .clicked()
                {
                    output.requests.push(ChangeOrderRequest::PrintOverview);
                }
                if ui
                    .button("Neuer Nachtrag")
                    .on_hover_text("Nachtrag mit der nächsten Nummer anlegen (Status angekündigt)")
                    .clicked()
                {
                    let id = self.new_change_order(project, output);
                    self.open(id);
                    self.editor.focus_title = true;
                }
            });
        });
        let contract = project.contract().cloned();
        match contract {
            Some(contract) => {
                ui.label(
                    RichText::new(format!(
                        "Auftrag: {}{} vom {} – Auftragssumme {} netto",
                        contract.contractor,
                        if contract.number.is_empty() {
                            String::new()
                        } else {
                            format!(", Nr. {}", contract.number)
                        },
                        contract.date,
                        contract.total
                    ))
                    .size(12.5)
                    .color(palette.text_dim),
                );
            }
            None => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(
                            "Noch kein Auftrag übernommen – Mengenänderungen von Auftragspositionen und die \
                             Auftragssumme brauchen ihn. Nachträge lassen sich trotzdem anlegen.",
                        )
                        .size(12.5)
                        .color(palette.warn),
                    );
                    if project.awarded_bid().is_some() && ui.button("Auftrag aus dem beauftragten Angebot übernehmen").clicked() {
                        match project.take_over_contract() {
                            Ok(contract) => {
                                output.response.toast =
                                    Some(format!("Auftrag übernommen: {} – {}", contract.contractor, contract.total));
                                output.response.dirty = true;
                                output.label = Some("Auftrag übernehmen");
                            }
                            Err(error) => output.response.toast = Some(error),
                        }
                    }
                });
            }
        }
    }

    fn new_change_order(
        &mut self,
        project: &mut Project,
        output: &mut ChangeOrdersOutput,
    ) -> ChangeOrderId {
        let mut order = ChangeOrder::new(ChangeOrderId(0), 0, "");
        order.announced_date = self.today.clone();
        let id = project.add_change_order(order);
        if let Some(order) = project.change_order_mut(id) {
            order.title = format!("Nachtrag {}", order.label());
        }
        output.label = Some("Neuer Nachtrag");
        output.response.dirty = true;
        id
    }

    fn list(&mut self, ui: &mut egui::Ui, palette: &Palette, project: &mut Project) {
        crate::widgets::row(ui, |ui| {
            ui.label(RichText::new("Status").size(12.5).color(palette.text_dim));
            egui::ComboBox::from_id_salt("change_order_filter")
                .selected_text(self.filter.label())
                .width(240.0)
                .show_ui(ui, |ui| {
                    for filter in ListFilter::all() {
                        ui.selectable_value(&mut self.filter, filter, filter.label());
                    }
                });
            ui.add_space(12.0);
            ui.label(RichText::new("Suche").size(12.5).color(palette.text_dim));
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Nummer, Bezeichnung, Begründung, Firma")
                    .desired_width(280.0),
            );
        });
        ui.add_space(8.0);
        let lines = list_lines(project, self.filter, &self.search);
        if project.change_orders().is_empty() {
            ui.label(
                RichText::new(
                    "Noch keine Nachträge. „Neuer Nachtrag“ legt einen an; seine Positionen stehen im LV \
                     im Nachtragsbereich am Ende (OZ-Vorschlag) oder werden aus dem LV zugeordnet.",
                )
                .size(12.5)
                .color(palette.text_dim),
            );
            return;
        }
        let mut open = None;
        egui::Grid::new("change_orders_grid")
            .striped(true)
            .num_columns(10)
            .spacing([14.0, 6.0])
            .show(ui, |ui| {
                for (head, right) in [
                    ("Nr.", false),
                    ("Bezeichnung", false),
                    ("Anlass", false),
                    ("Status", false),
                    ("Datum", false),
                    ("angeboten", true),
                    ("geprüft", true),
                    ("beauftragt", true),
                    ("Abw. %", true),
                    ("Prognose", false),
                ] {
                    let text = RichText::new(head)
                        .size(12.0)
                        .strong()
                        .color(palette.text_dim);
                    if right {
                        right_cell(ui, if head == "Abw. %" { 56.0 } else { MONEY_WIDTH }, text);
                    } else {
                        ui.label(text);
                    }
                }
                ui.end_row();
                for line in &lines {
                    let label = ui
                        .selectable_label(false, &line.label)
                        .on_hover_text("Nachtrag bearbeiten");
                    // One line, „…“ after TITLE_WIDTH, the full title in the tooltip.
                    let font = egui::TextStyle::Button.resolve(ui.style());
                    let (shown, cut) =
                        crate::widgets::fit_line(ui.painter(), &line.title, &font, TITLE_WIDTH);
                    let hint = if cut {
                        format!(
                            "{}\nNachtrag bearbeiten",
                            crate::format::one_line(&line.title)
                        )
                    } else {
                        "Nachtrag bearbeiten".to_string()
                    };
                    let title = ui.selectable_label(false, shown).on_hover_text(hint);
                    if label.clicked()
                        || title.clicked()
                        || label.double_clicked()
                        || title.double_clicked()
                    {
                        open = Some(line.id);
                    }
                    ui.label(short_basis(line.basis));
                    ui.label(
                        RichText::new(line.status.label())
                            .color(status_color(palette, line.status)),
                    );
                    ui.label(&line.date);
                    money_cell(ui, line.offered);
                    money_cell(ui, line.checked);
                    money_cell(ui, line.approved);
                    right_cell(
                        ui,
                        56.0,
                        line.deviation
                            .map(|value| format_decimal_de(value, 1))
                            .unwrap_or_default(),
                    );
                    ui.label(if line.forecast { "ja" } else { "nein" });
                    ui.end_row();
                }
                let sum = |pick: fn(&ListLine) -> Money| lines.iter().map(pick).sum::<Money>();
                ui.label("");
                ui.label(strong(
                    palette,
                    format!("Summe ({})", count(lines.len(), "Nachtrag", "Nachträge")),
                ));
                ui.label("");
                ui.label("");
                ui.label("");
                money_cell_strong(ui, palette, sum(|line| line.offered));
                money_cell_strong(ui, palette, sum(|line| line.checked));
                money_cell_strong(ui, palette, sum(|line| line.approved));
                ui.label("");
                ui.label("");
                ui.end_row();
            });
        if let Some(id) = open {
            self.open(id);
        }
        ui.add_space(14.0);
        totals_block(ui, palette, project);
    }

    fn delete_dialog(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut ChangeOrdersOutput,
    ) {
        let Some((id, mut with_rows)) = self.delete else {
            return;
        };
        let Some(order) = project.change_order(id) else {
            self.delete = None;
            return;
        };
        let label = format!("{} {}", order.label(), order.title);
        let new_positions = order
            .items
            .iter()
            .filter(|item| item.change == proava_domain::ItemChange::NewPosition)
            .count();
        let ((cancel, confirm), dismissed) = crate::widgets::modal(ui, palette, 480.0, |ui| {
            crate::widgets::title(ui, palette, "Nachtrag löschen", &label);
            if new_positions > 0 {
                ui.checkbox(
                    &mut with_rows,
                    format!("Auch die {new_positions} neuen Positionen aus dem LV löschen"),
                );
                ui.add_space(8.0);
            }
            crate::widgets::danger_dialog_buttons(ui, palette, "Löschen")
        });
        let mut close = cancel || dismissed;
        self.delete = Some((id, with_rows));
        if confirm {
            let before = project.contract_snapshot_with_lv(project.active_lv());
            if project.delete_change_order(id, with_rows).is_some() {
                output.label = Some("Nachtrag löschen");
                if with_rows {
                    output.before = before.map(|before| (before, false));
                }
                output.lv_changed = with_rows;
                output.response.dirty = true;
                output.response.toast = Some(format!("{label} gelöscht."));
            }
            self.page = Page::List;
            close = true;
        }
        if close {
            self.delete = None;
        }
    }

    pub(crate) fn ask_delete(&mut self, id: ChangeOrderId) {
        self.delete = Some((id, true));
    }

    pub(crate) fn today(&self) -> &str {
        &self.today
    }
}

/// Anlass in the list: the words before the paragraph.
fn short_basis(basis: ChangeBasis) -> String {
    let label = basis.label();
    label.split(" (").next().unwrap_or(label).to_string()
}

pub(crate) fn status_color(palette: &Palette, status: ChangeOrderStatus) -> egui::Color32 {
    match status {
        ChangeOrderStatus::Approved => palette.done,
        ChangeOrderStatus::Rejected | ChangeOrderStatus::Withdrawn => palette.text_dim,
        ChangeOrderStatus::Reviewed => palette.accent,
        _ => palette.text,
    }
}

/// Width of an amount column (up to 99.999.999,99 €).
const MONEY_WIDTH: f32 = 104.0;
/// Widest title in the list of Nachträge before it ends with „…“.
const TITLE_WIDTH: f32 = 320.0;

/// Right-aligned text in a cell of fixed width (a right-to-left layout
/// alone would take the whole rest of the grid row).
pub(crate) fn right_cell(ui: &mut egui::Ui, width: f32, text: impl Into<egui::WidgetText>) {
    let height = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            ui.label(text);
        },
    );
}

pub(crate) fn money_cell(ui: &mut egui::Ui, value: Money) {
    right_cell(ui, MONEY_WIDTH, value.to_string());
}

fn money_cell_strong(ui: &mut egui::Ui, palette: &Palette, value: Money) {
    right_cell(
        ui,
        MONEY_WIDTH,
        RichText::new(value.to_string())
            .strong()
            .color(palette.text),
    );
}

fn strong(palette: &Palette, text: impl Into<String>) -> RichText {
    RichText::new(text).strong().color(palette.text)
}

/// Sums by status and the effect on the contract sum.
fn totals_block(ui: &mut egui::Ui, palette: &Palette, project: &Project) {
    let contract = project.contract();
    let sums = sums_by_status(contract, project.change_orders());
    let overview = change_order_overview(contract, project.change_orders());
    ui.horizontal_top(|ui| {
        egui::Frame::new()
            .fill(palette.panel)
            .stroke(egui::Stroke::new(1.0, palette.line))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.label(strong(palette, "Summen je Status"));
                ui.add_space(4.0);
                egui::Grid::new("change_order_status_sums")
                    .num_columns(5)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for (index, head) in
                            ["Status", "Anzahl", "angeboten", "geprüft", "beauftragt"]
                                .into_iter()
                                .enumerate()
                        {
                            let text = RichText::new(head).size(12.0).color(palette.text_dim);
                            if index >= 2 {
                                right_cell(ui, MONEY_WIDTH, text);
                            } else {
                                ui.label(text);
                            }
                        }
                        ui.end_row();
                        for sum in sums.iter().filter(|sum| sum.count > 0) {
                            ui.label(sum.status.label());
                            ui.label(sum.count.to_string());
                            money_cell(ui, sum.offered);
                            money_cell(ui, sum.checked);
                            money_cell(ui, sum.approved);
                            ui.end_row();
                        }
                    });
            });
        ui.add_space(12.0);
        egui::Frame::new()
            .fill(palette.panel)
            .stroke(egui::Stroke::new(1.0, palette.line))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.label(strong(palette, "Auftragssumme"));
                ui.add_space(4.0);
                egui::Grid::new("change_order_contract_sums")
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        let rows: [(&str, Money, bool); 6] = [
                            (
                                "Auftragssumme alt (netto nach Nachlass)",
                                overview.contract_total,
                                false,
                            ),
                            ("+ beauftragte Nachträge", overview.approved, false),
                            (
                                "= Auftragssumme neu",
                                overview.total_with_change_orders,
                                true,
                            ),
                            (
                                "offene Nachträge (geprüft bzw. angeboten)",
                                overview.open,
                                false,
                            ),
                            (
                                "davon Mehrungen / Minderungen beauftragt",
                                overview.increases + overview.decreases,
                                false,
                            ),
                            (
                                "Prognose (mit offenen Nachträgen, die eingehen)",
                                overview.forecast,
                                true,
                            ),
                        ];
                        for (label, value, bold) in rows {
                            ui.label(if bold {
                                strong(palette, label)
                            } else {
                                RichText::new(label)
                            });
                            if bold {
                                money_cell_strong(ui, palette, value);
                            } else {
                                money_cell(ui, value);
                            }
                            ui.end_row();
                        }
                    });
                if contract.is_none() {
                    ui.label(
                        RichText::new("Ohne Auftrag: Auftragssumme 0 €.")
                            .size(11.5)
                            .color(palette.text_dim),
                    );
                }
            });
    });
}

/// Nachtragsübersicht (like the Nachtragsaufschlüsselung): every change
/// order with reason, status and amounts, then the totals.
fn overview(ui: &mut egui::Ui, palette: &Palette, project: &Project) {
    let contract = project.contract();
    let overview = change_order_overview(contract, project.change_orders());
    if overview.lines.is_empty() {
        ui.label(RichText::new("Noch keine Nachträge.").color(palette.text_dim));
        return;
    }
    egui::Grid::new("change_order_overview")
        .striped(true)
        .num_columns(9)
        .spacing([14.0, 6.0])
        .show(ui, |ui| {
            for (index, head) in [
                "Nr.",
                "Nachtrag",
                "Anlass",
                "Begründung",
                "Status",
                "angeboten",
                "geprüft",
                "beauftragt",
                "Gutschrift",
            ]
            .into_iter()
            .enumerate()
            {
                let text = RichText::new(head)
                    .size(12.0)
                    .strong()
                    .color(palette.text_dim);
                if index >= 5 {
                    right_cell(ui, MONEY_WIDTH, text);
                } else {
                    ui.label(text);
                }
            }
            ui.end_row();
            for line in &overview.lines {
                ui.label(&line.label);
                ui.label(&line.title);
                ui.label(short_basis(line.basis));
                let reason = line.reason.lines().next().unwrap_or("");
                ui.label(numbers::fit_text(reason, 260.0, 13.0).0)
                    .on_hover_text(&line.reason);
                ui.label(
                    RichText::new(line.status.label()).color(status_color(palette, line.status)),
                );
                money_cell(ui, line.offered);
                money_cell(ui, line.checked);
                money_cell(ui, line.approved);
                money_cell(ui, line.credit);
                ui.end_row();
            }
            for _ in 0..4 {
                ui.label("");
            }
            ui.label(strong(palette, "Summe"));
            money_cell_strong(ui, palette, overview.offered);
            money_cell_strong(ui, palette, overview.checked);
            money_cell_strong(ui, palette, overview.approved);
            money_cell_strong(
                ui,
                palette,
                overview.lines.iter().map(|line| line.credit).sum(),
            );
            ui.end_row();
        });
    ui.add_space(14.0);
    totals_block(ui, palette, project);
}

#[cfg(test)]
mod tests {
    use proava_domain::{ChangeOrderStatus, Money};

    use super::{list_lines, ListFilter};

    #[test]
    fn filter_and_search_of_the_list() {
        let project = crate::demo_change_orders::change_order_demo_project();
        let labels = |filter: ListFilter, search: &str| -> Vec<String> {
            list_lines(&project, filter, search)
                .into_iter()
                .map(|line| line.label)
                .collect()
        };
        assert_eq!(
            labels(ListFilter::All, ""),
            ["N01", "N02", "N03", "N04", "N05", "N06"]
        );
        assert_eq!(labels(ListFilter::Open, ""), ["N02", "N03", "N04", "N06"]);
        assert_eq!(labels(ListFilter::Gone, ""), ["N05"]);
        assert_eq!(
            labels(ListFilter::Status(ChangeOrderStatus::Approved), ""),
            ["N01"]
        );
        // Suche über Bezeichnung und Begründung, alle Wörter müssen passen.
        assert_eq!(labels(ListFilter::All, "wetter"), ["N01"]);
        assert_eq!(labels(ListFilter::All, "bauherr"), ["N01", "N05"]);
        assert_eq!(labels(ListFilter::All, "n03 werkstatt"), ["N03"]);
        assert_eq!(labels(ListFilter::All, "Bauherrenwunsch"), ["N01"]);
        assert!(labels(ListFilter::All, "gibt es nicht").is_empty());

        let lines = list_lines(&project, ListFilter::All, "");
        let n01 = &lines[0];
        assert_eq!(n01.approved, Money::from_cents(204_700));
        assert_eq!(n01.date, "28.10.2026");
        assert!(n01.forecast);
        assert!(!lines[4].forecast, "abgelehnt geht nie in die Prognose");
        // N03: geprüft 1.830 gegen angeboten 2.490 = −26,5 %.
        assert_eq!(lines[2].deviation, Some("-26.5".parse().unwrap()));
    }
}
