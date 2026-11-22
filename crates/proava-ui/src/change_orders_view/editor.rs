//! Bearbeiten eines Nachtrags: Kopf, Statuswechsel mit Verlauf,
//! Nachtrags-LV (neue Positionen mit OZ-Vorschlag, Mehr-/Mindermengen),
//! angebotene und geprüfte Werte, Prüfhilfe und Langtext.

use std::collections::HashMap;

use eframe::egui::{self, RichText};
use proava_domain::{
    deviation_percent, format_decimal_de, price_hints, ChangeBasis, ChangeOrder, ChangeOrderId,
    ChangeOrderItem, ChangeOrderStatus, Decimal, ItemChange, LvRow, Money, Position, Project,
    RowId,
};

use super::{
    money_cell, right_cell, status_color, ChangeOrderRequest, ChangeOrdersOutput, ChangeOrdersView,
};
use crate::form_kit;
use crate::numbers;
use crate::rich_editor::RichEditor;
use crate::theme::Palette;
use crate::view::ViewRequest;

pub use proava_application::ChangeOrderExport;

struct StatusForm {
    status: ChangeOrderStatus,
    date: String,
    note: String,
}

struct NewPositionForm {
    short_text: String,
    quantity: String,
    unit: String,
    price: String,
    ordinal: String,
    area: String,
    error: Option<String>,
    focus: bool,
}

#[derive(Default)]
pub(super) struct EditorState {
    pub focus_title: bool,
    selected: Option<RowId>,
    long_text: Option<(RowId, RichEditor)>,
    status: Option<StatusForm>,
    new_position: Option<NewPositionForm>,
    assign_search: String,
}

// Columns of the table with typed numbers.
const OFFERED_QTY: u8 = 1;
const OFFERED_PRICE: u8 = 2;
const CHECKED_QTY: u8 = 3;
const CHECKED_PRICE: u8 = 4;

/// Names the change of this frame for the undo history; a change of the
/// LV keeps the state before it (with the LV).
fn record(
    output: &mut ChangeOrdersOutput,
    project: &Project,
    with_lv: bool,
    label: &'static str,
    join: bool,
) {
    if output.label.is_none() {
        output.label = Some(label);
    }
    if with_lv && output.before.is_none() {
        output.before = project
            .contract_snapshot_with_lv(project.active_lv())
            .map(|snapshot| (snapshot, join));
    }
    output.response.dirty = true;
    output.lv_changed |= with_lv;
}

/// Writes an edited change order back (one undo step, typing joins).
fn store(
    project: &mut Project,
    output: &mut ChangeOrdersOutput,
    edited: ChangeOrder,
    label: &'static str,
    join: bool,
) {
    if project.change_order(edited.id) == Some(&edited) {
        return;
    }
    record(output, project, false, label, join);
    if let Some(target) = project.change_order_mut(edited.id) {
        *target = edited;
    }
}

/// Text field for a number in the table: shows the value, keeps what is
/// typed while it has the focus, and hands the text over when leaving.
fn number_cell(
    ui: &mut egui::Ui,
    typed: &mut HashMap<(RowId, u8), String>,
    key: (RowId, u8),
    shown: String,
    width: f32,
    hint: &str,
) -> Option<String> {
    let id = ui.make_persistent_id(("change_order_cell", key.0 .0, key.1));
    let mut text = typed.get(&key).cloned().unwrap_or_else(|| shown.clone());
    let response = ui.add_sized(
        [width, ui.spacing().interact_size.y],
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .align(egui::Align2::RIGHT_CENTER)
            .hint_text(hint),
    );
    if response.has_focus() {
        typed.insert(key, text.clone());
    }
    if response.lost_focus() {
        let text = typed.remove(&key).unwrap_or(text);
        return (text.trim() != shown.trim()).then_some(text);
    }
    None
}

fn price_or_none(text: &str) -> Result<Option<Decimal>, String> {
    numbers::parse_price(text).map_err(|error| format!("EP: {error}"))
}

fn quantity_or_none(text: &str) -> Result<Option<Decimal>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    numbers::parse_quantity(text)
        .map(Some)
        .map_err(|error| format!("Menge: {error}"))
}

/// „neu“, „Mehrmenge“, „Mindermenge“.
pub fn item_kind_label(item: &ChangeOrderItem) -> &'static str {
    match item.change {
        ItemChange::NewPosition => "neu",
        ItemChange::Quantity if item.offered_quantity.is_sign_negative() => "Mindermenge",
        ItemChange::Quantity => "Mehrmenge",
    }
}

