//! Eigenschaftstests „kaputte Eingaben“: beschädigte Projektdateien (Zip
//! abgeschnitten, Bytes gekippt, kaputtes XML, falsche Zahlen) und
//! beschädigte GAEB-Dateien (X83, X84, D83: Bytes vertauscht, abgeschnitten,
//! falsche Zahlen) dürfen nie abstürzen – nur eine klare Fehlermeldung oder
//! ein (teilweise) gelesenes Projekt.
//!
//! Ausgangspunkt sind gültige Zufallsprojekte (siehe `zufall`); feste
//! Zufallssaat, längere Läufe siehe docs/BUILD.md.

mod zufall;

use proava_domain::{Project, ProjectName};
use proava_exchange::{gaeb_to_project, lv_to_gaeb, match_bid};
use proava_gaeb::Phase;
use proava_infrastructure::project_xml::write_project_xml;
use proava_infrastructure::{project_from_bytes, project_to_bytes};
use proptest::prelude::*;
use proptest::sample::Index;
use zufall::{config, project, Alphabet, Levels};

/// One change to a file's bytes.
#[derive(Debug, Clone)]
enum Damage {
    /// Cut off from here on.
    Truncate(Index),
    /// One byte changed.
    Flip(Index, u8),
    /// Two bytes swapped.
    Swap(Index, Index),
    /// A run of bytes gone.
    Remove(Index, usize),
    /// A run of bytes repeated.
    Repeat(Index, usize),
}

fn damage() -> impl Strategy<Value = Damage> {
    let i = || any::<Index>();
    prop_oneof![
        1 => i().prop_map(Damage::Truncate),
        3 => (i(), any::<u8>()).prop_map(|(at, byte)| Damage::Flip(at, byte)),
        2 => (i(), i()).prop_map(|(a, b)| Damage::Swap(a, b)),
        1 => (i(), 1usize..64).prop_map(|(at, length)| Damage::Remove(at, length)),
        1 => (i(), 1usize..64).prop_map(|(at, length)| Damage::Repeat(at, length)),
    ]
}

fn apply(bytes: &mut Vec<u8>, damage: &Damage) {
    if bytes.is_empty() {
        return;
    }
    let len = bytes.len();
    match damage {
        Damage::Truncate(at) => bytes.truncate(at.index(len)),
        Damage::Flip(at, byte) => bytes[at.index(len)] = *byte,
        Damage::Swap(a, b) => bytes.swap(a.index(len), b.index(len)),
        Damage::Remove(at, length) => {
            let start = at.index(len);
            bytes.drain(start..(start + length).min(len));
        }
        Damage::Repeat(at, length) => {
            let start = at.index(len);
            let piece: Vec<u8> = bytes[start..(start + length).min(len)].to_vec();
            bytes.splice(start..start, piece);
        }
    }
}

/// Values that are no numbers, or none proAVA can use.
const BAD_NUMBERS: &[&str] = &[
    "",
    " ",
    "x",
    "-",
    "-1",
    "-0",
    "0",
    "1e999",
    "-1e999",
    "NaN",
    "inf",
    "1,5",
    "0x10",
    "1.2.3",
    "99999999999999999999999999999",
    "4294967296",
    "18446744073709551616",
    "١٢",
    "½",
    "\u{a0}1",
];

/// Replaces the `nth` number in a text (an attribute value, an element
/// content or a fixed GAEB 90 column) with a bad one.
fn spoil_number(text: &str, nth: &Index, bad: &str) -> String {
    let numbers: Vec<(usize, usize)> = {
        let bytes = text.as_bytes();
        let mut found = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index].is_ascii_digit() {
                let start = index;
                while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.')
                {
                    index += 1;
                }
                found.push((start, index));
            } else {
                index += 1;
            }
        }
        found
    };
    if numbers.is_empty() {
        return text.to_string();
    }
    let (start, end) = numbers[nth.index(numbers.len())];
    format!("{}{bad}{}", &text[..start], &text[end..])
}

