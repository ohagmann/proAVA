//! Tool list with JSON schemas and short German descriptions.
//!
//! Everything that holds for many tools (writing, `lv`, rows, places,
//! numbering, Langtext markup, `bestaetigen`, windows) is said once in the
//! server instructions (`server.rs`); a description here says what the tool
//! does and what a model would otherwise get wrong. Parameters without a
//! description explain themselves or are covered there. `docs/MCP.md` is the
//! long version for people.

use serde_json::{json, Map, Value};

fn tool(
    name: &str,
    description: String,
    properties: Value,
    required: &[&str],
    read_only: bool,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
        "annotations": {"readOnlyHint": read_only, "destructiveHint": !read_only && name.ends_with("loeschen"), "openWorldHint": false},
    })
}

/// Marks a tool that reaches the internet (MCP `openWorldHint`).
fn open_world(mut tool: Value) -> Value {
    tool["annotations"]["openWorldHint"] = json!(true);
    tool
}

fn props(entries: &[(&str, Value)]) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert((*key).to_string(), value.clone());
    }
    Value::Object(map)
}

/// Adds the description, unless it is empty (then the name says enough).
fn described(mut value: Value, description: &str) -> Value {
    if !description.is_empty() {
        value["description"] = json!(description);
    }
    value
}

fn text(description: &str) -> Value {
    described(json!({"type": "string"}), description)
}

fn flag(description: &str) -> Value {
    described(json!({"type": "boolean"}), description)
}

fn count(description: &str) -> Value {
    described(json!({"type": "integer", "minimum": 0}), description)
}

fn number(description: &str) -> Value {
    described(json!({"type": ["number", "string"]}), description)
}

fn project() -> (&'static str, Value) {
    ("projekt", text(""))
}

fn oz(key: &'static str, description: &str) -> (&'static str, Value) {
    (key, text(description))
}

fn row_ref() -> Vec<(&'static str, Value)> {
    vec![oz("oz", ""), ("id", count(""))]
}

fn place() -> Vec<(&'static str, Value)> {
    vec![
        oz("nach_oz", ""),
        ("nach_id", count("")),
        oz("unter_titel", ""),
        ("am_anfang", flag("")),
    ]
}

fn kind() -> (&'static str, Value) {
    (
        "art",
        json!({"type": "string", "enum": ["normal", "bedarf", "bedarf_mit_gb", "grund", "wahl", "zulage"]}),
    )
}

fn position_fields() -> Vec<(&'static str, Value)> {
    vec![
        ("kurztext", text("")),
        ("langtext", text("")),
        ("menge", number("")),
        ("einheit", text("")),
        ("ep", number("")),
        kind(),
        ("wahlgruppe", count("")),
        ("wahl_nr", count("")),
        ("pauschal", flag("")),
        ("stundenlohn", flag("")),
        ("stlb", flag("")),
        (
            "mengenermittlung",
            json!({"type": "array", "items": {"type": "object"}}),
        ),
        (
            "textergaenzungen",
            json!({"type": "array", "items": {"type": "object"}}),
        ),
        ("ep_aufgliederung", flag("")),
        (
            "ep_anteile_werte",
            json!({"type": "array", "items": {"type": ["number", "string", "null"]}}),
        ),
        (
            "zuschlag",
            json!({"type": ["object", "string", "boolean", "null"]}),
        ),
        (
            "aufteilung",
            json!({"type": ["object", "number", "boolean", "null"], "description": "nur Kosten-LV: wie zuschlag, Zahl = Prozent"}),
        ),
        ("zu_bezuschlagen", flag("")),
        ("entfaellt", flag("")),
        (
            "unterbeschreibungen",
            json!({"type": "array", "items": {"type": "object"}}),
        ),
        ("wiederholung_von", text("")),
        ("zeitansatz", json!({"type": ["number", "string", "null"]})),
        ("ausfuehrungsbeschreibung", text("")),
        ("kg", text("")),
        ("lb", text("")),
    ]
}

/// `textergaenzungen` of hints and Ausführungsbeschreibungen (as for positions).
fn complements_field() -> (&'static str, Value) {
    (
        "textergaenzungen",
        json!({"type": "array", "items": {"type": "object"}}),
    )
}

/// Address fields as in GAEB DA XML 3.3 (tgAddress).
fn address(description: &str) -> Value {
    described(
        json!({"type": "object", "properties": {
        "name1": {"type": "string"}, "name2": {"type": "string"}, "name3": {"type": "string"}, "name4": {"type": "string"},
        "strasse": {"type": "string"}, "plz": {"type": "string"}, "ort": {"type": "string"}, "land": {"type": "string"},
        "ansprechpartner": {"type": "string"}, "telefon": {"type": "string"}, "fax": {"type": "string"},
        "email": {"type": "string"}, "ust_id": {"type": "string"}, "iln": {"type": "string"}}}),
        description,
    )
}

/// Tools that work on one LV get the argument `lv`.
fn with_lv_argument(mut list: Vec<Value>) -> Vec<Value> {
    const WITHOUT: [&str; 12] = [
        "fenster_projekt_oeffnen",
        "fenster_speichern",
        "projekte_suchen",
        "lv_suchen",
        "projekt_anlegen",
        "lv_aus_gaeb",
        "lvs_lesen",
        "gaeb_importieren",
        "csv_importieren",
        "bibliothek_suchen",
        // Kostenverfolgung: das ganze Projekt (kostenverfolgung_aendern nimmt lv für angebot_wahl).
        "kostenverfolgung_lesen",
        "kostenvorgabe_setzen",
    ];
    for tool in &mut list {
        let name = tool["name"].as_str().unwrap_or_default().to_string();
        let properties = &mut tool["inputSchema"]["properties"];
        // Kostenermittlungen gelten für das ganze Projekt.
        if WITHOUT.contains(&name.as_str())
            || name == "lv_anlegen"
            || name.starts_with("kosten_")
            || properties.get("projekt").is_none()
        {
            continue;
        }
        properties["lv"] = json!({
            "type": ["string", "integer"]
        });
    }
    list
}

/// Tools that add or remove positions of an LV: with bids or an Auftrag
/// they save only with `bestaetigen` (Anwender 19.11.).
const ASK_FOR_BIDS: [&str; 7] = [
    "position_einfuegen",
    "positionen_einfuegen",
    "positionen_uebernehmen",
    "bibliothek_einfuegen",
    "stlb_uebernehmen",
    "zeile_loeschen",
    "zeile_umwandeln",
];

/// The tools of [`ASK_FOR_BIDS`] get `bestaetigen`; when it is needed says
/// the server instructions.
fn with_bids_question(mut list: Vec<Value>) -> Vec<Value> {
    for tool in &mut list {
        let name = tool["name"].as_str().unwrap_or_default().to_string();
        if !ASK_FOR_BIDS.contains(&name.as_str()) {
            continue;
        }
        let properties = &mut tool["inputSchema"]["properties"];
        if properties.get("bestaetigen").is_none() {
            properties["bestaetigen"] = flag("");
        }
    }
    list
}

pub(crate) fn tools() -> Vec<Value> {
    with_bids_question(with_lv_argument(tool_list()))
}

