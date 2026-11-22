//! All visible German texts of the start screen. One place to change wording.

pub const APP_TITLE: &str = "proAVA";
pub const APP_SUBTITLE: &str = "Ausschreibung · Vergabe · Abrechnung";
pub const WINDOW_TITLE: &str = "proAVA – Projekte";

pub const SECTION_RECENT: &str = "Zuletzt verwendet";
pub const SECTION_PROJECT: &str = "Projekt";
pub const SECTION_BACKUP: &str = "Sicherung";

pub const SEARCH_HINT: &str = "Projekt suchen …";
pub const NEW_PROJECT: &str = "Neues Projekt …";
pub const OPEN_PROJECT: &str = "Projekt öffnen …";
pub const CREATE_BACKUP: &str = "Sicherung anlegen …";
pub const RESTORE_BACKUP: &str = "Sicherung wiederherstellen …";
pub const FORGET_PROJECT: &str = "Aus der Liste entfernen";
pub const SETTINGS_HINT: &str = "Einstellungen – Aussehen und Standardordner";

pub const EMPTY_TITLE: &str = "Noch keine Projekte";
pub const EMPTY_HINT: &str = "Lege ein neues Projekt an oder öffne ein vorhandenes.";

pub const BACKUP_HINT: &str =
    "Sicherungen liegen als Zip neben dem Projekt und tragen Datum und Uhrzeit.";
pub const STATUS_MCP: &str = "MCP-Server: proava-mcp";

pub const DIALOG_NEW_TITLE: &str = "Neues Projekt";
pub const DIALOG_NEW_HINT: &str =
    "Ein Projekt liegt in einem eigenen Ordner und enthält das Leistungsverzeichnis.";
pub const DIALOG_OPEN_TITLE: &str = "Projekt öffnen";
pub const DIALOG_OPEN_HINT: &str = "Pfad zur proAVA-Projektdatei (.proava).";
pub const DIALOG_BACKUP_TITLE: &str = "Sicherung anlegen";
pub const DIALOG_BACKUP_HINT: &str = "Das Projekt wird als Zip mit Datum und Uhrzeit gesichert.";
pub const DIALOG_RESTORE_TITLE: &str = "Sicherung wiederherstellen";
pub const DIALOG_RESTORE_HINT: &str = "Kopie der Sicherung; das Original bleibt unberührt.";

pub const DIALOG_SETTINGS_TITLE: &str = "Einstellungen";
/// Section of Extras ▸ Einstellungen ▸ Darstellung (editor and print).
pub const TEXT_SPACING_SECTION: &str = "Zeilen- und Absatzabstand der Texte";
pub const THEME_LIGHT: &str = "Hell";
pub const THEME_DARK: &str = "Dunkel";
pub const CHOOSE_FOLDER: &str = "Ordner wählen …";
/// Rechtsklick auf Zeilen und Menü LV ▸ Bibliothek (one command).
pub const LIBRARY_STORE: &str = "In Bibliothek übernehmen …";
pub const LIBRARY_STORE_TITLE: &str = "In Bibliothek übernehmen";
pub const LIBRARY_NEW: &str = "Neue Bibliothek anlegen …";

pub const PICKER_TITLE: &str = "Ordner wählen";
pub const PICKER_HINT: &str =
    "Ein Klick wählt einen Unterordner, ein Doppelklick öffnet ihn. Der Knopf unten \
                               übernimmt den gewählten Ordner, ohne Wahl den gezeigten.";
pub const PICKER_UP: &str = "Nach oben";

pub const FIELD_NAME: &str = "Name";
pub const FIELD_FOLDER: &str = "Ordner";
pub const FIELD_FILE: &str = "Datei";
pub const FIELD_TARGET_FOLDER: &str = "Zielordner";

pub const CANCEL: &str = "Abbrechen";
pub const CREATE: &str = "Anlegen";
pub const OPEN: &str = "Öffnen";
pub const RESTORE: &str = "Wiederherstellen";
pub const DONE: &str = "Fertig";

