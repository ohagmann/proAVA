//! Two proAVA windows: copy in one, paste in the other (rows, titles,
//! whole LVs), and a project another window has open (lock file).
//!
//! Both `App`s run in one test thread; the system clipboard is replaced by
//! a stand-in of that thread (tests never touch the real clipboard).

mod common;

use common::*;
use proava_application::ProjectRepository;
use proava_domain::{
    EditStatus, Lv, LvRow, OzMask, PerformanceDescription, Project, ProjectName, RichText,
    TextBlock,
};
use proava_ui::StartFile;

/// Demo project with an image in the first position's long text and an
/// Ausführungsbeschreibung it refers to.
fn source_project() -> Project {
    let mut project = proava_ui::demo::demo_project();
    let image = project.add_image("lageplan.png", vec![137, 80, 78, 71, 1, 2, 3]);
    let lv = project.lv_mut();
    let ab = lv.push_row(LvRow::Performance(PerformanceDescription::new(
        "1",
        "Kabelwege",
        RichText::plain("verzinkt"),
    )));
    let first = lv.rows().iter().position(LvRow::is_position).unwrap();
    let id = lv.rows()[first].id();
    if let Some(LvRow::Position(position)) = lv.row_mut(first) {
        let mut text = position.long_text().clone();
        text.blocks_mut().push(TextBlock::Image {
            file: image,
            width_mm: Some(60.0),
        });
        position.set_long_text(text);
        position.set_status(EditStatus::Done);
    }
    lv.link_performance(id, Some(ab)).unwrap();
    project
}

fn empty_project(name: &str) -> Project {
    Project::new(
        ProjectName::parse(name).unwrap(),
        "2026-11-17T22:00:00",
        Lv::new("Ziel", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
    )
}

fn window(folder: &std::path::Path, project: &Project) -> UiHarness {
    let file = save(folder, project);
    let harness = harness(folder, Some(StartFile::Project(file)));
    assert!(
        harness.state().project().is_some(),
        "Projekt nicht geöffnet"
    );
    harness
}

fn clipboard_html() -> String {
    proava_ui::system_clipboard::html().unwrap_or_default()
}

#[test]
fn a_title_goes_completely_into_another_window_and_back_with_undo() {
    let (folder_a, folder_b) = (TempDir::new("zwei-a"), TempDir::new("zwei-b"));
    let mut a = window(&folder_a.0, &source_project());
    let mut b = window(&folder_b.0, &empty_project("Ziel"));

    // A: first title (with its block) → Strg+C.
    press(&mut a, Key::Home);
    a.event(egui::Event::Copy);
    settle(&mut a);
    let html = clipboard_html();
    assert!(
        html.contains("proAVA-Zwischenablage|1|zeilen|Titel 01"),
        "{html}"
    );
    assert!(html.contains("<td>"), "Tabelle für Excel/Word");
    assert!(shows_part(&a, "Zwischenablage: Titel 01"), "Statuszeile");

    // B: Strg+V (the key event brings the plain text along).
    let plain = proava_ui::system_clipboard::text().unwrap();
    b.event(egui::Event::Paste(plain));
    settle(&mut b);
    let source = source_project();
    let block = source.lv().block_range(0).len();
    let target = project(&b);
    assert_eq!(
        target.lv().rows().len(),
        block + 1,
        "Titel mit Inhalt und die Ausführungsbeschreibung davor"
    );
    assert!(target.lv().rows()[0].is_performance());
    let pasted = target.lv().positions().next().unwrap();
    assert_eq!(
        pasted.status(),
        EditStatus::Open,
        "Status offen wie bei der Übernahme"
    );
    assert_eq!(
        pasted.performance(),
        Some(target.lv().rows()[0].id()),
        "AB-Verweis folgt"
    );
    let image = pasted.long_text().images().next().unwrap().to_string();
    assert_eq!(
        target.image(&image).unwrap().bytes,
        vec![137, 80, 78, 71, 1, 2, 3]
    );
    assert!(target.lv().duplicate_ordinals().is_empty());

    // One undo step.
    press_with(&mut b, Modifiers::COMMAND, Key::Z);
    assert!(
        project(&b).lv().rows().is_empty(),
        "Strg+Z nimmt das Einfügen zurück"
    );
}

#[test]
fn strg_a_and_several_positions_from_different_titles() {
    let (folder_a, folder_b) = (TempDir::new("zwei-c"), TempDir::new("zwei-d"));
    let mut a = window(&folder_a.0, &source_project());
    let mut b = window(&folder_b.0, &proava_ui::demo::demo_project());

    // Two positions from different titles (Strg+click = Command on the keys).
    let positions: Vec<usize> = project(&a)
        .lv()
        .rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.is_position())
        .map(|(index, _)| index)
        .collect();
    let (first, last) = (positions[0], *positions.last().unwrap());
    select_first_position(&mut a);
    while selected(&a) != Some(last) {
        press_with(&mut a, Modifiers::SHIFT, Key::ArrowDown);
    }
    let marked = a.state().workspace().unwrap().selection_ids().len();
    assert!(marked > 2);
    a.event(egui::Event::Copy);
    settle(&mut a);
    assert!(clipboard_html().contains("|zeilen|"), "mehrere Zeilen");
    assert_eq!(selected(&a), Some(last));
    let _ = first;

    // Strg+A marks everything.
    press_with(&mut a, Modifiers::COMMAND, Key::A);
    let all = project(&a).lv().rows().len();
    assert_eq!(a.state().workspace().unwrap().selection_ids().len(), all);
    a.event(egui::Event::Copy);
    settle(&mut a);
    let before = project(&b).lv().rows().len();
    select_first_position(&mut b);
    let plain = proava_ui::system_clipboard::text().unwrap();
    b.event(egui::Event::Paste(plain));
    settle(&mut b);
    let after = project(&b).lv().rows().len();
    assert!(
        after >= before + all,
        "alles eingefügt: {before} + {all} → {after}"
    );
    assert!(project(&b).lv().duplicate_ordinals().is_empty());
}

