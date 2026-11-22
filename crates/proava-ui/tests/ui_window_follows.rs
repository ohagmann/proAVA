//! Die Fenster „Eigenschaften“ (Doppelklick, Alt+Enter) und „Langtext“
//! (Umschalt+F2) folgen der Auswahl der Tabelle, wie im bisherigen
//! AVA-Programm (Wunsch 18.11.2026): Klick, Pfeile, Bild↑/↓, Pos1/Ende,
//! Suchtreffer, Mehrfachauswahl (aktive Zeile). Die Tabelle behält die
//! Tastatur, der gewählte Reiter bleibt, „Festhalten“ hält die Zeile fest.
//! Eine offene Eingabe wird beim Wechsel übernommen, Strg+Z nimmt sie zurück.

mod common;

use common::*;
use egui::accesskit::Role;
use proava_domain::{
    Decimal, Lv, LvGroup, LvRow, Note, OzMask, PerformanceDescription, Position, Project,
    ProjectName, RichText, RowId,
};

struct Rows {
    title: RowId,
    note: RowId,
    first: RowId,
    second: RowId,
    performance: RowId,
    third: RowId,
    spacer: RowId,
}

/// Width of the test screen: the table in the middle stays free of the
/// windows (embedded here; as windows of their own they lie beside it).
const WIDTH: f32 = 2600.0;