/// Text-level damage of an XML file: cut at a character, a tag broken,
/// a number spoiled.
#[derive(Debug, Clone)]
enum XmlDamage {
    Cut(Index),
    Number(Index, usize),
    DropChar(Index, char),
    Insert(Index, &'static str),
}

fn xml_damage() -> impl Strategy<Value = XmlDamage> {
    prop_oneof![
        1 => any::<Index>().prop_map(XmlDamage::Cut),
        4 => (any::<Index>(), 0..BAD_NUMBERS.len()).prop_map(|(at, bad)| XmlDamage::Number(at, bad)),
        2 => (any::<Index>(), prop::sample::select(vec!['<', '>', '"', '/', '='])).prop_map(|(at, character)| XmlDamage::DropChar(at, character)),
        2 => (any::<Index>(), prop::sample::select(vec!["<", ">", "&", "\"", "<x>", "</lv>", "<!--", "]]>", "&#0;", "&unbekannt;", "\u{0}"]))
            .prop_map(|(at, text)| XmlDamage::Insert(at, text)),
    ]
}

fn apply_xml(text: &str, damage: &XmlDamage) -> String {
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(at, _)| at)
        .chain([text.len()])
        .collect();
    let at = |index: &Index| boundaries[index.index(boundaries.len())];
    match damage {
        XmlDamage::Cut(index) => text[..at(index)].to_string(),
        XmlDamage::Number(index, bad) => spoil_number(text, index, BAD_NUMBERS[*bad]),
        XmlDamage::DropChar(index, character) => {
            let places: Vec<usize> = text.match_indices(*character).map(|(at, _)| at).collect();
            if places.is_empty() {
                return text.to_string();
            }
            let place = places[index.index(places.len())];
            format!(
                "{}{}",
                &text[..place],
                &text[place + character.len_utf8()..]
            )
        }
        XmlDamage::Insert(index, piece) => {
            let place = at(index);
            format!("{}{piece}{}", &text[..place], &text[place..])
        }
    }
}

/// With `PROAVA_PROPTEST_STATS` set: what came of a damaged file, so a
/// long run shows that the readers are really reached.
fn stat(what: &str, result: &Result<impl Sized, String>) {
    if std::env::var_os("PROAVA_PROPTEST_STATS").is_some() {
        match result {
            Ok(_) => eprintln!("STAT {what} gelesen"),
            Err(error) => eprintln!(
                "STAT {what} Fehler: {}",
                error.chars().take(60).collect::<String>()
            ),
        }
    }
}

/// An error says what is wrong in German first; library details (English)
/// may follow, but not lead („invalid Zip archive …“ alone is no message).
fn clear_message(error: &str) -> Result<(), TestCaseError> {
    let first = error.trim_start().chars().next();
    prop_assert!(
        first.is_some_and(|first| !first.is_ascii_lowercase()),
        "unklare Fehlermeldung: {error:?}"
    );
    Ok(())
}

fn clear(result: Result<Project, String>) -> Result<(), TestCaseError> {
    match result {
        Err(error) => clear_message(&error),
        Ok(_) => Ok(()),
    }
}

/// A bidder's file for `project`: the priced LV as X84.
fn bid_bytes(project: &Project) -> Vec<u8> {
    let (mut document, _) = lv_to_gaeb(project, Phase::Dp84);
    document.phase = Phase::Dp84;
    proava_gaeb::xml::write(&document, Phase::Dp84)
        .map(String::into_bytes)
        .unwrap_or_default()
}

/// Reads a (damaged) GAEB file like the program does: as a new project and
/// as a bid against `project`. Nothing may panic.
fn read_gaeb(project: &Project, bytes: &[u8], datan: bool) -> Result<(), TestCaseError> {
    let document = if datan {
        proava_gaeb::datan::read(bytes)
            .map(|report| report.document)
            .map_err(|error| error.to_string())
    } else {
        proava_gaeb::xml::read(bytes).map_err(|error| error.to_string())
    };
    stat(if datan { "GAEB 90" } else { "GAEB XML" }, &document);
    match document {
        Ok(document) => {
            let (imported, _) =
                gaeb_to_project(&document, ProjectName::parse("Kaputt").unwrap(), "");
            // What came in must also go out again.
            let _ = lv_to_gaeb(&imported, Phase::Dp83);
            let _ = project_to_bytes(&imported);
            let _ = match_bid(project, &document, "kaputt.X84");
            Ok(())
        }
        Err(error) => clear_message(&error),
    }
}

