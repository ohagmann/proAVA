//! Wächter: Symbole nur als übliche Bedien-Symbole an Knöpfen.
//!
//! Anwender, 17.11.2026 21:02: „diese Symbole wie der Stift gehen nicht … wir
//! bauen ja Expertensoftware“ – gemeint waren die verspielten Bildchen. Seine
//! Klarstellung 22:19: Standardsymbole wie in Word (Ausrichtung, Aufzählung,
//! Tabelle, Bild, Pfeile, Lupe, Zahnrad, Sonne/Mond …) bleiben, „die kennt
//! jeder und sie sind nicht verspielt … die helfen ja bei der Bedienung“.
//!
//! Erlaubt: die einfarbigen Linienzeichnungen aus `icons.rs` (Liste unten) an
//! Knöpfen mit Tooltip, dazu Klappdreieck, Sortierpfeil, Häkchenfeld und
//! Aufklapp-Pfeil einer Auswahl. Verboten bleiben: Emojis und Symbolzeichen
//! in Texten, Knöpfe mit einem Zeichen statt Wort („×“, „↑“ – dafür das
//! gezeichnete Symbol mit Tooltip), gezeichnete Status-Bildchen in
//! Tabellenzellen (Stift/Haken – Status steht als Text „fertig“/„in Bearb.“)
//! und bunte Piktogramme (Symbole haben keine eigene Farbe).
//!
//! Der Test liest alle Quellen unter `crates/proava-ui/src` und schlägt fehl,
//! wenn dort etwas davon auftaucht – damit fällt es auch nach einem Merge
//! anderer Zweige auf. Einen anderen Stand prüfen, ohne ihn zu mergen:
//! `PROAVA_UI_SRC=/pfad/zum/worktree/crates/proava-ui/src cargo test -p proava-ui --test ui_symbols`

use std::path::{Path, PathBuf};

/// `Icon::…`-Arten, die es geben darf (Enum in `icons.rs`): übliche
/// Bedien-Symbole, jedes an einem Knopf mit Tooltip.
const ALLOWED_ICONS: &[&str] = &[
    "Back",         // Kopfleiste: zurück zur Projektliste
    "Forward",      // Sprung, z. B. zur Ausführungsbeschreibung
    "Up",           // nach oben verschieben, übergeordneter Ordner
    "Down",         // nach unten verschieben
    "Plus",         // neu, hinzufügen
    "Close",        // leeren, schließen
    "Trash",        // entfernen
    "Search",       // Suche, Befehlspalette
    "Gear",         // Einstellungen
    "Sun",          // helles Aussehen
    "Moon",         // dunkles Aussehen
    "Document",     // Projekt, Datei
    "Folder",       // öffnen, Ordner
    "NewFolder",    // Dateidialog: Neuer Ordner (Anwender 18.11.: Ordner mit Plus)
    "Import",       // Aus GAEB
    "Save",         // Sicherung anlegen
    "Restore",      // Sicherung wiederherstellen
    "AlignLeft",    // Langtext: linksbündig
    "AlignCenter",  // Langtext: zentriert
    "AlignRight",   // Langtext: rechtsbündig
    "AlignJustify", // Langtext: Blocksatz
    "Bullets",      // Langtext: Aufzählung
    "Numbering",    // Langtext: Nummerierung
    "Table",        // Langtext: Tabelle
    "Image",        // Langtext: Bild
    "Calendar",     // Knopf neben Datumsfeldern
    "Field",        // Langtext: Bieterangabe einfügen (Eingabefeld wie in Word)
    "FieldBelow",   // Langtext: Bieterangabe unter dem Langtext
    "Link",         // Verknüpfung, z. B. zur Ausführungsbeschreibung
];

/// Gezeichnete Formen (Dreiecke, Kreise, Kurven) nur hier, je Datei höchstens
/// so oft. `icons.rs` ist der Ort für die erlaubten Symbole.
const ALLOWED_SHAPES: &[(&str, usize, &str)] = &[
    ("icons.rs", usize::MAX, "erlaubte Symbole und Klappdreieck"),
    ("bids_view.rs", 1, "Sortierpfeil im Spaltenkopf"),
    (
        "lv_list.rs",
        1,
        "Aufklapp-Pfeil der LV-Auswahl (wie ComboBox)",
    ),
];

const SHAPE_CALLS: &[&str] = &[
    "convex_polygon",
    "circle_filled",
    "circle_stroke",
    "PathShape",
    "CubicBezierShape",
    "QuadraticBezierShape",
    "Shape::Path",
    "Shape::Circle",
];

/// Bezeichner, die nicht wiederkommen sollen (Symbolknöpfe, Symbolschriften).
const FORBIDDEN_WORDS: &[&str] = &[
    "special_emojis",
    "egui_phosphor",
    "egui_material_icons",
    "include_image!",
];