fn open(folder: &std::path::Path) -> (UiHarness, Rows) {
    // Without the edit panel: only the windows show the fields. The window
    // „Eigenschaften“ on the right, „Langtext“ comes at the top left.
    std::fs::write(
        folder.join("settings.json"),
        r#"{"preferences":{"detail_visible":false},"position_window":[1000,950],"position_window_at":[1560,40]}"#,
    )
    .unwrap();
    let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
    let title = lv.push_group(LvGroup::new(0, "01", "Automation"));
    lv.push_group(LvGroup::new(1, "01.01", "Stationen"));
    let note = lv.push_note(Note::new("Alle Geräte mit Prüfprotokoll."));
    let first = lv.push_position(Position::new(
        "01.01.0010",
        "DDC-Station",
        Decimal::from(2),
        "St",
        None,
    ));
    let second = lv.push_position(Position::new(
        "01.01.0020",
        "Fühler",
        Decimal::from(24),
        "St",
        None,
    ));
    let performance = lv.push_row(LvRow::Performance(PerformanceDescription::new(
        "1",
        "Kabeltrassen",
        RichText::plain("Verzinkt."),
    )));
    let third = lv.push_position(Position::new(
        "01.01.0030",
        "Schaltschrank",
        Decimal::ONE,
        "St",
        None,
    ));
    let spacer = lv.push_spacer();
    let project = Project::new(ProjectName::parse("Folgen").unwrap(), "2026-11-18", lv);
    let file = save(folder, &project);
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(folder), Some(proava_ui::StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(WIDTH, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    (
        harness,
        Rows {
            title,
            note,
            first,
            second,
            performance,
            third,
            spacer,
        },
    )
}

fn window_row(harness: &UiHarness) -> Option<RowId> {
    harness.state().workspace().unwrap().position_window_row()
}

fn long_text_row(harness: &UiHarness) -> Option<RowId> {
    harness.state().workspace().unwrap().long_text_window_row()
}

fn selected_id(harness: &UiHarness) -> RowId {
    project(harness).lv().rows()[selected(harness).unwrap()].id()
}

fn select(harness: &mut UiHarness, row: RowId) {
    press(harness, Key::Home);
    for _ in 0..20 {
        if selected_id(harness) == row {
            return;
        }
        press(harness, Key::ArrowDown);
    }
    panic!("Zeile nicht erreicht");
}

/// Click into the table row `row` (on its short text).
fn click_row(harness: &mut UiHarness, row: RowId) {
    let lv = project(harness).lv();
    let index = lv.index_of(row).unwrap();
    let row = &lv.rows()[index];
    let label = format!("{} {}", row.ordinal(), row.short_text());
    let at = harness
        .query_all_by(|node| {
            node.role() == Role::Row
                && node
                    .label()
                    .is_some_and(|text| text.trim() == label.trim() || text.contains(&label))
        })
        .next()
        .unwrap_or_else(|| panic!("Tabellenzeile „{label}“"))
        .rect()
        .center();
    assert!(at.x < 1500.0, "Zeile frei von den Fenstern: {at:?}");
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

fn double_click_row(harness: &mut UiHarness, row: RowId) {
    click_row(harness, row);
    click_row(harness, row);
}

fn short_text(harness: &UiHarness, row: RowId) -> String {
    project(harness)
        .lv()
        .rows()
        .iter()
        .find(|other| other.id() == row)
        .unwrap()
        .short_text()
        .to_string()
}

/// Click into the text field showing `value`.
fn click_field(harness: &mut UiHarness, value: &str) {
    harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.value().as_deref() == Some(value)
        })
        .next()
        .unwrap_or_else(|| panic!("Feld mit „{value}“"))
        .click();
    settle(harness);
}

#[test]
fn the_window_follows_clicks_keys_and_search_hits_and_the_table_keeps_the_keyboard() {
    let folder = TempDir::new("folgen");
    let (mut harness, rows) = open(&folder.0);
    double_click_row(&mut harness, rows.first);
    assert_eq!(window_row(&harness), Some(rows.first), "Doppelklick öffnet");

    click_row(&mut harness, rows.second);
    assert_eq!(window_row(&harness), Some(rows.second), "Klick wechselt");
    assert!(
        !harness.ctx.text_edit_focused(),
        "kein Feld des Fensters hat die Schreibmarke"
    );
    // The table still has the keyboard: the arrow goes on.
    press(&mut harness, Key::ArrowDown);
    assert_eq!(selected_id(&harness), rows.performance);
    assert_eq!(
        window_row(&harness),
        Some(rows.performance),
        "Pfeil wechselt"
    );
    press(&mut harness, Key::ArrowDown);
    assert_eq!(window_row(&harness), Some(rows.third));
    press(&mut harness, Key::Home);
    assert_eq!(window_row(&harness), Some(rows.title), "Pos1: Titel");
    press(&mut harness, Key::End);
    assert_eq!(
        window_row(&harness),
        Some(rows.spacer),
        "Ende: Abstandshalter"
    );
    press(&mut harness, Key::PageUp);
    assert_eq!(window_row(&harness), Some(selected_id(&harness)), "Bild↑");

    // A search hit (Strg+F, then F3).
    press_with(&mut harness, Modifiers::COMMAND, Key::F);
    type_text(&mut harness, "Fühler");
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::Escape);
    assert_eq!(selected_id(&harness), rows.second, "Suchtreffer gewählt");
    assert_eq!(
        window_row(&harness),
        Some(rows.second),
        "Suchtreffer im Fenster"
    );

    // Several rows: the window shows the active one.
    select(&mut harness, rows.first);
    press_with(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    assert_eq!(selected_id(&harness), rows.second);
    assert_eq!(
        window_row(&harness),
        Some(rows.second),
        "aktive Zeile der Mehrfachauswahl"
    );
}

#[test]
fn titles_hints_and_performance_descriptions_are_shown_too() {
    let folder = TempDir::new("folgen-arten");
    let (mut harness, rows) = open(&folder.0);
    double_click_row(&mut harness, rows.first);
    assert_eq!(window_row(&harness), Some(rows.first));
    for (row, word) in [
        (rows.title, "Titel"),
        (rows.note, "Hinweis"),
        (rows.performance, "Ausführungsbeschreibung"),
    ] {
        click_row(&mut harness, row);
        assert_eq!(window_row(&harness), Some(row), "{word}");
        assert!(shows_part(&harness, word), "Fenster zeigt {word}");
    }
    press(&mut harness, Key::End);
    assert_eq!(window_row(&harness), Some(rows.spacer), "Abstandshalter");
    assert!(shows_part(&harness, "Leerzeile nur für die Bearbeitung"));
}

#[test]
fn the_chosen_tab_comes_back_after_a_title() {
    let folder = TempDir::new("folgen-reiter");
    let (mut harness, rows) = open(&folder.0);
    select(&mut harness, rows.first);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    click(&mut harness, "Weiteres");
    assert!(shows(&harness, "Positionsart"), "Reiter Weiteres");
    select(&mut harness, rows.title);
    assert_eq!(window_row(&harness), Some(rows.title));
    select(&mut harness, rows.second);
    assert_eq!(window_row(&harness), Some(rows.second));
    assert!(shows(&harness, "Positionsart"), "wieder Reiter Weiteres");
}

#[test]
fn held_windows_stay_for_comparing() {
    let folder = TempDir::new("festhalten");
    let (mut harness, rows) = open(&folder.0);
    select(&mut harness, rows.first);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    click(&mut harness, "Festhalten");
    select(&mut harness, rows.third);
    assert_eq!(window_row(&harness), Some(rows.first), "festgehalten");
    assert_eq!(selected_id(&harness), rows.third, "Tabelle frei");
    click(&mut harness, "Festhalten");
    settle(&mut harness);
    assert_eq!(
        window_row(&harness),
        Some(rows.third),
        "losgelassen: folgt wieder"
    );
}

#[test]
fn an_open_entry_is_taken_over_and_undone_with_strg_z() {
    let folder = TempDir::new("folgen-eingabe");
    let (mut harness, rows) = open(&folder.0);
    double_click_row(&mut harness, rows.first);
    click_field(&mut harness, "DDC-Station");
    press(&mut harness, Key::End);
    type_text(&mut harness, " neu");
    click_row(&mut harness, rows.second);
    assert_eq!(window_row(&harness), Some(rows.second));
    assert_eq!(
        short_text(&harness, rows.first),
        "DDC-Station neu",
        "übernommen"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(short_text(&harness, rows.first), "DDC-Station", "Strg+Z");

    // A wrong quantity is not taken (as with „Nächste Position“) and the
    // window goes on.
    select(&mut harness, rows.first);
    click_field(&mut harness, "2");
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "abc");
    click_row(&mut harness, rows.third);
    assert_eq!(window_row(&harness), Some(rows.third));
    let quantity = project(&harness)
        .lv()
        .position_by_id(rows.first)
        .unwrap()
        .quantity();
    assert_eq!(quantity, Decimal::from(2), "Menge unverändert");
}

#[test]
fn the_long_text_window_follows_too_and_stays_at_rows_without_long_text() {
    let folder = TempDir::new("folgen-langtext");
    let (mut harness, rows) = open(&folder.0);
    select(&mut harness, rows.first);
    press_with(&mut harness, Modifiers::SHIFT, Key::F2);
    assert_eq!(long_text_row(&harness), Some(rows.first));
    // The table keeps the keyboard after a click.
    click_row(&mut harness, rows.second);
    assert_eq!(long_text_row(&harness), Some(rows.second), "Klick wechselt");
    press(&mut harness, Key::ArrowDown);
    assert_eq!(
        long_text_row(&harness),
        Some(rows.performance),
        "AB hat Langtext"
    );
    // A title has no long text: the window stays, with a note.
    select(&mut harness, rows.title);
    assert_eq!(long_text_row(&harness), Some(rows.performance), "bleibt");
    assert!(
        shows_part(
            &harness,
            "Titel 01 hat keinen Langtext – das Fenster bleibt bei A0001."
        ),
        "Hinweis"
    );
    select(&mut harness, rows.note);
    assert_eq!(
        long_text_row(&harness),
        Some(rows.note),
        "Hinweis hat Langtext"
    );
    // Festhalten.
    click(&mut harness, "Festhalten");
    select(&mut harness, rows.third);
    assert_eq!(long_text_row(&harness), Some(rows.note), "festgehalten");
}
