//! Schreibschutz für STLB-Bau-Texte (Anwender 18.11.): Positionsfenster und
//! Bearbeitungsbereich zeigen die Kennung mit Version, Langtext und Einheit
//! nehmen keine Eingabe an, Kurztext und die Werte der Textergänzungen
//! bleiben frei; „In freie Position umwandeln …“ fragt, entfernt die
//! Kennung und lässt sich mit Strg+Z zurücknehmen; die Prüfung bietet
//! „STLB-Bau*“ zum Umwandeln an. Alles mit erfundenen Daten.

mod common;

use common::*;
use proava_domain::stlb::{StlbCharacteristic, StlbReference};
use proava_domain::{ComplementKind, LvRow, Position, Project, RichText, RowId, TextComplement};
use proava_ui::StartFile;

const LONG: &str = "Wandschrank aus Stahlblech, einflügelig, Farbe";

fn reference() -> StlbReference {
    StlbReference {
        catalog: "STLB-Bau".into(),
        version: Some("2026-10".into()),
        work_category: "070".into(),
        group: 4711,
        cost_group: "482".into(),
        characteristics: vec![StlbCharacteristic {
            characteristic: 1,
            index: Some(0),
            value: Some(2),
            short_text_part: true,
        }],
        owner_complements: true,
        bidder_complements: false,
        text_fingerprint: None,
        legacy_fingerprint: None,
        complement_links: Vec::new(),
        complement_numbers: Vec::new(),
    }
}

/// Makes `position` a text from STLB-Bau (field „Farbe“ filled with „weiß“).
fn as_stlb(position: &mut Position) {
    position.set_long_text(RichText::plain(LONG));
    position.set_unit("St");
    position.set_complements(vec![TextComplement {
        marker: "1".into(),
        kind: ComplementKind::Owner,
        body: "weiß".into(),
        ..TextComplement::default()
    }]);
    position.attach_stlb(reference());
}

/// The demo project with its first `count` positions from STLB-Bau; their
/// ids. `changed`: the first one's Langtext changed afterwards (an old
/// project from before the lock, „STLB-Bau*“).
fn open_with_stlb(
    folder: &std::path::Path,
    count: usize,
    panel: bool,
    changed: bool,
) -> (UiHarness, Vec<RowId>) {
    if !panel {
        std::fs::write(
            folder.join("settings.json"),
            r#"{"preferences":{"detail_visible":false}}"#,
        )
        .unwrap();
    }
    let mut project: Project = proava_ui::demo::demo_project();
    let indices: Vec<usize> = project
        .lv()
        .rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.is_position())
        .map(|(index, _)| index)
        .take(count)
        .collect();
    let mut ids = Vec::new();
    for (number, &index) in indices.iter().enumerate() {
        if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(index) {
            as_stlb(position);
            if changed && number == 0 {
                position.set_long_text(RichText::plain("Eigener Text von früher"));
            }
            ids.push(position.id());
        }
    }
    let file = save(folder, &project);
    let harness = harness(folder, Some(StartFile::Project(file)));
    (harness, ids)
}

fn position(harness: &UiHarness, id: RowId) -> Position {
    project(harness)
        .lv()
        .position_by_id(id)
        .expect("Position")
        .clone()
}

fn row_label(harness: &UiHarness, id: RowId) -> String {
    let position = position(harness, id);
    format!("{} {}", position.ordinal(), position.short_text())
        .trim()
        .to_string()
}

/// Clicks into the running text of the Langtext editor (below its
/// toolbar), as the user would.
fn click_into_long_text(harness: &mut UiHarness) {
    let toolbar = harness
        .get_all_by_label("Tabelle einfügen …")
        .next()
        .expect("Werkzeugleiste des Langtexts")
        .rect();
    let at = egui::pos2(toolbar.left() + 40.0, toolbar.bottom() + 40.0);
    harness.event(egui::Event::PointerMoved(at));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(harness);
}

/// Clicks the text field showing `value`.
fn click_field(harness: &mut UiHarness, value: &str) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.value().as_deref() == Some(value)
        })
        .next()
        .unwrap_or_else(|| {
            let fields: Vec<String> = harness
                .query_all_by(|node| node.role() == egui::accesskit::Role::TextInput)
                .map(|node| format!("{:?} {:?}", node.value(), node.rect()))
                .collect();
            panic!("Feld mit „{value}“ fehlt: {fields:#?}")
        })
        .click();
    settle(harness);
}