/// Order number, short text and unit of an item's position (LV, else contract).
pub fn item_texts(project: &Project, item: &ChangeOrderItem) -> (String, String, String) {
    if let Some(position) = project.lv().position_by_id(item.position) {
        return (
            position.ordinal().to_string(),
            position.short_text().to_string(),
            position.unit().to_string(),
        );
    }
    project
        .contract()
        .and_then(|contract| contract.item(item.position))
        .map(|item| {
            (
                item.ordinal.clone(),
                item.short_text.clone(),
                item.unit.clone(),
            )
        })
        .unwrap_or_else(|| {
            (
                String::new(),
                "(nicht mehr im LV)".to_string(),
                String::new(),
            )
        })
}

/// Shows the editor; returns true for „Zur Nachtragsliste“.
pub(super) fn show(
    view: &mut ChangeOrdersView,
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    id: ChangeOrderId,
    output: &mut ChangeOrdersOutput,
) -> bool {
    let Some(order) = project.change_order(id).cloned() else {
        return true;
    };
    let mut back = false;
    ui.horizontal(|ui| {
        if ui.button("Zur Nachtragsliste").clicked() {
            back = true;
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new(format!("{} {}", order.label(), order.title))
                .size(16.0)
                .strong()
                .color(palette.text),
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new(order.status.label())
                .size(13.0)
                .strong()
                .color(status_color(palette, order.status)),
        );
        if !order.status_date().is_empty() {
            ui.label(RichText::new(format!("seit {}", order.status_date())).size(12.5).color(palette.text_dim));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Löschen …").clicked() {
                view.ask_delete(id);
            }
            if ui
                .button("Prüfung drucken …")
                .on_hover_text("Prüfung Nachtragsangebot: Positionen angeboten/geprüft, Vermerke, Ergebnis, Empfehlung, Unterschrift")
                .clicked()
            {
                output.requests.push(ChangeOrderRequest::PrintCheck(id));
            }
            crate::menus::menu(ui, "GAEB", |ui| {
                if ui
                    .button("Nachtragsangebot einlesen (X84, X86, X82, D8x) …")
                    .on_hover_text("Angebotene Mengen und Preise der Firma übernehmen; unbekannte Positionen kommen mit Text in den Nachtragsbereich")
                    .clicked()
                {
                    output.requests.push(ChangeOrderRequest::ImportOffer(id));
                    ui.close();
                }
                ui.separator();
                for kind in [ChangeOrderExport::Request, ChangeOrderExport::Order] {
                    if ui.button(format!("{} …", kind.label())).clicked() {
                        output.requests.push(ChangeOrderRequest::Export(id, kind));
                        ui.close();
                    }
                }
            });
            if ui.button("Status ändern …").clicked() {
                view.editor.status = Some(StatusForm {
                    status: next_status(order.status),
                    date: view.today().to_string(),
                    note: String::new(),
                });
            }
        });
    });
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .id_salt("change_order_edit_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            head(view, ui, palette, project, &order, output);
            ui.add_space(8.0);
            items(view, ui, palette, project, id, output);
        });
    status_dialog(view, ui, palette, project, id, output);
    back
}

fn next_status(status: ChangeOrderStatus) -> ChangeOrderStatus {
    match status {
        ChangeOrderStatus::Announced => ChangeOrderStatus::Offered,
        ChangeOrderStatus::Offered => ChangeOrderStatus::InReview,
        ChangeOrderStatus::InReview => ChangeOrderStatus::Reviewed,
        ChangeOrderStatus::Reviewed => ChangeOrderStatus::Approved,
        other => other,
    }
}

