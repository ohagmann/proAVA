//! „Über proAVA“ names the own licence (Business Source License 1.1, a
//! German summary, where the binding English text is) and shows the text.

mod common;

use common::*;
use proava_ui::about::{LICENSE_SUMMARY, WARRANTY_DE, WARRANTY_EN};

#[test]
fn the_about_dialog_names_and_shows_the_licence() {
    let folder = TempDir::new("about-lizenz");
    let mut harness = harness(&folder.0, None);
    settle(&mut harness);
    click(
        &mut harness,
        &format!("Version {} · Über proAVA …", env!("CARGO_PKG_VERSION")),
    );
    settle(&mut harness);
    assert!(shows(&harness, "© 2026 Oliver Hagmann"));
    assert!(shows(&harness, LICENSE_SUMMARY));
    assert!(shows_part(&harness, "Datei LICENSE neben dem Programm"));
    assert!(!shows_part(&harness, "Proprietär"));
    assert!(
        shows(&harness, WARRANTY_EN),
        "Gewährleistungsausschluss englisch"
    );
    assert!(shows(&harness, WARRANTY_DE), "und deutsch");

    click(&mut harness, "Lizenz anzeigen");
    settle(&mut harness);
    assert!(shows(&harness, "Change Date:          2030-11-20"));
    assert!(shows_part(
        &harness,
        "GNU Affero General Public License v3.0 or later"
    ));

    click(&mut harness, "Lizenz ausblenden");
    settle(&mut harness);
    assert!(!shows(&harness, "Change Date:          2030-11-20"));
}

/// First start (no project used yet): the disclaimer stands quietly in the
/// empty list of the start page; with a project in the list it is gone.
#[test]
fn the_first_start_shows_the_disclaimer_on_the_start_page() {
    let folder = TempDir::new("start-gewaehrleistung");
    let first_start = harness(&folder.0, None);
    assert!(shows(&first_start, WARRANTY_EN));
    assert!(shows(&first_start, WARRANTY_DE));

    let folder = TempDir::new("start-gewaehrleistung-projekt");
    let file = save(&folder.0, &proava_ui::demo::demo_project());
    launcher(&folder.0).open_project(&file).unwrap();
    let later = harness(&folder.0, None);
    assert!(shows(&later, "Zuletzt verwendet"));
    assert!(!shows(&later, WARRANTY_EN), "nur im Willkommensbereich");
}

/// The disclaimer reads the same in README, handbook and both LIESMICH.txt
/// as in the program, and not in LICENSE (that stays the pure BSL text).
#[test]
fn the_disclaimer_reads_the_same_everywhere() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Line breaks, Markdown quotes/bold and indentation do not count.
    let plain = |text: &str| {
        text.replace(['>', '*'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    for file in [
        "README.md",
        "docs/handbuch/handbuch.typ",
        "assets/windows/LIESMICH.txt",
        "assets/linux/LIESMICH.txt",
    ] {
        let text = plain(&std::fs::read_to_string(root.join(file)).unwrap());
        assert!(text.contains(WARRANTY_EN), "englisch fehlt in {file}");
        assert!(text.contains(WARRANTY_DE), "deutsch fehlt in {file}");
    }
    let license = plain(&std::fs::read_to_string(root.join("LICENSE")).unwrap());
    assert!(!license.contains(&plain(WARRANTY_EN)));
    // Windows: briefly tried (2026-11-20), said the same way everywhere.
    for file in [
        "README.md",
        "assets/windows/LIESMICH.txt",
        "docs/handbuch/handbuch.typ",
    ] {
        let text = plain(&std::fs::read_to_string(root.join(file)).unwrap());
        assert!(
            text.contains(
                "Unter Windows 11 nur kurz erprobt (Start, Projekt laden); nicht umfassend getestet."
            ),
            "Windows-Hinweis fehlt in {file}"
        );
    }
}
