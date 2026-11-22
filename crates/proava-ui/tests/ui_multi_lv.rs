//! Several LVs of a project one below the other in the LV table (no LV
//! switch in the header): head rows, the LV of the Schreibmarke, copying and
//! dragging between LVs, Strg+Z, search over all LVs, the LV choice of the
//! other views.

mod common;

use common::*;
use egui::accesskit::Role;
use proava_domain::{
    Decimal, Lv, LvGroup, LvRow, OzMask, PerformanceDescription, Position, Project, ProjectName,
    RichText,
};
use proava_ui::StartFile;

fn lv(number: &str, title: &str, mask: &str, rows: &[(&str, &str)]) -> Lv {
    let mut lv = Lv::new(title, OzMask::parse(mask).unwrap());
    lv.set_number(number);
    for level in 0..lv.level_count() {
        lv.push_group(LvGroup::new(
            level,
            "",
            if level == 0 { "Titel" } else { "Unter" },
        ));
    }
    for (short, long) in rows {
        let mut position = Position::new("", *short, Decimal::TWO, "St", Some(Decimal::TEN));
        position.set_long_text(RichText::plain(long));
        lv.push_position(position);
    }
    lv.renumber();
    lv
}

/// LV 1 (11.PPPP) with an Ausführungsbeschreibung the first position refers
/// to, LV 2 (11.22.PPPP, another mask) without one.
fn two_lvs() -> Project {
    let mut first = lv(
        "1",
        "Gebäudeautomation",
        "11.PPPP",
        &[
            ("DDC-Station", "Automationsstation"),
            ("E/A-Modul", "Erweiterung"),
        ],
    );
    let ab = first.push_row(LvRow::Performance(PerformanceDescription::new(
        "1",
        "Schaltschränke",
        RichText::plain("verzinkt"),
    )));
    let ddc = first.positions().next().unwrap().id();
    first.link_performance(ddc, Some(ab)).unwrap();
    let mut project = Project::new(ProjectName::parse("Zwei LV").unwrap(), "2026-11-18", first);
    project.add_lv(lv(
        "2",
        "Elektro",
        "11.22.PPPP",
        &[("Mantelleitung", "NYM-J 3x1,5")],
    ));
    project
}

fn open(folder: &std::path::Path, project: &Project) -> UiHarness {
    let file = save(folder, project);
    let harness = harness(folder, Some(StartFile::Project(file)));
    assert!(
        harness.state().project().is_some(),
        "Projekt nicht geöffnet"
    );
    harness
}

/// Head rows of the table (screen reader name „LV-Kopf …“).
fn heads(harness: &UiHarness) -> Vec<String> {
    let names = std::cell::RefCell::new(Vec::new());
    let _ = harness
        .query_all_by(|node| {
            if node.role() == Role::Row {
                if let Some(label) = node.label().filter(|label| label.starts_with("LV-Kopf ")) {
                    names.borrow_mut().push(label);
                }
            }
            false
        })
        .count();
    names.into_inner()
}

/// The table row with this name („OZ Kurztext“).
fn row_rect(harness: &UiHarness, name: &str) -> egui::Rect {
    harness
        .query_all_by(|node| node.role() == Role::Row && node.label().as_deref() == Some(name))
        .next()
        .unwrap_or_else(|| panic!("Zeile „{name}“"))
        .rect()
}

fn click_row(harness: &mut UiHarness, name: &str) {
    harness
        .query_all_by(|node| node.role() == Role::Row && node.label().as_deref() == Some(name))
        .next()
        .unwrap_or_else(|| panic!("Zeile „{name}“"))
        .click();
    settle(harness);
}

fn selected_id(harness: &UiHarness) -> Option<proava_domain::RowId> {
    selected(harness).map(|index| project(harness).lv().rows()[index].id())
}

fn short_texts(project: &Project, lv: usize) -> Vec<String> {
    project
        .lv_at(lv)
        .unwrap()
        .rows()
        .iter()
        .map(LvRow::short_text)
        .collect()
}

/// Press at `from`, move in steps to `to`, release (with `modifiers` held at the release).
fn drag(harness: &mut UiHarness, from: egui::Pos2, to: egui::Pos2, modifiers: Modifiers) {
    harness.event(egui::Event::PointerMoved(from));
    harness.step();
    harness.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    for step in 1..=10 {
        let t = step as f32 / 10.0;
        harness.event(egui::Event::PointerMoved(from + (to - from) * t));
        harness.step();
    }
    harness.event(egui::Event::ModifiersChanged(modifiers));
    harness.step();
    harness.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.step();
    harness.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    settle(harness);
}

