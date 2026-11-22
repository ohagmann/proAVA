//! Titelzeilen (Anwender 18.11.): hell gelb hinterlegt – Titel wie
//! „Hellgelb 3“, Untertitel eine Stufe heller („Hellgelb 4“); dunkel ohne
//! eigene Fläche, nur OZ und Titeltext hellgelb („gelbe Schrift“, 22:29).
//! Positionen nicht. Abschaltbar unter Extras ▸ Einstellungen ▸ Darstellung ▸
//! „Titelzeilen farbig hinterlegen“. Geprüft an den tatsächlich gezeichneten
//! Flächen und Texten.
mod common;

use common::*;
use egui::accesskit::Role;
use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Rect};
use proava_application::ThemeChoice;
use proava_ui::theme::Palette;
use proava_ui::StartFile;

/// Fills of all rectangles painted in the last frame.
fn painted_fills(harness: &UiHarness) -> Vec<(Rect, Color32)> {
    fn collect(shape: &Shape, out: &mut Vec<(Rect, Color32)>) {
        match shape {
            Shape::Rect(rect) => out.push((rect.rect, rect.fill)),
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        collect(shape, &mut out);
    }
    out
}

/// Colours of the painted texts that read exactly `text`.
fn text_colors(harness: &UiHarness, text: &str) -> Vec<Color32> {
    fn collect(shape: &Shape, text: &str, out: &mut Vec<Color32>) {
        match shape {
            Shape::Text(shape) if shape.galley.job.text == text => {
                let color = shape.override_text_color.unwrap_or_else(|| {
                    shape
                        .galley
                        .job
                        .sections
                        .first()
                        .map(|section| section.format.color)
                        .unwrap_or(shape.fallback_color)
                });
                out.push(color);
            }
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, text, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        collect(shape, text, &mut out);
    }
    out
}

/// The background painted under the table row with this OZ.
fn row_fill(harness: &UiHarness, ordinal: &str) -> Color32 {
    let row = harness
        .query_all_by(|node| {
            node.role() == Role::Row
                && node
                    .label()
                    .is_some_and(|label| label.starts_with(&format!("{ordinal} ")))
        })
        .next()
        .unwrap_or_else(|| panic!("Zeile {ordinal}"))
        .rect();
    painted_fills(harness)
        .into_iter()
        .rev()
        .find(|(rect, _)| {
            (rect.min - row.min).length() < 0.6 && (rect.max - row.max).length() < 0.6
        })
        .map(|(_, fill)| fill)
        .unwrap_or_else(|| panic!("keine Fläche unter Zeile {ordinal}"))
}

fn open(folder: &std::path::Path, settings: &str) -> UiHarness {
    std::fs::write(folder.join("settings.json"), settings).unwrap();
    let file = save(folder, &proava_ui::demo::demo_project());
    let mut harness = harness(folder, Some(StartFile::Project(file)));
    // A position selected: the titles show their own fill.
    select_first_position(&mut harness);
    // The pointer away from the table (no hover).
    harness.event(egui::Event::PointerGone);
    settle(&mut harness);
    harness
}

#[test]
fn title_rows_are_yellow_and_positions_are_not() {
    let folder = TempDir::new("titelgelb-hell");
    let mut harness = open(&folder.0, r#"{"theme":"light"}"#);
    let light = Palette::of(ThemeChoice::Light);
    let rows = light.title_rows.unwrap().fills.unwrap();
    assert_eq!(row_fill(&harness, "01"), rows.title, "Titel: Hellgelb 3");
    assert_eq!(
        row_fill(&harness, "01.01"),
        rows.subtitle,
        "Untertitel: Hellgelb 4"
    );
    let position = selected(&harness).unwrap();
    let below = project(&harness).lv().rows()[position + 1]
        .ordinal()
        .to_string();
    assert_eq!(row_fill(&harness, &below), light.panel, "Position: weiß");

    // A folded title keeps its yellow.
    press(&mut harness, Key::Home);
    // ← walks the cells first (Kurztext → OZ), then folds the title.
    for _ in 0..3 {
        press(&mut harness, Key::ArrowLeft);
    }
    press(&mut harness, Key::ArrowDown);
    harness.event(egui::Event::PointerGone);
    settle(&mut harness);
    assert!(
        harness
            .query_all_by(|node| node.role() == Role::Row
                && node
                    .label()
                    .is_some_and(|label| label.starts_with("01.01 ")))
            .next()
            .is_none(),
        "Titel 01 eingeklappt"
    );
    assert_eq!(row_fill(&harness, "01"), rows.title, "eingeklappt gelb");

    // The selection stays visible on a title.
    press(&mut harness, Key::Home);
    assert_eq!(
        row_fill(&harness, "01"),
        light.selected,
        "Auswahl auf dem Titel"
    );

    // The Preisspiegel draws its title rows the same.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num3);
    harness.event(egui::Event::PointerGone);
    settle(&mut harness);
    let fills = painted_fills(&harness);
    assert!(
        fills.iter().any(|(_, fill)| *fill == rows.title),
        "Preisspiegel: Titel gelb"
    );
}

#[test]
fn switched_off_the_title_rows_keep_the_surface() {
    let folder = TempDir::new("titelgelb-aus");
    let mut harness = open(
        &folder.0,
        r#"{"theme":"light","preferences":{"title_rows_filled":false}}"#,
    );
    let light = Palette::of(ThemeChoice::Light);
    let rows = light.title_rows.unwrap().fills.unwrap();
    assert_eq!(row_fill(&harness, "01"), light.panel, "Titel ohne Gelb");
    assert_eq!(row_fill(&harness, "01.01"), light.panel);
    press_with(&mut harness, Modifiers::COMMAND, Key::Num3);
    settle(&mut harness);
    assert!(
        !painted_fills(&harness)
            .iter()
            .any(|(_, fill)| *fill == rows.title || *fill == rows.subtitle),
        "Preisspiegel ohne Gelb"
    );
}

#[test]
fn dark_title_rows_have_yellow_text_and_no_fill_of_their_own() {
    let folder = TempDir::new("titelgelb-dunkel");
    let mut harness = open(&folder.0, r#"{"theme":"dark"}"#);
    let dark = Palette::of(ThemeChoice::Dark);
    let rows = dark.title_rows.unwrap();
    // No band: titles on the surface of every row.
    assert_eq!(row_fill(&harness, "01"), dark.panel, "Titel ohne Fläche");
    assert_eq!(
        row_fill(&harness, "01.01"),
        dark.panel,
        "Untertitel ohne Fläche"
    );
    let position = selected(&harness).unwrap();
    let below = project(&harness).lv().rows()[position + 1]
        .ordinal()
        .to_string();
    assert_eq!(row_fill(&harness, &below), dark.panel);
    // OZ and title text yellow, the Untertitel paler.
    assert!(
        text_colors(&harness, "01").contains(&rows.title_text),
        "OZ Titel"
    );
    assert!(
        text_colors(&harness, "Automationsstationen").contains(&rows.title_text),
        "Titeltext"
    );
    assert!(
        text_colors(&harness, "01.01").contains(&rows.subtitle_text),
        "OZ Untertitel"
    );
    assert!(
        text_colors(&harness, "ASP01 Heizzentrale").contains(&rows.subtitle_text),
        "Untertiteltext"
    );
    // Selected: the selection of every row, the text stays yellow.
    press(&mut harness, Key::Home);
    harness.event(egui::Event::PointerGone);
    settle(&mut harness);
    assert_eq!(
        row_fill(&harness, "01"),
        dark.selected,
        "Auswahl wie bei Positionen"
    );
    assert!(
        text_colors(&harness, "01").contains(&rows.title_text),
        "gelb auf der Auswahl"
    );
    // Preisspiegel: the usual title fill, yellow title text.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num3);
    harness.event(egui::Event::PointerGone);
    settle(&mut harness);
    let fills = painted_fills(&harness);
    assert!(
        fills.iter().any(|(_, fill)| *fill == dark.accent_soft),
        "Titelfläche wie bisher"
    );
    assert!(
        text_colors(&harness, "Automationsstationen").contains(&rows.title_text),
        "Preisspiegel: Titeltext gelb"
    );
}

#[test]
fn dark_and_switched_off_the_titles_keep_their_usual_colour() {
    let folder = TempDir::new("titelgelb-dunkel-aus");
    let harness = open(
        &folder.0,
        r#"{"theme":"dark","preferences":{"title_rows_filled":false}}"#,
    );
    let dark = Palette::of(ThemeChoice::Dark);
    let rows = dark.title_rows.unwrap();
    assert_eq!(row_fill(&harness, "01"), dark.panel);
    let colors = text_colors(&harness, "Automationsstationen");
    assert!(
        colors.contains(&dark.group_text),
        "übliche Titelfarbe: {colors:?}"
    );
    assert!(!colors.contains(&rows.title_text), "keine gelbe Schrift");
}