fn head(
    view: &mut ChangeOrdersView,
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    order: &ChangeOrder,
    output: &mut ChangeOrdersOutput,
) {
    let mut edited = order.clone();
    let contractor = project
        .contract()
        .map(|contract| contract.contractor.clone())
        .unwrap_or_default();
    egui::CollapsingHeader::new(
        RichText::new("Kopf und Prüfvermerk")
            .strong()
            .color(palette.text),
    )
    .id_salt(("change_order_head", order.id.0))
    .default_open(true)
    .show(ui, |ui| {
        form_kit::columns(ui, 3, |column, ui| match column {
            0 => {
                crate::widgets::row(ui, |ui| {
                    ui.label(RichText::new("Nummer").size(12.0).color(palette.text_dim));
                    ui.add(
                        egui::DragValue::new(&mut edited.number)
                            .range(1..=999)
                            .speed(0.1),
                    )
                    .on_hover_text("Nachtragsnummer = GAEB CONo (N01 = 1)");
                });
                ui.add_space(4.0);
                let caption = ui.label(
                    RichText::new("Bezeichnung (Inhalt, Zweck)")
                        .size(12.0)
                        .color(palette.text_dim),
                );
                let title = ui
                    .add(
                        egui::TextEdit::singleline(&mut edited.title)
                            .desired_width(ui.available_width()),
                    )
                    .labelled_by(caption.id);
                if std::mem::take(&mut view.editor.focus_title) {
                    title.request_focus();
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Anlass (VOB/B)")
                        .size(12.0)
                        .color(palette.text_dim),
                );
                egui::ComboBox::from_id_salt(("change_order_basis", order.id.0))
                    .selected_text(edited.basis.label())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for basis in ChangeBasis::ALL {
                            ui.selectable_value(&mut edited.basis, basis, basis.label());
                        }
                    });
                ui.add_space(8.0);
                form_kit::multiline(
                    ui,
                    palette,
                    "Begründung der Erfordernis",
                    &mut edited.reason,
                    3,
                );
                form_kit::field_with(
                    ui,
                    palette,
                    "Firma",
                    &mut edited.contractor,
                    None,
                    &contractor,
                );
            }
            1 => {
                let date = |value: &str| proava_domain::optional_date(value).err();
                let error = date(&edited.announced_date);
                form_kit::field_with(
                    ui,
                    palette,
                    "Angekündigt am",
                    &mut edited.announced_date,
                    error.as_deref(),
                    "TT.MM.JJJJ",
                );
                let error = date(&edited.offer_due);
                form_kit::field_with(
                    ui,
                    palette,
                    "Angebotsfrist",
                    &mut edited.offer_due,
                    error.as_deref(),
                    "TT.MM.JJJJ",
                );
                form_kit::field(
                    ui,
                    palette,
                    "Angebots-Nr. der Firma",
                    &mut edited.offer_number,
                    None,
                );
                let error = date(&edited.offer_date);
                form_kit::field_with(
                    ui,
                    palette,
                    "Angebot vom",
                    &mut edited.offer_date,
                    error.as_deref(),
                    "TT.MM.JJJJ",
                );
                let error = date(&edited.decision_due);
                form_kit::field_with(
                    ui,
                    palette,
                    "Entscheidung bis",
                    &mut edited.decision_due,
                    error.as_deref(),
                    "TT.MM.JJJJ",
                );
                let error = date(&edited.decision_date);
                form_kit::field_with(
                    ui,
                    palette,
                    "Beauftragt/entschieden am",
                    &mut edited.decision_date,
                    error.as_deref(),
                    "TT.MM.JJJJ",
                );
            }
            _ => {
                let error = proava_domain::optional_date(&edited.checked_date).err();
                form_kit::field_with(
                    ui,
                    palette,
                    "Geprüft am",
                    &mut edited.checked_date,
                    error.as_deref(),
                    "TT.MM.JJJJ",
                );
                form_kit::field(ui, palette, "Geprüft von", &mut edited.checked_by, None);
                form_kit::multiline(
                    ui,
                    palette,
                    "Prüfvermerk (Ergebnis)",
                    &mut edited.check_note,
                    2,
                );
                form_kit::multiline(
                    ui,
                    palette,
                    "Empfehlung an den Bauherrn",
                    &mut edited.recommendation,
                    2,
                );
                ui.checkbox(&mut edited.discount_applies, "Nachlass des Auftrags gilt")
                    .on_hover_text(
                        "Der Nachlass des Hauptauftrags wird auch auf diesen Nachtrag gerechnet",
                    );
                ui.checkbox(
                    &mut edited.in_forecast,
                    "Geht in die Prognose ein, solange offen",
                )
                .on_hover_text(
                    "Beauftragte Nachträge zählen immer, abgelehnte und zurückgezogene nie",
                );
            }
        });
        if !order.history.is_empty() {
            egui::CollapsingHeader::new(format!("Statusverlauf ({})", order.history.len()))
                .id_salt(("change_order_history", order.id.0))
                .default_open(false)
                .show(ui, |ui| {
                    egui::Grid::new(("change_order_history_grid", order.id.0))
                        .num_columns(3)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for step in &order.history {
                                ui.label(&step.date);
                                ui.label(
                                    RichText::new(step.status.label())
                                        .color(status_color(palette, step.status)),
                                );
                                ui.label(&step.note);
                                ui.end_row();
                            }
                        });
                });
        }
    });
    if edited.title != order.title && order.group.is_some() {
        // The title of the Nachtragsbereich follows (LV step).
        if let Some(index) = project.change_order_area(order.id) {
            let wanted = proava_domain::area_title(&edited);
            let current = project.lv().rows()[index].short_text();
            if current == proava_domain::area_title(order) {
                record(output, project, true, "Nachtrag bearbeiten", true);
                if let Some(LvRow::Group(group)) = project.lv_mut().row_mut(index) {
                    group.set_short_text(wanted);
                }
            }
        }
    }
    store(project, output, edited, "Nachtrag bearbeiten", true);
}

