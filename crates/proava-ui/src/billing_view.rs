//! Abrechnung: the contract of the LV, the contractor's invoices with the
//! planner's check (positions, takeoff, progress) and the release
//! calculation (`proava_domain::billing_check`). Changes go straight into the
//! project; the workspace records them as undo steps.

use std::collections::BTreeMap;

use eframe::egui::{self, Align, Layout, RichText, Sense, Stroke};
use proava_domain::{
    billable_items, format_decimal_de, parse_decimal_de, BillableItem, Correction, Decimal,
    Finding, Invoice, InvoiceCheck, InvoiceId, InvoiceKind, Mode, Money, Project, RowId, Severity,
    VatRule,
};

use crate::theme::Palette;
use crate::view::{ViewRequest, ViewResponse};
use crate::widgets;

mod table;
mod takeoff;

const LIST_ROW: f32 = 26.0;

/// Which editable cell of the selected position row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Claimed,
    Checked,
    Progress,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confirm {
    DeleteInvoice(InvoiceId),
    RetakeContract,
}

/// Results of the check, kept until the project changes.
struct Cache {
    revision: u64,
    lv: usize,
    checks: Vec<InvoiceCheck>,
    billable: BTreeMap<RowId, BillableItem>,
}

/// Text typed into the cells of the selected position row.
#[derive(Default)]
struct CellEdits {
    key: Option<(InvoiceId, RowId)>,
    claimed: String,
    checked: String,
    progress: String,
    note: String,
    changed: bool,
}

pub struct BillingView {
    invoice: Option<InvoiceId>,
    row: Option<RowId>,
    /// Invoice whose header opens by itself (a new one).
    header_open_for: Option<InvoiceId>,
    /// Bring the selected invoice into view in the list.
    scroll_to_invoice: bool,
    /// Show hints (Info) in the findings, not only warnings and errors.
    show_hints: bool,
    /// Show every LV position, not only contract and change-order positions.
    show_all: bool,
    /// Progress in percent (else as factor).
    percent: bool,
    edits: CellEdits,
    /// Revision of the project when the cell texts were filled.
    edits_revision: u64,
    /// A cell of the selected row had the focus in the last frame.
    focused: Option<Field>,
    /// A cell was left: write the texts after drawing.
    commit_pending: bool,
    focus: Option<Field>,
    scroll_to_row: bool,
    takeoff: Option<RowId>,
    confirm: Option<Confirm>,
    cache: Option<Cache>,
    today: String,
    /// Text of the number fields in the header while they are edited.
    header_texts: BTreeMap<&'static str, String>,
    header_key: Option<InvoiceId>,
}

impl Default for BillingView {
    fn default() -> Self {
        Self::new()
    }
}

impl BillingView {
    pub fn new() -> Self {
        Self {
            invoice: None,
            row: None,
            header_open_for: None,
            scroll_to_invoice: false,
            show_hints: false,
            show_all: false,
            percent: true,
            edits: CellEdits::default(),
            edits_revision: 0,
            focused: None,
            commit_pending: false,
            focus: None,
            scroll_to_row: false,
            takeoff: None,
            confirm: None,
            cache: None,
            today: String::new(),
            header_texts: BTreeMap::new(),
            header_key: None,
        }
    }

    /// Today as TT.MM.JJJJ (from the launcher's ISO time).
    pub fn set_today(&mut self, iso: &str) {
        let date = iso.get(..10).unwrap_or_default();
        let parts: Vec<&str> = date.split('-').collect();
        if let [year, month, day] = parts[..] {
            self.today = format!("{day}.{month}.{year}");
        }
    }

    pub fn selected_invoice(&self) -> Option<InvoiceId> {
        self.invoice
    }

    pub fn selected_row(&self) -> Option<RowId> {
        self.row
    }

    /// A dialog or window of the view is open (no shell shortcuts then).
    pub fn has_dialog(&self) -> bool {
        self.confirm.is_some() || self.takeoff.is_some()
    }

    /// Show this invoice (e.g. after reading it from GAEB), header open.
    pub fn select_invoice(&mut self, id: InvoiceId) {
        self.invoice = Some(id);
        self.row = None;
        self.header_open_for = Some(id);
        self.scroll_to_invoice = true;
        self.header_key = None;
    }

    /// Select a position and open its Aufmaß window (e.g. from the
    /// Positionsfenster, Reiter Mengenermittlung ▸ „Aufmaß bearbeiten …“).
    pub fn open_takeoff(&mut self, row: RowId) {
        self.select_row(row);
        self.takeoff = Some(row);
    }

    /// Select a position (e.g. from the findings) and bring it into view.
    pub fn select_row(&mut self, row: RowId) {
        self.row = Some(row);
        self.scroll_to_row = true;
    }