/// Knöpfe, deren Beschriftung nur aus Zeichen ohne Buchstaben/Ziffern
/// besteht („×“, „→“, „↑“, „“), sind Zeichen-Knöpfe ohne Tooltip-Zwang:
/// stattdessen `widgets::tool_button` mit dem gezeichneten Symbol.
const BUTTON_CALLS: &[&str] = &[
    "button(\"",
    "Button::new(\"",
    "small_button(\"",
    "selectable_label(true, \"",
    "selectable_label(false, \"",
    "Button::new(RichText::new(\"",
    "Button::new(egui::RichText::new(\"",
];

/// Hier entstehen die Symbolknöpfe (`tool_button`): leerer Knopftext, das
/// Symbol wird darübergezeichnet, der Tooltip ist Pflicht.
const SYMBOL_BUTTON_FILES: &[&str] = &["widgets.rs"];

/// Bilddateien einbetten nur für das Programmsymbol im Info-Dialog.
const ALLOWED_IMAGE_FILES: &[&str] = &["about.rs"];

/// Zeichen, die als Symbol gelten. Pfeile (U+2190–21FF) bleiben erlaubt: sie
/// stehen in Tastennamen („Alt+↑“) und in „alt → neu“. Erlaubt ist auch „▸“
/// als Trenner von Menüpfaden („LV ▸ Prüfen“) und „•“ als Listenpunkt.
fn is_symbol(c: char) -> bool {
    if c == '▸' {
        return false;
    }
    matches!(c as u32,
        0x2300..=0x23FF   // Technisches (⌘ ⌫ ⏎ ⏱ …)
        | 0x2460..=0x24FF // Eingekreistes (ⓘ ①)
        | 0x25A0..=0x25FF // Geometrisches (● ■ ▲ ◆)
        | 0x2600..=0x26FF // Verschiedene Symbole (☀ ☾ ⚙ ⚠)
        | 0x2700..=0x27BF // Dingbats (✓ ✔ ✎ ✗)
        | 0x2B00..=0x2BFF // Pfeile/Symbole (⬆ ⭐)
        | 0x1F000..=0x1FAFF // Emojis
        | 0xFE0F          // Emoji-Darstellung
    )
}

fn findings(file: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut shapes = 0;
    for (number, line) in text.lines().enumerate() {
        let at = format!("{file}:{}", number + 1);
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        if let Some(c) = code.chars().find(|&c| is_symbol(c)) {
            found.push(format!(
                "{at}: Symbolzeichen „{c}“ (U+{:04X}) – Text oder Symbol aus icons.rs",
                c as u32
            ));
        }
        for word in FORBIDDEN_WORDS {
            if code.contains(word) {
                found.push(format!(
                    "{at}: „{word}“ – keine Symbolschrift, Symbole aus icons.rs"
                ));
            }
        }
        let mut rest = code;
        while let Some(start) = rest.find("Icon::") {
            let before = &rest[..start];
            let after = &rest[start + "Icon::".len()..];
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !before.ends_with("Cursor") && !ALLOWED_ICONS.contains(&name.as_str()) {
                found.push(format!(
                    "{at}: Icon::{name} – nicht erlaubt (nur {})",
                    ALLOWED_ICONS.join(", ")
                ));
            }
            rest = after;
        }
        let mut labels = std::collections::BTreeSet::new();
        for call in BUTTON_CALLS {
            for (start, _) in code.match_indices(call) {
                labels.insert(start + call.len());
            }
        }
        for start in labels {
            let label: String = code[start..].chars().take_while(|c| *c != '"').collect();
            if !label.chars().any(char::is_alphanumeric)
                && !(label.is_empty() && SYMBOL_BUTTON_FILES.contains(&file_name(file)))
            {
                found.push(format!(
                    "{at}: Knopf „{label}“ ohne Text – Wort oder widgets::tool_button mit Tooltip"
                ));
            }
        }
        shapes += SHAPE_CALLS
            .iter()
            .filter(|call| code.contains(*call))
            .count();
        let embeds_image = code.contains("include_bytes!")
            && [".png", ".svg", ".jpg", ".jpeg", ".ico", ".gif", ".webp"]
                .iter()
                .any(|ending| code.to_lowercase().contains(ending));
        if embeds_image && !ALLOWED_IMAGE_FILES.contains(&file_name(file)) {
            found.push(format!(
                "{at}: eingebettete Bilddatei – nur das Programmsymbol in about.rs"
            ));
        }
    }
    let allowed = ALLOWED_SHAPES
        .iter()
        .find(|(name, _, _)| *name == file_name(file))
        .map_or(0, |(_, count, _)| *count);
    if shapes > allowed {
        found.push(format!(
            "{file}: {shapes} gezeichnete Form(en) (Dreieck/Kreis/Kurve), erlaubt {allowed} – \
             Status/Markierungen als Text, Symbole nur in icons.rs"
        ));
    }
    found
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn sources(dir: &Path, list: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("Quellordner lesbar")
        .map(|entry| entry.expect("Eintrag").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            sources(&path, list);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            list.push(path);
        }
    }
}