/// Clicks the field of a Textergänzung value in the list „Werte der
/// Textergänzungen (frei)“ (the Langtext shows the same value, locked).
fn click_value_field(harness: &mut UiHarness, value: &str) {
    let below = harness
        .get_all_by_label("Werte der Textergänzungen (frei)")
        .next()
        .expect("Liste der Werte")
        .rect()
        .bottom();
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.value().as_deref() == Some(value)
        })
        .filter(|node| node.rect().top() >= below - 1.0)
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap_or_else(|| panic!("Wertfeld „{value}“ fehlt"))
        .click();
    settle(harness);
}

/// Replaces the content of the focused field.
fn retype(harness: &mut UiHarness, text: &str) {
    press_with(harness, Modifiers::COMMAND, Key::A);
    type_text(harness, text);
}

#[test]
fn window_shows_the_key_locks_long_text_and_unit_and_leaves_short_text_and_values_free() {
    let folder = TempDir::new("stlb-sperre-fenster");
    let (mut harness, ids) = open_with_stlb(&folder.0, 1, false, false);
    let id = ids[0];
    let label = row_label(&harness, id);
    double_click(&mut harness, &label);
    assert_eq!(
        harness.state().workspace().unwrap().position_window_row(),
        Some(id),
        "Positionsfenster offen"
    );
    // The key with its version, the protection and the way out.
    assert!(
        shows_part(
            &harness,
            "STLB-Bau 10/2026 · LB 070 · Leistung 4711 · KG 482"
        ),
        "Kennungszeile"
    );
    assert!(shows_part(&harness, "schreibgeschützt (Kurztext frei)"));
    assert!(shows(&harness, "In freie Position umwandeln …"));

    // Typing into the Langtext changes nothing; a click explains it.
    let before = position(&harness, id);
    click_into_long_text(&mut harness);
    type_text(&mut harness, "XYZ");
    assert_eq!(
        position(&harness, id).long_text(),
        before.long_text(),
        "Langtext unverändert"
    );
    assert!(!position(&harness, id).stlb_text_changed());
    assert_eq!(
        position(&harness, id).short_text(),
        before.short_text(),
        "nichts anderswo"
    );
    assert!(
        shows_part(&harness, "Bearbeiten › Umwandeln in"),
        "Klick erklärt die Sperre"
    );

    // The unit: shown, but locked (no enabled field to type into).
    click_field(&mut harness, "St");
    retype(&mut harness, "Stk");
    assert_eq!(position(&harness, id).unit(), "St", "Einheit gesperrt");

    // Kurztext: free, the key stays and the text stays unchanged.
    click_field(&mut harness, before.short_text());
    retype(&mut harness, "Wandschrank ASP01");
    press(&mut harness, Key::Tab);
    let now = position(&harness, id);
    assert_eq!(now.short_text(), "Wandschrank ASP01");
    assert!(now.is_text_locked(), "Kennung bleibt");
    assert!(!now.stlb_text_changed(), "kein STLB-Bau*");

    // Value of the Textergänzung: free.
    click_value_field(&mut harness, "weiß");
    retype(&mut harness, "anthrazit");
    press(&mut harness, Key::Tab);
    let now = position(&harness, id);
    assert_eq!(now.complements()[0].body, "anthrazit");
    assert_eq!(now.complements()[0].kind, ComplementKind::Owner);
    assert!(!now.stlb_text_changed(), "Werte ändern den Text nicht");
}

#[test]
fn edit_panel_shows_the_key_and_takes_no_typing_into_the_long_text() {
    let folder = TempDir::new("stlb-sperre-bereich");
    let (mut harness, ids) = open_with_stlb(&folder.0, 1, true, false);
    // Tall enough that the Langtext of the panel is in sight.
    harness.set_size(egui::vec2(1400.0, 1400.0));
    settle(&mut harness);
    let id = ids[0];
    let label = row_label(&harness, id);
    click(&mut harness, &label);
    assert!(shows_part(&harness, "STLB-Bau 10/2026"), "Kennungszeile");
    assert!(shows_part(&harness, "Werte der Textergänzungen (frei)"));
    let before = position(&harness, id);
    click_into_long_text(&mut harness);
    type_text(&mut harness, "XYZ");
    assert_eq!(position(&harness, id).long_text(), before.long_text());
    assert_eq!(
        position(&harness, id).short_text(),
        before.short_text(),
        "nichts anderswo"
    );
    click_value_field(&mut harness, "weiß");
    retype(&mut harness, "grau");
    press(&mut harness, Key::Tab);
    assert_eq!(position(&harness, id).complements()[0].body, "grau");
}

