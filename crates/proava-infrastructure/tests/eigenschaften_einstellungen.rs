//! Eigenschaftstest: eine beschädigte oder von Hand verdorbene
//! `settings.json` (abgeschnitten, Bytes gekippt, falsche Zahlen wie
//! „1e39“, „-5“, „1e999“) darf nie abstürzen. Was geladen wird, ist
//! brauchbar: Größen endlich und positiv, Einstellungen ohne Befund.
//!
//! Feste Zufallssaat; längere Läufe siehe docs/BUILD.md.

use std::path::PathBuf;

use proava_application::{RecentProject, SettingsFiles};
use proava_infrastructure::JsonSettingsFile;
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};

fn config(default_cases: u32) -> Config {
    let cases = std::env::var("PROAVA_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default_cases);
    let seed = std::env::var("PROAVA_PROPTEST_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0x0A7A_2026);
    Config {
        cases,
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..Config::default()
    }
}

/// `settings.json` in a folder only this test uses (unique, created
/// exclusively); the folder goes with this value, nothing else.
struct TempFile(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempFile {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("proava-einstellungen-")
            .tempdir()
            .unwrap();
        Self(dir.path().join("settings.json"), dir)
    }
}

/// A full, valid settings file as the program writes it.
fn valid_file() -> String {
    let file = TempFile::new();
    let store = JsonSettingsFile::at(file.0.clone());
    let mut settings = store.load();
    settings.recent.push(RecentProject {
        name: "Schulzentrum \"Park\"".into(),
        path: PathBuf::from("/tmp/proava/robust/Projekt.proava"),
        opened_at: "2026-11-17T21:00:00".into(),
        kind: proava_application::ProjectKind::ProAva,
    });
    settings.detail_width = Some(420.0);
    settings.long_text_window = Some((1000.0, 760.0));
    settings.column_widths.insert("oz".into(), 120.0);
    settings.column_widths.insert("ep".into(), 96.5);
    settings.print.insert("lv".into(), "a4;quer".into());
    settings.transfer_sources.push(PathBuf::from("/tmp/x.X83"));
    store.save(&settings);
    std::fs::read_to_string(&file.0).unwrap()
}

const BAD_NUMBERS: &[&str] = &[
    "-5",
    "0",
    "-0",
    "1e39",
    "-1e39",
    "1e999",
    "3.5e38",
    "1e-50",
    "99999999999",
    "4294967296",
    "-1",
    "0.0001",
    "\"12\"",
    "null",
    "true",
    "[]",
    "{}",
];

#[derive(Debug, Clone)]
enum Damage {
    Cut(Index),
    Number(Index, usize),
    Byte(Index, u8),
}

fn damage() -> impl Strategy<Value = Damage> {
    prop_oneof![
        1 => any::<Index>().prop_map(Damage::Cut),
        4 => (any::<Index>(), 0..BAD_NUMBERS.len()).prop_map(|(at, bad)| Damage::Number(at, bad)),
        1 => (any::<Index>(), any::<u8>()).prop_map(|(at, byte)| Damage::Byte(at, byte)),
    ]
}

fn apply(bytes: &mut Vec<u8>, damage: &Damage) {
    if bytes.is_empty() {
        return;
    }
    match damage {
        Damage::Cut(at) => bytes.truncate(at.index(bytes.len())),
        Damage::Byte(at, byte) => {
            let at = at.index(bytes.len());
            bytes[at] = *byte;
        }
        Damage::Number(nth, bad) => {
            let mut numbers = Vec::new();
            let mut index = 0;
            while index < bytes.len() {
                if bytes[index].is_ascii_digit() {
                    let start = index;
                    while index < bytes.len()
                        && (bytes[index].is_ascii_digit() || bytes[index] == b'.')
                    {
                        index += 1;
                    }
                    numbers.push((start, index));
                } else {
                    index += 1;
                }
            }
            if let Some(&(start, end)) =
                (!numbers.is_empty()).then(|| &numbers[nth.index(numbers.len())])
            {
                bytes.splice(start..end, BAD_NUMBERS[*bad].bytes());
            }
        }
    }
}

fn sane(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

proptest! {
    #![proptest_config(config(256))]

    #[test]
    fn damaged_settings_load_usable_values(damages in prop::collection::vec(damage(), 1..4)) {
        let valid = valid_file();
        let mut bytes = valid.clone().into_bytes();
        for damage in &damages {
            apply(&mut bytes, damage);
        }
        // Whether „Zuletzt verwendet“ came through the damage unchanged.
        let recent_of = |text: &[u8]| {
            serde_json::from_slice::<serde_json::Value>(text)
                .ok()
                .and_then(|value| value.get("recent").cloned())
        };
        let recent_intact = recent_of(&bytes).is_some() && recent_of(&bytes) == recent_of(valid.as_bytes());
        let file = TempFile::new();
        std::fs::write(&file.0, &bytes).unwrap();
        let store = JsonSettingsFile::at(file.0.clone());
        let settings = store.load();
        prop_assert!(settings.detail_width.is_none_or(sane), "{:?}", settings.detail_width);
        prop_assert!(
            settings.long_text_window.is_none_or(|(width, height)| sane(width) && sane(height)),
            "{:?}",
            settings.long_text_window
        );
        prop_assert!(settings.column_widths.values().all(|width| sane(*width)), "{:?}", settings.column_widths);
        prop_assert!(settings.preferences.problems().is_empty(), "{:?}", settings.preferences.problems());
        // Only what is damaged falls back: an intact list stays.
        if recent_intact {
            prop_assert_eq!(settings.recent.len(), 1, "Zuletzt verwendet verloren: {:?}", settings.notes);
        }
        // Repairing is said (an intact file says nothing).
        if bytes != valid.as_bytes() && settings.notes.is_empty() {
            prop_assert!(
                serde_json::from_slice::<serde_json::Value>(&bytes).is_ok(),
                "beschädigt, aber kein Hinweis: {}",
                String::from_utf8_lossy(&bytes)
            );
        }
        // What was loaded can be saved and loaded again.
        store.save(&settings);
        let again = store.load();
        prop_assert_eq!(again.preferences, settings.preferences);
        prop_assert_eq!(again.recent.len(), settings.recent.len());
    }
}

/// Gefunden mit diesem Test: „1e39“ wird als f32 unendlich, negative
/// Breiten kamen ungeprüft bis zur Fenstergröße des Langtextfensters.
#[test]
fn infinite_and_negative_sizes_are_dropped() {
    let file = TempFile::new();
    std::fs::write(
        &file.0,
        r#"{"detail_width":1e39,"long_text_window":[-5,760],"column_widths":{"oz":1e39,"ep":96.5}}"#,
    )
    .unwrap();
    let settings = JsonSettingsFile::at(file.0.clone()).load();
    assert_eq!(settings.detail_width, None);
    assert_eq!(settings.long_text_window, None);
    assert_eq!(settings.column_widths.get("oz"), None);
    assert_eq!(settings.column_widths.get("ep"), Some(&96.5));
}