    fn refresh(&mut self, project: &Project) {
        let lv = project.active_lv();
        if self
            .cache
            .as_ref()
            .is_some_and(|cache| cache.revision == project.revision() && cache.lv == lv)
        {
            return;
        }
        let (checks, billable) = match project.billing_context() {
            Some(context) => (
                project.billing().check_all(&context),
                billable_items(context.contract, context.change_orders, context.lv),
            ),
            None => (Vec::new(), BTreeMap::new()),
        };
        self.cache = Some(Cache {
            revision: project.revision(),
            lv,
            checks,
            billable,
        });
        let invoices = project.billing().invoices();
        if self
            .invoice
            .is_none_or(|id| !invoices.iter().any(|invoice| invoice.id == id))
        {
            self.invoice = invoices.last().map(|invoice| invoice.id);
            self.scroll_to_invoice = true;
        }
    }

    fn check(&self, id: InvoiceId) -> Option<&InvoiceCheck> {
        self.cache
            .as_ref()?
            .checks
            .iter()
            .find(|check| check.invoice == id)
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
    ) -> ViewResponse {
        let mut response = ViewResponse::default();
        self.refresh(project);
        if project.contract().is_none() {
            self.no_contract(ui, palette, project, &mut response);
            self.dialogs(ui, palette, project, &mut response);
            return response;
        }
        self.contract_strip(ui, palette, project, &mut response);
        ui.add_space(8.0);
        self.invoice_list(ui, palette, project, &mut response);
        ui.add_space(8.0);
        let Some(id) = self
            .invoice
            .filter(|id| project.billing().invoice(*id).is_some())
        else {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(
                        "Noch keine Rechnung erfasst – „Neue Rechnung“ legt die erste an.",
                    )
                    .color(palette.text_dim),
                );
            });
            self.dialogs(ui, palette, project, &mut response);
            return response;
        };
        self.header(ui, palette, project, id, &mut response);
        ui.add_space(6.0);
        let room = ui.available_height();
        egui::Panel::bottom("billing_bottom")
            .resizable(true)
            .default_size((room * 0.35).clamp(150.0, 260.0))
            .size_range(110.0..=(room * 0.6).max(120.0))
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                left: 0,
                right: 0,
                top: 8,
                bottom: 0,
            }))
            .show(ui, |ui| {
                ui.columns(2, |columns| {
                    self.sums(&mut columns[0], palette, project, id);
                    self.findings(&mut columns[1], palette, project, id);
                });
            });
        self.table(ui, palette, project, id, &mut response);
        self.takeoff_window(ui, palette, project, id, &mut response);
        self.dialogs(ui, palette, project, &mut response);
        response
    }

    // ---------- without contract ----------

    fn no_contract(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        response: &mut ViewResponse,
    ) {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Abrechnung")
                    .size(20.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Grundlage ist der Auftrag: Positionen mit Auftragsmenge und Auftrags-EP, Nachlass und Vertragsbedingungen.",
                )
                .color(palette.text_dim),
            );
            ui.add_space(16.0);
            match project.awarded_bid() {
                Some(bid) => {
                    let label = format!(
                        "Auftrag übernehmen – {} ({} €)",
                        bid.name(),
                        bid.award().map(|award| award.total.plain()).unwrap_or_default()
                    );
                    if widgets::button(ui, palette, &label, true, 260.0).clicked() {
                        self.take_over(project, response);
                    }
                }
                None => {
                    ui.label(
                        RichText::new(
                            "Im LV ist noch kein Angebot beauftragt – im Reiter Angebote „Auftrag erteilen …“, dann hier übernehmen.",
                        )
                        .color(palette.warn),
                    );
                }
            }
            ui.add_space(10.0);
            if widgets::button(ui, palette, "Auftrag aus GAEB X86 einlesen …", false, 260.0).clicked() {
                response.requests.push(ViewRequest::ImportContract);
            }
        });
    }

    fn take_over(&mut self, project: &mut Project, response: &mut ViewResponse) {
        match project.take_over_contract() {
            Ok(contract) => {
                response.toast = Some(format!(
                    "Auftrag übernommen: {} – {}, Auftragssumme {} €.",
                    contract.contractor,
                    crate::format::count(contract.items.len(), "Position", "Positionen"),
                    contract.total.plain()
                ));
                response.dirty = true;
            }
            Err(error) => response.toast = Some(error),
        }
    }

    // ---------- contract and invoice list ----------

    fn contract_strip(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        response: &mut ViewResponse,
    ) {
        let contract = project.contract().expect("checked");
        let overview =
            proava_domain::change_order_overview(Some(contract), project.change_orders());
        let latest = self.cache.as_ref().and_then(|cache| cache.checks.last());
        // Line 1: who and the contract commands; line 2: the figures (wraps
        // on narrow windows instead of running under the buttons).
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Auftrag")
                    .size(15.0)
                    .strong()
                    .color(palette.text),
            );
            let mut text = contract.contractor.clone();
            if !contract.date.is_empty() {
                text.push_str(&format!(" vom {}", contract.date));
            }
            if !contract.number.trim().is_empty() {
                text.push_str(&format!(" · Auftrags-Nr. {}", contract.number.trim()));
            }
            ui.add(egui::Label::new(RichText::new(text).color(palette.text)).truncate());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::small_button(ui, palette, "Auftrag aus GAEB …").clicked() {
                    response.requests.push(ViewRequest::ImportContract);
                }
                if widgets::small_button(ui, palette, "Auftrag neu übernehmen …").clicked() {
                    self.confirm = Some(Confirm::RetakeContract);
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            let figure = |ui: &mut egui::Ui, label: &str, value: Money| {
                ui.label(RichText::new(label).size(12.0).color(palette.text_dim));
                ui.label(
                    RichText::new(format!("{} €", value.plain()))
                        .monospace()
                        .color(palette.text),
                );
                ui.add_space(8.0);
            };
            figure(ui, "Auftrag netto", overview.contract_total);
            figure(ui, "beauftragte Nachträge", overview.approved);
            figure(ui, "gesamt", overview.total_with_change_orders);
            if let Some(check) = latest {
                ui.separator();
                ui.label(
                    RichText::new("Stand geprüft")
                        .size(12.0)
                        .color(palette.text_dim),
                );
                ui.label(
                    RichText::new(format!(
                        "{} € ({} %)",
                        check.performed.plain(),
                        format_decimal_de(check.performed_percent, 1)
                    ))
                    .monospace()
                    .color(palette.text),
                );
            }
        });
    }

    fn invoice_list(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        response: &mut ViewResponse,
    ) {
        crate::widgets::row(ui, |ui| {
            ui.label(
                RichText::new("Rechnungen")
                    .size(14.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(10.0);
            if widgets::button(ui, palette, "Neue Rechnung", true, 120.0).clicked() {
                self.new_invoice(project, response);
            }
            if widgets::button(ui, palette, "Rechnung aus GAEB (X89/X31) …", false, 200.0).clicked()
            {
                response
                    .requests
                    .push(ViewRequest::ImportInvoice(self.invoice));
            }
            let selected = self.invoice;
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Rechnung löschen …"))
                .clicked()
            {
                self.confirm = selected.map(Confirm::DeleteInvoice);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let enabled = selected.is_some();
                if ui
                    .add_enabled(enabled, egui::Button::new("Anschreiben an den Bauherrn …"))
                    .on_hover_text("Schriftverkehr: Vorlage „Freigabe an den Bauherrn“ mit Freigabebetrag und Rechnungsnummer")
                    .clicked()
                {
                    response.requests.push(ViewRequest::ReleaseLetter);
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("PDF Rechnungsfreigabe …"))
                    .clicked()
                {
                    response.requests.push(ViewRequest::BillingPdf(true));
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("PDF Aufmaßprüfung …"))
                    .clicked()
                {
                    response.requests.push(ViewRequest::BillingPdf(false));
                }
            });
        });
        ui.add_space(4.0);
        let invoices = project.billing().invoices().to_vec();
        let contractor = project
            .contract()
            .map(|c| c.contractor.clone())
            .unwrap_or_default();
        let width = ui.available_width();
        // Firma takes what is left (at least 170 px).
        let firma = (width - 110.0 - 160.0 - 86.0 - 3.0 * 120.0 - 200.0 - 12.0).max(170.0);
        let columns: [(&str, f32, bool); 8] = [
            ("Nr.", 110.0, false),
            ("Art", 160.0, false),
            ("Datum", 86.0, false),
            ("Firma", firma, false),
            ("Forderung netto", 120.0, true),
            ("geprüft netto", 120.0, true),
            ("Freigabe brutto", 120.0, true),
            ("Status", 200.0, false),
        ];
        let (head, _) = ui.allocate_exact_size(egui::vec2(width, 22.0), Sense::hover());
        paint_cells(
            ui,
            head,
            &columns,
            |index| columns[index].0.to_string(),
            palette.text_dim,
            true,
        );
        // Low windows: fewer invoice rows, more room for the positions.
        let rows = if ui.available_height() < 700.0 {
            3.5
        } else {
            5.5
        };
        let height = (invoices.len().max(1) as f32 * (LIST_ROW + ui.spacing().item_spacing.y))
            .min(LIST_ROW * rows);
        egui::ScrollArea::vertical()
            .id_salt("billing_invoices")
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for invoice in &invoices {
                    let check = self.check(invoice.id).cloned();
                    let (rect, row) =
                        ui.allocate_exact_size(egui::vec2(width, LIST_ROW), Sense::click());
                    let selected = self.invoice == Some(invoice.id);
                    let fill = if selected {
                        palette.selected
                    } else if row.hovered() {
                        palette.row_hover
                    } else {
                        palette.panel
                    };
                    ui.painter().rect_filled(rect, 0.0, fill);
                    let label = check.as_ref().map(|c| c.label.clone()).unwrap_or_default();
                    let warnings = check
                        .as_ref()
                        .map(|c| {
                            c.all_findings()
                                .filter(|f| f.severity() >= Severity::Warning)
                                .count()
                        })
                        .unwrap_or(0);
                    let status = match (invoice.checked_date.is_empty(), warnings) {
                        (true, 0) => "in Prüfung".to_string(),
                        (true, n) => format!(
                            "in Prüfung · {}",
                            crate::format::count(n, "Befund", "Befunde")
                        ),
                        (false, 0) => format!("geprüft {}", invoice.checked_date),
                        (false, n) => format!(
                            "geprüft {} · {}",
                            invoice.checked_date,
                            crate::format::count(n, "Befund", "Befunde")
                        ),
                    };
                    let checked = check.as_ref().map(|c| performance_in_sense(c, invoice));
                    let texts = [
                        if invoice.number.is_empty() {
                            "(ohne Nr.)".to_string()
                        } else {
                            invoice.number.clone()
                        },
                        label,
                        invoice.date.clone(),
                        contractor.clone(),
                        invoice.claimed_net.map(|m| m.plain()).unwrap_or_default(),
                        checked.map(|m| m.plain()).unwrap_or_default(),
                        check
                            .as_ref()
                            .map(|c| c.release.release_gross.plain())
                            .unwrap_or_default(),
                        status,
                    ];
                    let color = palette.text;
                    paint_cells(
                        ui,
                        rect,
                        &columns,
                        |index| texts[index].clone(),
                        color,
                        false,
                    );
                    if selected && self.scroll_to_invoice {
                        ui.scroll_to_rect(rect, None);
                        self.scroll_to_invoice = false;
                    }
                    let name = format!("Rechnung {}", texts[0]);
                    row.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Row, true, selected, &name)
                    });
                    if row.clicked() && self.invoice != Some(invoice.id) {
                        self.commit_edits(project, response);
                        self.invoice = Some(invoice.id);
                        self.row = None;
                    }
                }
            });
    }

    fn new_invoice(&mut self, project: &mut Project, response: &mut ViewResponse) {
        self.commit_edits(project, response);
        let vat_default = VatRule::Rate(project.lv().vat_percent());
        let billing = project.billing_mut();
        let mut invoice = Invoice::new(InvoiceKind::Partial, "", self.today.clone());
        invoice.received = self.today.clone();
        match billing.last_in_sequence() {
            Some(last) => {
                invoice.quantities = last.quantities;
                invoice.release = last.release;
                invoice.vat = last.vat;
            }
            None => invoice.vat = vat_default,
        }
        let id = billing.add_invoice(invoice);
        self.invoice = Some(id);
        self.row = None;
        self.header_open_for = Some(id);
        self.scroll_to_invoice = true;
        self.header_key = None;
        response.dirty = true;
        response.toast =
            Some("Neue Rechnung angelegt – Nummer und Daten im Rechnungskopf eintragen.".into());
    }

    // ---------- invoice header ----------

    fn header(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: InvoiceId,
        response: &mut ViewResponse,
    ) {
        if self.header_key != Some(id) {
            self.header_texts.clear();
            self.header_key = Some(id);
        }
        let title = {
            let label = self.check(id).map(|c| c.label.clone()).unwrap_or_default();
            let number = project
                .billing()
                .invoice(id)
                .map(|i| i.number.clone())
                .unwrap_or_default();
            format!("Rechnungskopf – {label} {number}")
        };
        let header = egui::CollapsingHeader::new(RichText::new(title).strong().color(palette.text))
            .id_salt(("billing_header", id.0))
            .default_open(self.header_open_for == Some(id))
            .show(ui, |ui| {
                self.header_fields(ui, palette, project, id, response)
            });
        let _ = header;
    }

    fn header_fields(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: InvoiceId,
        response: &mut ViewResponse,
    ) {
        let lv_vat = project.lv().vat_percent();
        let Some(invoice) = project.billing_mut().invoice_mut(id) else {
            return;
        };
        let mut changed = false;
        let label = |ui: &mut egui::Ui, text: &str| {
            ui.label(RichText::new(text).size(12.0).color(palette.text_dim));
        };
        egui::Grid::new(("billing_header_grid", id.0))
            .num_columns(8)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                label(ui, "Rechnungs-Nr.");
                changed |= text_field(ui, &mut invoice.number, 120.0);
                label(ui, "Art");
                let mut kind = invoice.kind;
                egui::ComboBox::from_id_salt(("billing_kind", id.0))
                    .selected_text(kind.label())
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for option in InvoiceKind::ALL {
                            ui.selectable_value(&mut kind, option, option.label());
                        }
                    });
                if kind != invoice.kind {
                    invoice.kind = kind;
                    changed = true;
                }
                label(ui, "Rechnungsdatum");
                changed |= text_field(ui, &mut invoice.date, 90.0);
                label(ui, "Eingang");
                changed |= text_field(ui, &mut invoice.received, 90.0);
                ui.end_row();

                label(ui, "Mengen der Firma");
                changed |= mode_combo(
                    ui,
                    ("billing_quantities", id.0),
                    &mut invoice.quantities,
                    true,
                    invoice.kind,
                );
                label(ui, "Freigabe");
                changed |= mode_combo(
                    ui,
                    ("billing_release", id.0),
                    &mut invoice.release,
                    false,
                    invoice.kind,
                );
                label(ui, "Zahlungsziel");
                changed |= text_field(ui, &mut invoice.due, 90.0);
                label(ui, "Leistungszeitraum");
                ui.horizontal(|ui| {
                    changed |= text_field(ui, &mut invoice.period_start, 80.0);
                    ui.label("–");
                    changed |= text_field(ui, &mut invoice.period_end, 80.0);
                });
                ui.end_row();

                label(ui, "Forderung netto €");
                if let Some(value) = decimal_field(
                    ui,
                    &mut self.header_texts,
                    "claimed",
                    invoice.claimed_net.map(Money::to_decimal),
                    2,
                    120.0,
                    response,
                ) {
                    invoice.claimed_net = value.map(Money::from_decimal);
                    changed = true;
                }
                label(ui, "USt");
                let mut vat_choice = invoice.vat;
                egui::ComboBox::from_id_salt(("billing_vat", id.0))
                    .selected_text(vat_choice.label())
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for option in [
                            VatRule::Rate(lv_vat),
                            VatRule::Rate(Decimal::from(19)),
                            VatRule::Rate(Decimal::from(7)),
                            VatRule::ReverseCharge,
                        ] {
                            ui.selectable_value(&mut vat_choice, option, option.label());
                        }
                    });
                if vat_choice != invoice.vat {
                    invoice.vat = vat_choice;
                    changed = true;
                }
                label(ui, "Einbehalt %");
                if let Some(value) = decimal_field(
                    ui,
                    &mut self.header_texts,
                    "security",
                    invoice.security_percent,
                    2,
                    90.0,
                    response,
                )
                .map(|value| value.filter(|v| !v.is_sign_negative()))
                {
                    invoice.security_percent = value;
                    changed = true;
                }
                label(ui, "angerechnet €");
                if let Some(value) = decimal_field(
                    ui,
                    &mut self.header_texts,
                    "credited",
                    invoice.credited.map(Money::to_decimal),
                    2,
                    120.0,
                    response,
                ) {
                    invoice.credited = value.map(Money::from_decimal);
                    changed = true;
                }
                ui.end_row();

                label(ui, "Pauschal gefordert €");
                if let Some(value) = decimal_field(
                    ui,
                    &mut self.header_texts,
                    "claimed_lump",
                    invoice.claimed_lump.map(Money::to_decimal),
                    2,
                    120.0,
                    response,
                ) {
                    invoice.claimed_lump = value.map(Money::from_decimal);
                    changed = true;
                }
                label(ui, "Pauschal geprüft €");
                if let Some(value) = decimal_field(
                    ui,
                    &mut self.header_texts,
                    "checked_lump",
                    invoice.checked_lump.map(Money::to_decimal),
                    2,
                    120.0,
                    response,
                ) {
                    invoice.checked_lump = value.map(Money::from_decimal);
                    changed = true;
                }
                label(ui, "Korrektur €");
                let correction = invoice.correction.clone().unwrap_or_default();
                if let Some(value) = decimal_field(
                    ui,
                    &mut self.header_texts,
                    "correction",
                    invoice.correction.as_ref().map(|c| c.amount.to_decimal()),
                    2,
                    90.0,
                    response,
                ) {
                    invoice.correction = value.map(|amount| Correction {
                        amount: Money::from_decimal(amount),
                        text: correction.text.clone(),
                    });
                    changed = true;
                }
                label(ui, "Text der Korrektur");
                let mut text = correction.text.clone();
                if text_field(ui, &mut text, 160.0) {
                    let amount = invoice
                        .correction
                        .as_ref()
                        .map(|c| c.amount)
                        .unwrap_or_default();
                    invoice.correction = Some(Correction { amount, text });
                    changed = true;
                }
                ui.end_row();

                label(ui, "Prüfvermerk");
                changed |= text_field(ui, &mut invoice.note, 260.0);
                label(ui, "geprüft am");
                ui.horizontal(|ui| {
                    changed |= text_field(ui, &mut invoice.checked_date, 90.0);
                    if invoice.checked_date.is_empty() && ui.small_button("heute").clicked() {
                        invoice.checked_date = self.today.clone();
                        changed = true;
                    }
                });
                label(ui, "geprüft von");
                changed |= text_field(ui, &mut invoice.checked_by, 120.0);
                ui.end_row();
            });
        if changed {
            response.dirty = true;
        }
    }

    // ---------- sums and findings ----------

    fn sums(&mut self, ui: &mut egui::Ui, palette: &Palette, project: &Project, id: InvoiceId) {
        let Some(check) = self.check(id).cloned() else {
            return;
        };
        let release = &check.release;
        ui.label(
            RichText::new("Rechenweg der Freigabe")
                .strong()
                .color(palette.text),
        );
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("billing_sums")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new(("billing_sums_grid", id.0))
                    .num_columns(2)
                    .spacing([16.0, 3.0])
                    .show(ui, |ui| {
                        let line =
                            |ui: &mut egui::Ui, label: String, value: Money, strong: bool| {
                                let text = RichText::new(label).color(if strong {
                                    palette.text
                                } else {
                                    palette.text_dim
                                });
                                ui.label(if strong { text.strong() } else { text });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let value = RichText::new(format!("{} €", value.plain()))
                                        .monospace()
                                        .color(palette.text);
                                    ui.label(if strong { value.strong() } else { value });
                                });
                                ui.end_row();
                            };
                        if check.kind == InvoiceKind::SecurityRelease {
                            line(
                                ui,
                                "Freigabe Sicherheitseinbehalt netto".into(),
                                release.release_net,
                                true,
                            );
                        } else {
                            let mode = match release.mode {
                                Mode::Cumulative => "kumuliert",
                                Mode::Increment => "Zuwachs dieser Rechnung",
                            };
                            line(
                                ui,
                                format!("Leistung netto geprüft ({mode})"),
                                release.performance,
                                false,
                            );
                            if !release.discount.is_zero() {
                                line(
                                    ui,
                                    format!(
                                        "− Nachlass {} %",
                                        format_decimal_de(release.discount_percent, 2)
                                    ),
                                    -release.discount,
                                    false,
                                );
                            }
                            for (label, percent, amount) in &release.deductions {
                                line(
                                    ui,
                                    format!("− {label} {} %", format_decimal_de(*percent, 2)),
                                    -*amount,
                                    false,
                                );
                            }
                            line(ui, "= Rechnung netto".into(), release.net, true);
                            line(
                                ui,
                                format!(
                                    "− Sicherheitseinbehalt {} %",
                                    format_decimal_de(release.security_percent, 2)
                                ),
                                -release.security,
                                false,
                            );
                            line(
                                ui,
                                "− anrechenbare Freigaben".into(),
                                -release.credited,
                                false,
                            );
                            if !release.correction.is_zero() {
                                let text = if release.correction_text.is_empty() {
                                    "Korrektur".to_string()
                                } else {
                                    release.correction_text.clone()
                                };
                                line(ui, format!("± {text}"), release.correction, false);
                            }
                            line(ui, "= Freigabe netto".into(), release.release_net, true);
                        }
                        line(
                            ui,
                            format!("+ {}", release.vat.label()),
                            release.vat_amount,
                            false,
                        );
                        line(ui, "= Freigabe brutto".into(), release.release_gross, true);
                        if let Some(percent) = release.cash_discount_percent {
                            line(
                                ui,
                                format!("Skonto {} % (Information)", format_decimal_de(percent, 2)),
                                release.cash_discount,
                                false,
                            );
                        }
                        line(
                            ui,
                            "aktuell abgerechnet inkl. dieser Freigabe".into(),
                            release.released_total,
                            false,
                        );
                        line(ui, "Einbehalt danach".into(), release.withheld, false);
                        line(
                            ui,
                            "Auftragssumme (mit beauftragten Nachträgen)".into(),
                            check.ordered_total,
                            false,
                        );
                        line(
                            ui,
                            format!(
                                "Stand geprüft ({} %)",
                                format_decimal_de(check.performed_percent, 1)
                            ),
                            check.performed,
                            false,
                        );
                        line(ui, "Prognose laut Aufmaß".into(), check.forecast, false);
                    });
                if !check.earlier.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Bisherige Freigaben")
                            .strong()
                            .color(palette.text),
                    );
                    for earlier in &check.earlier {
                        ui.label(
                            RichText::new(format!(
                                "{} {} {} – Leistung {} €, Freigabe {} € netto",
                                earlier.label,
                                earlier.number,
                                earlier.date,
                                earlier.performance.plain(),
                                earlier.release_net.plain()
                            ))
                            .size(12.0)
                            .color(palette.text_dim),
                        );
                    }
                }
                let _ = project;
            });
    }

    fn findings(&mut self, ui: &mut egui::Ui, palette: &Palette, project: &Project, id: InvoiceId) {
        let Some(check) = self.check(id).cloned() else {
            return;
        };
        let warnings = check
            .all_findings()
            .filter(|f| f.severity() >= Severity::Warning)
            .count();
        let hints = check.all_findings().count() - warnings;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Befunde ({warnings})"))
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(8.0);
            ui.checkbox(&mut self.show_hints, format!("auch Hinweise ({hints})"));
        });
        ui.add_space(4.0);
        let count = if self.show_hints {
            warnings + hints
        } else {
            warnings
        };
        let show_hints = self.show_hints;
        let shown = |finding: &Finding| show_hints || finding.severity() >= Severity::Warning;
        egui::ScrollArea::vertical()
            .id_salt("billing_findings")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if count == 0 {
                    ui.label(RichText::new("Keine Befunde.").color(palette.text_dim));
                }
                for finding in check.findings.iter().filter(|f| shown(f)) {
                    ui.label(
                        RichText::new(finding.message()).color(severity_color(palette, finding)),
                    );
                }
                let mut jump = None;
                for line in check.positions.iter().chain(check.unchanged.iter()) {
                    for finding in line.findings.iter().filter(|f| shown(f)) {
                        let text = format!(
                            "{} {}: {}",
                            line.ordinal,
                            line.short_text,
                            finding.message()
                        );
                        let link = ui.add(
                            egui::Label::new(
                                RichText::new(text).color(severity_color(palette, finding)),
                            )
                            .sense(Sense::click()),
                        );
                        if link.on_hover_text("Zur Position").clicked() {
                            jump = Some(line.position);
                        }
                    }
                }
                if let Some(row) = jump {
                    self.select_row(row);
                }
                let _ = project;
            });
    }

    // ---------- dialogs ----------

    fn dialogs(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        response: &mut ViewResponse,
    ) {
        let Some(confirm) = self.confirm else {
            return;
        };
        let (title, hint, button) = match confirm {
            Confirm::DeleteInvoice(id) => {
                let number = project.billing().invoice(id).map(|i| i.number.clone()).unwrap_or_default();
                (
                    format!("Rechnung {number} löschen?"),
                    "Die Aufmaßzeilen der Rechnung bleiben erhalten (nicht zugeordnet). Strg+Z holt die Rechnung zurück.".to_string(),
                    "Löschen",
                )
            }
            Confirm::RetakeContract => (
                "Auftrag neu übernehmen?".to_string(),
                "Der Auftragsstand wird aus dem beauftragten Angebot neu festgeschrieben (Mengen, Preise, Nachlass, Vertragsbedingungen). Rechnungen, Aufmaß und Nachträge bleiben. Strg+Z macht es rückgängig.".to_string(),
                "Übernehmen",
            ),
        };
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 480.0, |ui| {
            widgets::title(ui, palette, &title, &hint);
            match confirm {
                Confirm::DeleteInvoice(_) => widgets::danger_dialog_buttons(ui, palette, button),
                Confirm::RetakeContract => widgets::dialog_buttons(ui, palette, button),
            }
        });
        if close || cancelled {
            self.confirm = None;
        } else if confirmed {
            self.confirm = None;
            match confirm {
                Confirm::DeleteInvoice(id) => {
                    if project.billing_mut().remove_invoice(id).is_some() {
                        self.invoice = None;
                        self.row = None;
                        response.dirty = true;
                        response.toast = Some("Rechnung gelöscht.".into());
                    }
                }
                Confirm::RetakeContract => self.take_over(project, response),
            }
        }
    }
}