// --- Arbeitsbereich (Leistungsverzeichnis) ---

/// Knopf links vor den Reitern: Projekt schließen, zurück zur Startseite.
pub const WS_BACK: &str = "Start";
pub const WS_BACK_HINT: &str = "Projekt schließen, zurück zur Startseite";
pub const WS_SAVE: &str = "Speichern";
pub const WS_COL_OZ: &str = "OZ";
pub const WS_COL_STATUS: &str = "Status";
pub const WS_COL_TEXT: &str = "Kurztext";
pub const WS_COL_QTY: &str = "Menge";
pub const WS_COL_UNIT: &str = "Einheit";
pub const WS_COL_EP: &str = "EP";
pub const WS_COL_GP: &str = "GP";
pub const WS_EMPTY_TITLE: &str = "Noch keine Zeilen";
pub const WS_NO_SELECTION: &str = "Zeile in der Liste wählen, um sie zu bearbeiten.";

pub const WS_LEAVE_TITLE: &str = "Projekt verlassen";
pub const WS_LEAVE_HINT: &str = "Das Projekt ist ungesichert.";
pub const WS_SAVE_AND_LEAVE: &str = "Speichern und zurück";
pub const WS_QUIT_TITLE: &str = "proAVA beenden";
pub const WS_SAVE_AND_QUIT: &str = "Speichern und beenden";
pub const WS_RESTART_TITLE: &str = "proAVA neu starten";
pub const WS_SAVE_AND_RESTART: &str = "Speichern und neu starten";
pub const WS_DISCARD: &str = "Verwerfen";

pub const WS_STLB_DETAIL: &str = "Platzhalter aus STLB-Bau – Text und Preis noch offen.";
pub const STLB_TEXT_CHANGED: &str = "Text gegenüber STLB-Bau geändert – die Kennung bleibt, STLB-Bau Check würde die Position bemängeln.";
pub const STLB_DIALOG_TITLE: &str = "Aus STLB-Bau übernehmen";
pub const STLB_DIALOG_HINT: &str = "In STLB-Bau die Positionen bilden und als GAEB-Datei speichern (STLB-Bau Online: Leistungsverzeichnis, Export GAEB). Die Datei aus dem Browserfenster oder eine neue Datei im Download-Ordner wird eingelesen und übernommen; der Dialog bleibt für die nächste Position offen. Die STLB-Bau-Kennung bleibt an der Position.";
pub const STLB_TAKE_BUTTON: &str = "Aus STLB-Bau übernehmen …";
pub const STLB_TAKE_HINT: &str = "Position in STLB-Bau bilden, als GAEB-Datei speichern und hier einlesen – der Platzhalter wird ersetzt.";

pub const AUSSCHREIBEN_DIALOG_TITLE: &str = "Aus AUSSCHREIBEN.DE übernehmen";
pub const AUSSCHREIBEN_DIALOG_HINT: &str = "Herstellertexte von AUSSCHREIBEN.DE (kostenlos, ohne Anmeldung): im Browser den Text oder die Merkliste als GAEB exportieren. Die Datei aus dem Browserfenster oder eine neue Datei im Download-Ordner wird eingelesen und übernommen; der Dialog bleibt für den nächsten Text offen. Der Text bleibt wie geliefert, die Textquelle steht im Fenster der Position.";
pub const GAEB_DROP_DIALOG_TITLE: &str = "Positionen aus GAEB-Datei übernehmen";
pub const GAEB_DROP_DIALOG_HINT: &str = "Positionen wählen und an der Ablagestelle einfügen. Weitere Dateien können auf die LV-Tabelle gezogen oder im Download-Ordner gespeichert werden.";
pub const DROP_HINT: &str = "GAEB-Datei hier ablegen – Positionen werden an dieser Stelle eingefügt (Bilder: in den Langtext der gewählten Position)";

