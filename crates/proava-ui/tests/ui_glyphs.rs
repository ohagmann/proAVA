//! Wächter: jedes Zeichen, das die Oberfläche anzeigen kann, gibt es in den
//! Schriften der App. Sonst zeigt egui ein leeres Kästchen („Tofu“) – und in
//! Tests mit egui_kittest bricht es ab. Gefunden mit dem Affentest: „▸“ in
//! Hinweisen („LV ▸ Neu nummerieren“) fehlt in Liberation Sans.
//!
//! Geprüft werden die Zeichenketten im Quelltext der Oberfläche und der
//! Schichten, deren Meldungen sie zeigt (ohne Kommentare und Testmodule).

use std::path::{Path, PathBuf};

use egui::{FontFamily, FontId};
use proava_ui::App;

/// Characters inside string literals of `source`, with their line.
fn literal_chars(source: &str) -> Vec<(usize, char)> {
    let chars: Vec<char> = source.chars().collect();
    let mut found = Vec::new();
    let mut line = 1;
    let mut index = 0;
    let mut in_string = false;
    while index < chars.len() {
        let character = chars[index];
        if character == '\n' {
            line += 1;
        }
        if in_string {
            match character {
                '\\' => {
                    // An escaped line break (continued literal) is a line too.
                    if chars.get(index + 1) == Some(&'\n') {
                        line += 1;
                    }
                    index += 1;
                }
                '"' => in_string = false,
                _ => found.push((line, character)),
            }
            index += 1;
            continue;
        }
        match character {
            '/' if chars.get(index + 1) == Some(&'/') => {
                while index < chars.len() && chars[index] != '\n' {
                    index += 1;
                }
                continue;
            }
            '/' if chars.get(index + 1) == Some(&'*') => {
                while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                    if chars[index] == '\n' {
                        line += 1;
                    }
                    index += 1;
                }
                index += 2;
                continue;
            }
            // Char literals ('"', '\'') – not lifetimes ('a).
            '\'' if chars.get(index + 2) == Some(&'\'') => index += 3,
            '\'' if chars.get(index + 1) == Some(&'\\') => {
                while index + 1 < chars.len() && chars[index + 1] != '\'' {
                    index += 1;
                }
                index += 2;
            }
            '"' => {
                in_string = true;
                index += 1;
            }
            _ => index += 1,
        }
    }
    found
}

fn sources(folder: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_character_of_the_interface_has_a_glyph() {
    let ctx = egui::Context::default();
    App::setup(&ctx);
    // Fonts take effect with the next pass.
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();

    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for name in [
        "proava-ui",
        "proava-domain",
        "proava-application",
        "proava-exchange",
        "proava-infrastructure",
    ] {
        sources(&crates.join(name).join("src"), &mut files);
    }
    files.sort();
    let mut missing = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).unwrap();
        // Test modules show nothing.
        let source = source.split("#[cfg(test)]").next().unwrap_or_default();
        for (line, character) in literal_chars(source) {
            if character.is_ascii() || character.is_control() {
                continue;
            }
            let drawable = ctx.fonts_mut(|fonts| {
                fonts.has_glyph(&FontId::new(12.0, FontFamily::Proportional), character)
            });
            if !drawable {
                missing.push(format!(
                    "„{character}“ (U+{:04X}) in {}:{line}",
                    character as u32,
                    file.strip_prefix(crates).unwrap_or(file).display()
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "Zeichen ohne Schrift (Kästchen in der Oberfläche) – für Menüpfade „›“ statt „▸“ schreiben:\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_scanner_finds_literals_but_not_comments() {
    let source =
        "// „▸“ im Kommentar\nlet a = \"LV ▸ Prüfen\"; let c = '\"'; let d = \"x\\\"y•\";\n/* ▶ */";
    let found: String = literal_chars(source)
        .into_iter()
        .map(|(_, character)| character)
        .collect();
    // Escapes (quote, backslash, line break) are ASCII and left out.
    assert_eq!(found, "LV ▸ Prüfenxy•");
    let continued = literal_chars("let a = \"eins \\\n  zwei ▸\";");
    assert_eq!(
        continued.last(),
        Some(&(2, '▸')),
        "Zeile nach der Fortsetzung"
    );
}