#[test]
fn one_lv_has_no_head_row() {
    let folder = TempDir::new("mehrlv-eins");
    let mut harness = open_demo(&folder.0);
    settle(&mut harness);
    assert!(heads(&harness).is_empty(), "{:?}", heads(&harness));
}

#[test]
fn two_lvs_stand_one_below_the_other_and_a_click_makes_the_lv_active() {
    let folder = TempDir::new("mehrlv-zwei");
    // Wide like ui_window_follows, so the windows leave the rows free.
    let file = save(&folder.0, &two_lvs());
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.0.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(&folder.0), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(2600.0, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    assert_eq!(
        heads(&harness),
        ["LV-Kopf LV 1 – Gebäudeautomation", "LV-Kopf LV 2 – Elektro"]
    );
    // Both LVs' rows are in the table, LV 2 below LV 1.
    let ddc = row_rect(&harness, "01.0010 DDC-Station");
    let cable = row_rect(&harness, "01.01.0010 Mantelleitung");
    assert!(cable.top() > ddc.bottom());
    assert!(
        !shows_part(&harness, "Es gibt keine weiteren LVs"),
        "keine LV-Auswahl mehr"
    );
    assert_eq!(project(&harness).active_lv(), 0);

    click_row(&mut harness, "01.01.0010 Mantelleitung");
    assert_eq!(project(&harness).active_lv(), 1, "Klick macht LV 2 aktiv");
    assert_eq!(
        project(&harness)
            .lv()
            .position_by_id(selected_id(&harness).unwrap())
            .map(|p| p.short_text()),
        Some("Mantelleitung")
    );
    assert!(
        shows_part(&harness, "LV 2 – Elektro ·"),
        "Statusleiste nennt das LV"
    );

    // Arrow keys go on over the LV border.
    press(&mut harness, Key::Home);
    press(&mut harness, Key::ArrowUp);
    assert_eq!(
        project(&harness).active_lv(),
        0,
        "↑ am Anfang von LV 2 geht in LV 1"
    );
    let last = project(&harness).lv().rows().len() - 1;
    assert_eq!(selected(&harness), Some(last), "letzte Zeile von LV 1");
    press(&mut harness, Key::ArrowDown);
    assert_eq!(project(&harness).active_lv(), 1);

    // Folding an LV: only its head row stays.
    let head = row_rect(&harness, "LV-Kopf LV 2 – Elektro");
    let chevron = egui::pos2(head.left() + 21.0, head.center().y);
    harness.event(egui::Event::PointerMoved(chevron));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: chevron,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(&mut harness);
    assert!(
        !shows(&harness, "01.01.0010 Mantelleitung"),
        "LV 2 zugeklappt"
    );
}

#[test]
fn copy_in_lv_1_and_paste_in_lv_2_brings_everything_and_undo_takes_it_back() {
    let folder = TempDir::new("mehrlv-kopieren");
    // Wide like ui_window_follows, so the windows leave the rows free.
    let file = save(&folder.0, &two_lvs());
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.0.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(&folder.0), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(2600.0, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    let before = short_texts(project(&harness), 0);
    click_row(&mut harness, "01.0010 DDC-Station");
    harness.event(egui::Event::Copy);
    settle(&mut harness);

    click_row(&mut harness, "01.01.0010 Mantelleitung");
    assert_eq!(project(&harness).active_lv(), 1);
    let plain = proava_ui::system_clipboard::text().unwrap();
    harness.event(egui::Event::Paste(plain));
    settle(&mut harness);

    let target = project(&harness).lv_at(1).unwrap();
    let pasted = target
        .positions()
        .find(|position| position.short_text() == "DDC-Station")
        .expect("in LV 2 eingefügt");
    assert_eq!(pasted.ordinal(), "01.01.0020", "OZ nach der Maske von LV 2");
    assert_eq!(pasted.long_text().to_plain(), "Automationsstation");
    let ab = target.performance_of(pasted).expect("AB kommt mit");
    assert_eq!(ab.label(), "Schaltschränke");
    assert_eq!(
        short_texts(project(&harness), 0),
        before,
        "LV 1 unverändert"
    );

    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(
        !project(&harness)
            .lv_at(1)
            .unwrap()
            .positions()
            .any(|p| p.short_text() == "DDC-Station"),
        "Strg+Z nimmt das Einfügen in LV 2 zurück"
    );
    assert_eq!(project(&harness).lv_at(1).unwrap().performance_count(), 0);
    assert_eq!(project(&harness).lv_at(0).unwrap().position_count(), 2);
}

#[test]
fn dragging_into_another_lv_moves_with_ctrl_copies_and_undo_is_one_step() {
    let folder = TempDir::new("mehrlv-ziehen");
    let mut harness = open_wide(&folder.0, &two_lvs());
    click_row(&mut harness, "01.0020 E/A-Modul");
    let from = row_rect(&harness, "01.0020 E/A-Modul").center();
    let to = row_rect(&harness, "01.01.0010 Mantelleitung");
    drag(
        &mut harness,
        from,
        egui::pos2(to.center().x, to.bottom() - 4.0),
        Modifiers::NONE,
    );
    let after_move = project(&harness).clone();
    assert!(
        !after_move
            .lv_at(0)
            .unwrap()
            .positions()
            .any(|p| p.short_text() == "E/A-Modul"),
        "verschoben: aus LV 1 weg"
    );
    let moved = after_move
        .lv_at(1)
        .unwrap()
        .positions()
        .find(|p| p.short_text() == "E/A-Modul")
        .expect("in LV 2");
    assert_eq!(moved.ordinal(), "01.01.0020");
    assert_eq!(after_move.active_lv(), 1, "Ziel-LV ist aktiv");

    // One Strg+Z: both LVs as before.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    let back = project(&harness);
    assert!(back
        .lv_at(0)
        .unwrap()
        .positions()
        .any(|p| p.short_text() == "E/A-Modul"));
    assert!(!back
        .lv_at(1)
        .unwrap()
        .positions()
        .any(|p| p.short_text() == "E/A-Modul"));

    // With Strg: a copy, LV 1 keeps the row (E/A-Modul is selected again –
    // a second click on it would edit its Kurztext).
    assert_eq!(project(&harness).active_lv(), 0);
    let from = row_rect(&harness, "01.0020 E/A-Modul").center();
    let to = row_rect(&harness, "01.01.0010 Mantelleitung");
    drag(
        &mut harness,
        from,
        egui::pos2(to.center().x, to.bottom() - 4.0),
        Modifiers::COMMAND,
    );
    let copied = project(&harness);
    assert!(copied
        .lv_at(0)
        .unwrap()
        .positions()
        .any(|p| p.short_text() == "E/A-Modul"));
    assert!(copied
        .lv_at(1)
        .unwrap()
        .positions()
        .any(|p| p.short_text() == "E/A-Modul"));
}

#[test]
fn search_goes_over_all_lvs() {
    let folder = TempDir::new("mehrlv-suche");
    // Wide like ui_window_follows, so the windows leave the rows free.
    let file = save(&folder.0, &two_lvs());
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.0.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(&folder.0), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(2600.0, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    click_row(&mut harness, "01.0010 DDC-Station");
    press_with(&mut harness, Modifiers::COMMAND, Key::F);
    type_text(&mut harness, "Mantel");
    press(&mut harness, Key::Enter);
    assert_eq!(project(&harness).active_lv(), 1, "Treffer in LV 2");
    let index = selected(&harness).unwrap();
    assert_eq!(
        project(&harness).lv().rows()[index].short_text(),
        "Mantelleitung"
    );
}

#[test]
fn the_bids_view_has_an_lv_choice_with_several_lvs() {
    let folder = TempDir::new("mehrlv-angebote");
    // Wide like ui_window_follows, so the windows leave the rows free.
    let file = save(&folder.0, &two_lvs());
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.0.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(&folder.0), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(2600.0, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    click_row(&mut harness, "01.01.0010 Mantelleitung");
    press_with(&mut harness, Modifiers::COMMAND, Key::Num2);
    assert!(shows(&harness, "LV:"), "LV-Auswahl über den Angeboten");
    assert!(
        shows_part(&harness, "LV 2 – Elektro"),
        "Vorgabe: LV der Schreibmarke"
    );
    let combo = harness
        .query_all_by(|node| {
            node.role() == Role::ComboBox && node.value().as_deref() == Some("LV 2 – Elektro")
        })
        .next()
        .expect("LV-Auswahl")
        .rect()
        .center();
    harness.event(egui::Event::PointerMoved(combo));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: combo,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(&mut harness);

    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("LV 1 – Gebäudeautomation"))
        })
        .last()
        .expect("Eintrag LV 1")
        .click();
    settle(&mut harness);
    assert_eq!(project(&harness).active_lv(), 0);

    // One LV: no choice.
    let folder = TempDir::new("mehrlv-angebote-eins");
    let mut harness = open_demo(&folder.0);
    press_with(&mut harness, Modifiers::COMMAND, Key::Num2);
    assert!(!shows(&harness, "LV:"));
}

#[test]
fn replace_all_goes_over_all_lvs_and_undo_is_one_step() {
    let folder = TempDir::new("mehrlv-ersetzen");
    // Wide like ui_window_follows, so the windows leave the rows free.
    let file = save(&folder.0, &two_lvs());
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.0.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(&folder.0), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(2600.0, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    click_row(&mut harness, "01.0010 DDC-Station");
    press_with(&mut harness, Modifiers::COMMAND, Key::R);
    assert!(shows(&harness, "Alle ersetzen"), "Leiste offen");
    type_text(&mut harness, "leitung");
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "kabel");
    click(&mut harness, "Alle ersetzen");
    assert_eq!(
        short_texts(project(&harness), 1),
        ["Titel", "Unter", "Mantelkabel"],
        "in LV 2 ersetzt, obwohl die Schreibmarke in LV 1 steht"
    );
    assert!(
        shows_part(&harness, "LV 2 – Elektro: 1"),
        "Meldung nennt das LV"
    );
    click(&mut harness, "Schließen");
    click_row(&mut harness, "01.0010 DDC-Station");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        short_texts(project(&harness), 1),
        ["Titel", "Unter", "Mantelleitung"],
        "ein Rückgängig-Schritt"
    );
}