#[test]
fn a_whole_lv_becomes_a_new_lv_in_the_other_window() {
    let (folder_a, folder_b) = (TempDir::new("zwei-e"), TempDir::new("zwei-f"));
    let mut a = window(&folder_a.0, &source_project());
    let mut b = window(&folder_b.0, &empty_project("Ziel"));

    open_bar_menu(&mut a, "LV");
    click_menu(&mut a, "Ganzes LV kopieren");
    assert!(clipboard_html().contains("|lv|LV "), "{}", clipboard_html());

    open_bar_menu(&mut b, "LV");
    click_menu(&mut b, "LV aus Zwischenablage einfügen …");
    assert!(shows(&b, "LV als neues LV einfügen?"));
    click(&mut b, "Ohne Preise");
    let target = project(&b);
    assert_eq!(target.lv_count(), 2);
    let copy = target.lv_at(1).unwrap();
    assert_eq!(copy.rows().len(), source_project().lv().rows().len());
    assert!(
        copy.positions()
            .all(|position| position.unit_price().is_none()),
        "ohne Preise"
    );
    assert!(!target.images().is_empty(), "Bilder mit");
    assert_eq!(target.active_lv(), 1, "das neue LV ist offen");
    press_with(&mut b, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&b).lv_count(), 1, "Strg+Z nimmt das neue LV zurück");
}

#[test]
fn foreign_text_is_refused_and_own_rows_paste_as_before() {
    let folder = TempDir::new("zwei-g");
    let mut a = window(&folder.0, &proava_ui::demo::demo_project());
    let rows = project(&a).lv().rows().len();
    a.event(egui::Event::Paste("irgendein Text aus Word".into()));
    settle(&mut a);
    assert_eq!(project(&a).lv().rows().len(), rows);
    assert!(shows_part(&a, "keine LV-Zeilen aus proAVA"));

    let index = select_first_position(&mut a);
    a.event(egui::Event::Copy);
    settle(&mut a);
    let plain = proava_ui::system_clipboard::text().unwrap();
    a.event(egui::Event::Paste(plain));
    settle(&mut a);
    assert_eq!(
        project(&a).lv().rows().len(),
        rows + 1,
        "eigene Zeile wie bisher"
    );
    assert_eq!(selected(&a), Some(index + 1));
}