// ---------- helpers ----------

/// Checked performance in the invoice's quantity sense (for the list).
fn performance_in_sense(check: &InvoiceCheck, invoice: &Invoice) -> Money {
    let lines = check.positions.iter().chain(check.unchanged.iter());
    match invoice.quantity_mode() {
        Mode::Cumulative => lines.map(|line| line.cumulative_total).sum(),
        Mode::Increment => lines.map(|line| line.increment_total).sum(),
    }
}

pub(crate) fn severity_color(palette: &Palette, finding: &Finding) -> egui::Color32 {
    match finding.severity() {
        Severity::Error => palette.error,
        Severity::Warning => palette.warn,
        Severity::Info => palette.text_dim,
    }
}

/// Quantity in German format without trailing zeros (up to four decimals).
pub(crate) fn qty(value: Decimal) -> String {
    let text = format_decimal_de(value, 4);
    text.trim_end_matches('0').trim_end_matches(',').to_string()
}

fn paint_cells(
    ui: &egui::Ui,
    rect: egui::Rect,
    columns: &[(&str, f32, bool)],
    text: impl Fn(usize) -> String,
    color: egui::Color32,
    head: bool,
) {
    let painter = ui.painter_at(rect);
    let mut x = rect.left() + 6.0;
    for (index, (_, width, right)) in columns.iter().enumerate() {
        let cell = egui::Rect::from_min_size(
            egui::pos2(x, rect.top()),
            egui::vec2(*width - 8.0, rect.height()),
        );
        let font = if head {
            egui::FontId::proportional(12.0)
        } else {
            egui::FontId::proportional(13.0)
        };
        let content = text(index);
        let galley = painter.layout_no_wrap(content, font, color);
        let pos = if *right {
            egui::pos2(
                cell.right() - galley.size().x,
                cell.center().y - galley.size().y / 2.0,
            )
        } else {
            egui::pos2(cell.left(), cell.center().y - galley.size().y / 2.0)
        };
        painter.with_clip_rect(cell).galley(pos, galley, color);
        x += width;
    }
    if head {
        painter.line_segment(
            [
                egui::pos2(rect.left(), rect.bottom() - 0.5),
                egui::pos2(rect.right(), rect.bottom() - 0.5),
            ],
            Stroke::new(1.0, color.gamma_multiply(0.5)),
        );
    }
}