#[test]
fn collapse_all_folds_the_titles_of_every_lv() {
    let folder = TempDir::new("mehrlv-gliederung");
    // Wide like ui_window_follows, so the windows leave the rows free.
    let file = save(&folder.0, &two_lvs());
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.0.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(&folder.0), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(2600.0, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    click_row(&mut harness, "01.0010 DDC-Station");
    open_bar_menu(&mut harness, "Bearbeiten");
    click_menu(&mut harness, "Alle Titel zuklappen");
    let rows = row_names(&harness);
    assert!(
        !rows.iter().any(|row| row.contains("DDC-Station")),
        "LV 1 zu: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("Mantelleitung")),
        "LV 2 zu: {rows:?}"
    );
    assert_eq!(heads(&harness).len(), 2, "Kopfzeilen bleiben");
}

/// Table rows on screen (screen reader names).
fn row_names(harness: &UiHarness) -> Vec<String> {
    let names = std::cell::RefCell::new(Vec::new());
    let _ = harness
        .query_all_by(|node| {
            if node.role() == Role::Row {
                if let Some(label) = node.label() {
                    names.borrow_mut().push(label);
                }
            }
            false
        })
        .count();
    names.into_inner()
}

#[test]
fn two_lvs_with_2000_positions_draw_only_the_rows_in_view() {
    let folder = TempDir::new("mehrlv-gross");
    let rows: Vec<(String, String)> = (0..2000)
        .map(|number| (format!("Position {number}"), format!("Langtext {number}")))
        .collect();
    let rows: Vec<(&str, &str)> = rows.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let mut big = Project::new(
        ProjectName::parse("Gross").unwrap(),
        "2026-11-18",
        lv("1", "Gebäudeautomation", "11.PPPPP", &rows),
    );
    big.add_lv(lv("2", "Elektro", "11.22.PPPPP", &rows));
    let mut harness = open(&folder.0, &big);
    settle(&mut harness);
    let drawn = row_names(&harness).len();
    assert!(drawn < 120, "nur sichtbare Zeilen: {drawn}");

    // Ende of LV 1, ↓ goes on into LV 2, Ende there: the last row of the table.
    click_row(&mut harness, "01.00010 Position 0");
    press(&mut harness, Key::End);
    press(&mut harness, Key::ArrowDown);
    settle(&mut harness);
    assert_eq!(project_active(&harness), 1, "in LV 2 weiter");
    press(&mut harness, Key::End);
    settle(&mut harness);
    let last = project(&harness).lv().rows().last().unwrap().id();
    assert_eq!(selected_id(&harness), Some(last));
    assert!(
        row_names(&harness)
            .iter()
            .any(|name| name.ends_with("Position 1999")),
        "letzte Zeile im Blick"
    );
    let drawn = row_names(&harness).len();
    assert!(drawn < 120, "nur sichtbare Zeilen: {drawn}");
}

fn project_active(harness: &UiHarness) -> usize {
    project(harness).active_lv()
}

#[test]
fn the_windows_follow_the_selection_into_another_lv() {
    let folder = TempDir::new("mehrlv-fenster");
    // The window „Eigenschaften“ beside the table (as in ui_window_follows).
    std::fs::write(
        folder.0.join("settings.json"),
        r#"{"preferences":{"detail_visible":false},"position_window":[1000,950],"position_window_at":[1560,40]}"#,
    )
    .unwrap();
    // Wide like ui_window_follows, so the windows leave the rows free.
    let file = save(&folder.0, &two_lvs());
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.0.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(&folder.0), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(2600.0, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    let windows = |harness: &UiHarness| {
        let workspace = harness.state().workspace().unwrap();
        (
            workspace.position_window_row(),
            workspace.long_text_window_row(),
        )
    };
    click_row(&mut harness, "01.0020 E/A-Modul");
    let module = selected_id(&harness).unwrap();
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    settle(&mut harness);
    assert_eq!(
        windows(&harness),
        (Some(module), None),
        "Eigenschaften offen"
    );

    press_click(&mut harness, "01.01.0010 Mantelleitung");
    assert_eq!(project(&harness).active_lv(), 1);
    let cable = selected_id(&harness).unwrap();
    assert_eq!(windows(&harness).0, Some(cable), "Eigenschaften in LV 2");
    press_with(&mut harness, Modifiers::SHIFT, Key::F2);
    settle(&mut harness);
    assert_eq!(
        windows(&harness),
        (Some(cable), Some(cable)),
        "Langtext dazu"
    );

    press_click(&mut harness, "01.0020 E/A-Modul");
    assert_eq!(project(&harness).active_lv(), 0);
    assert_eq!(
        windows(&harness),
        (Some(module), Some(module)),
        "beide Fenster zurück in LV 1"
    );
    // Nothing of one LV reached the other (the rows are as they were).
    assert_eq!(
        short_texts(project(&harness), 1),
        ["Titel", "Unter", "Mantelleitung"]
    );
    assert_eq!(short_texts(project(&harness), 0)[2], "E/A-Modul");
}

/// A real pointer click near the right end of the table row (the windows
/// open over the left part).
fn press_click(harness: &mut UiHarness, name: &str) {
    let rect = row_rect(harness, name);
    let at = egui::pos2(rect.right() - 120.0, rect.center().y);
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

/// LV 1 and LV 3 for the tender, LV 2 a Kostenberechnung between them.
fn with_cost_lv() -> Project {
    let mut project = two_lvs();
    let mut cost = lv(
        "2",
        "KB Elektro",
        "11.22.PPPP",
        &[("Kostenkennwert Leitungen", "")],
    );
    cost.set_purpose(proava_domain::LvPurpose::Cost(
        proava_domain::CostStage::Calculation,
    ));
    project.add_lv(cost);
    project.lv_at_mut(1).unwrap().set_number("3");
    project.lv_at_mut(1).unwrap().set_title("Sanitär");
    // Order: Gebäudeautomation, KB Elektro, Sanitär.
    project.move_lv(2, true);
    project
}

#[test]
fn a_cost_lv_stands_alone_and_never_among_the_lvs_of_the_tender() {
    let folder = TempDir::new("mehrlv-kosten");
    let project_file = with_cost_lv();
    assert_eq!(project_file.lv_at(1).unwrap().title(), "KB Elektro");
    let mut harness = open(&folder.0, &project_file);
    settle(&mut harness);
    assert_eq!(
        heads(&harness),
        ["LV-Kopf LV 1 – Gebäudeautomation", "LV-Kopf LV 3 – Sanitär"],
        "nur die LVs der Ausschreibung untereinander"
    );
    assert!(
        !shows_part(&harness, "Kostenkennwert"),
        "keine Zeile des Kosten-LV"
    );

    // Search in the table of the tender does not go into the Kostenermittlung.
    click_row(&mut harness, "01.0010 DDC-Station");
    press_with(&mut harness, Modifiers::COMMAND, Key::F);
    type_text(&mut harness, "Kostenkennwert");
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::Escape);
    assert_eq!(project(&harness).active_lv(), 0, "nicht ins Kosten-LV");

    // Menü LV ▸ Gehe zu LV: the Kostenermittlung alone, with its head row.
    open_bar_menu(&mut harness, "LV");
    open_submenu(&mut harness, "Gehe zu LV");
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.contains("KB Elektro"))
        })
        .last()
        .expect("Eintrag KB Elektro")
        .click();
    settle(&mut harness);
    assert_eq!(project(&harness).active_lv(), 1);
    assert_eq!(
        heads(&harness),
        ["LV-Kopf LV 2 – KB Elektro · Kostenberechnung"],
        "allein, die Kopfzeile nennt die Stufe"
    );
    assert!(
        !shows_part(&harness, "DDC-Station"),
        "keine Zeile der Ausschreibung"
    );

    // Angebote: the LV choice lists only LVs of the tender.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num2);
    assert!(shows(&harness, "LV:"), "LV-Auswahl");
    let combo = harness
        .query_all_by(|node| node.role() == Role::ComboBox)
        .next()
        .expect("LV-Auswahl")
        .rect()
        .center();
    harness.event(egui::Event::PointerMoved(combo));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: combo,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(&mut harness);
    let entries = |text: &str| {
        harness
            .query_all_by(|node| {
                node.label()
                    .is_some_and(|label| label.contains(text) && label.contains("Angebot"))
            })
            .count()
    };
    assert_eq!(entries("Gebäudeautomation"), 1);
    assert_eq!(entries("Sanitär"), 1);
    assert_eq!(entries("KB Elektro"), 0, "kein Kosten-LV in der Auswahl");
}

