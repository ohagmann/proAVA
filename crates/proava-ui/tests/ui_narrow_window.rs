//! Schmale und niedrige Fenster (Wunsch 18.11.2026): proAVA lässt sich mit
//! Super+Pfeil auf eine Bildschirmhälfte legen (1920er Bildschirm: etwa
//! 950 × 520), die Mindestgröße liegt darunter. Kopfleiste (Zurück, Name,
//! LV-Auswahl, alle Reiter, Knöpfe), Menüleiste und Statusleiste bleiben
//! vollständig und überdecken sich nicht; das Eigenschaftenfenster wird
//! schmaler und lässt der Tabelle Platz.

mod common;

use common::*;
use egui::accesskit::Role;
use egui::Rect;
use egui_kittest::kittest::NodeT;
use egui_kittest::Harness;
use proava_domain::ProjectName;
use proava_ui::{App, StartFile};

const NAME: &str = "Neubau Sporthalle Süd";

/// The twelve menus of the menu bar.
const MENUS: &[&str] = &[
    "Projekt",
    "Bearbeiten",
    "Einfügen",
    "LV",
    "Prüfen",
    "GAEB",
    "Angebote",
    "Kosten",
    "Drucken",
    "Ansicht",
    "Extras",
    "Hilfe",
];

fn workspace(folder: &std::path::Path, width: f32, height: f32) -> UiHarness {
    workspace_named(folder, width, height, NAME)
}

fn workspace_named(folder: &std::path::Path, width: f32, height: f32, name: &str) -> UiHarness {
    let project = proava_ui::demo::demo_project().with_name(ProjectName::parse(name).unwrap());
    let file = save(folder, &project);
    let app = App::new(launcher(folder), Some(StartFile::Project(file)));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(width, height))
        .with_context_setup(App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run_steps(8);
    assert!(harness.state().project().is_some(), "Projekt offen");
    harness
}

/// Buttons, tabs and texts above `bottom` with their text.
fn parts_above(harness: &UiHarness, bottom: f32) -> Vec<(Rect, Role, String)> {
    harness
        .query_all_by(|node| matches!(node.role(), Role::Button | Role::Tab | Role::Label))
        .filter_map(|node| {
            let rect = node.rect();
            let text = node
                .accesskit_node()
                .label()
                .or_else(|| node.value())
                .unwrap_or_default();
            (rect.center().y < bottom && rect.width() > 1.0)
                .then(|| (rect, node.accesskit_node().role(), text))
        })
        .collect()
}

#[test]
fn header_and_menus_fit_narrow_windows() {
    for (width, height) in [
        (640.0, 420.0),
        (800.0, 500.0),
        (950.0, 520.0),
        (1100.0, 800.0),
        (1400.0, 800.0),
        (1950.0, 800.0),
    ] {
        let folder = TempDir::new("schmal");
        let harness = workspace(&folder.0, width, height);
        // The header ends under the menu row.
        let menu_row = harness
            .query_all_by(|node| {
                node.role() == Role::Button && node.label().as_deref() == Some("Projekt")
            })
            .next()
            .unwrap_or_else(|| panic!("{width}: Menü „Projekt“ fehlt"))
            .rect();
        let bottom = menu_row.bottom() + 4.0;
        let parts = parts_above(&harness, bottom);
        let has = |text: &str| parts.iter().any(|(_, _, part)| part == text);
        let tab = |text: &str| {
            parts
                .iter()
                .any(|(_, role, part)| *role == Role::Tab && part.starts_with(text))
        };
        for wanted in ["Start", "Einstellungen", NAME] {
            assert!(has(wanted), "{width}: „{wanted}“ fehlt: {parts:?}");
        }
        for wanted in ["LV", "Angebote", "Preisspiegel", "Abrechnung", "Nachträge"] {
            assert!(tab(wanted), "{width}: Reiter „{wanted}“ fehlt: {parts:?}");
        }
        // Every menu is shown or sits under „Weitere“ (the first ones stay).
        let shown = MENUS.iter().take_while(|name| has(name)).count();
        assert!(shown >= 6, "{width}: nur {shown} Menüs");
        if shown < MENUS.len() {
            assert!(has("Weitere"), "{width}: Menü „Weitere“ fehlt");
        } else {
            assert!(!has("Weitere"), "{width}: „Weitere“ ohne Not");
        }
        for (index, (rect, _, text)) in parts.iter().enumerate() {
            assert!(
                rect.right() <= width + 0.5,
                "{width}: „{text}“ {rect:?} ragt aus dem Fenster"
            );
            for (other, _, other_text) in parts.iter().skip(index + 1) {
                assert!(
                    !rect.shrink(0.5).intersects(other.shrink(0.5)),
                    "{width}: „{text}“ {rect:?} überdeckt „{other_text}“ {other:?}"
                );
            }
        }
        // Wide windows keep the header in one row with everything spelled out.
        if width >= 1950.0 {
            assert!(has("Befehl …"), "{width}");
            assert!(has("Speichern"), "{width}");
            assert!(tab("Angebote ("), "{width}");
            let lv = parts
                .iter()
                .find(|(_, role, text)| *role == Role::Tab && text == "LV");
            assert!(
                lv.unwrap().0.center().y < 40.0,
                "{width}: Reiter in der ersten Zeile"
            );
        }
    }
}

