//! Shared setup of the UI tests: the whole program in an `egui_kittest`
//! harness (no window, no display), with the real storage adapters in a
//! temporary folder – never the user's settings or projects.

#![allow(dead_code)]

pub mod invariants;

use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use proava_application::{Launcher, ProjectRepository};
use proava_domain::{LvRow, Project};
use proava_exchange::GaebPdfExchange;
use proava_infrastructure::{DiskFolders, JsonSettingsFile, XmlProjectFile};
use proava_ui::{App, StartFile};

pub use egui::{Key, Modifiers};

/// Temporary folder, removed at the end of the test.
/// A folder only this test uses: a unique name, created exclusively (two
/// tests – also of other test runs at the same time – never share one); it is
/// removed with this value, nothing else.
pub struct TempDir(pub PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempDir {
    pub fn new(label: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("proava-uitest-{label}-"))
            .tempdir()
            .unwrap();
        Self(dir.path().to_path_buf(), dir)
    }
}

/// Launcher with the real adapters; settings in `folder/settings.json`.
// The UI takes an `Arc<Launcher>` like `main.rs`; tests stay on one thread.
#[allow(clippy::arc_with_non_send_sync)]
pub fn launcher(folder: &Path) -> Arc<Launcher> {
    Arc::new(
        Launcher::new(
            Box::new(XmlProjectFile::new()),
            Box::new(JsonSettingsFile::at(folder.join("settings.json"))),
            Box::new(DiskFolders::new()),
        )
        .with_exchange(Box::new(GaebPdfExchange)),
    )
}

/// Saves `project` as `folder/<name>.proava` and returns the path.
pub fn save(folder: &Path, project: &Project) -> PathBuf {
    let file = folder.join(format!("{}.proava", project.name().as_str()));
    XmlProjectFile::new().save_project(&file, project).unwrap();
    file
}

pub type UiHarness = Harness<'static, App>;

/// The program on the start screen, or with `start` opened.
pub fn harness(folder: &Path, start: Option<StartFile>) -> UiHarness {
    // Copies for the PDF viewer (Schriftverkehr ▸ Öffnen) in the test's own
    // folder: the tests of one program share a process (and its PID folder).
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.join("pdf-anzeige")));
    let app = App::new(launcher(folder), start);
    let mut harness = harness_of(app);
    settle(&mut harness);
    harness
}

/// A harness around an app built by the test (e.g. with a bridge).
pub fn harness_of(app: App) -> UiHarness {
    Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_context_setup(App::setup)
        .with_accessibility_check(false)
        // Real frame time, so double clicks (≤ 0,3 s apart) are recognised.
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}

/// The demo project opened in the workspace.
pub fn open_demo(folder: &Path) -> UiHarness {
    let file = save(folder, &proava_ui::demo::demo_project());
    let harness = harness(folder, Some(StartFile::Project(file)));
    assert!(
        harness.state().project().is_some(),
        "Projekt nicht geöffnet"
    );
    harness
}

/// A few frames: enough for focus changes, dialogs and repaints, without
/// waiting for status messages to expire.
pub fn settle(harness: &mut UiHarness) {
    harness.run_steps(4);
}

pub fn press(harness: &mut UiHarness, key: Key) {
    harness.key_press(key);
    settle(harness);
}

pub fn press_with(harness: &mut UiHarness, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    settle(harness);
}

pub fn type_text(harness: &mut UiHarness, text: &str) {
    harness.event(egui::Event::Text(text.to_string()));
    settle(harness);
}

pub fn project(harness: &UiHarness) -> &Project {
    harness.state().project().expect("Projekt offen")
}

/// Short text of every row of the active LV, in order.
pub fn row_texts(harness: &UiHarness) -> Vec<String> {
    project(harness)
        .lv()
        .rows()
        .iter()
        .map(LvRow::short_text)
        .collect()
}

/// Whether some widget shows exactly this text (AccessKit label of
/// buttons, value of plain labels).
pub fn shows(harness: &UiHarness, text: &str) -> bool {
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some(text) || node.value().as_deref() == Some(text)
        })
        .next()
        .is_some()
}

/// Whether some widget's text contains this.
pub fn shows_part(harness: &UiHarness, text: &str) -> bool {
    harness
        .query_all_by(|node| {
            node.label().is_some_and(|label| label.contains(text))
                || node.value().is_some_and(|value| value.contains(text))
        })
        .next()
        .is_some()
}

/// The question before new or removed positions in an LV with bids (the
/// demo has four, Anwender 19.11.): checks it and keeps the change with
/// „Trotzdem <verb>“ (einfügen, löschen, ändern).
pub fn keep_despite_bids(harness: &mut UiHarness, verb: &str) {
    settle(harness);
    assert!(
        shows(harness, "LV hat bereits Angebote"),
        "Rückfrage wegen der Angebote"
    );
    assert!(
        shows_part(harness, "Das LV hat bereits 4 Angebote"),
        "Grund"
    );
    click(harness, &format!("Trotzdem {verb}"));
    assert!(!shows(harness, "LV hat bereits Angebote"), "Rückfrage zu");
}