#[allow(clippy::too_many_lines)]
fn items(
    view: &mut ChangeOrdersView,
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    id: ChangeOrderId,
    output: &mut ChangeOrdersOutput,
) {
    let Some(order) = project.change_order(id).cloned() else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "Nachtrags-LV ({})",
                crate::format::count(order.items.len(), "Position", "Positionen")
            ))
            .size(14.0)
            .strong()
            .color(palette.text),
        );
        ui.add_space(10.0);
        if ui
            .button("Neue Position")
            .on_hover_text(
                "Neue Nachtragsposition im Nachtragsbereich am Ende des LV (OZ-Vorschlag)",
            )
            .clicked()
        {
            open_new_position(view, project, id, output);
        }
        assign_menu(view, ui, palette, project, id, output);
        let selected = view
            .editor
            .selected
            .filter(|row| order.item(*row).is_some());
        ui.add_enabled_ui(selected.is_some(), |ui| {
            crate::menus::menu(ui, "Entfernen", |ui| {
                if let Some(row) = selected {
                    if ui.button("Nur aus dem Nachtrag nehmen").clicked() {
                        remove_item(project, id, row, false, output);
                        ui.close();
                    }
                    let new = order
                        .item(row)
                        .is_some_and(|item| item.change == ItemChange::NewPosition);
                    if new
                        && ui
                            .button("Aus dem Nachtrag nehmen und im LV löschen")
                            .clicked()
                    {
                        remove_item(project, id, row, true, output);
                        ui.close();
                    }
                }
            });
            if ui.button("Im LV zeigen").clicked() {
                if let Some(row) = selected {
                    output.response.requests.push(ViewRequest::ShowInLv(row));
                }
            }
        });
    });
    ui.add_space(6.0);
    new_position_form(view, ui, palette, project, id, output);

    let Some(order) = project.change_order(id).cloned() else {
        return;
    };
    let contract = project.contract().cloned();
    let mut edited = order.clone();
    let mut error = None;
    if order.items.is_empty() {
        ui.label(
            RichText::new(
                "Noch keine Positionen. „Neue Position“ schreibt sie in den Nachtragsbereich am Ende des LV; \
                 „Zuordnen“ nimmt Positionen oder Titel aus dem LV und Mehr-/Mindermengen von Auftragspositionen.",
            )
            .size(12.5)
            .color(palette.text_dim),
        );
    } else {
        egui::ScrollArea::horizontal()
            .id_salt("change_order_items_h")
            .show(ui, |ui| {
                egui::Grid::new(("change_order_items", id.0))
                    .striped(true)
                    .num_columns(13)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for head in [
                            "OZ",
                            "Kurztext",
                            "Art",
                            "Einheit",
                            "Menge angeb.",
                            "EP angeb.",
                            "GP angeb.",
                            "Menge gepr.",
                            "EP gepr.",
                            "GP gepr.",
                            "Abw. %",
                            "Status",
                            "Vermerk",
                        ] {
                            ui.label(
                                RichText::new(head)
                                    .size(12.0)
                                    .strong()
                                    .color(palette.text_dim),
                            );
                        }
                        ui.end_row();
                        for (index, item) in order.items.iter().enumerate() {
                            let (ordinal, short, unit) = item_texts(project, item);
                            let selected = view.editor.selected == Some(item.position);
                            if ui
                                .selectable_label(
                                    selected,
                                    if ordinal.is_empty() { "–" } else { &ordinal },
                                )
                                .clicked()
                            {
                                view.editor.selected = Some(item.position);
                            }
                            if ui
                                .selectable_label(
                                    selected,
                                    crate::widgets::fit_line(
                                        ui.painter(),
                                        &short,
                                        &egui::TextStyle::Button.resolve(ui.style()),
                                        240.0,
                                    )
                                    .0,
                                )
                                .on_hover_text(&short)
                                .clicked()
                            {
                                view.editor.selected = Some(item.position);
                            }
                            ui.label(item_kind_label(item));
                            ui.label(unit);
                            let typed = &mut view.typed;
                            let target = &mut edited.items[index];
                            if let Some(text) = number_cell(
                                ui,
                                typed,
                                (item.position, OFFERED_QTY),
                                numbers::quantity_text(item.offered_quantity),
                                76.0,
                                "",
                            ) {
                                match numbers::parse_quantity(&text) {
                                    Ok(value) => target.offered_quantity = value,
                                    Err(message) => error = Some(format!("Menge: {message}")),
                                }
                            }
                            let contract_price = (item.change == ItemChange::Quantity)
                                .then(|| {
                                    contract
                                        .as_ref()
                                        .and_then(|c| c.item(item.position))
                                        .map(|c| c.unit_price)
                                })
                                .flatten();
                            let hint = contract_price
                                .map(|price| format_decimal_de(price, 2))
                                .unwrap_or_default();
                            if let Some(text) = number_cell(
                                ui,
                                typed,
                                (item.position, OFFERED_PRICE),
                                numbers::price_text(item.offered_unit_price),
                                104.0,
                                &hint,
                            ) {
                                match price_or_none(&text) {
                                    Ok(value) => target.offered_unit_price = value,
                                    Err(message) => error = Some(message),
                                }
                            }
                            money_cell(ui, item.offered_total(contract.as_ref()));
                            if let Some(text) = number_cell(
                                ui,
                                typed,
                                (item.position, CHECKED_QTY),
                                item.checked_quantity
                                    .map(numbers::quantity_text)
                                    .unwrap_or_default(),
                                76.0,
                                &numbers::quantity_text(item.offered_quantity),
                            ) {
                                match quantity_or_none(&text) {
                                    Ok(value) => target.checked_quantity = value,
                                    Err(message) => error = Some(message),
                                }
                            }
                            let offered = item.offered_price(contract.as_ref());
                            if let Some(text) = number_cell(
                                ui,
                                typed,
                                (item.position, CHECKED_PRICE),
                                numbers::price_text(item.checked_unit_price),
                                104.0,
                                &numbers::price_text(offered),
                            ) {
                                match price_or_none(&text) {
                                    Ok(value) => target.checked_unit_price = value,
                                    Err(message) => error = Some(message),
                                }
                            }
                            if matches!(
                                order.item_status(item),
                                ChangeOrderStatus::Rejected | ChangeOrderStatus::Withdrawn
                            ) {
                                // Zählt nicht: gedämpft in Klammern.
                                right_cell(
                                    ui,
                                    104.0,
                                    RichText::new(format!(
                                        "({})",
                                        item.checked_total(contract.as_ref())
                                    ))
                                    .color(palette.text_dim),
                                );
                            } else {
                                money_cell(ui, item.checked_total(contract.as_ref()));
                            }
                            let deviation = match (item.checked_price(contract.as_ref()), offered) {
                                (Some(checked), Some(offered)) => {
                                    deviation_percent(checked, offered)
                                }
                                _ => None,
                            };
                            right_cell(
                                ui,
                                50.0,
                                deviation
                                    .map(|value| format_decimal_de(value, 1))
                                    .unwrap_or_default(),
                            );
                            let mut status = target.status;
                            egui::ComboBox::from_id_salt((
                                "change_order_item_status",
                                id.0,
                                item.position.0,
                            ))
                            .selected_text(match status {
                                Some(status) => status.label().to_string(),
                                None => "wie Nachtrag".to_string(),
                            })
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut status, None, "wie Nachtrag");
                                for choice in ChangeOrderStatus::ALL {
                                    ui.selectable_value(&mut status, Some(choice), choice.label());
                                }
                            });
                            target.status = status;
                            ui.add(
                                egui::TextEdit::singleline(&mut target.note)
                                    .desired_width(220.0)
                                    .hint_text("Prüfvermerk"),
                            );
                            ui.end_row();
                        }
                        // Sums.
                        ui.label("");
                        ui.label(RichText::new("Summe").strong().color(palette.text));
                        for _ in 0..4 {
                            ui.label("");
                        }
                        money_cell(ui, order.offered_total(contract.as_ref()));
                        ui.label("");
                        ui.label("");
                        money_cell(ui, order.checked_total(contract.as_ref()));
                        let deviation = deviation_percent(
                            order.checked_total(contract.as_ref()).to_decimal(),
                            order.offered_total(contract.as_ref()).to_decimal(),
                        );
                        right_cell(
                            ui,
                            50.0,
                            deviation
                                .map(|value| format_decimal_de(value, 1))
                                .unwrap_or_default(),
                        );
                        ui.end_row();
                    });
            });
        let approved = order.approved_total(contract.as_ref());
        let after = approved - order.discount_on(approved, contract.as_ref());
        let credit = order.credit_total(contract.as_ref());
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "Beauftragt: {after}{}{}",
                if order.discount_applies
                    && contract
                        .as_ref()
                        .is_some_and(|c| !c.discount_percent.is_zero())
                {
                    " (nach Nachlass)"
                } else {
                    ""
                },
                if credit.cents() < 0 {
                    format!(" · Gutschrift/Minderung: {credit}")
                } else {
                    String::new()
                },
            ))
            .size(12.5)
            .color(palette.text_dim),
        );
        for problem in order.problems(contract.as_ref(), project.lv()) {
            ui.label(RichText::new(problem).size(12.0).color(palette.warn));
        }
    }
    if let Some(message) = error {
        output.response.toast = Some(message);
    }
    let numbers_changed = edited
        .items
        .iter()
        .zip(&order.items)
        .any(|(a, b)| a.note == b.note && a != b);
    store(
        project,
        output,
        edited,
        "Nachtragsposition bearbeiten",
        !numbers_changed,
    );

    if let Some(row) = view
        .editor
        .selected
        .filter(|row| order.item(*row).is_some())
    {
        ui.add_space(10.0);
        detail(view, ui, palette, project, id, row, output);
    }
}