/// Prüfen ▸ Alle LVs prüfen …: the findings per LV of the tender (the
/// Kostenermittlung only when it is the active LV); a click goes into the
/// right LV.
#[test]
fn check_all_lvs_lists_the_findings_per_lv_and_a_click_goes_into_that_lv() {
    let folder = TempDir::new("mehrlv-pruefen");
    let mut project_file = with_cost_lv();
    // LV 3 Sanitär (index 2): a position without quantity.
    let missing = {
        let lv = project_file.lv_at_mut(2).unwrap();
        let row = lv.push_position(Position::new("", "Druckprobe", Decimal::ZERO, "psch", None));
        lv.renumber();
        row
    };
    let groups = proava_ui::lv_check::check_all(&project_file, &proava_exchange::GaebPdfExchange);
    let labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
    assert_eq!(
        labels,
        ["LV 1 – Gebäudeautomation", "LV 3 – Sanitär"],
        "die LVs der Ausschreibung, kein Kosten-LV"
    );
    let finding = groups[1]
        .findings
        .iter()
        .find(|finding| finding.row == Some(missing))
        .expect("Befund zur Position ohne Menge")
        .text
        .clone();

    let mut harness = open(&folder.0, &project_file);
    click_row(&mut harness, "01.0010 DDC-Station");
    assert_eq!(project(&harness).active_lv(), 0);
    menu_path(&mut harness, "Prüfen", "Alle LVs prüfen …");
    assert!(shows(&harness, "Alle LVs prüfen"));
    assert!(shows(&harness, "LV 1 – Gebäudeautomation"));
    assert!(shows(&harness, "LV 3 – Sanitär"));
    assert!(!shows_part(&harness, "KB Elektro"));
    assert!(shows_part(&harness, "2 LVs ·"));
    harness
        .query_all_by(|node| {
            node.role() == Role::Row && node.label().as_deref() == Some(finding.as_str())
        })
        .next()
        .expect("Befundzeile")
        .click();
    settle(&mut harness);
    assert!(!shows(&harness, "Alle LVs prüfen"), "Dialog zu");
    assert_eq!(project(&harness).active_lv(), 2, "ins LV Sanitär");
    assert_eq!(selected_id(&harness), Some(missing));

    // With the Kostenermittlung active it is checked, too.
    let folder = TempDir::new("mehrlv-pruefen-kosten");
    project_file.set_active_lv(1);
    let mut harness = open(&folder.0, &project_file);
    assert_eq!(project(&harness).active_lv(), 1);
    menu_path(&mut harness, "Prüfen", "Alle LVs prüfen …");
    assert!(shows(&harness, "LV 2 – KB Elektro (Kostenberechnung)"));
    assert!(shows(&harness, "LV 1 – Gebäudeautomation"));
    assert!(shows_part(&harness, "3 LVs ·"));
}

