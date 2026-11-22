//! Lange und mehrzeilige Kurztexte (GAEB) bleiben in der LV-Tabelle auf einer
//! Zeile: gekürzt mit „…“, innerhalb ihres Zeilenrechtecks; wird das
//! Eigenschaftenfenster ausgeblendet, zeigt die Zeile mehr vom Text (Anwender
//! 18.11.). Geprüft an den tatsächlich gezeichneten Textformen.

mod common;

use common::*;
use egui::epaint::{ClippedShape, Shape};
use proava_domain::LvRow;
use proava_ui::StartFile;

const LINES: &str = "2-Wege-Umschaltventil (Auf / Zu) inkl. Stellantrieb\n- V1, V2 -\nMotorklappe Zuluft 1, Zuluft 2";
const LONG: &str = "Raumtemperaturfühler Unterputz mit Sollwertsteller und Präsenztaste, Kommunikation über \
                    Modbus RTU, Farbe reinweiß, Sockel für Hohlwanddose, Beschriftungsfeld, Montagezubehör";

/// Every text painted in the last frame: (shown text, rows, rect on screen).
fn painted_texts(harness: &UiHarness) -> Vec<(String, usize, egui::Rect)> {
    fn collect(shape: &Shape, out: &mut Vec<(String, usize, egui::Rect)>) {
        match shape {
            Shape::Text(text) => {
                let shown: String = text.galley.rows.iter().map(|row| row.text()).collect();
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                out.push((shown, text.galley.rows.len(), rect));
            }
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

/// The table row with this OZ (AccessKit name „OZ Kurztext …“) and its rect.
fn row_rect(harness: &UiHarness, ordinal: &str) -> egui::Rect {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Row
                && node
                    .label()
                    .is_some_and(|label| label.starts_with(&format!("{ordinal} ")))
        })
        .next()
        .unwrap_or_else(|| panic!("Zeile {ordinal} fehlt"))
        .rect()
}

/// The Kurztext painted in `row`: one line, inside the row, „…“ when cut.
fn shown_in_row(harness: &UiHarness, row: egui::Rect, start: &str) -> String {
    let found: Vec<_> = painted_texts(harness)
        .into_iter()
        .filter(|(text, _, rect)| text.starts_with(start) && row.intersects(*rect))
        .collect();
    assert_eq!(found.len(), 1, "genau ein Kurztext in der Zeile: {found:?}");
    let (text, rows, rect) = found.into_iter().next().unwrap();
    assert_eq!(rows, 1, "eine Zeile statt Umbruch: {text:?}");
    assert!(!text.contains('\n'), "kein Zeilenumbruch: {text:?}");
    assert!(
        rect.top() >= row.top() - 0.5 && rect.bottom() <= row.bottom() + 0.5,
        "Text {rect:?} ragt aus der Zeile {row:?}"
    );
    text
}

#[test]
fn long_short_texts_stay_on_one_line_and_grow_without_the_side_window() {
    let folder = TempDir::new("kurztext-zeile");
    let mut demo = proava_ui::demo::demo_project();
    let positions: Vec<usize> = demo
        .lv()
        .rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.is_position())
        .map(|(index, _)| index)
        .take(2)
        .collect();
    let mut ordinals = Vec::new();
    for (index, text) in positions.iter().zip([LINES, LONG]) {
        if let Some(LvRow::Position(position)) = demo.lv_mut().row_mut(*index) {
            position.set_short_text(text);
            ordinals.push(position.ordinal().to_string());
        }
    }
    let file = save(&folder.0, &demo);
    // Measured without the column „KG“ (on by default since 18.11.): the
    // Kurztext column must reach past the first line break.
    std::fs::write(
        folder.0.join("settings.json"),
        r#"{"table_columns":{"kg":false}}"#,
    )
    .unwrap();
    let mut harness = harness(&folder.0, Some(StartFile::Project(file)));
    settle(&mut harness);
    assert!(harness.state().project().is_some());

    // Several lines (GAEB): one line, the breaks as spaces, cut with „…“.
    let first = shown_in_row(
        &harness,
        row_rect(&harness, &ordinals[0]),
        "2-Wege-Umschaltventil",
    );
    assert!(
        first.starts_with("2-Wege-Umschaltventil (Auf / Zu) inkl. Stellantrieb - "),
        "{first}"
    );
    assert!(first.ends_with('…'), "gekürzt: {first}");
    // One long line: cut with „…“ at the column.
    let long = shown_in_row(
        &harness,
        row_rect(&harness, &ordinals[1]),
        "Raumtemperaturfühler",
    );
    assert!(long.ends_with('…'), "gekürzt: {long}");
    // The full text stays in the project.
    assert_eq!(
        project(&harness).lv().rows()[positions[0]].short_text(),
        LINES
    );

    // Without the Eigenschaftenfenster the column is wider: more text shows.
    press_with(&mut harness, Modifiers::COMMAND, Key::K);
    type_text(&mut harness, "Eigenschaftenfenster ein");
    press(&mut harness, Key::Enter);
    settle(&mut harness);
    let wider = shown_in_row(
        &harness,
        row_rect(&harness, &ordinals[1]),
        "Raumtemperaturfühler",
    );
    assert!(
        wider.chars().count() > long.chars().count(),
        "breiter ohne Eigenschaftenfenster: {long:?} → {wider:?}"
    );
    let all = shown_in_row(
        &harness,
        row_rect(&harness, &ordinals[0]),
        "2-Wege-Umschaltventil",
    );
    assert_eq!(all, "2-Wege-Umschaltventil (Auf / Zu) inkl. Stellantrieb - V1, V2 - Motorklappe Zuluft 1, Zuluft 2");
}