#[test]
fn the_properties_panel_leaves_the_table_room_in_half_a_screen() {
    let folder = TempDir::new("schmal-panel");
    let mut harness = workspace(&folder.0, 950.0, 520.0);
    press(&mut harness, Key::Home);
    press(&mut harness, Key::ArrowDown);
    // The Kurztext field spans the panel.
    let field = harness
        .query_all_by(|node| node.role() == Role::TextInput)
        .map(|node| node.rect())
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .expect("Feld im Eigenschaftenfenster");
    assert!(
        field.right() < 950.0 * 0.4,
        "Eigenschaftenfenster schmaler: Feld endet bei {}",
        field.right()
    );
    // A wider window brings the chosen width back.
    harness.set_size(egui::vec2(1600.0, 900.0));
    harness.run_steps(6);
    let wide = harness
        .query_all_by(|node| node.role() == Role::TextInput)
        .map(|node| node.rect())
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .unwrap();
    assert!(
        wide.right() > 380.0,
        "wieder breit: Feld endet bei {}",
        wide.right()
    );
    // Very narrow (a quarter screen at 150 %): the table gets the whole width,
    // the setting stays; wider again, the panel is back.
    harness.set_size(egui::vec2(700.0, 420.0));
    harness.run_steps(6);
    assert!(
        !shows(&harness, "Ordnungszahl"),
        "Eigenschaftenfenster bei 700 px ausgeblendet"
    );
    assert!(
        harness.state().workspace().unwrap().detail_visible(),
        "Einstellung bleibt"
    );
    harness.set_size(egui::vec2(950.0, 520.0));
    harness.run_steps(6);
    assert!(shows(&harness, "Ordnungszahl"), "bei 950 px wieder da");
}

#[test]
fn the_status_bar_keeps_its_parts_apart() {
    for (width, height) in [(640.0, 420.0), (950.0, 520.0)] {
        let folder = TempDir::new("schmal-status");
        let mut harness = workspace(&folder.0, width, height);
        press(&mut harness, Key::Home);
        press(&mut harness, Key::ArrowDown);
        press(&mut harness, Key::ArrowDown);
        // A message and the clipboard note.
        harness.event(egui::Event::Copy);
        settle(&mut harness);
        // The status bar's own texts (fields of the scrolled panel behind
        // it count as hidden, accesskit has no clip).
        let top = height - 30.0;
        let parts: Vec<(Rect, String)> = parts_above(&harness, f32::INFINITY)
            .into_iter()
            .filter(|(rect, _, text)| {
                rect.top() >= top
                    && (text.starts_with("Position ")
                        || text.starts_with("Zwischenablage:")
                        || text.ends_with("kopiert.")
                        || text == "gespeichert")
            })
            .map(|(rect, _, text)| (rect, text))
            .collect();
        for wanted in ["Position ", "1 Zeile kopiert.", "gespeichert"] {
            assert!(
                parts.iter().any(|(_, text)| text.starts_with(wanted)),
                "{width}: „{wanted}“ fehlt in der Statusleiste: {parts:?}"
            );
        }
        for (index, (rect, text)) in parts.iter().enumerate() {
            assert!(rect.right() <= width + 0.5, "{width}: „{text}“ ragt hinaus");
            for (other, other_text) in parts.iter().skip(index + 1) {
                assert!(
                    !rect.shrink(0.5).intersects(other.shrink(0.5)),
                    "{width}: „{text}“ {rect:?} überdeckt „{other_text}“ {other:?}"
                );
            }
        }
    }
}