/// A single-line text field; true when the text changed.
fn text_field(ui: &mut egui::Ui, value: &mut String, width: f32) -> bool {
    ui.add_sized([width, 22.0], egui::TextEdit::singleline(value))
        .changed()
}

fn mode_combo(
    ui: &mut egui::Ui,
    id: (&'static str, u64),
    mode: &mut Mode,
    quantities: bool,
    kind: InvoiceKind,
) -> bool {
    let fixed = matches!(
        kind,
        InvoiceKind::Final | InvoiceKind::Single | InvoiceKind::SecurityRelease
    );
    let label = |mode: Mode| {
        if quantities {
            mode.quantity_label()
        } else {
            mode.release_label()
        }
    };
    let shown = match kind {
        InvoiceKind::Final => Mode::Cumulative,
        InvoiceKind::Single => Mode::Increment,
        _ => *mode,
    };
    let mut choice = *mode;
    ui.add_enabled_ui(!fixed, |ui| {
        egui::ComboBox::from_id_salt(id)
            .selected_text(label(shown))
            .width(250.0)
            .show_ui(ui, |ui| {
                for option in [Mode::Cumulative, Mode::Increment] {
                    ui.selectable_value(&mut choice, option, label(option));
                }
            });
    });
    if choice != *mode {
        *mode = choice;
        true
    } else {
        false
    }
}

/// A number field that keeps the typed text while it has the focus and
/// hands the value over when it is left: Some(new value) when it changed
/// (None inside = emptied). A wrong number is reported and dropped.
fn decimal_field(
    ui: &mut egui::Ui,
    texts: &mut BTreeMap<&'static str, String>,
    key: &'static str,
    current: Option<Decimal>,
    places: u32,
    width: f32,
    response: &mut ViewResponse,
) -> Option<Option<Decimal>> {
    let shown = current
        .map(|value| format_decimal_de(value, places))
        .unwrap_or_default();
    let text = texts.entry(key).or_insert_with(|| shown.clone());
    let field = ui.add_sized(
        [width, 22.0],
        egui::TextEdit::singleline(text).align(egui::Align2::RIGHT_CENTER),
    );
    if field.has_focus() || field.changed() {
        return None;
    }
    if field.lost_focus() {
        let typed = texts.remove(key).unwrap_or_default();
        if typed.trim() == shown.trim() {
            return None;
        }
        if typed.trim().is_empty() {
            return Some(None);
        }
        return match parse_decimal_de(&typed) {
            Ok(value) => Some(Some(value)),
            Err(error) => {
                response.toast = Some(error);
                None
            }
        };
    }
    // Not focused: show the stored value (it may change by undo).
    texts.insert(key, shown);
    None
}