#[test]
fn a_project_open_elsewhere_opens_read_only_on_request() {
    let folder = TempDir::new("zwei-h");
    let file = save(&folder.0, &proava_ui::demo::demo_project());
    let lock = folder.0.join(format!(
        ".~lock.{}#",
        file.file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(
        &lock,
        "proAVA;kollege;anderer-rechner;4711;17.11.2026 08:00\n",
    )
    .unwrap();
    let modified = std::fs::metadata(&file).unwrap().modified().unwrap();

    let mut window = harness(&folder.0, Some(StartFile::Project(file.clone())));
    assert!(shows(&window, "Projekt ist schon geöffnet"), "Rückfrage");
    assert!(shows_part(
        &window,
        "kollege auf anderer-rechner seit 17.11.2026 08:00"
    ));
    click(&mut window, "Schreibgeschützt öffnen");
    let workspace = window.state().workspace().expect("geöffnet");
    assert!(workspace.is_read_only());

    // Copying works, saving does not.
    select_first_position(&mut window);
    window.event(egui::Event::Copy);
    settle(&mut window);
    assert!(clipboard_html().contains("proAVA-Zwischenablage"));
    press(&mut window, Key::Delete);
    press_with(&mut window, Modifiers::COMMAND, Key::S);
    assert!(shows_part(&window, "Speichern ist gesperrt"));
    assert_eq!(
        std::fs::metadata(&file).unwrap().modified().unwrap(),
        modified
    );
    drop(window);
    assert!(lock.is_file(), "die fremde Sperre bleibt");
}

#[test]
fn a_lock_left_by_an_ended_proava_goes_without_a_question() {
    let folder = TempDir::new("zwei-verwaist");
    let file = save(&folder.0, &proava_ui::demo::demo_project());
    let lock = folder.0.join(format!(
        ".~lock.{}#",
        file.file_name().unwrap().to_string_lossy()
    ));
    // This computer, a PID without proAVA (killed or crashed).
    proava_infrastructure::XmlProjectFile::new()
        .write_lock(&file)
        .unwrap();
    let line = std::fs::read_to_string(&lock).unwrap();
    let mut fields: Vec<String> = line.trim().split(';').map(str::to_string).collect();
    fields[3] = (u32::MAX - 1).to_string();
    std::fs::write(&lock, fields.join(";") + "\n").unwrap();
    let stale = proava_infrastructure::XmlProjectFile::new()
        .read_lock(&file)
        .is_some_and(|lock| lock.stale);
    if !stale {
        return; // no process list on this system: the question stays
    }

    let window = harness(&folder.0, Some(StartFile::Project(file)));
    assert!(
        !shows(&window, "Projekt ist schon geöffnet"),
        "keine Rückfrage"
    );
    let workspace = window.state().workspace().expect("geöffnet");
    assert!(!workspace.is_read_only());
    let now = std::fs::read_to_string(&lock).unwrap();
    assert!(
        now.contains(&format!(";{};", std::process::id())),
        "jetzt die eigene Sperre: {now}"
    );
}

#[test]
fn opening_sets_the_own_lock_and_closing_removes_it() {
    let folder = TempDir::new("zwei-i");
    let file = save(&folder.0, &proava_ui::demo::demo_project());
    let lock = folder.0.join(format!(
        ".~lock.{}#",
        file.file_name().unwrap().to_string_lossy()
    ));
    let window = harness(&folder.0, Some(StartFile::Project(file)));
    assert!(window
        .state()
        .workspace()
        .is_some_and(|workspace| !workspace.is_read_only()));
    assert!(lock.is_file(), "eigene Sperre");
    drop(window);
    assert!(!lock.exists(), "beim Schließen entfernt");
}