/// Two positions in LV 2, so rows pasted after the first move order numbers.
fn two_lvs_with_more_in_lv_2() -> Project {
    let mut project = two_lvs();
    let lv = project.lv_at_mut(1).unwrap();
    lv.push_position(Position::new(
        "",
        "Schalter",
        Decimal::TWO,
        "St",
        Some(Decimal::TEN),
    ));
    lv.renumber();
    project
}

/// Umschalt+Klick into another LV: the selection stays in its own LV and
/// goes to the end of it (into an LV above: to its start).
#[test]
fn shift_click_into_another_lv_selects_to_the_end_of_the_own_lv() {
    let folder = TempDir::new("mehrlv-umschalt");
    let mut harness = open(&folder.0, &two_lvs_with_more_in_lv_2());
    settle(&mut harness);
    press_click(&mut harness, "01.0010 DDC-Station");
    assert_eq!(project(&harness).active_lv(), 0);
    let rect = row_rect(&harness, "01.01.0010 Mantelleitung");
    let at = egui::pos2(rect.right() - 120.0, rect.center().y);
    harness.event(egui::Event::PointerMoved(at));
    harness.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::SHIFT,
        });
        harness.step();
    }
    harness.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    settle(&mut harness);
    assert_eq!(project(&harness).active_lv(), 0, "das LV bleibt");
    let lv = project(&harness).lv_at(0).unwrap();
    let first = lv.positions().next().unwrap().id();
    let selection = harness.state().workspace().unwrap().selection_ids();
    let expected: std::collections::HashSet<_> = lv
        .rows()
        .iter()
        .skip_while(|row| row.id() != first)
        .map(LvRow::id)
        .collect();
    // Row ids are unique per LV only: the selection belongs to LV 1, the
    // active one – exactly from the first position to its end.
    assert_eq!(selection, expected, "bis zum Ende von LV 1");
    assert_eq!(selection.len(), lv.rows().len() - 1, "ohne den Titel davor");
}