pub const SIDE_LEFT: &str = "Links";
pub const SIDE_RIGHT: &str = "Rechts";

// --- Dateidialog ---

pub const FD_SAVE: &str = "Speichern";
pub const FD_EMPTY: &str = "Keine passenden Dateien oder Ordner";
pub const FD_FILE_NAME: &str = "Dateiname";
pub const FD_PICK_ONE: &str = "Bitte eine Datei wählen.";
pub const FD_PICK_MANY: &str = "Dateien wählen – mit Strg+Klick mehrere";
pub const FD_NAME_MISSING: &str = "Bitte einen Dateinamen eintragen.";

// --- Arbeitsbereich: Rahmen ---

pub const NAV_LV: &str = "LV";
pub const NAV_BIDS: &str = "Angebote";
pub const NAV_COMPARISON: &str = "Preisspiegel";
pub const NAV_BILLING: &str = "Abrechnung";
pub const MENU_PROJECT: &str = "Projekt";
pub const MENU_EDIT: &str = "Bearbeiten";
pub const MENU_INSERT: &str = "Einfügen";
pub const MENU_LV: &str = "LV";
pub const MENU_REVIEW: &str = "Prüfen";
pub const MENU_GAEB: &str = "GAEB";
pub const MENU_BIDS: &str = "Angebote";
pub const MENU_PRINT: &str = "Drucken";
pub const MENU_SAVE: &str = "Speichern";
pub const MENU_BACKUP: &str = "Sicherung anlegen";
pub const MENU_MASK: &str = "Gliederung und OZ-Maske …";
/// Below the GA template in „Neues Projekt“ when the OZ-Maske has no
/// three-digit level (titles 01, 02 … instead of 481, 482 …).
pub const GA_TEMPLATE_MASK_HINT: &str =
    "\nDie Titel bekommen 01, 02 … (KG 481–489 steht am Titel). \
     Für die OZ 481 … braucht die OZ-Maske eine dreistellige Stufe wie die Vorgabe der \
     Vorlage 11.222.PPPPI.";
pub const MENU_CHECK: &str = "LV prüfen …";
pub const MENU_IMPORT_BIDS: &str = "Angebote einlesen (X84/D84) …";

pub const CHECK_TITLE: &str = "Prüfung vor dem Export";
pub const CHECK_TITLE_LV: &str = "LV prüfen";
pub const CHECK_TITLE_ALL: &str = "Alle LVs prüfen";
pub const MENU_CHECK_ALL: &str = "Alle LVs prüfen …";
pub const CHECK_OK: &str = "Keine Auffälligkeiten.";
pub const CHECK_ANYWAY: &str = "Trotzdem exportieren";
pub const WARNINGS_TITLE: &str = "Hinweise";

pub const IMPORT_GAEB: &str = "Neues Projekt aus GAEB …";
pub const IMPORT_GAEB_TITLE: &str = "Neues Projekt aus GAEB";
pub const IMPORT_GAEB_HINT: &str = "Das LV der GAEB-Datei wird in ein neues Projekt übernommen.";
pub const IMPORT_CSV_TITLE: &str = "Neues Projekt aus CSV-Tabelle";
pub const IMPORT_CSV_HINT: &str = "Das LV der CSV-Tabelle (OZ, Positionstext, Einheit, Menge, EP, GP, KG, Positionsart) wird \
                                   in ein neues Projekt übernommen; GP-Abweichungen nennt das Importprotokoll.";
/// „Neues Projekt aus GAEB …“ on the start screen also takes CSV tables.
pub const NEW_PROJECT_EXTENSIONS: [&str; 12] = [
    "x81", "x82", "x83", "x84", "x86", "d81", "d82", "d83", "d84", "d86", "csv", "tsv",
];
pub const GAEB_EXTENSIONS: [&str; 10] = [
    "x81", "x82", "x83", "x84", "x86", "d81", "d82", "d83", "d84", "d86",
];
pub const BID_EXTENSIONS: [&str; 2] = ["x84", "d84"];
/// Abrechnung: Auftrags-LV.
pub const CONTRACT_EXTENSIONS: [&str; 2] = ["x86", "d86"];
/// Abrechnung: Rechnung und Mengenermittlung.
pub const INVOICE_EXTENSIONS: [&str; 3] = ["x89", "x89b", "x31"];
pub const IMAGE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];
pub const WS_STEP_LABEL: &str = "Positionsnummern in Schritten";