fn remove_item(
    project: &mut Project,
    id: ChangeOrderId,
    row: RowId,
    delete: bool,
    output: &mut ChangeOrdersOutput,
) {
    record(
        output,
        project,
        delete,
        "Nachtragsposition entfernen",
        false,
    );
    if let Err(error) = project.remove_change_order_item(id, row, delete) {
        output.response.toast = Some(error);
    }
}

fn open_new_position(
    view: &mut ChangeOrdersView,
    project: &Project,
    id: ChangeOrderId,
    output: &mut ChangeOrdersOutput,
) {
    match project.change_order_proposal(id) {
        Ok(proposal) => {
            view.editor.new_position = Some(NewPositionForm {
                short_text: String::new(),
                quantity: "1".into(),
                unit: "St".into(),
                price: String::new(),
                ordinal: proposal.ordinal,
                area: proposal.title,
                error: None,
                focus: true,
            });
        }
        Err(error) => output.response.toast = Some(error),
    }
}

fn new_position_form(
    view: &mut ChangeOrdersView,
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    id: ChangeOrderId,
    output: &mut ChangeOrdersOutput,
) {
    let Some(form) = view.editor.new_position.as_mut() else {
        return;
    };
    let mut insert = false;
    let mut cancel = false;
    egui::Frame::new()
        .fill(palette.panel)
        .stroke(egui::Stroke::new(1.0, palette.line))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("Neue Position in „{}“", form.area))
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(4.0);
            crate::widgets::row(ui, |ui| {
                ui.label("OZ");
                ui.add(egui::TextEdit::singleline(&mut form.ordinal).desired_width(110.0));
                ui.label("Kurztext");
                let short =
                    ui.add(egui::TextEdit::singleline(&mut form.short_text).desired_width(320.0));
                if std::mem::take(&mut form.focus) {
                    short.request_focus();
                }
                ui.label("Menge");
                ui.add(egui::TextEdit::singleline(&mut form.quantity).desired_width(70.0));
                ui.label("Einheit");
                ui.add(egui::TextEdit::singleline(&mut form.unit).desired_width(50.0));
                ui.label("EP angeboten");
                let price = ui.add(egui::TextEdit::singleline(&mut form.price).desired_width(90.0));
                if price.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    insert = true;
                }
                if ui.button("Einfügen").clicked() {
                    insert = true;
                }
                if ui.button("Abbrechen").clicked() {
                    cancel = true;
                }
            });
            if let Some(error) = &form.error {
                ui.label(RichText::new(error).size(12.0).color(palette.error));
            }
        });
    ui.add_space(6.0);
    if cancel {
        view.editor.new_position = None;
        return;
    }
    if !insert {
        return;
    }
    let parsed = (|| -> Result<(Decimal, Option<Decimal>), String> {
        if form.short_text.trim().is_empty() {
            return Err("Bitte einen Kurztext eintragen.".to_string());
        }
        let quantity =
            numbers::parse_quantity(&form.quantity).map_err(|error| format!("Menge: {error}"))?;
        let price = price_or_none(&form.price)?;
        Ok((quantity, price))
    })();
    match parsed {
        Ok((quantity, price)) => {
            let position = Position::new(
                form.ordinal.trim(),
                form.short_text.trim(),
                quantity,
                form.unit.trim(),
                None,
            );
            record(output, project, true, "Nachtragsposition einfügen", false);
            match project.add_change_order_position(id, position, price) {
                Ok(row) => {
                    view.editor.selected = Some(row);
                    // Ready for the next one.
                    let next = project
                        .change_order_proposal(id)
                        .map(|proposal| proposal.ordinal)
                        .unwrap_or_default();
                    if let Some(form) = view.editor.new_position.as_mut() {
                        form.short_text.clear();
                        form.price.clear();
                        form.quantity = "1".into();
                        form.ordinal = next;
                        form.error = None;
                        form.focus = true;
                    }
                }
                Err(error) => form.error = Some(error),
            }
        }
        Err(error) => form.error = Some(error),
    }
}