/// Einfügen in ein LV mit Angeboten oder Auftrag fragt – auch ohne Versand
/// aus proAVA (Anwender 19.11.). Versandt nennt die Rückfrage beide Gründe;
/// mit Auftrag macht „Als Nachtrag anlegen …“ die neue Position zum Nachtrag.
#[test]
fn pasting_into_an_lv_with_bids_or_contract_asks_first() {
    for (awarded, sent) in [(false, false), (false, true), (true, false)] {
        let folder = TempDir::new(match (awarded, sent) {
            (true, _) => "mehrlv-auftrag",
            (false, true) => "mehrlv-versandt",
            (false, false) => "mehrlv-angebote",
        });
        let mut project_file = two_lvs_with_more_in_lv_2();
        let mut bid =
            proava_domain::Bid::new(proava_domain::Address::named("Elektro Nord"), "Nord.X84");
        let rows: Vec<_> = project_file
            .lv_at(1)
            .unwrap()
            .positions()
            .map(|p| p.id())
            .collect();
        for row in rows {
            bid.set_unit_price(row, Some(Decimal::from(12)));
        }
        let id = project_file.add_bid_to(1, bid);
        if sent {
            project_file
                .lv_at_mut(1)
                .unwrap()
                .mark_sent("2026-11-18T09:00:00".into(), None);
        }
        if awarded {
            project_file.set_active_lv(1);
            project_file.award_bid(id, "01.12.2026").unwrap();
            project_file.take_over_contract().unwrap();
            project_file.set_active_lv(0);
        }
        let mut harness = open(&folder.0, &project_file);
        settle(&mut harness);
        click_row(&mut harness, "01.0010 DDC-Station");
        harness.event(egui::Event::Copy);
        settle(&mut harness);
        click_row(&mut harness, "01.01.0010 Mantelleitung");
        assert_eq!(project(&harness).active_lv(), 1);
        let before = short_texts(project(&harness), 1);
        let plain = proava_ui::system_clipboard::text().unwrap();
        harness.event(egui::Event::Paste(plain.clone()));
        settle(&mut harness);
        let title = if awarded {
            "LV hat bereits einen Auftrag"
        } else {
            "LV hat bereits Angebote"
        };
        assert!(shows(&harness, title), "Rückfrage ({awarded}, {sent})");
        let has = if awarded {
            "ein Angebot und einen Auftrag"
        } else {
            "ein Angebot"
        };
        assert!(shows_part(
            &harness,
            &format!("Das LV hat bereits {has} – die neue Position fehlt dort.")
        ));
        assert!(shows_part(&harness, "Trotzdem einfügen?"));
        assert_eq!(
            shows_part(
                &harness,
                "Das LV wurde am 18.11.2026 versandt. Die OZ ab 01.01.0020"
            ),
            sent,
            "Versand"
        );
        assert_eq!(
            shows_part(
                &harness,
                "Zusätzliche Leistungen gehören in einen Nachtrag."
            ),
            awarded
        );
        assert_eq!(shows(&harness, "Als Nachtrag anlegen …"), awarded);
        click(&mut harness, "Abbrechen");
        assert_eq!(short_texts(project(&harness), 1), before, "zurückgenommen");
        assert!(shows_part(&harness, "Änderung zurückgenommen"));
        assert_eq!(project(&harness).bids_of(1).len(), 1);
        assert_eq!(project(&harness).contract_of(1).is_some(), awarded);

        click_row(&mut harness, "01.01.0010 Mantelleitung");
        harness.event(egui::Event::Paste(plain.clone()));
        settle(&mut harness);
        if awarded {
            // „Als Nachtrag anlegen …“: the new position (where it was
            // pasted) becomes the first position of a new Nachtrag.
            click(&mut harness, "Als Nachtrag anlegen …");
            let orders = project(&harness).change_orders_of(1);
            assert_eq!(orders.len(), 1, "neuer Nachtrag");
            let lv = project(&harness).lv_at(1).unwrap();
            let items: Vec<_> = orders[0]
                .items
                .iter()
                .map(|item| {
                    (
                        lv.position_by_id(item.position)
                            .unwrap()
                            .short_text()
                            .to_string(),
                        item.change,
                    )
                })
                .collect();
            assert_eq!(
                items,
                vec![(
                    "DDC-Station".to_string(),
                    proava_domain::ItemChange::NewPosition
                )]
            );
            assert!(shows_part(&harness, "als neue Leistung angelegt"));
            // One Strg+Z takes the Nachtrag back, the position stays.
            press_with(&mut harness, Modifiers::COMMAND, Key::Z);
            assert!(project(&harness).change_orders_of(1).is_empty());
        } else {
            click(&mut harness, "Trotzdem einfügen");
            assert!(shows_part(&harness, "Beibehalten"));
        }
        // The position and its Ausführungsbeschreibung, which LV 2 lacked.
        assert_eq!(short_texts(project(&harness), 1).len(), before.len() + 2);
        assert!(short_texts(project(&harness), 1).contains(&"DDC-Station".to_string()));
        if !awarded {
            // Kept once: no further question until bids or Auftrag change.
            click_row(&mut harness, "01.01.0010 Mantelleitung");
            harness.event(egui::Event::Paste(plain));
            settle(&mut harness);
            assert!(!shows(&harness, title), "keine zweite Rückfrage");
            let texts = short_texts(project(&harness), 1);
            assert_eq!(texts.len(), before.len() + 4, "noch einmal Position und AB");
            assert_eq!(
                texts.iter().filter(|text| *text == "DDC-Station").count(),
                2
            );
        }
    }
}