#[test]
fn no_pictograms_in_the_user_interface() {
    let root = std::env::var_os("PROAVA_UI_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    let mut files = Vec::new();
    sources(&root, &mut files);
    assert!(files.len() > 20, "Quellen gefunden: {}", files.len());
    let mut found = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("Quelle lesbar");
        let shown = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        found.extend(findings(&shown, &text));
    }
    assert!(
        found.is_empty(),
        "Symbole außerhalb der Regel (erlaubt: Bedien-Symbole aus icons.rs an Knöpfen mit \
         Tooltip; verboten: Emojis, Zeichen-Knöpfe, Status-Bildchen, bunte Piktogramme):\n{}",
        found.join("\n")
    );
}

#[test]
fn only_allowed_icon_kinds_exist() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/icons.rs");
    let text = std::fs::read_to_string(path).expect("icons.rs");
    let body = text
        .split("pub enum Icon {")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .expect("enum Icon in icons.rs");
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with("#[") {
            continue;
        }
        let name = line
            .trim_end_matches(',')
            .split(['(', ' '])
            .next()
            .unwrap_or(line);
        assert!(
            ALLOWED_ICONS.contains(&name),
            "Icon::{name} ist nicht auf der Erlaubnisliste ({}) – erst abstimmen, sonst Text",
            ALLOWED_ICONS.join(", ")
        );
    }
}

/// Bunte Piktogramme: die Symbole zeichnen nur in der Farbe, die der Knopf
/// übergibt (Textfarbe des Aussehens), nie in einer eigenen.
#[test]
fn symbols_have_no_colour_of_their_own() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/icons.rs");
    let text = std::fs::read_to_string(path).expect("icons.rs");
    for (number, line) in text.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        let fixed = code.match_indices("Color32::").any(|(start, _)| {
            let rest = &code[start + "Color32::".len()..];
            rest.starts_with("from") || rest.starts_with(|c: char| c.is_ascii_uppercase())
        });
        assert!(
            !fixed,
            "icons.rs:{}: feste Farbe im Symbol – nur `color`/`backdrop`",
            number + 1
        );
    }
}

#[test]
fn the_watch_finds_what_it_should() {
    let sample = "\
        let ok = Icon::Gear;\n\
        let cursor = egui::CursorIcon::PointingHand;\n\
        // Kommentar mit ✓ zählt nicht\n\
        let pencil = Icon::Pencil;\n\
        ui.button(\"✓ fertig\");\n\
        widgets::tool_button(ui, palette, Icon::AlignLeft, false, \"Linksbündig\");\n\
        painter.circle_filled(center, 3.0, color);\n\
        let key = \"Alt+↑\"; let path = \"LV ▸ Prüfen\";\n\
        if ui.small_button(\"×\").clicked() {}\n\
        ui.add(egui::Button::new(\"↑\").small());\n\
        ui.add(egui::Button::new(\"Nach oben\").small());\n\
        let font = egui_phosphor::regular::PENCIL;\n";
    let found = findings("lv_editor.rs", sample);
    let text = found.join("\n");
    assert!(text.contains("Icon::Pencil"), "{text}");
    assert!(text.contains("„✓“"), "{text}");
    assert!(text.contains("egui_phosphor"), "{text}");
    assert!(
        text.contains("gezeichnete Form"),
        "Status-Bildchen in der Tabelle: {text}"
    );
    assert!(
        !text.contains("Icon::Gear") && !text.contains("CursorIcon"),
        "{text}"
    );
    assert!(
        !text.contains("lv_editor.rs:6:"),
        "Bedien-Symbol am Knopf erlaubt: {text}"
    );
    assert!(
        !text.contains("lv_editor.rs:8:"),
        "Pfeil in Tastennamen und ▸ erlaubt: {text}"
    );
    assert!(
        text.contains("Knopf „×“") && text.contains("Knopf „↑“"),
        "{text}"
    );
    assert!(
        !text.contains("lv_editor.rs:11:"),
        "Knopf mit Text ist erlaubt: {text}"
    );
    assert_eq!(found.len(), 6, "{text}");
    assert!(
        findings(
            "widgets.rs",
            "let response = ui.add(egui::Button::new(\"\").selected(on));"
        )
        .is_empty(),
        "widgets.rs baut die Symbolknöpfe (leerer Text, Symbol darüber, Tooltip Pflicht)"
    );
    assert!(findings(
        "bids_view.rs",
        "painter.add(Shape::convex_polygon(p, c, s));"
    )
    .is_empty());
}