/// „Zuordnen“: LV positions and titles that are not in the contract and in
/// no change order (new positions), and contract positions (Mehr-/Mindermenge).
fn assign_menu(
    view: &mut ChangeOrdersView,
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    id: ChangeOrderId,
    output: &mut ChangeOrdersOutput,
) {
    let mut chosen = None;
    crate::menus::menu(ui, "Zuordnen", |ui| {
        ui.set_min_width(460.0);
        crate::widgets::row(ui, |ui| {
            ui.label("Suche");
            ui.add(egui::TextEdit::singleline(&mut view.editor.assign_search).desired_width(300.0));
        });
        let words: Vec<String> = view
            .editor
            .assign_search
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let matches = |text: &str| {
            let text = text.to_lowercase();
            words.iter().all(|word| text.contains(word))
        };
        let contract = project.contract();
        let in_contract =
            |row: RowId| contract.is_some_and(|contract| contract.item(row).is_some());
        ui.separator();
        ui.label(
            RichText::new("Als neue Nachtragsposition (Titel: alle Positionen darunter)")
                .size(12.0)
                .strong()
                .color(palette.text),
        );
        let mut shown = 0;
        for row in project.lv().rows() {
            let free = match row {
                LvRow::Position(position) => {
                    !in_contract(position.id())
                        && project.change_order_of_position(position.id()).is_none()
                }
                LvRow::Group(_) => true,
                _ => false,
            };
            let text = format!("{} {}", row.ordinal(), row.short_text());
            if !free || !matches(&text) {
                continue;
            }
            if shown >= 40 {
                ui.label("… weitere mit der Suche eingrenzen");
                break;
            }
            shown += 1;
            let text = if row.is_group() {
                format!("Titel {text}")
            } else {
                text
            };
            if ui.button(numbers::fit_text(&text, 440.0, 13.0).0).clicked() {
                chosen = Some(row.id());
                ui.close();
            }
        }
        if let Some(contract) = contract {
            ui.separator();
            ui.label(
                RichText::new("Mehr-/Mindermenge einer Auftragsposition")
                    .size(12.0)
                    .strong()
                    .color(palette.text),
            );
            let mut shown = 0;
            for (row, item) in &contract.items {
                let text = format!("{} {}", item.ordinal, item.short_text);
                if !matches(&text) {
                    continue;
                }
                if shown >= 40 {
                    ui.label("… weitere mit der Suche eingrenzen");
                    break;
                }
                shown += 1;
                if ui.button(numbers::fit_text(&text, 440.0, 13.0).0).clicked() {
                    chosen = Some(*row);
                    ui.close();
                }
            }
        }
    });
    if let Some(row) = chosen {
        record(output, project, false, "Nachtrag zuordnen", false);
        match project.assign_to_change_order(id, row) {
            Ok(count) => {
                output.response.toast = Some(format!(
                    "{} zugeordnet.",
                    crate::format::count(count, "Position", "Positionen")
                ));
                view.editor.selected =
                    Some(row).filter(|row| project.lv().position_by_id(*row).is_some());
            }
            Err(error) => output.response.toast = Some(error),
        }
    }
}