/// Haken im Dialog „Angebote eingelesen“ (Anwender 18.11.).
pub const ADOPT_BIDDERS: &str = "Ins Bieterverzeichnis aufnehmen";
pub const ADOPT_BIDDERS_HINT: &str = "Firmen der Angebote im Bieterverzeichnis anlegen („Angebot abgegeben“); \
     vorhandene Einträge mit gleichem Namen oder gleicher Anschrift werden zugeordnet, nicht doppelt angelegt. \
     Nötig z. B. für Zuschlags- und Absageschreiben.";

/// Mengen nach Angebotseingang geändert (Anwender 18.11.).
pub const QUANTITY_CHANGE_ORDER: &str = "Als Mehr-/Mindermenge in Nachtrag übernehmen …";
pub const QUANTITY_CHANGE_ORDER_HINT: &str = "Legt einen Nachtrag (Mengenänderung, § 2 Abs. 3 VOB/B) mit der Differenz \
     je Position zum Auftrags-EP an; ohne festgeschriebenen Auftrag wird zuerst das beauftragte Angebot übernommen.";

/// Nach dem Versand geändert (Anwender 18.11.).
pub const SENT_CHANGE_TITLE: &str = "LV bereits versandt";
pub const SENT_CHANGE_KEEP: &str = "Trotzdem ändern";
pub const SENT_CHANGE_KEPT: &str = "Geändert – das LV muss neu versendet werden (bis zum nächsten X83-Export keine weitere Rückfrage).";
pub const SENT_CHANGE_UNDONE: &str = "Änderung zurückgenommen – das LV bleibt wie versandt.";
pub const SENT_CHANGED: &str = "Seit dem Versand geändert – neu versenden";

/// Positionen kommen oder gehen in einem LV mit Angeboten oder Auftrag – auch
/// ohne Versand aus proAVA (Anwender 19.11.).
pub const BIDS_CHANGE_TITLE: &str = "LV hat bereits Angebote";
pub const ORDER_CHANGE_TITLE: &str = "LV hat bereits einen Auftrag";
pub const BIDS_CHANGE_KEPT: &str =
    "Beibehalten – Angebote bzw. Auftrag passen nicht mehr ganz zum LV (keine weitere \
     Rückfrage, bis neue Angebote kommen oder sich der Auftrag ändert).";
pub const BIDS_CHANGE_UNDONE: &str =
    "Änderung zurückgenommen – das LV passt wieder zu Angeboten und Auftrag.";
pub const AS_CHANGE_ORDER: &str = "Als Nachtrag anlegen …";

/// Kopfzeile der Fenster „Eigenschaften“ und „Langtext“: das Fenster folgt
/// der Auswahl der Tabelle, außer es ist festgehalten (Wunsch 18.11.2026).
pub const HOLD_WINDOW: &str = "Festhalten";
pub const HOLD_WINDOW_HINT: &str = "Das Fenster bleibt bei dieser Zeile, auch wenn du in der Tabelle eine andere wählst – zum Vergleichen zweier Zeilen. Ohne Haken zeigt es immer die gewählte Zeile.";

/// LV-Auswahl in der Kopfleiste: nur zum Wechseln (Anwender 18.11.).
pub const LV_CHOICE_LABEL: &str = "LV:";
pub const LV_CHOICE_HINT: &str =
    "Diese Ansicht gilt für ein LV. Gewählt ist das LV der Schreibmarke in der LV-Tabelle.";
