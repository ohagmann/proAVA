//! Editor for the Kopf-/Fußzeile of the printouts (Einstellungen ▸ Kopf-/
//! Fußzeile and Projektstammdaten ▸ Kopf-/Fußzeile): three fields per side,
//! rules, logo, font size, the list of placeholders and a preview.

use eframe::egui::{self, Align2, FontId, RichText, Stroke};
use proava_domain::{FrameLogoPlace, PageFrame, FRAME_FONT_SIZES, PAGE_PLACEHOLDERS};

use crate::theme::Palette;

/// Example values for the preview of the settings (no project at hand).
pub fn sample_value(name: &str) -> String {
    match name {
        "projekt" => "Neubau Verwaltungsgebäude",
        "projektnummer" => "VG-2026-014",
        "projektbezeichnung" => "Neubau Verwaltungsgebäude Musterstadt",
        "baumassnahme" => "Neubau eines Verwaltungsgebäudes",
        "lv" | "lv_bezeichnung" => "Gebäudeautomation",
        "lv_nr" => "480",
        "dokumentart" => "Angebotsaufforderung",
        "titel" => "01 Automationsstationen",
        "datum" | "stand" => "17.11.2026",
        "seite" => "2",
        "seiten" => "12",
        "planer" => "Planungsbüro Beispiel",
        "bearbeiter" | "ersteller" => "M. Muster",
        "bauherr" | "auftraggeber" => "Stadt Musterstadt",
        "vergabenummer" => "V-2026-0815",
        "datei" => "LV_480_LV_ohne_Preise.pdf",
        _ => "",
    }
    .to_string()
}

/// One field with placeholders replaced, as the PDF shows it (at most two
/// lines; a line of only empty placeholders disappears).
pub fn resolve(field: &str, value: &dyn Fn(&str) -> String) -> Vec<String> {
    let mut lines = Vec::new();
    for line in field.lines().take(2) {
        let mut out = String::new();
        let mut rest = line;
        let mut placeholders = 0;
        let mut filled = 0;
        while let Some(start) = rest.find('{') {
            let Some(length) = rest[start..].find('}') else {
                break;
            };
            let name = &rest[start + 1..start + length];
            out.push_str(&rest[..start]);
            if PAGE_PLACEHOLDERS.iter().any(|(known, _)| *known == name) {
                placeholders += 1;
                let text = value(name);
                if !text.trim().is_empty() {
                    filled += 1;
                    out.push_str(text.trim());
                }
            } else {
                out.push_str(&rest[start..=start + length]);
            }
            rest = &rest[start + length + 1..];
        }
        out.push_str(rest);
        let out = out.trim().to_string();
        // „Nr. {vergabenummer}“ without a number disappears, plain text stays.
        if out.is_empty() || (placeholders > 0 && filled == 0) {
            continue;
        }
        lines.push(out);
    }
    lines
}

/// Ready-made field contents of the editor („Baustein …“).
pub const BUILDING_BLOCKS: &[(&str, &str)] = &[
    ("Seite x von y", "Seite {seite} von {seiten}"),
    ("Seite x", "Seite {seite}"),
    ("Ersteller", "{ersteller}"),
    ("Datum", "{datum}"),
    ("Stand: Datum", "Stand: {datum}"),
    ("Projekt", "{projekt}"),
    ("Projektnummer", "{projektnummer}"),
    ("Dokumentart", "{dokumentart}"),
    ("LV", "{lv}"),
    ("Titel der Seite", "{titel}"),
    ("Vergabenummer", "Vergabe-Nr. {vergabenummer}"),
    ("Auftraggeber", "{auftraggeber}"),
    ("Planer", "{planer}"),
    ("Dateiname", "{datei}"),
    ("Zusatzzeile Druckdialog", "{kopftext}"),
    ("Hinweis Beträge", "Alle Einzelbeträge netto in {waehrung}"),
];

/// Puts a building block into a field: an empty field gets it, otherwise it
/// becomes the second line (a field has at most two).
pub fn put_block(field: &mut String, block: &str) {
    let first = field.lines().next().unwrap_or_default().to_string();
    *field = if first.trim().is_empty() {
        block.to_string()
    } else {
        format!("{first}\n{block}")
    };
}