/// Wide like ui_window_follows, so the windows leave the rows free.
fn open_wide(folder: &std::path::Path, project: &Project) -> UiHarness {
    let file = save(folder, project);
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(folder), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(2600.0, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    harness
}

/// Ziehen aus einem LV mit Angeboten nimmt ihnen Positionen, Ziehen in eines
/// hinein bringt welche, die ihnen fehlen: beides fragt (Anwender 19.11.).
#[test]
fn dragging_out_of_or_into_an_lv_with_bids_asks_first() {
    let folder = TempDir::new("mehrlv-ziehen-angebote");
    let mut project_file = two_lvs();
    let mut bid = proava_domain::Bid::new(proava_domain::Address::named("Nord"), "Nord.X84");
    let rows: Vec<_> = project_file.lv().positions().map(|p| p.id()).collect();
    for row in rows {
        bid.set_unit_price(row, Some(Decimal::from(12)));
    }
    project_file.add_bid_to(0, bid);
    let mut harness = open_wide(&folder.0, &project_file);

    // Verschieben aus LV 1 (mit Angebot) nach LV 2.
    click_row(&mut harness, "01.0020 E/A-Modul");
    let from = row_rect(&harness, "01.0020 E/A-Modul").center();
    let to = row_rect(&harness, "01.01.0010 Mantelleitung");
    drag(
        &mut harness,
        from,
        egui::pos2(to.center().x, to.bottom() - 4.0),
        Modifiers::NONE,
    );
    assert!(shows(&harness, "LV hat bereits Angebote"), "Rückfrage");
    assert!(shows_part(
        &harness,
        "„LV 1 – Gebäudeautomation“, aus dem die Zeilen kommen, hat bereits ein Angebot – die \
         verschobene Position ist dort enthalten. Trotzdem verschieben?"
    ));
    click(&mut harness, "Abbrechen");
    let back = project(&harness);
    assert!(back
        .lv_at(0)
        .unwrap()
        .positions()
        .any(|p| p.short_text() == "E/A-Modul"));
    assert!(!back
        .lv_at(1)
        .unwrap()
        .positions()
        .any(|p| p.short_text() == "E/A-Modul"));

    // Mit Strg kopiert: LV 1 behält alles, LV 2 hat keine Angebote – keine Rückfrage.
    let from = row_rect(&harness, "01.0020 E/A-Modul").center();
    let to = row_rect(&harness, "01.01.0010 Mantelleitung");
    drag(
        &mut harness,
        from,
        egui::pos2(to.center().x, to.bottom() - 4.0),
        Modifiers::COMMAND,
    );
    assert!(!shows(&harness, "LV hat bereits Angebote"));
    assert!(project(&harness)
        .lv_at(1)
        .unwrap()
        .positions()
        .any(|p| p.short_text() == "E/A-Modul"));

    // Hinein in LV 1: die neue Position fehlt dem Angebot.
    click_row(&mut harness, "01.01.0010 Mantelleitung");
    let from = row_rect(&harness, "01.01.0010 Mantelleitung").center();
    let to = row_rect(&harness, "01.0010 DDC-Station");
    drag(
        &mut harness,
        from,
        egui::pos2(to.center().x, to.bottom() - 4.0),
        Modifiers::COMMAND,
    );
    assert!(shows_part(
        &harness,
        "Das LV hat bereits ein Angebot – die neue Position fehlt dort. Trotzdem einfügen?"
    ));
    click(&mut harness, "Trotzdem einfügen");
    assert!(project(&harness)
        .lv_at(0)
        .unwrap()
        .positions()
        .any(|p| p.short_text() == "Mantelleitung"));
}