/// Clicks the (first) widget with exactly this label and settles.
pub fn click(harness: &mut UiHarness, label: &str) {
    harness.get_all_by_label(label).next().expect(label).click();
    settle(harness);
}

/// Double click on the (first) widget with exactly this label.
pub fn double_click(harness: &mut UiHarness, label: &str) {
    let center = harness
        .get_all_by_label(label)
        .next()
        .expect(label)
        .rect()
        .center();
    harness.event(egui::Event::PointerMoved(center));
    harness.step();
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            harness.step();
        }
    }
    settle(harness);
}

/// Index of the selected row in the active LV.
pub fn selected(harness: &UiHarness) -> Option<usize> {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.selected_index())
}

/// Moves the selection down (from the top) to the first position.
pub fn select_first_position(harness: &mut UiHarness) -> usize {
    press(harness, Key::Home);
    for _ in 0..40 {
        if let Some(index) = selected(harness) {
            if project(harness).lv().rows()[index].is_position() {
                return index;
            }
        }
        press(harness, Key::ArrowDown);
    }
    panic!("keine Position erreicht");
}

/// Opens a submenu (egui opens them on hover) and waits until it shows.
/// The submenu shown last – „Einfügen“ and „Bearbeiten“ are also menus of
/// the menu bar.
pub fn open_submenu(harness: &mut UiHarness, label: &str) {
    harness.get_all_by_label(label).last().expect(label).hover();
    harness.run_steps(30);
}

/// Rect of an entry of the menu bar (Projekt, Bearbeiten, Einfügen, LV,
/// Prüfen …): the button on the row of „GAEB“ at the top – not the tab „LV“
/// nor the submenus „Einfügen“/„Bearbeiten“ of the right click.
pub fn menu_bar_entry(harness: &UiHarness, name: &str) -> egui::Rect {
    let row = harness
        .query_all_by(|node| node.label().as_deref() == Some("GAEB"))
        .map(|node| node.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("Menüleiste")
        .center()
        .y;
    harness
        .query_all_by(|node| node.label().as_deref() == Some(name))
        .map(|node| node.rect())
        .find(|rect| (rect.center().y - row).abs() < 2.0)
        .unwrap_or_else(|| panic!("Menü „{name}“ in der Menüleiste"))
}

/// Opens a menu of the menu bar.
pub fn open_bar_menu(harness: &mut UiHarness, name: &str) {
    let rect = menu_bar_entry(harness, name);
    harness
        .query_all_by(|node| node.label().as_deref() == Some(name))
        .find(|node| node.rect() == rect)
        .unwrap()
        .click();
    settle(harness);
}

/// Menu bar ▸ entry, e.g. `menu_path(h, "Prüfen", "LV prüfen …")`.
pub fn menu_path(harness: &mut UiHarness, menu: &str, entry: &str) {
    open_bar_menu(harness, menu);
    click_menu(harness, entry);
}

/// The right-click menu of the LV table is open (its submenu „Einfügen“
/// besides the menu „Einfügen“ of the menu bar).
pub fn row_menu_open(harness: &UiHarness) -> bool {
    harness.query_all_by_label("Einfügen").count() >= 2
}

/// Clicks a menu entry by its text; egui adds the shortcut to the name
/// („Hinweis darunter Strg+H“). A long menu scrolls in a low window
/// (`menus::scroll`): like a user, rest on its arrow until the entry shows.
pub fn click_menu(harness: &mut UiHarness, text: &str) {
    let find = |harness: &UiHarness| {
        harness
            .query_all_by(|node| {
                node.label()
                    .is_some_and(|label| label == text || label.starts_with(&format!("{text} ")))
            })
            .next()
            .map(|node| node.rect())
    };
    let arrow = |harness: &UiHarness, name: &str| {
        harness
            .query_all_by(|node| node.label().as_deref() == Some(name))
            .next()
            .map(|node| node.rect())
    };
    for _ in 0..600 {
        let Some(entry) = find(harness) else { break };
        match (
            arrow(harness, "Nach unten scrollen"),
            arrow(harness, "Nach oben scrollen"),
        ) {
            (Some(down), _) if entry.bottom() > down.top() + 0.5 => harness.hover_at(down.center()),
            (_, Some(up)) if entry.top() < up.bottom() - 0.5 => harness.hover_at(up.center()),
            _ => break,
        }
        harness.step();
    }
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label == text || label.starts_with(&format!("{text} ")))
        })
        .next()
        .unwrap_or_else(|| panic!("Menüeintrag „{text}“ fehlt"))
        .click();
    settle(harness);
}
