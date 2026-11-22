//! Building blocks of the larger forms (Stammdaten, LV-Eigenschaften,
//! Einstellungen): reiter, fields with their error right below, sections.

use eframe::egui::{self, RichText, Stroke};

use crate::theme::Palette;

/// A row of tabs (Reiter); returns true when the tab changed.
pub fn tab_bar<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    palette: &Palette,
    tabs: &[(T, &str)],
    current: &mut T,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (tab, label) in tabs {
            let active = *current == *tab;
            let text = RichText::new(*label).size(13.5).color(if active {
                palette.accent
            } else {
                palette.text
            });
            let button = egui::Button::new(if active { text.strong() } else { text })
                .fill(if active {
                    palette.accent_soft
                } else {
                    egui::Color32::TRANSPARENT
                })
                .stroke(Stroke::NONE)
                .min_size(egui::vec2(0.0, 30.0));
            if ui.add(button).clicked() && !active {
                *current = *tab;
                changed = true;
            }
        }
    });
    let rect = ui.min_rect();
    ui.painter().hline(
        rect.x_range(),
        ui.cursor().top() + 2.0,
        Stroke::new(1.0, palette.line),
    );
    ui.add_space(12.0);
    changed
}

/// A small heading inside a tab.
pub fn section(ui: &mut egui::Ui, palette: &Palette, title: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(title).size(13.0).strong().color(palette.text));
    ui.add_space(6.0);
}

/// A dim one-line explanation.
pub fn hint(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).size(11.5).color(palette.text_dim));
}

/// Label, single-line field and – if any – the error right under it in red.
pub fn field(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    error: Option<&str>,
) -> egui::Response {
    field_with(ui, palette, label, value, error, "")
}

/// Like [`field`] with a placeholder („TT.MM.JJJJ“).
pub fn field_with(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    error: Option<&str>,
    placeholder: &str,
) -> egui::Response {
    let caption = ui.label(RichText::new(label).size(12.0).color(palette.text_dim));
    ui.add_space(3.0);
    let mut edit = egui::TextEdit::singleline(value)
        .desired_width(ui.available_width())
        .margin(egui::Margin::symmetric(8, 6))
        .hint_text(placeholder);
    if error.is_some() {
        edit = edit.text_color(palette.error);
    }
    // The caption names the field for screen readers and UI tests.
    let response = ui.add(edit).labelled_by(caption.id);
    if error.is_some() {
        ui.painter().rect_stroke(
            response.rect,
            egui::CornerRadius::same(4),
            Stroke::new(1.5, palette.error),
            egui::StrokeKind::Outside,
        );
    }
    if let Some(error) = error {
        ui.add_space(2.0);
        ui.label(RichText::new(error).size(11.5).color(palette.error));
    }
    ui.add_space(8.0);
    response
}

/// A path field with its chooser button on the same line („Datei wählen …“):
/// typing stays possible, the button opens the file dialog. Returns whether
/// the button was clicked.
pub fn path_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    placeholder: &str,
    button: &str,
) -> bool {
    let caption = ui.label(RichText::new(label).size(12.0).color(palette.text_dim));
    ui.add_space(3.0);
    let mut clicked = false;
    ui.horizontal(|ui| {
        let button_width = ui
            .painter()
            .layout_no_wrap(
                button.to_string(),
                egui::FontId::proportional(13.0),
                palette.text,
            )
            .size()
            .x
            + 24.0;
        let width = (ui.available_width() - button_width - ui.spacing().item_spacing.x).max(120.0);
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(width)
                .margin(egui::Margin::symmetric(8, 6))
                .hint_text(placeholder),
        )
        .labelled_by(caption.id);
        clicked = ui
            .add(egui::Button::new(button).min_size(egui::vec2(button_width, 28.0)))
            .clicked();
    });
    ui.add_space(8.0);
    clicked
}

/// A date field: typed by hand (short forms are written out when the field
/// is left) or picked in the calendar behind the calendar button.
pub fn date_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    error: Option<&str>,
) -> egui::Response {
    crate::date_picker::date_field(ui, palette, label, value, error)
}

/// Several-line field.
pub fn multiline(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    rows: usize,
) {
    ui.label(RichText::new(label).size(12.0).color(palette.text_dim));
    ui.add_space(3.0);
    ui.add(
        egui::TextEdit::multiline(value)
            .desired_width(ui.available_width())
            .desired_rows(rows)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    ui.add_space(8.0);
}

/// Places `count` equally wide columns side by side; `add` gets the column index.
pub fn columns(ui: &mut egui::Ui, count: usize, mut add: impl FnMut(usize, &mut egui::Ui)) {
    let gap = 12.0;
    let width = ((ui.available_width() - gap * (count as f32 - 1.0)) / count as f32).max(60.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for index in 0..count {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(width);
                    add(index, ui);
                },
            );
        }
    });
}