fn detail(
    view: &mut ChangeOrdersView,
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    id: ChangeOrderId,
    row: RowId,
    output: &mut ChangeOrdersOutput,
) {
    let Some(item) = project
        .change_order(id)
        .and_then(|order| order.item(row))
        .cloned()
    else {
        return;
    };
    let hints = price_hints(project, &item);
    let (ordinal, short, _) = item_texts(project, &item);
    let mut take = None;
    let mut reset = false;
    let mut text_change = None;
    let offered = item.offered_price(project.contract());
    let position = project.lv().position_by_id(row).cloned();
    let images = project.images().to_vec();
    egui::Frame::new()
        .fill(palette.panel)
        .stroke(egui::Stroke::new(1.0, palette.line))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(RichText::new(format!("{ordinal} {short}")).strong().color(palette.text));
            ui.add_space(6.0);
            form_kit::split(
                ui,
                0.42,
                |ui| {
                    ui.label(RichText::new("Prüfhilfe EP").strong().color(palette.text));
                    ui.label(
                        RichText::new(format!(
                            "Angeboten: {}",
                            offered.map(|price| format!("{} €", format_decimal_de(price, 2))).unwrap_or_else(|| "–".into())
                        ))
                        .size(12.5),
                    );
                    if hints.is_empty() {
                        form_kit::hint(
                            ui,
                            palette,
                            "Kein Vergleichspreis: weder Auftrags-EP, ähnliche Auftragsposition, Planerpreis noch Preisspiegel.",
                        );
                    }
                    egui::Grid::new(("change_order_hints", row.0)).num_columns(4).spacing([12.0, 4.0]).show(ui, |ui| {
                        for hint in &hints {
                            ui.label(&hint.label);
                            money_cell(ui, Money::from_decimal(hint.price));
                            ui.label(
                                hint.deviation
                                    .map(|value| format!("{} %", format_decimal_de(value, 1)))
                                    .unwrap_or_default(),
                            )
                            .on_hover_text("Angebotener EP gegen diesen Preis");
                            if ui.small_button("als geprüften EP").clicked() {
                                take = Some(hint.price);
                            }
                            ui.end_row();
                        }
                    });
                    ui.add_space(4.0);
                    if ui
                        .button("Wie angeboten")
                        .on_hover_text("Geprüfte Menge und geprüften EP leeren: es gilt das Angebot")
                        .clicked()
                    {
                        reset = true;
                    }
                },
                |ui| {
                    if item.change != ItemChange::NewPosition {
                        form_kit::hint(
                            ui,
                            palette,
                            "Mengenänderung einer Auftragsposition: Text und Einheit stehen im Auftrag. \
                             Menge = Mehrmenge (negativ = Mindermenge); ohne EP gilt der Auftrags-EP.",
                        );
                        return;
                    }
                    let Some(position) = position else { return };
                    let mut short_text = position.short_text().to_string();
                    let mut unit = position.unit().to_string();
                    crate::widgets::row(ui, |ui| {
                        ui.label("Kurztext");
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut short_text).desired_width(ui.available_width() - 120.0),
                        );
                        // As typed while the field has the keyboard (a space at
                        // the end shows at once), trimmed when it is left.
                        if let Some(value) = crate::widgets::live_text(&response, &short_text, position.short_text()) {
                            short_text = value;
                        }
                        ui.label("Einheit");
                        let response = ui.add(egui::TextEdit::singleline(&mut unit).desired_width(50.0));
                        if let Some(value) = crate::widgets::live_text(&response, &unit, position.unit()) {
                            unit = value;
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("Langtext").size(12.0).color(palette.text_dim));
                    if view.editor.long_text.as_ref().is_none_or(|(known, _)| *known != row) {
                        let mut editor = RichEditor::new("change_order_long_text");
                        editor.min_height = 140.0;
                        view.editor.long_text = Some((row, editor));
                    }
                    let mut text = position.long_text().clone();
                    let changed = view
                        .editor
                        .long_text
                        .as_mut()
                        .map(|(_, editor)| editor.show(ui, palette, &mut text, &images).changed)
                        .unwrap_or(false);
                    if changed || short_text != position.short_text() || unit != position.unit() {
                        text_change = Some((short_text, unit, text));
                    }
                },
            );
        });
    if take.is_some() || reset {
        record(output, project, false, "Nachtragsposition prüfen", false);
        if let Some(target) = project
            .change_order_mut(id)
            .and_then(|order| order.item_mut(row))
        {
            if reset {
                target.checked_quantity = None;
                target.checked_unit_price = None;
            } else {
                target.checked_unit_price = take;
            }
        }
        view.typed.retain(|key, _| key.0 != row);
    }
    if let Some((short_text, unit, text)) = text_change {
        if let Some(index) = project.lv().index_of(row) {
            record(output, project, true, "Nachtragsposition Text", true);
            if let Some(target) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                target.set_short_text(&short_text);
                target.set_unit(&unit);
                target.set_long_text(text);
            }
        }
    }
}