fn damaged_project_xml(project: &Project, damages: &[XmlDamage]) -> Result<(), TestCaseError> {
    let mut xml = write_project_xml(project).map_err(TestCaseError::fail)?;
    for damage in damages {
        xml = apply_xml(&xml, damage);
    }
    // Schema 1 (plain XML) takes the same reader as the zip's projekt.xml.
    let loaded = project_from_bytes(xml.as_bytes());
    stat("Projekt-XML", &loaded);
    if let Ok(loaded) = &loaded {
        // A project that loads must also save again.
        project_to_bytes(loaded).map_err(TestCaseError::fail)?;
    }
    clear(loaded)
}

proptest! {
    #![proptest_config(config(300))]

    /// Teurer Lauf (`cargo test -- --ignored lang_`): große Projekte, viele Schäden.
    #[test]
    #[ignore]
    fn lang_heavily_damaged_big_projects_never_crash(
        project in project(Alphabet::All, Levels::Free, 3, 80),
        damages in prop::collection::vec(xml_damage(), 1..30),
    ) {
        damaged_project_xml(&project, &damages)?;
    }
}

proptest! {
    #![proptest_config(config(64))]

    #[test]
    fn damaged_project_files_never_crash(
        project in project(Alphabet::All, Levels::Free, 2, 12),
        damages in prop::collection::vec(damage(), 1..5),
    ) {
        let mut bytes = project_to_bytes(&project).map_err(TestCaseError::fail)?;
        for damage in &damages {
            apply(&mut bytes, damage);
        }
        let loaded = project_from_bytes(&bytes);
        stat("Projekt-Zip", &loaded);
        clear(loaded)?;
    }

    #[test]
    fn damaged_project_xml_never_crashes(
        project in project(Alphabet::All, Levels::Free, 2, 12),
        damages in prop::collection::vec(xml_damage(), 1..5),
    ) {
        damaged_project_xml(&project, &damages)?;
    }

    #[test]
    fn damaged_gaeb_files_never_crash(
        project in project(Alphabet::Printable, Levels::Gaeb, 1, 12),
        damages in prop::collection::vec(damage(), 1..5),
        xml_damages in prop::collection::vec(xml_damage(), 1..4),
        format in 0u8..3,
    ) {
        let (document, _) = lv_to_gaeb(&project, Phase::Dp83);
        match format {
            0 => {
                let xml = proava_gaeb::xml::write(&document, Phase::Dp83).map_err(|error| TestCaseError::fail(error.to_string()))?;
                let mut bytes = xml.clone().into_bytes();
                for damage in &damages {
                    apply(&mut bytes, damage);
                }
                read_gaeb(&project, &bytes, false)?;
                let mut text = xml;
                for damage in &xml_damages {
                    text = apply_xml(&text, damage);
                }
                read_gaeb(&project, text.as_bytes(), false)?;
            }
            1 => {
                let mut bytes = bid_bytes(&project);
                for damage in &damages {
                    apply(&mut bytes, damage);
                }
                read_gaeb(&project, &bytes, false)?;
            }
            _ => {
                // GAEB 90: fixed columns – spoil bytes and numbers.
                let Ok(text) = proava_gaeb::datan::write_text(&document) else {
                    return Ok(());
                };
                let mut bytes = proava_gaeb::datan::encode(&text, proava_gaeb::datan::Encoding::Cp850);
                for damage in &damages {
                    apply(&mut bytes, damage);
                }
                read_gaeb(&project, &bytes, true)?;
                let mut spoiled = text;
                for damage in &xml_damages {
                    if let XmlDamage::Number(at, bad) = damage {
                        spoiled = spoil_number(&spoiled, at, BAD_NUMBERS[*bad]);
                    }
                }
                read_gaeb(&project, spoiled.as_bytes(), true)?;
            }
        }
    }
}