#[test]
fn converting_asks_removes_the_key_and_strg_z_brings_it_back() {
    let folder = TempDir::new("stlb-umwandeln");
    let (mut harness, ids) = open_with_stlb(&folder.0, 1, false, false);
    let id = ids[0];
    let label = row_label(&harness, id);
    double_click(&mut harness, &label);
    let before = position(&harness, id);

    click(&mut harness, "In freie Position umwandeln …");
    assert!(shows(&harness, "In freie Position umwandeln?"), "Rückfrage");
    assert!(shows_part(
        &harness,
        "Die STLB-Bau-Kennung wird entfernt, danach ist der Langtext frei änderbar."
    ));
    assert!(
        position(&harness, id).stlb_reference().is_some(),
        "erst nach dem Klick"
    );
    click(&mut harness, "Umwandeln");
    let free = position(&harness, id);
    assert!(free.stlb_reference().is_none(), "Kennung weg");
    assert!(!free.is_text_locked());
    assert_eq!(free.long_text(), before.long_text(), "Text bleibt");
    let note = &free.text_origin().expect("Textquelle bleibt").note;
    assert!(
        note.starts_with("in freie Position umgewandelt am "),
        "{note}"
    );
    assert!(note.contains("STLB-Bau 10/2026"), "{note}");

    // Now the Langtext takes input.
    click_into_long_text(&mut harness);
    press(&mut harness, Key::End);
    type_text(&mut harness, " XYZ");
    assert!(
        position(&harness, id)
            .long_text()
            .to_plain()
            .contains("XYZ"),
        "Langtext frei"
    );

    click(&mut harness, "Schließen (Esc)");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(
        shows_part(&harness, "STLB-Bau-Kennung"),
        "Meldung nennt die Kennung"
    );
    let back = position(&harness, id);
    assert_eq!(
        back.stlb_reference(),
        before.stlb_reference(),
        "Kennung zurück"
    );
    assert!(back.is_text_locked());
    assert_eq!(back.long_text(), before.long_text());
}

#[test]
fn right_click_converts_several_positions_in_one_undo_step() {
    let folder = TempDir::new("stlb-umwandeln-mehrere");
    let (mut harness, ids) = open_with_stlb(&folder.0, 2, true, false);
    let first = row_label(&harness, ids[0]);
    click(&mut harness, &first);
    press_with(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    harness
        .get_all_by_label(&first)
        .next()
        .expect("Tabellenzeile")
        .click_secondary();
    settle(&mut harness);
    open_submenu(&mut harness, "Bearbeiten");
    open_submenu(&mut harness, "Umwandeln in");
    click_menu(&mut harness, "Freie Position (ohne STLB-Bau-Kennung) …");
    assert!(shows(&harness, "In freie Position umwandeln?"));
    assert!(shows_part(&harness, "2 Positionen"), "beide");
    click(&mut harness, "Umwandeln");
    for &id in &ids {
        assert!(position(&harness, id).stlb_reference().is_none());
    }
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    for &id in &ids {
        assert!(
            position(&harness, id).is_text_locked(),
            "ein Schritt zurück"
        );
    }
}

#[test]
fn the_check_offers_to_convert_changed_stlb_texts() {
    let folder = TempDir::new("stlb-pruefung");
    let (mut harness, ids) = open_with_stlb(&folder.0, 2, true, true);
    assert!(position(&harness, ids[0]).stlb_text_changed(), "STLB-Bau*");
    menu_path(&mut harness, "Prüfen", "LV prüfen …");
    click(&mut harness, "STLB-Bau* in freie Positionen umwandeln (1)");
    assert!(
        position(&harness, ids[0]).stlb_reference().is_none(),
        "STLB-Bau* umgewandelt"
    );
    assert!(
        position(&harness, ids[1]).is_text_locked(),
        "unverändertes STLB-Bau bleibt"
    );
    assert!(!shows_part(
        &harness,
        "STLB-Bau* in freie Positionen umwandeln ("
    ));
    press(&mut harness, Key::Escape);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(position(&harness, ids[0]).stlb_reference().is_some());
}