/// Two columns with given width shares (e.g. PLZ narrow, Ort wide).
pub fn split(
    ui: &mut egui::Ui,
    left_share: f32,
    left: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui),
) {
    let gap = 12.0;
    let total = ui.available_width() - gap;
    let left_width = (total * left_share).max(60.0);
    let right_width = (total - left_width).max(60.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.allocate_ui_with_layout(
            egui::vec2(left_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(left_width);
                left(ui);
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(right_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(right_width);
                right(ui);
            },
        );
    });
}

/// Choice buttons in a row (like the theme buttons of the start screen).
pub fn choice<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    palette: &Palette,
    options: &[(T, &str)],
    current: &mut T,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for (value, label) in options {
            let active = *current == *value;
            let button = egui::Button::new(RichText::new(*label).size(12.5).color(if active {
                palette.on_accent
            } else {
                palette.text
            }))
            .fill(if active {
                palette.accent
            } else {
                palette.button
            })
            .min_size(egui::vec2(84.0, 28.0));
            if ui.add(button).clicked() && !active {
                *current = *value;
                changed = true;
            }
        }
    });
    ui.add_space(8.0);
    changed
}

/// Name, Zusatz, Straße, PLZ/Ort, Telefon, E-Mail of one address.
pub fn address(ui: &mut egui::Ui, palette: &Palette, address: &mut proava_domain::Address) {
    address_with_focus(ui, palette, address, &mut false);
}

/// Like [`address`]; the name field takes the keyboard focus once when `focus` is set.
pub fn address_with_focus(
    ui: &mut egui::Ui,
    palette: &Palette,
    address: &mut proava_domain::Address,
    focus: &mut bool,
) {
    let name = field(ui, palette, "Name", &mut address.name1, None);
    if std::mem::take(focus) {
        name.request_focus();
    }
    field(ui, palette, "Zusatz (Abteilung)", &mut address.name2, None);
    field(ui, palette, "Straße", &mut address.street, None);
    split(
        ui,
        0.28,
        |ui| {
            field(ui, palette, "PLZ", &mut address.postal_code, None);
        },
        |ui| {
            field(ui, palette, "Ort", &mut address.city, None);
        },
    );
    split(
        ui,
        0.45,
        |ui| {
            field(ui, palette, "Telefon", &mut address.phone, None);
        },
        |ui| {
            field(ui, palette, "E-Mail", &mut address.email, None);
        },
    );
    // The other fields of the GAEB address: open when one is filled.
    let filled = [
        &address.contact,
        &address.country,
        &address.fax,
        &address.name3,
        &address.name4,
        &address.vat_id,
        &address.iln,
    ]
    .iter()
    .any(|text| !text.trim().is_empty());
    egui::CollapsingHeader::new(
        RichText::new("Weitere Angaben (Ansprechpartner, Land, Fax, USt-IdNr. …)")
            .size(12.0)
            .color(palette.text_dim),
    )
    .id_salt(ui.next_auto_id())
    .default_open(filled)
    .show(ui, |ui| {
        split(
            ui,
            0.55,
            |ui| {
                field(ui, palette, "Ansprechpartner", &mut address.contact, None);
            },
            |ui| {
                field(ui, palette, "Land", &mut address.country, None);
            },
        );
        split(
            ui,
            0.45,
            |ui| {
                field(ui, palette, "Fax", &mut address.fax, None);
            },
            |ui| {
                field(ui, palette, "USt-IdNr.", &mut address.vat_id, None);
            },
        );
        split(
            ui,
            0.5,
            |ui| {
                field(ui, palette, "Zusatz 2", &mut address.name3, None);
            },
            |ui| {
                field(ui, palette, "Zusatz 3", &mut address.name4, None);
            },
        );
        field(ui, palette, "ILN/GLN", &mut address.iln, None);
    });
}

/// The address with every field trimmed.
pub fn trimmed(address: &proava_domain::Address) -> proava_domain::Address {
    let trim = |text: &str| text.trim().to_string();
    proava_domain::Address {
        name1: trim(&address.name1),
        name2: trim(&address.name2),
        name3: trim(&address.name3),
        name4: trim(&address.name4),
        street: trim(&address.street),
        postal_code: trim(&address.postal_code),
        city: trim(&address.city),
        country: trim(&address.country),
        iln: trim(&address.iln),
        contact: trim(&address.contact),
        phone: trim(&address.phone),
        fax: trim(&address.fax),
        email: trim(&address.email),
        vat_id: trim(&address.vat_id),
    }
}