/// Shows the editor; returns true when something changed.
pub fn editor(
    ui: &mut egui::Ui,
    palette: &Palette,
    frame: &mut PageFrame,
    value: &dyn Fn(&str) -> String,
) -> bool {
    let before = frame.clone();
    let label_width = 70.0;
    let width = ((ui.available_width() - label_width - 24.0) / 3.0).clamp(120.0, 220.0);
    ui.horizontal(|ui| {
        ui.add_space(label_width + 8.0);
        for heading in ["links", "Mitte", "rechts"] {
            ui.add_sized(
                [width, 16.0],
                egui::Label::new(RichText::new(heading).size(12.0).color(palette.text_dim)),
            );
        }
    });
    for (label, fields) in [
        ("Kopfzeile", &mut frame.header),
        ("Fußzeile", &mut frame.footer),
    ] {
        ui.horizontal(|ui| {
            ui.add_sized(
                [label_width, 20.0],
                egui::Label::new(RichText::new(label).size(12.5).color(palette.text)),
            );
            for (index, field) in fields.iter_mut().enumerate() {
                ui.add_sized(
                    [width, 44.0],
                    egui::TextEdit::multiline(field)
                        .id(ui.scope_id().with((label, index)))
                        .desired_rows(2)
                        .hint_text("leer"),
                )
                .on_hover_text("Text und Platzhalter, zweite Zeile mit Enter");
            }
        });
        // Bausteine: one click puts „Seite x von y“, „{ersteller}“ … into a field.
        ui.horizontal(|ui| {
            ui.add_space(label_width + 8.0);
            for (index, field) in fields.iter_mut().enumerate() {
                ui.allocate_ui(egui::vec2(width, 18.0), |ui| {
                    ui.set_width(width);
                    crate::menus::menu(ui, RichText::new("Baustein …").size(11.5).color(palette.text_dim), |ui| {
                        for (name, text) in BUILDING_BLOCKS {
                            if ui.button(*name).on_hover_text(*text).clicked() {
                                put_block(field, text);
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("Feld leeren").clicked() {
                            field.clear();
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text(format!(
                        "{label} {}: leeres Feld = Baustein, sonst als zweite Zeile (ersetzt eine vorhandene zweite Zeile)",
                        ["links", "Mitte", "rechts"][index]
                    ));
                });
            }
        });
    }
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut frame.header_rule, "Linie unter der Kopfzeile");
        ui.add_space(10.0);
        ui.checkbox(&mut frame.footer_rule, "Linie über der Fußzeile");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Logo").size(12.5).color(palette.text_dim));
        for place in FrameLogoPlace::ALL {
            ui.radio_value(&mut frame.logo, place, place.label());
        }
    });
    crate::widgets::row(ui, |ui| {
        ui.label(
            RichText::new("Schriftgröße")
                .size(12.5)
                .color(palette.text_dim),
        );
        ui.add(
            egui::DragValue::new(&mut frame.font_size_pt)
                .range(FRAME_FONT_SIZES)
                .suffix(" pt"),
        );
        ui.add_space(16.0);
        if ui
            .button("Standard")
            .on_hover_text(
                "Projekt | Dokumentart | Datum, darunter LV | Bearbeiter | Seite x von y",
            )
            .clicked()
        {
            *frame = PageFrame::default();
        }
    });
    let unknown = frame.unknown_placeholders();
    if !unknown.is_empty() {
        ui.label(
            RichText::new(format!(
                "Unbekannte Platzhalter bleiben so stehen: {}",
                unknown
                    .iter()
                    .map(|name| format!("{{{name}}}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .size(12.0)
            .color(palette.warn),
        );
    }
    ui.add_space(8.0);
    ui.label(RichText::new("Vorschau").size(12.0).color(palette.text_dim));
    preview(ui, palette, frame, value);
    ui.add_space(6.0);
    egui::CollapsingHeader::new(RichText::new("Platzhalter").size(12.5))
        .id_salt(ui.scope_id().with("frame_placeholders"))
        .show(ui, |ui| {
            egui::Grid::new(ui.scope_id().with("frame_placeholder_list"))
                .num_columns(2)
                .spacing([12.0, 3.0])
                .show(ui, |ui| {
                    for (name, meaning) in PAGE_PLACEHOLDERS {
                        ui.label(RichText::new(format!("{{{name}}}")).monospace().size(12.0));
                        ui.label(RichText::new(*meaning).size(12.0).color(palette.text_dim));
                        ui.end_row();
                    }
                });
        });
    *frame != before
}

/// Header and footer of a sample page, cut like in the PDF.
pub fn preview(
    ui: &mut egui::Ui,
    palette: &Palette,
    frame: &PageFrame,
    value: &dyn Fn(&str) -> String,
) {
    let width = ui.available_width().min(560.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 118.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(3), egui::Color32::WHITE);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(3),
        Stroke::new(1.0, palette.line),
        egui::StrokeKind::Inside,
    );
    let size = f32::from(frame.font_size_pt) * 1.25;
    // Like the print style „Klassisch“ (the default): black text.
    let ink = egui::Color32::from_rgb(20, 20, 20);
    let accent = ink;
    let inner = rect.shrink2(egui::vec2(16.0, 10.0));
    let logo = frame.logo != FrameLogoPlace::Off;
    let logo_width = if logo { 44.0 } else { 0.0 };
    let text_left = inner.left()
        + if frame.logo == FrameLogoPlace::Left {
            logo_width + 8.0
        } else {
            0.0
        };
    let text_right = inner.right()
        - if frame.logo == FrameLogoPlace::Right {
            logo_width + 8.0
        } else {
            0.0
        };
    let third = (text_right - text_left) / 3.0;
    let draw_side = |fields: &[String; 3], top: f32, head: bool| -> f32 {
        let mut bottom = top;
        for (index, field) in fields.iter().enumerate() {
            let (x, align) = match index {
                0 => (text_left, Align2::LEFT_TOP),
                1 => (text_left + 1.5 * third, Align2::CENTER_TOP),
                _ => (text_right, Align2::RIGHT_TOP),
            };
            for (line, text) in resolve(field, value).iter().enumerate() {
                let bold = head && index == 1 && line == 0;
                let font = if bold {
                    crate::fonts::bold(size)
                } else {
                    FontId::proportional(size)
                };
                let color = if head && index == 1 { accent } else { ink };
                let shown = fit(&painter, text, &font, third - 6.0);
                let y = top + line as f32 * (size + 3.0);
                painter.text(egui::pos2(x, y), align, shown, font, color);
                bottom = bottom.max(y + size);
            }
        }
        bottom
    };
    let head_bottom = draw_side(&frame.header, inner.top(), true);
    if logo {
        let x = if frame.logo == FrameLogoPlace::Left {
            inner.left()
        } else {
            inner.right() - logo_width
        };
        let logo_rect = egui::Rect::from_min_size(
            egui::pos2(x, inner.top()),
            egui::vec2(logo_width, 2.0 * size),
        );
        painter.rect_stroke(
            logo_rect,
            egui::CornerRadius::ZERO,
            Stroke::new(1.0, palette.line),
            egui::StrokeKind::Inside,
        );
        painter.text(
            logo_rect.center(),
            Align2::CENTER_CENTER,
            "Logo",
            FontId::proportional(10.0),
            ink,
        );
    }
    if frame.header_rule {
        let y = head_bottom.max(inner.top() + if logo { 2.0 * size } else { 0.0 }) + size;
        painter.line_segment(
            [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
            Stroke::new(0.8, palette.line),
        );
    }
    let lines = |fields: &[String; 3]| {
        fields
            .iter()
            .map(|field| resolve(field, value).len())
            .max()
            .unwrap_or(0) as f32
    };
    let foot_height = lines(&frame.footer) * (size + 3.0);
    let foot_top = inner.bottom() - foot_height;
    if frame.footer_rule {
        let y = foot_top - size;
        painter.line_segment(
            [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
            Stroke::new(0.8, palette.line),
        );
    }
    draw_side(&frame.footer, foot_top, false);
    painter.text(
        egui::pos2(rect.center().x, (head_bottom + foot_top) / 2.0),
        Align2::CENTER_CENTER,
        "…",
        FontId::proportional(12.0),
        palette.text_dim,
    );
}

/// Cuts `text` with „…“ to `width`.
fn fit(painter: &egui::Painter, text: &str, font: &FontId, width: f32) -> String {
    let measure = |piece: &str| {
        painter
            .layout_no_wrap(piece.to_string(), font.clone(), egui::Color32::WHITE)
            .size()
            .x
    };
    if measure(text) <= width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut keep = chars.len();
    while keep > 0 && measure(&format!("{}…", chars[..keep].iter().collect::<String>())) > width {
        keep -= 1;
    }
    format!("{}…", chars[..keep].iter().collect::<String>().trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_blocks_fill_an_empty_field_or_the_second_line() {
        let mut field = String::new();
        super::put_block(&mut field, "Seite {seite}");
        assert_eq!(field, "Seite {seite}");
        super::put_block(&mut field, "{ersteller}");
        assert_eq!(field, "Seite {seite}\n{ersteller}");
        super::put_block(&mut field, "{datum}");
        assert_eq!(field, "Seite {seite}\n{datum}", "zweite Zeile ersetzt");
        for (_, text) in super::BUILDING_BLOCKS {
            let mut frame = proava_domain::PageFrame::default();
            frame.header[0] = text.to_string();
            assert!(frame.unknown_placeholders().is_empty(), "{text}");
        }
    }

    #[test]
    fn preview_resolves_like_the_pdf() {
        let value = |name: &str| sample_value(name);
        assert_eq!(
            resolve("Seite {seite} von {seiten}", &value),
            ["Seite 2 von 12"]
        );
        assert_eq!(
            resolve("{dokumentart}\n{kopftext}", &value),
            ["Angebotsaufforderung"]
        );
        assert_eq!(resolve("Vorabzug", &value), ["Vorabzug"]);
        assert_eq!(resolve("{Projekt}", &value), ["{Projekt}"]);
        assert!(resolve("Nr. {fusstext}", &value).is_empty());
    }
}
