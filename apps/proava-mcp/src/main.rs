//! Composition root of the MCP server: wires project storage and the GAEB/PDF
//! adapter, then speaks MCP (JSON-RPC 2.0) over stdin/stdout.

use proava_application::SettingsFiles;
use proava_exchange::GaebPdfExchange;
use proava_infrastructure::{JsonSettingsFile, XmlProjectFile};
use proava_mcp::{
    allowed_roots_from_env, run_stdio, BridgeWindows, HelperWindow, LibraryDefaults, PrintDefaults,
    Server, SpellDefaults, TemplateDefaults,
};

fn main() {
    let settings = JsonSettingsFile::new().load();
    let server = Server::new(
        Box::new(XmlProjectFile::new()),
        Box::new(GaebPdfExchange::default()),
    )
    .with_allowed_roots(allowed_roots_from_env())
    .with_print_defaults({
        // Logo wie in proAVA (Extras ▸ Einstellungen ▸ Drucken); nur gelesen.
        PrintDefaults {
            logo_file: settings.preferences.logo_file.clone(),
            page_frame: settings.preferences.page_frame.clone(),
            cover_blocks: settings.preferences.cover_blocks,
            look: settings.preferences.print_look.clone(),
            author: settings.preferences.print_author.clone(),
        }
    })
    .with_library_defaults(LibraryDefaults {
        // Bibliotheken wie in proAVA (Einstellungen ▸ Neue Projekte); nur gelesen.
        own: settings.preferences.library_file.clone(),
        others: settings.preferences.libraries.clone(),
    })
    .with_template_defaults(TemplateDefaults {
        // Vorlagenordner und Vorgaben für neue Projekte wie in proAVA; nur gelesen.
        folder: Some(
            settings
                .preferences
                .template_folder
                .clone()
                .unwrap_or_else(|| {
                    proava_infrastructure::paths::home_dir()
                        .join(proava_application::TEMPLATE_FOLDER_NAME)
                }),
        ),
        preferences: settings.preferences.clone(),
    })
    .with_spell_defaults(SpellDefaults {
        // Wörterbuch-Ordner und eigene Wörter wie in proAVA; nur gelesen.
        dictionary_folder: settings.preferences.dictionary_folder.clone(),
        personal_words: settings.personal_words.clone(),
    })
    // AUSSCHREIBEN.DE: nur die veröffentlichten Katalogverzeichnisse (curl,
    // Zwischenspeicher wie in proAVA) und das Browserfenster für den Anwender.
    .with_web(std::sync::Arc::new(
        proava_infrastructure::CurlWebTexts::new(),
    ))
    .with_browser_window(Box::new(HelperWindow::new(
        proava_infrastructure::paths::download_dir(),
    )))
    // Updated while it runs (the old code keeps running): say so once.
    .with_update_check({
        let program = proava_infrastructure::program_update::ProgramFile::started_now();
        Box::new(move || program.replaced())
    })
    // Die geöffneten proAVA-Fenster dieses Benutzers (lokale Brücke).
    .with_windows(Box::new(BridgeWindows::standard()));

    if let Err(error) = run_stdio(server) {
        eprintln!("[proava-mcp] {error}");
        std::process::exit(1);
    }
}