fn tool_list() -> Vec<Value> {
    let mut list = Vec::new();

    list.push(tool(
        "projekte_suchen",
        "proAVA-Projekte in einem Ordner und darunter finden, neueste zuerst.".into(),
        props(&[
            ("ordner", text("")),
            ("muster", text("Teil des Dateinamens")),
            ("mit_alten_versionen", flag("")),
        ]),
        &["ordner"],
        true,
    ));
    list.push(tool(
        "projekt_lesen",
        "Überblick, zuerst aufrufen: Stammdaten, LVs, Maske, Summen, Angebote.".into(),
        props(&[project()]),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "lvs_lesen",
        "Die LVs des Projekts mit Nr., Art und Summe.".into(),
        props(&[project()]),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "lv_anlegen",
        "Weiteres LV anlegen (wie das aktive LV, nächste Nummer); art kostenschaetzung/kostenberechnung = Kosten-LV nach DIN 276.".into(),
        props(&[
            project(),
            ("bezeichnung", text("Pflicht außer mit aus")),
            ("nummer", text("")),
            ("art", json!({"type": "string", "enum": ["ausschreibung", "kostenschaetzung", "kostenberechnung"]})),
            ("kostengruppen", json!({"type": "array", "items": {"type": ["string", "integer"]}, "description": "KG der 2. Ebene; ohne = alle, [] = keine Titel"})),
            ("aus", json!({"type": ["string", "integer"]})),
            ("oz_maske", text("")),
            ("schrittweite", count("")),
            ("mwst", number("")),
            ("waehrung", text("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "bieter_lesen",
        "Bieterverzeichnis des LV.".into(),
        props(&[project()]),
        &["projekt"],
        true,
    ));
    let bidder_fields = || {
        vec![
            ("name2", text("")),
            ("strasse", text("")),
            ("plz", text("")),
            ("ort", text("")),
            ("telefon", text("")),
            ("email", text("")),
            ("ansprechpartner", text("")),
            ("land", text("")),
            ("fax", text("")),
            ("ust_id", text("")),
            ("angefordert", text("")),
            (
                "status",
                json!({"type": "string", "enum": ["angefragt", "abgegeben", "kein Angebot", "ausgeschlossen"]}),
            ),
            (
                "angebot",
                json!({"type": ["integer", "null"], "description": "null löst"}),
            ),
            ("eingang", text("")),
            ("nebenangebote", flag("")),
            ("bemerkung", text("")),
            ("absagegrund", text("")),
        ]
    };
    let mut fields = vec![project(), ("name1", text(""))];
    fields.extend(bidder_fields());
    list.push(tool(
        "bieter_anlegen",
        "Angefragte Firma ins Bieterverzeichnis aufnehmen.".into(),
        props(&fields),
        &["projekt", "name1"],
        false,
    ));
    let mut fields = vec![
        project(),
        ("bieter", count("")),
        ("name1", text("")),
        ("entfernen", flag("")),
    ];
    fields.extend(bidder_fields());
    list.push(tool(
        "bieter_aendern",
        "Bieter ändern (nur angegebene Felder) oder entfernen.".into(),
        props(&fields),
        &["projekt", "bieter"],
        false,
    ));
    list.push(tool(
        "lv_lesen",
        "LV als Liste „OZ | Art | Kurztext | Menge Einheit | EP | GP | Id“; GP in Klammern zählt nicht.".into(),
        props(&[
            project(),
            oz("ab_oz", ""),
            oz("bis_oz", ""),
            ("nur_titel", flag("")),
            ("mit_langtext", flag("")),
            ("max_zeilen", count("")),
        ]),
        &["projekt"],
        true,
    ));
    let mut fields = vec![project()];
    fields.extend(row_ref());
    list.push(tool(
        "position_lesen",
        "Eine Zeile vollständig mit Langtext in Auszeichnung.".into(),
        props(&fields),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "lv_pruefen",
        "Prüfung vor dem GAEB-Export: Fehler, Hinweise und GAEB-Fachregeln.".into(),
        props(&[
            project(),
            (
                "art",
                json!({"type": "string", "enum": ["X83", "X82", "X81"]}),
            ),
            ("alle", flag("alle LVs (Kosten-LVs nur, wenn lv eines ist)")),
        ]),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "rechtschreibung_pruefen",
        "Rechtschreibung des LV: unbekannte Wörter mit Fundstellen und Vorschlägen.".into(),
        props(&[project(), ("vorschlaege", count(""))]),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "kostengruppen_lesen",
        "Kosten je KG (DIN 276) aus Planerpreisen und Angeboten.".into(),
        props(&[project()]),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "angebote_lesen",
        "Eingelesene Angebote mit Summen und Befunden; mit angebot die Einzelheiten, Strukturabweichungen und Zuordnungsvorschläge.".into(),
        props(&[project(), ("angebot", count(""))]),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "preisspiegel_lesen",
        "Preisspiegel der Angebote mit Rang, Abstand und Ausreißern.".into(),
        props(&[
            project(),
            ("je_titel", flag("")),
            ("positionen", flag("")),
            ("ausreisser_prozent", number("")),
        ]),
        &["projekt"],
        true,
    ));

    list.push(tool(
        "projekt_anlegen",
        "Neues Projekt <ordner>/<name>/<name>.proava, leer oder aus vorlage; der Ordner muss existieren.".into(),
        props(&[
            ("ordner", text("")),
            ("name", text("")),
            ("vorlage", text("leer, „GA“, kostenschaetzung, kostenberechnung, eigene Vorlage (Name) oder Pfad")),
            ("positionen", flag("Standard ja; false = nur Gliederung und Texte")),
            ("oz_maske", text("1/2/3 Titelebenen, L Buchstaben, P Position, I Index; nicht mit eigener Vorlage")),
            ("schrittweite", count("nicht mit eigener Vorlage")),
            ("lv_titel", text("")),
        ]),
        &["ordner", "name"],
        false,
    ));
    list.push(tool(
        "projektangaben_setzen",
        "Projektstammdaten und LV-Kopf ändern; Unstimmiges wird abgelehnt. oz_maske/schrittweite nur bei fortlaufender Nummerierung.".into(),
        props(&[
            project(),
            ("projektbezeichnung", text("")),
            ("kurzname", text("Projektnummer des Auftraggebers")),
            ("baumassnahme", text("")),
            ("vergabeart", text("z. B. „Öffentliche Ausschreibung“ oder frei")),
            ("vergabenummer", text("")),
            ("datenaustauschnummer", text("wenn abweichend von der Vergabenummer")),
            ("abgabetermin", text("„TT.MM.JJJJ, HH:MM Uhr“")),
            ("abgabe_uhrzeit", text("HH:MM")),
            ("abgabeort", text("")),
            ("bindefrist", text("")),
            ("ausfuehrung_beginn", text("")),
            ("ausfuehrung_ende", text("")),
            ("weitere_fristen", text("")),
            ("bauherr", address("")),
            ("auftraggeber", address("Vergabestelle, wenn nicht der Bauherr")),
            ("auftraggeber_wie_bauherr", flag("Bauherr gilt als Auftraggeber")),
            ("aufsteller", address("")),
            ("baustelle", address("")),
            ("baustelle_nummer", text("")),
            ("baustelle_bezeichnung", text("")),
            ("benachrichtigungsort", json!({"type": ["object", "null"], "description": "Adresse + nummer, bezeichnung; null = keiner"})),
            ("weitere_ausfuehrungsorte", json!({"type": "array", "description": "je {Adresse, nummer, bezeichnung, benachrichtigungsort}", "items": {"type": "object"}})),
            ("bedarfstraeger", json!({"type": "array", "description": "je {Adresse, nummer, bezeichnung}", "items": {"type": "object"}})),
            ("debitorennummer", text("")),
            ("auftraggeber_staat", json!({"type": "string", "enum": ["ewr", "anderer", ""]})),
            ("auftraggeber_kfz_kennzeichen", text("")),
            ("adressen", json!({"type": "array", "description": "je {rolle, Adresse}", "items": {"type": "object"}})),
            ("sicherheit_vertragserfuellung", number("% der Auftragssumme")),
            ("sicherheit_maengel", number("% der Abrechnungssumme")),
            ("skonto_prozent", number("")),
            ("skonto_tage", count("")),
            ("abzuege", json!({"type": "array", "description": "je {bezeichnung, prozent}", "items": {"type": "object", "properties": {"bezeichnung": {"type": "string"}, "prozent": {"type": ["number", "string"]}}}})),
            ("zahlungsbedingungen", text("")),
            ("weitere_vereinbarungen", text("")),
            ("lv_nummer", text("")),
            ("lv_titel", text("")),
            ("mwst", number("")),
            ("waehrung", text("")),
            ("oz_maske", text("")),
            ("schrittweite", count("")),
            ("ebenen", json!({"type": "array", "items": {"type": "string"}, "description": "z. B. [„Los“, „Titel“]"})),
            ("pauschalsumme", json!({"type": ["number", "string", "null"], "description": "LV-Summe netto statt Summe der Positionen"})),
            ("positionen_mit_buchstaben", flag("OZ wie „3A“")),
            ("zeiteinheit", text("")),
            ("ep_anteile", json!({"type": "array", "items": {"type": "string"}, "description": "z. B. [„Lohn“, „Material“]"})),
            ("kopf_fusszeile_standard", flag("eigene Kopf-/Fußzeile löschen")),
        ]
        .into_iter()
        .chain(crate::frame_args::props())
        .collect::<Vec<_>>()),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "titel_einfuegen",
        "Titel einfügen; unter_titel = Untertitel am Ende dieses Titels, ohne Ort auf Ebene 1 ans Ende.".into(),
        props(&[
            project(),
            ("kurztext", text("")),
            ("oz", text("")),
            ("ebene", count("1 = Titel, 2 = Untertitel …")),
            oz("nach_oz", ""),
            ("nach_id", count("")),
            oz("unter_titel", ""),
        ]),
        &["projekt", "kurztext"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(place());
    fields.extend(position_fields());
    fields.push(("oz", text("")));
    list.push(tool(
        "position_einfuegen",
        "Eine Position einfügen, z. B. {unter_titel: „01.02“, kurztext: „Kanalfühler“, menge: 4, einheit: „St“, ep: 85}.".into(),
        props(&fields),
        &["projekt", "kurztext"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(place());
    let mut row_props: Vec<(&str, Value)> = vec![
        ("titel", text("statt Position: Titeltext")),
        (
            "ebene",
            count("Standard: nach einem Titel der Liste dessen Untertitel"),
        ),
        ("oz", text("")),
        (
            "hinweis",
            text("statt Position: Hinweistext, kurztext = Überschrift"),
        ),
        ("vorbemerkung", flag("")),
        ("schlussbemerkung", flag("")),
    ];
    row_props.extend(position_fields());
    fields.push((
        "zeilen",
        json!({"type": "array", "description": "Positionen, Titel (titel) oder Hinweise (hinweis)", "items": {"type": "object", "properties": props(&row_props)}}),
    ));
    list.push(tool(
        "positionen_einfuegen",
        "Mehrere Zeilen der Reihe nach einfügen, alles oder nichts. Beispiel {zeilen: [{titel: „Fühler“, ebene: 2}, {kurztext: „Raumfühler“, menge: 12, einheit: „St“}]}.".into(),
        props(&fields),
        &["projekt", "zeilen"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(place());
    fields.push(("text", text("")));
    fields.push(("kurztext", text("Überschrift")));
    fields.push(("vorbemerkung", flag("")));
    fields.push(("schlussbemerkung", flag("")));
    fields.push(complements_field());
    list.push(tool(
        "hinweis_einfuegen",
        "Hinweis (ohne OZ und Preis) einfügen: text und/oder kurztext.".into(),
        props(&fields),
        &["projekt"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(place());
    list.push(tool(
        "abstand_einfuegen",
        "Leerzeile nur zur Gliederung in proAVA (nicht im Druck, GAEB, CSV).".into(),
        props(&fields),
        &["projekt"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(place());
    fields.push(("nummer", text("von Hand, A0001–A9999")));
    fields.push(("bezeichnung", text("höchstens 55 Zeichen")));
    fields.push(("kurztext", text("")));
    fields.push(("langtext", text("")));
    fields.push(complements_field());
    list.push(tool(
        "ausfuehrungsbeschreibung_einfuegen",
        "Ausführungsbeschreibung einfügen; Positionen verweisen mit ausfuehrungsbeschreibung: „A0001“ darauf.".into(),
        props(&fields),
        &["projekt"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(row_ref());
    fields.push(("datei", text("PNG oder JPEG")));
    fields.push(("breite_mm", number("5–400")));
    fields.push((
        "absatz",
        count("nach diesem Block (wie in position_lesen); 0 = vorne, ohne = Ende"),
    ));
    fields.push(("name", text("Dateiname im Projekt")));
    let mut clip_fields = vec![project()];
    clip_fields.extend(row_ref());
    clip_fields.push(("datei", text("")));
    clip_fields.push(("seite", count("")));
    clip_fields.push((
        "ausschnitt_mm",
        json!({"type": "object", "description": "{links, oben, breite, hoehe} in mm", "properties": {"links": {"type": "number"}, "oben": {"type": "number"}, "breite": {"type": "number"}, "hoehe": {"type": "number"}}}),
    ));
    clip_fields.push((
        "ausschnitt_anteile",
        json!({"type": "object", "description": "{links, oben, rechts, unten} als 0…1; ohne = ganze Seite", "properties": {"links": {"type": "number"}, "oben": {"type": "number"}, "rechts": {"type": "number"}, "unten": {"type": "number"}}}),
    ));
    clip_fields.push(("breite_mm", number("10–180")));
    clip_fields.push(("absatz", count("wie bei bild_einfuegen")));
    list.push(tool(
        "pdf_ausschnitt_einfuegen",
        "Ausschnitt einer PDF-Seite in einen Langtext einfügen (Druck: Vektor, GAEB: PNG).".into(),
        props(&clip_fields),
        &["projekt", "datei"],
        false,
    ));
    let mut field_fields = vec![project()];
    field_fields.extend(row_ref());
    field_fields.push(("vortext", text("z. B. „Fabrikat:“")));
    field_fields.push(("art", text("„Bieter“ (Standard) oder „AG“")));
    field_fields.push(("inhalt", text("nur AG: der Wert")));
    field_fields.push(("nachtext", text("z. B. „ oder gleichwertig“")));
    field_fields.push(("kennung", text("")));
    field_fields.push(("nach", text("hinter diesem Text")));
    field_fields.push(("absatz", count("eigene Zeile nach diesem Block")));
    list.push(tool(
        "bieterangabe_einfuegen",
        "Bieterangabe („Fabrikat: …… (vom Bieter)“) oder mit art AG eine Vorgabe in einen Langtext einfügen.".into(),
        props(&field_fields),
        &["projekt", "vortext"],
        false,
    ));
    list.push(tool(
        "bild_einfuegen",
        "Bild in einen Langtext einfügen ([Bild: name, breite mm]).".into(),
        props(&fields),
        &["projekt", "datei"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(row_ref());
    fields.extend(position_fields());
    fields.push(("text", text("nur Hinweis, Ausführungsbeschreibung")));
    fields.push(("vorbemerkung", flag("")));
    fields.push(("schlussbemerkung", flag("")));
    fields.push(("bezeichnung", text("nur Ausführungsbeschreibung")));
    fields.push(("nummer", text("nur Ausführungsbeschreibung")));
    fields.push((
        "status",
        json!({"type": "string", "enum": ["offen", "in Bearbeitung", "fertig"], "description": "fertig sperrt; bei Titeln für alles darunter"}),
    ));
    fields.push(("ebene", count("")));
    fields.push((
        "pauschalsumme",
        json!({"type": ["number", "string", "null"], "description": "nur Titel, netto; null = keine"}),
    ));
    fields.push((
        "fertige_erben",
        flag("Pflicht, wenn fertige Positionen die KG/LB des Titels erben"),
    ));
    fields.push(oz("neue_oz", "danach nicht mehr fortlaufend"));
    fields.push(("in_freie_position", flag("STLB-Kennung entfernen")));
    list.push(tool(
        "zeile_aendern",
        "Eine Zeile ändern, z. B. {oz: „01.02.0010“, menge: 6}. STLB-Bau-Positionen: Langtext und Einheit gesperrt (in_freie_position).".into(),
        props(&fields),
        &["projekt"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(row_ref());
    fields.push((
        "zeilenart",
        json!({"type": "string", "enum": ["position", "hinweis", "ausfuehrungsbeschreibung"]}),
    ));
    fields.push((
        "bestaetigen",
        flag("trotz Wegfall bzw. Angeboten umwandeln; ohne = Vorschau"),
    ));
    list.push(tool(
        "zeile_umwandeln",
        "Zeilenart ändern; fällt dabei etwas weg (Menge, EP, OZ), ohne bestaetigen nur Vorschau."
            .into(),
        props(&fields),
        &["projekt", "zeilenart"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(row_ref());
    fields.push(("mit_inhalt", flag("")));
    list.push(tool(
        "zeile_loeschen",
        "Zeile löschen (Titel mit Inhalt nur mit mit_inhalt).".into(),
        props(&fields),
        &["projekt"],
        false,
    ));
    let mut fields = vec![project()];
    fields.extend(row_ref());
    fields.push((
        "richtung",
        json!({"type": "string", "enum": ["hoch", "runter"]}),
    ));
    fields.push(("schritte", count("")));
    list.push(tool(
        "zeile_verschieben",
        "Zeile (Titel samt Inhalt) nach oben oder unten verschieben.".into(),
        props(&fields),
        &["projekt", "richtung"],
        false,
    ));
    list.push(tool(
        "neu_nummerieren",
        "Alle OZ nach Maske und Schrittweite neu vergeben; Vorsicht, wenn das LV schon an Bieter ging.".into(),
        props(&[project()]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "angebot_nachlass_setzen",
        "Nachlass eines Angebots setzen oder es aus dem Preisspiegel nehmen.".into(),
        props(&[
            project(),
            ("angebot", count("")),
            ("prozent", number("")),
            ("betrag", number("")),
            ("ausgeschlossen", flag("")),
        ]),
        &["projekt", "angebot"],
        false,
    ));

    list.push(tool(
        "auftrag_erteilen",
        "Einem Angebot den Auftrag erteilen (ersetzt einen früheren); aufheben nimmt ihn zurück."
            .into(),
        props(&[
            project(),
            ("angebot", count("")),
            ("datum", text("")),
            ("aufheben", flag("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "preise_uebernehmen",
        "Angebotspreise als Planerpreise übernehmen; ohne ueberschreiben nur leere EP.".into(),
        props(&[
            project(),
            (
                "quelle",
                json!({"type": "string", "enum": ["angebot", "mittelwert", "median", "minimum"]}),
            ),
            ("angebot", count("bei quelle angebot")),
            ("ueberschreiben", flag("")),
        ]),
        &["projekt", "quelle"],
        false,
    ));
    list.push(tool(
        "auftrags_lv_exportieren",
        "Auftrags-LV mit den Preisen des Auftragnehmers als X86, D86 oder PDF schreiben.".into(),
        props(&[
            project(),
            ("angebot", count("sonst das beauftragte")),
            (
                "format",
                json!({"type": "string", "enum": ["x86", "d86", "pdf"]}),
            ),
            ("ziel", text("")),
            ("ueberschreiben", flag("")),
        ]),
        &["projekt"],
        false,
    ));

    list.push(tool(
        "gaeb_exportieren",
        "LV als GAEB schreiben (X83 ohne Preise, X82 mit Planerpreisen, X81); vorher lv_pruefen."
            .into(),
        props(&[
            project(),
            (
                "art",
                json!({"type": "string", "enum": ["X83", "X82", "X81"]}),
            ),
            (
                "format",
                json!({"type": "string", "enum": ["xml", "gaeb90"]}),
            ),
            ("ziel", text("")),
            ("ueberschreiben", flag("")),
            ("bilder", flag("")),
        ]),
        &["projekt", "art"],
        false,
    ));
    let mut fields = vec![
        project(),
        ("datei", text("")),
        oz("platzhalter_oz", "wird mit der ersten Position gefüllt"),
        ("platzhalter_id", count("")),
        (
            "positionen",
            json!({"type": "array", "items": {"type": "string"}, "description": "nur diese OZ der Datei"}),
        ),
        ("quelle", text("z. B. „STLB-Bau“, „AUSSCHREIBEN.DE“")),
        (
            "als",
            json!({"type": ["string", "object"], "description": "Zeilenart für alle oder {OZ: Art}"}),
        ),
    ];
    fields.extend(place());
    list.push(tool(
        "stlb_uebernehmen",
        "GAEB-Datei aus STLB-Bau oder AUSSCHREIBEN.DE (vom Anwender exportiert) einfügen oder einen STLB-Platzhalter füllen.".into(),
        props(&fields),
        &["projekt", "datei"],
        false,
    ));
    list.push(open_world(tool(
        "ausschreiben_suchen",
        "Herstellertexte auf AUSSCHREIBEN.DE finden (Titel, Link); dann ausschreiben_oeffnen."
            .into(),
        props(&[
            (
                "suche",
                text("Hersteller oder Katalog zuerst, z. B. „Belimo Drehantrieb“"),
            ),
            ("max_treffer", count("")),
        ]),
        &["suche"],
        true,
    )));
    list.push(open_world(tool(
        "ausschreiben_oeffnen",
        "Seite von AUSSCHREIBEN.DE im proAVA-Browser öffnen; die KI bedient die Website nicht."
            .into(),
        props(&[("url", text(""))]),
        &[],
        false,
    )));
    list.push(tool(
        "stlb_pruefdatei",
        "STLB-Bau-Positionen als X81 zum Prüfen in STLB-Bau schreiben.".into(),
        props(&[project(), ("ziel", text("")), ("ueberschreiben", flag(""))]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "stlb_aktualisieren",
        "Datei aus STLB-Bau den STLB-Positionen zuordnen und Unterschiede zeigen; uebernehmen übernimmt sie.".into(),
        props(&[
            project(),
            ("datei", text("")),
            ("uebernehmen", flag("ohne = Vorschau")),
        ]),
        &["projekt", "datei"],
        false,
    ));
    list.push(tool(
        "lv_suchen",
        "Positionen in anderen LVs finden – in genau einem von projekt, ordner oder datei; danach positionen_uebernehmen.".into(),
        props(&[
            ("suche", text("")),
            ("kg", text("")),
            ("einheit", text("")),
            project(),
            ("lv", json!({"type": ["string", "integer"]})),
            ("ordner", text("")),
            ("datei", text("")),
            ("mit_alten_versionen", flag("")),
            ("max_treffer", count("")),
            ("zeitlimit_s", count("")),
        ]),
        &[],
        true,
    ));
    let mut fields = vec![
        project(),
        ("lv", text("")),
        ("quelle", text("")),
        ("quelle_lv", text("")),
        ("oz", json!({"type": "array", "items": {"type": "string"}})),
        (
            "ids",
            json!({"type": "array", "items": {"type": "integer"}}),
        ),
        ("mengen", flag("")),
        ("preise", flag("Standard false")),
    ];
    fields.extend(place());
    list.push(tool(
        "positionen_uebernehmen",
        "Titel und Positionen aus anderem Projekt, Bibliothek oder GAEB-Datei übernehmen (Stellen aus lv_suchen).".into(),
        props(&fields),
        &["projekt", "quelle"],
        false,
    ));
    list.extend(window_tools());
    list.push(tool(
        "lv_vergleichen",
        "Änderungsliste gegen ein anderes LV (vergleich_lv) oder eine Datei (vergleich).".into(),
        props(&[
            project(),
            ("lv", text("")),
            ("vergleich", text("")),
            ("vergleich_lv", text("")),
            ("nur", text("neu, entfallen, verschoben oder geändert")),
        ]),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "bibliothek_lesen",
        "Eigene Positionsbibliothek lesen.".into(),
        props(&[
            ("bibliothek", text("")),
            ("suche", text("")),
            ("max_zeilen", count("")),
        ]),
        &[],
        true,
    ));
    list.push(tool(
        "bibliothek_suchen",
        "Positionsbibliotheken durchsuchen (alle Wörter; Filter kg, art, stichwort).".into(),
        props(&[
            ("suche", text("")),
            ("kg", text("")),
            ("art", json!({"type": "string", "enum": ["normal", "bedarf", "wahl", "zulage", "pauschal", "stundenlohn"]})),
            ("stichwort", text("")),
            ("bibliothek", text("")),
            ("mit_langtext", flag("")),
            ("max_treffer", count("")),
        ]),
        &[],
        true,
    ));
    let mut fields = vec![
        project(),
        ("bibliothek", text("")),
        (
            "oz",
            json!({"type": "array", "items": {"type": "string"}, "description": "OZ in der Bibliothek"}),
        ),
        (
            "ids",
            json!({"type": "array", "items": {"type": "integer"}, "description": "statt oz: Ids aus bibliothek_suchen"}),
        ),
        ("mengen", flag("Standard false")),
        ("preise", flag("Standard true")),
    ];
    fields.extend(place());
    list.push(tool(
        "bibliothek_einfuegen",
        "Bibliothekseinträge (Titel samt Inhalt, Positionen) ins LV einfügen.".into(),
        props(&fields),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "bibliothek_ablegen",
        "Zeilen des LV in eine Bibliothek ablegen (unter titel oder neuer_titel).".into(),
        props(&[
            project(),
            ("oz", json!({"type": "array", "items": {"type": "string"}})),
            ("bibliothek", text("")),
            ("titel", text("")),
            ("neuer_titel", text("")),
            ("stichwoerter", text("")),
            ("preis_datum", text("")),
            ("mengen", flag("")),
            ("preise", flag("Standard true")),
        ]),
        &["projekt", "oz"],
        false,
    ));
    list.push(tool(
        "bibliothek_abgleichen",
        "Positionen aus der Bibliothek mit ihrem Eintrag vergleichen; uebernehmen aktualisiert sie.".into(),
        props(&[
            project(),
            ("uebernehmen", json!({"type": "array", "items": {"type": "string"}})),
            ("preise", flag("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "angebot_einlesen",
        "Bieterangebot (X84/D84) einlesen; die jetzigen LV-Mengen gelten als Angebotsmengen."
            .into(),
        props(&[
            project(),
            ("datei", text("")),
            ("ins_bieterverzeichnis", flag("")),
            ("zuordnung", json!({"type": "string", "enum": ["oz", "text"], "description": "text: sichere Vorschläge des Strukturvergleichs übernehmen, Rest melden"})),
            ("zuordnungen", json!({"type": "object", "description": "Von Hand: {\"Bieter-OZ\": \"LV-OZ\"}"})),
        ]),
        &["projekt", "datei"],
        false,
    ));
    list.push(tool(
        "lv_aus_gaeb",
        "Neues Projekt aus einer GAEB-Datei (OZ bleiben).".into(),
        props(&[
            ("datei", text("")),
            ("ordner", text("")),
            ("name", text("")),
        ]),
        &["datei", "ordner", "name"],
        false,
    ));
    list.push(tool(
        "gaeb_importieren",
        "GAEB-Datei als neues LV oder Projekt importieren (Angebote: angebot_einlesen).".into(),
        props(&[
            ("datei", text("")),
            ("ziel", json!({"type": "string", "enum": ["lv", "projekt"]})),
            project(),
            ("nummer", text("")),
            ("bezeichnung", text("")),
            ("ordner", text("")),
            ("name", text("")),
            ("preise", flag("")),
            ("vorschau", flag("")),
        ]),
        &["datei"],
        false,
    ));
    list.push(tool(
        "csv_importieren",
        "CSV (Kopf „OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:“) als neues LV oder Projekt importieren; Zeilen ohne OZ = Langtext.".into(),
        props(&[
            ("datei", text("")),
            ("ziel", json!({"type": "string", "enum": ["lv", "projekt"]})),
            project(),
            ("nummer", text("")),
            ("bezeichnung", text("")),
            ("ordner", text("")),
            ("name", text("")),
            ("waehrung", json!({"type": "string", "enum": ["EUR", "USD", "CHF"]})),
            ("preise", flag("")),
            ("vorschau", flag("")),
        ]),
        &["datei"],
        false,
    ));
    list.push(tool(
        "csv_exportieren",
        "LV als CSV im Format von csv_importieren.".into(),
        props(&[
            project(),
            ("ziel", text("")),
            (
                "zeichensatz",
                json!({"type": "string", "enum": ["utf-8", "windows-1252"]}),
            ),
            ("ueberschreiben", flag("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "anschreiben_drucken",
        "Schreiben an Bieter als PDF aus der Vorlage (aufforderung setzt das Versanddatum, freigabe geht an den Bauherrn).".into(),
        props(&[
            project(),
            ("art", json!({"type": "string", "enum": ["aufforderung", "zuschlag", "absage", "freigabe"]})),
            ("bieter", count("")),
            ("rechnung", count("nur freigabe; ohne = letzte")),
            ("alle", flag("")),
            ("einzeln", flag("")),
            ("ziel", text("PDF-Datei bzw. Ordner")),
            ("ueberschreiben", flag("")),
            ("ablegen", flag("")),
            ("logo", text("")),
        ]),
        &["projekt", "art"],
        false,
    ));
    // ---------- Kostenermittlung nach DIN 276 ----------
    let estimate_ref = || ("kostenermittlung", json!({"type": ["string", "integer"]}));
    // GU-Fall: the Datenpunkt-Abschnitt of a Kosten-LV instead of a standalone one.
    let section_ref = || -> Vec<(&'static str, Value)> {
        vec![
            ("lv", json!({"type": ["string", "integer"]})),
            ("abschnitt", text("")),
        ]
    };
    list.push(tool(
        "kosten_lesen",
        "Kostenermittlungen des Projekts (Kosten-LVs, Datenpunktrechnungen) mit Summen je KG."
            .into(),
        props(
            &[
                vec![project(), estimate_ref(), ("zeilen", flag(""))],
                section_ref(),
            ]
            .concat(),
        ),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "kosten_anlegen",
        "Neue Datenpunktrechnung (stufe; aus = aus einer Kostenschätzung); danach kosten_zeilen_schreiben.".into(),
        props(&[
            project(),
            ("stufe", json!({"type": "string", "enum": ["kostenschaetzung", "kostenberechnung"]})),
            ("aus", text("")),
            ("titel", text("")),
            ("stand", text("")),
            ("bearbeiter", text("")),
            ("bemerkung", text("")),
            ("bauteil", text("")),
            ("kennwert", number("")),
            ("kennwert_von", number("")),
            ("kennwert_bis", number("")),
            ("kennwert_je_dp_art", flag("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "kosten_aendern",
        "Datenpunktrechnung ändern: Kopf, Kennwerte, dp_arten, aufteilung (% je KG), Bauteile, loeschen.".into(),
        props(&[
            project(),
            estimate_ref(),
            section_ref()[0].clone(),
            section_ref()[1].clone(),
            ("titel", text("")),
            ("stand", text("")),
            ("bearbeiter", text("")),
            ("bemerkung", text("")),
            ("kennwert", number("")),
            ("kennwert_von", number("")),
            ("kennwert_bis", number("")),
            ("kennwert_je_dp_art", flag("")),
            ("dp_arten", json!({"type": "array", "items": {"type": "object"}})),
            ("aufteilung", json!({"type": "object"})),
            ("bauteil", text("")),
            ("vorgabe_aufteilung_fuer", text("")),
            ("bauteile_umbenennen", json!({"type": "object"})),
            ("loeschen", flag("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "kosten_zeilen_schreiben",
        "DP-Zeilen schreiben: zeilen [{bauteil, anlage, dp: {„AI“: 12}}]; ersetzen leert die Bauteile vorher.".into(),
        props(&[
            project(),
            estimate_ref(),
            section_ref()[0].clone(),
            section_ref()[1].clone(),
            ("zeilen", json!({"type": "array", "items": {"type": "object"}})),
            ("ersetzen", flag("")),
        ]),
        &["projekt", "zeilen"],
        false,
    ));
    // ---------- Kostenverfolgung (DIN 276, 4.4–4.6) ----------
    let view_args = || -> Vec<(&'static str, Value)> {
        vec![
            ("ebene", count("1–3")),
            (
                "ansicht",
                json!({"type": "string", "enum": ["kostengruppen", "vergabeeinheiten"]}),
            ),
            ("brutto", flag("")),
            (
                "abweichung",
                json!({"type": "string", "enum": ["budget", "kostenvorgabe", "vorstufe", "stand", "keine"]}),
            ),
            (
                "vergleich",
                json!({"type": ["string", "integer"], "description": "Kostenstand (Nummer oder Datum)"}),
            ),
        ]
    };
    list.push(tool(
        "kostenverfolgung_lesen",
        "Kostenverfolgung je KG oder Vergabeeinheit von der Kostenvorgabe bis zur Abrechnung."
            .into(),
        props(&[vec![project()], view_args()].concat()),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "kostenvorgabe_setzen",
        "Kostenvorgabe (Budget) je KG setzen, z. B. budget {„481“: 62500}, oder aus einer Kostenermittlung.".into(),
        props(&[
            project(),
            ("budget", json!({"type": "object"})),
            ("ersetzen", flag("")),
            ("aus", text("")),
            ("art", json!({"type": "string", "enum": ["obergrenze", "zielgroesse"]})),
            ("stand", text("")),
            ("grundlage", text("")),
            ("schwelle", number("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "kostenverfolgung_aendern",
        "Kostenverfolgung ändern: Erläuterungen je KG, zählende Kostenermittlungen, Angebotswahl, Kostenstände.".into(),
        props(&[
            project(),
            ("erlaeuterungen", json!({"type": "object"})),
            ("kostenermittlungen", json!({"type": "object"})),
            ("angebot_wahl", json!({"type": "object"})),
            ("kostenstand_festhalten", json!({"type": "object"})),
            ("kostenstand_loeschen", count("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "schriftverkehr_lesen",
        "Abgelegte Schreiben des Projekts.".into(),
        props(&[
            project(),
            ("art", json!({"type": "string", "enum": ["aufforderung", "zuschlag", "absage", "freigabe", "eigene"]})),
            ("empfaenger", text("")),
        ]),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "schriftverkehr_pdf_speichern",
        "PDF eines abgelegten Schreibens als Datei speichern.".into(),
        props(&[
            project(),
            ("id", count("")),
            ("ziel", text("")),
            ("ueberschreiben", flag("")),
        ]),
        &["projekt", "id"],
        false,
    ));
    list.push(tool(
        "submission_drucken",
        "Niederschrift über die Angebotsöffnung als PDF.".into(),
        props(&[
            project(),
            ("ziel", text("")),
            ("ueberschreiben", flag("")),
            ("logo", text("")),
        ]),
        &["projekt"],
        false,
    ));
    // ---------- Abrechnung ----------
    let invoice_header = || -> Vec<(&'static str, Value)> {
        vec![
            (
                "art",
                json!({"type": "string", "enum": ["abschlag", "schluss", "einzel", "sicherheit"], "description": "einzel: außerhalb der Folge, sicherheit: Einbehalt"}),
            ),
            ("nummer", text("")),
            ("datum", text("")),
            ("eingang", text("")),
            ("zahlungsziel", text("")),
            ("leistung_von", text("")),
            ("leistung_bis", text("")),
            (
                "mengen",
                json!({"type": "string", "enum": ["kumuliert", "zuwachs"], "description": "wie die Firma Mengen angibt"}),
            ),
            (
                "freigabe",
                json!({"type": "string", "enum": ["kumuliert", "zuwachs"], "description": "Rechenweg der Freigabe"}),
            ),
            (
                "ust",
                json!({"type": ["number", "string"], "description": "19, 7 oder „13b“"}),
            ),
            ("forderung_netto", number("")),
            ("einbehalt_prozent", number("null = laut Vertrag")),
            ("angerechnet", number("null = bisherige Freigaben")),
            ("pauschal_gefordert", number("")),
            ("pauschal_geprueft", number("null = wie gefordert")),
            ("korrektur", number("")),
            ("korrektur_text", text("")),
            ("sicherheit_freigabe", number("nur art sicherheit")),
            ("pruefvermerk", text("")),
            ("geprueft_am", text("")),
            ("geprueft_von", text("")),
        ]
    };
    list.push(tool(
        "abrechnung_lesen",
        "Abrechnung des LV: Auftrag und Rechnungen; mit rechnung je Position.".into(),
        props(&[
            project(),
            ("rechnung", count("")),
            ("alle_positionen", flag("auch unveränderte")),
            ("hinweise", flag("")),
        ]),
        &["projekt"],
        true,
    ));
    list.push(tool(
        "freigabe_berechnen",
        "Rechenweg der Freigabe einer Rechnung.".into(),
        props(&[project(), ("rechnung", count(""))]),
        &["projekt", "rechnung"],
        true,
    ));
    list.push(tool(
        "auftrag_festschreiben",
        "Auftrag als Grundlage der Abrechnung festschreiben, aus dem beauftragten Angebot oder datei (X86/D86); einen vorhandenen nur mit neu.".into(),
        props(&[
            project(),
            ("datei", text("")),
            ("auftrags_nr", text("")),
            ("neu", flag("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "rechnung_anlegen",
        "Rechnung der Firma anlegen, von Hand oder aus datei (X89).".into(),
        props(
            &[project(), ("datei", text(""))]
                .into_iter()
                .chain(invoice_header())
                .collect::<Vec<_>>(),
        ),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "rechnung_aendern",
        "Rechnungskopf ändern oder loeschen.".into(),
        props(
            &[project(), ("rechnung", count("")), ("loeschen", flag(""))]
                .into_iter()
                .chain(invoice_header())
                .collect::<Vec<_>>(),
        ),
        &["projekt", "rechnung"],
        false,
    ));
    list.push(tool(
        "rechnungsposition_setzen",
        "Positionen einer Rechnung setzen; Mengen so, wie die Rechnung sie angibt (kumuliert oder Zuwachs).".into(),
        props(&[
            project(),
            ("rechnung", count("")),
            oz("oz", ""),
            ("rechnung_menge", number("")),
            ("rechnung_ep", number("nur wenn abweichend")),
            ("rechnung_gp", number("")),
            ("gepruefte_menge", number("")),
            ("fortschritt_prozent", number("0–100, braucht vermerk")),
            ("quelle", json!({"type": "string", "enum": ["auto", "rechnung", "geprueft", "aufmass", "fortschritt"], "description": "auto: Fortschritt, geprüft, Aufmaß, Rechnung"})),
            ("vermerk", text("")),
            ("nachweis", flag("Stundennachweis liegt vor")),
            ("positionen", json!({"type": "array", "description": "je {oz, …}", "items": {"type": "object"}})),
        ]),
        &["projekt", "rechnung"],
        false,
    ));
    list.push(tool(
        "aufmass_setzen",
        "Aufmaß einer Position für rechnung setzen (zeilen oder datei X31).".into(),
        props(&[
            project(),
            ("rechnung", count("ohne = nicht zugeordnet")),
            oz("oz", ""),
            ("zeilen", json!({"type": "array", "description": "je {ort, rechenansatz}", "items": {"type": "object"}})),
            ("anhaengen", flag("")),
            ("datei", text("")),
        ]),
        &["projekt"],
        false,
    ));

    list.push(tool(
        "pdf_drucken",
        "PDF wie im Druckdialog (dokument); für Kosten-LVs keine Vergabe- und Abrechnungsdokumente.".into(),
        props(&[
            project(),
            ("dokument", json!({"type": "string", "enum": ["angebotsaufforderung", "lv_ohne_preise", "bepreistes_lv", "lv_mit_preisen", "kostenvoranschlag", "kurztext_lv", "zusammenstellung", "auftrags_lv", "preisspiegel", "bieterpreise", "kostengruppen", "niederschrift", "aufmasspruefung", "rechnungsfreigabe", "kostenermittlung", "kosten_lv", "kostenverfolgung"]})),
            ("angebot", count("bieterpreise, auftrags_lv")),
            ("rechnung", count("ohne = letzte")),
            ("kostenermittlung", json!({"type": ["string", "integer"]})),
            ("diagramm", flag("")),
            ("anlagenliste", flag("")),
            ("kostenkontrolle", flag("")),
            ("ebene", count("")),
            ("ansicht", json!({"type": "string", "enum": ["kostengruppen", "vergabeeinheiten"]})),
            ("brutto", flag("")),
            ("abweichung", json!({"type": "string", "enum": ["budget", "kostenvorgabe", "vorstufe", "stand", "keine"]})),
            ("vergleich", json!({"type": ["string", "integer"]})),
            ("langtexte", flag("")),
            ("bilder", flag("")),
            ("hinweise", flag("")),
            ("ausfuehrungsbeschreibungen", flag("")),
            ("zusammenstellung", flag("kosten_lv: false = ohne Kurztext-Liste")),
            ("titel_neue_seite", flag("")),
            ("quer", flag("")),
            ("blattrahmen", flag("")),
            ("gitter", flag("Tabellen voll umrandet; ohne Angabe die Einstellung, Preisspiegel an")),
            ("stil", text("klassisch oder proava")),
            ("inhaltsverzeichnis", flag("")),
            ("uebertrag", flag("")),
            ("lesezeichen_positionen", flag("")),
            ("preise", flag("Nur kurztext_lv")),
            ("mengenermittlung", flag("Nur bepreistes_lv, auftrags_lv, kurztext_lv")),
            ("kostengruppe", flag("")),
            ("leistungsbereich", flag("")),
            ("deckblatt", json!({"type": ["boolean", "string"], "description": "standard, briefkopf oder kein; true = letzte Wahl"})),
            ("schrift_pt", count("9, 10 oder 11")),
            ("titel", text("OZ, mit Komma")),
            ("kopftext", text("")),
            ("fusstext", text("")),
            ("logo", text("")),
            ("ziel", text("")),
            ("ueberschreiben", flag("")),
        ]
        .into_iter()
        .chain(crate::frame_args::props())
        .collect::<Vec<_>>()),
        &["projekt", "dokument"],
        false,
    ));
    list.extend(change_order_tools());
    list
}

fn change_order_ref() -> (&'static str, Value) {
    ("nachtrag", json!({"type": ["string", "integer"]}))
}

/// Nachträge (Nachtragsmanagement).
fn change_order_tools() -> Vec<Value> {
    let head = || -> Vec<(&'static str, Value)> {
        vec![
            ("bezeichnung", text("")),
            (
                "anlass",
                text("geändert, zusätzlich, mengenänderung, ohne_auftrag, behinderung, sonstig"),
            ),
            ("begruendung", text("")),
            ("firma", text("„“ = Auftragnehmer")),
            ("angekuendigt", text("")),
            ("angebotsfrist", text("")),
            ("angebot_nr", text("")),
            ("angebot_vom", text("")),
            ("entscheidung_bis", text("")),
            ("entschieden_am", text("")),
            ("geprueft_am", text("")),
            ("geprueft_von", text("")),
            ("pruefvermerk", text("")),
            ("empfehlung", text("")),
            ("nachlass_gilt", flag("")),
            ("prognose", flag("")),
            ("nummer", count("")),
        ]
    };
    let with = |mut base: Vec<(&'static str, Value)>, more: Vec<(&'static str, Value)>| {
        base.extend(more);
        base
    };
    vec![
        tool(
            "nachtraege_lesen",
            "Nachträge des LV; mit nachtrag die Positionen und Prüfhilfe.".into(),
            props(&[project(), change_order_ref()]),
            &["projekt"],
            true,
        ),
        tool(
            "nachtrag_anlegen",
            "Nachtrag anlegen; Positionen dann mit nachtragsposition_setzen.".into(),
            props(&with(vec![project()], head())),
            &["projekt", "bezeichnung"],
            false,
        ),
        tool(
            "nachtrag_aendern",
            "Nachtragskopf ändern oder loeschen (Status: nachtrag_status).".into(),
            props(&with(
                vec![project(), change_order_ref()],
                with(head(), vec![
                    ("loeschen", flag("")),
                    ("im_lv_loeschen", flag("")),
                ]),
            )),
            &["projekt", "nachtrag"],
            false,
        ),
        tool(
            "nachtragsposition_setzen",
            "Nachtragsposition setzen: neu (Felder wie position_einfuegen) oder oz/id einer LV-Zeile; Auftragsposition = Mengenänderung (Differenz).".into(),
            props(&with(
                vec![project(), change_order_ref(), ("neu", flag(""))],
                with(row_ref(), vec![
                    ("oz_neu", text("")),
                    ("kurztext", text("")),
                    ("langtext", text("")),
                    ("menge", number("")),
                    ("einheit", text("")),
                    ("pauschal", flag("")),
                    ("stundenlohn", flag("")),
                    ("ep", number("")),
                    ("angeboten_menge", number("")),
                    ("angeboten_ep", number("null = Auftrags-EP")),
                    ("geprueft_menge", number("null = wie angeboten")),
                    ("geprueft_ep", number("null = wie angeboten")),
                    ("vermerk", text("")),
                    ("status", text("z. B. abgelehnt; „wie Nachtrag“")),
                    ("entfernen", flag("")),
                    ("im_lv_loeschen", flag("")),
                ]),
            )),
            &["projekt", "nachtrag"],
            false,
        ),
        tool(
            "nachtrag_status",
            "Status eines Nachtrags wechseln, mit Datum und Vermerk.".into(),
            props(&[
                project(),
                change_order_ref(),
                ("status", text("")),
                ("datum", text("")),
                ("vermerk", text("")),
            ]),
            &["projekt", "nachtrag", "status"],
            false,
        ),
        tool(
            "nachtrag_drucken",
            "PDF: Prüfung eines Nachtragsangebots oder Übersicht aller Nachträge.".into(),
            props(&[
                project(),
                ("art", json!({"type": "string", "enum": ["pruefung", "uebersicht"]})),
                change_order_ref(),
                ("ziel", text("")),
                ("ueberschreiben", flag("")),
                ("logo", text("")),
            ]),
            &["projekt", "art"],
            false,
        ),
        tool(
            "nachtrag_gaeb_exportieren",
            "Nachtrags-LV als GAEB X83 (Anfrage) oder X86 (Auftrag), ohne Mengenänderungen.".into(),
            props(&[
                project(),
                change_order_ref(),
                ("art", json!({"type": "string", "enum": ["X83", "X86"]})),
                ("ziel", text("")),
                ("ueberschreiben", flag("")),
            ]),
            &["projekt", "nachtrag", "art"],
            false,
        ),
        tool(
            "nachtragsangebot_einlesen",
            "Nachtragsangebot (GAEB) in einen Nachtrag einlesen.".into(),
            props(&[project(), change_order_ref(), ("datei", text(""))]),
            &["projekt", "nachtrag", "datei"],
            false,
        ),
    ]
}

/// The tools that work in the open proAVA windows (local bridge).
fn window_tools() -> Vec<Value> {
    let window = || ("fenster", text(""));
    let rows = |lv_key: &'static str| -> Vec<(&'static str, Value)> {
        vec![
            ("auswahl", flag("")),
            ("oz", json!({"type": "array", "items": {"type": "string"}})),
            (
                "ids",
                json!({"type": "array", "items": {"type": "integer"}}),
            ),
            ("weitere", count("")),
            (lv_key, text("")),
        ]
    };
    let mut list = Vec::new();
    list.push(tool(
        "fenster_lesen",
        "Offene proAVA-Fenster; mit fenster Einzelheiten (erste gewählte Zeile = Schreibmarke), mit zeilen/ab/oz/ids/auswahl Zeilen.".into(),
        props(&[
            window(),
            ("zeilen", text("„auswahl“ oder OZ mit Komma")),
            ("ab", text("OZ oder #Id")),
            ("anzahl", count("")),
            ("auswahl", flag("")),
            ("oz", json!({"type": "array", "items": {"type": "string"}})),
            ("ids", json!({"type": "array", "items": {"type": "integer"}})),
            ("langtext", flag("")),
            ("lv", text("")),
        ]),
        &[],
        true,
    ));
    let mut show = vec![window()];
    show.extend(rows("lv"));
    show.extend([
        (
            "positionsfenster",
            flag("Eigenschaften der ersten Zeile öffnen"),
        ),
        ("markierung", text("Text nach „KI:“")),
        ("nur_markieren", flag("Auswahl bleibt")),
        ("sekunden", count("")),
        ("aufheben", flag("")),
    ]);
    list.push(tool(
        "fenster_zeigen",
        "Zeilen im Fenster wählen und markieren, ohne das LV zu ändern; danach nachfragen.".into(),
        props(&show),
        &[],
        true,
    ));
    list.push(tool(
        "fenster_suchen",
        "Im LV eines Fensters suchen und die Treffer dort markieren; danach dem Anwender nennen."
            .into(),
        props(&[
            window(),
            ("suche", text("")),
            ("kg", text("")),
            ("einheit", text("")),
            ("lv", text("")),
            ("max_treffer", count("")),
        ]),
        &[],
        true,
    ));
    let mut insert = vec![
        ("fenster", text("Ziel; hinter der Schreibmarke")),
        ("quelle_fenster", text("ohne oz/ids die dort gewählten")),
        ("quelle", text("Pfad, mit oz/ids")),
    ];
    insert.extend(rows("quelle_lv"));
    insert.extend([
        ("mengen", flag("")),
        ("preise", flag("Standard false")),
        ("nach_oz", text("statt Schreibmarke")),
        ("nach_id", count("")),
        ("lv", text("")),
    ]);
    list.push(tool(
        "fenster_einfuegen",
        "Zeilen aus quelle_fenster oder quelle ins Fenster einfügen wie Strg+C/Strg+V (ein Schritt, nicht gespeichert). Vorher fenster_zeigen und nachfragen.".into(),
        props(&insert),
        &[],
        false,
    ));
    list.push(tool(
        "fenster_projekt_oeffnen",
        "Projekt in einem proAVA-Fenster öffnen; nennt die PID.".into(),
        props(&[
            ("projekt", text("")),
            ("schreibgeschuetzt", flag("")),
            ("fenster", text("")),
        ]),
        &["projekt"],
        false,
    ));
    list.push(tool(
        "fenster_speichern",
        "Projekt im Fenster speichern wie Strg+S (vorher Sicherung).".into(),
        props(&[("projekt", text("")), ("fenster", text(""))]),
        &[],
        false,
    ));
    list
}