fn status_dialog(
    view: &mut ChangeOrdersView,
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    id: ChangeOrderId,
    output: &mut ChangeOrdersOutput,
) {
    let Some(form) = view.editor.status.as_mut() else {
        return;
    };
    let date_error = if form.date.trim().is_empty() {
        Some("Bitte ein Datum eintragen.".to_string())
    } else {
        proava_domain::optional_date(&form.date).err()
    };
    let ((cancel, confirm), dismissed) = crate::widgets::modal(ui, palette, 460.0, |ui| {
        crate::widgets::title(
            ui,
            palette,
            "Status ändern",
            "Der Wechsel wird mit Datum im Statusverlauf festgehalten.",
        );
        ui.label(
            RichText::new("Neuer Status")
                .size(12.0)
                .color(palette.text_dim),
        );
        egui::ComboBox::from_id_salt("change_order_new_status")
            .selected_text(form.status.label())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for status in ChangeOrderStatus::ALL {
                    ui.selectable_value(&mut form.status, status, status.label());
                }
            });
        ui.add_space(8.0);
        form_kit::field_with(
            ui,
            palette,
            "Datum",
            &mut form.date,
            date_error.as_deref(),
            "TT.MM.JJJJ",
        );
        form_kit::multiline(
            ui,
            palette,
            "Vermerk (z. B. Schreiben des Bauherrn vom …)",
            &mut form.note,
            2,
        );
        if form.status == ChangeOrderStatus::Approved {
            form_kit::hint(
                ui,
                palette,
                "Beauftragte Positionen werden abrechenbar und erhöhen die Auftragssumme.",
            );
        }
        crate::widgets::dialog_buttons(ui, palette, "Übernehmen")
    });
    if confirm && date_error.is_none() {
        let (status, date, note) = (form.status, form.date.clone(), form.note.clone());
        record(output, project, false, "Nachtragsstatus", false);
        if let Some(order) = project.change_order_mut(id) {
            order.set_status(status, &date, &note);
            output.response.toast = Some(format!("{}: {}", order.label(), status.label()));
        }
        view.editor.status = None;
    } else if cancel || dismissed {
        view.editor.status = None;
    }
}