#[test]
fn menus_without_room_sit_under_more() {
    let folder = TempDir::new("schmal-weitere");
    let mut harness = workspace(&folder.0, 640.0, 420.0);
    assert!(
        !shows(&harness, "Hilfe"),
        "„Hilfe“ hat bei 640 px keinen Platz"
    );
    click(&mut harness, "Weitere");
    assert!(
        shows(&harness, "Hilfe"),
        "„Weitere“ zeigt die übrigen Menüs"
    );
    click(&mut harness, "Hilfe");
    assert!(
        shows(&harness, "Über proAVA …"),
        "das Untermenü hat seine Einträge"
    );
}

/// Anwender 18.11. 16:00: the Kurztext keeps about 140 px; in half a screen
/// (table about 575 px) the OZ column shrinks to the longest OZ and the
/// Einheit gives way – Menge, EP and GP stay.
#[test]
fn the_short_text_keeps_140_px_in_half_a_screen() {
    use proava_ui::columns::MIN_TEXT_HARD;
    let folder = TempDir::new("schmal-kurztext");
    let mut harness = workspace(&folder.0, 950.0, 520.0);
    press(&mut harness, Key::Home);
    press(&mut harness, Key::ArrowDown);
    let head = harness
        .get_by_label(proava_ui::lv_editor::HEADER_NAME)
        .rect();
    let columns = harness
        .state()
        .workspace()
        .unwrap()
        .table_layout()
        .expect("Tabelle gezeichnet");
    let text = columns.text_right - columns.text_x;
    assert!(
        (540.0..=680.0).contains(&head.width()),
        "Tabelle bei halber Bildschirmbreite: {} px",
        head.width()
    );
    assert!(text >= MIN_TEXT_HARD, "Kurztext {text} px: {columns:?}");
    assert!(
        columns.show_ep && columns.show_gp,
        "Menge, EP und GP bleiben: {columns:?}"
    );
    assert!(
        columns.status_x - columns.left < 122.0,
        "OZ-Spalte auf die längste OZ verkleinert: {columns:?}"
    );
    // Narrower still (table about 450 px): the Kurztext keeps its 140 px,
    // the prices give way, the Menge stays.
    harness.set_size(egui::vec2(760.0, 520.0));
    harness.run_steps(6);
    let columns = harness.state().workspace().unwrap().table_layout().unwrap();
    assert!(
        columns.text_right - columns.text_x >= MIN_TEXT_HARD,
        "{columns:?}"
    );
    assert!(
        columns.qty_right > columns.qty_left,
        "Menge bleibt: {columns:?}"
    );
}

/// Free room left of the buttons on the right when the name is shortened:
/// the fixed 8 px plus less than one letter (the name is cut at a letter
/// and ends with „…“). Before the fix 40 px and more stayed free.
const GAP: f32 = 26.0;

/// With the tabs in their own row the project name gets the first row up
/// to the buttons on the right: shortened only when it reaches them
/// (Wunsch 18.11.2026: den freien Platz nutzen; no LV choice in the header
/// any more, the LVs stand one below the other in the table).
#[test]
fn the_name_uses_the_first_row() {
    for name in [
        NAME,
        "Neubau Sporthalle Süd Bauteil B mit Tiefgarage und Außenanlagen",
    ] {
        let folder = TempDir::new("schmal-name");
        let wide = workspace_named(&folder.0, 2600.0, 800.0, name);
        let natural = parts_above(&wide, 40.0)
            .into_iter()
            .find(|(_, role, text)| *role == Role::Label && text == name)
            .expect("Name")
            .0
            .width();
        drop(wide);
        for width in [560.0, 640.0, 800.0] {
            let folder = TempDir::new("schmal-name");
            let harness = workspace_named(&folder.0, width, 520.0, name);
            let first_row = parts_above(&harness, 40.0);
            let find = |role: Role, text: &str| {
                first_row
                    .iter()
                    .find(|(_, part_role, part)| *part_role == role && part == text)
                    .map(|(rect, _, _)| *rect)
                    .unwrap_or_else(|| panic!("{width}: „{text}“ fehlt: {first_row:?}"))
            };
            let title = find(Role::Label, name);
            let search = find(Role::Button, "Befehl …");
            assert!(
                !first_row.iter().any(|(_, role, _)| *role == Role::Tab),
                "{width}: Reiter in der zweiten Zeile"
            );
            if title.width() < natural - 0.5 {
                let gap = search.left() - title.right();
                assert!(
                    gap <= GAP,
                    "{width}: Name gekürzt ({:.0} von {natural:.0} px), aber {gap:.0} px frei",
                    title.width()
                );
            }
        }
    }
}
