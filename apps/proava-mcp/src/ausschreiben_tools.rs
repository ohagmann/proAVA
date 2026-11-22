//! AUSSCHREIBEN.DE for assistants (`docs/FACHLICHES.md`, texts of third parties):
//!
//! - `ausschreiben_suchen` reads only the catalogue lists AUSSCHREIBEN.DE
//!   publishes for AI assistants (`tender_index`: robots.txt first, cache of a
//!   few days) – title, manufacturer, article number, link; no long text.
//! - `ausschreiben_oeffnen` opens a hit in proAVA's browser window for
//!   the user, who exports there himself; the file then goes into the LV with
//!   `stlb_uebernehmen` (`quelle` „AUSSCHREIBEN.DE“).
//!
//! Neither tool clicks, searches or downloads on the web site itself.

use proava_application::tender_index;
use proava_web_protocol::{window_url, Site};

use crate::args::Args;
use crate::server::Server;

/// Hits listed without `max_treffer`.
const DEFAULT_HITS: usize = 50;

pub(crate) fn search(server: &mut Server, args: &Args) -> Result<String, String> {
    let query = args.text("suche")?.trim().to_string();
    if query.is_empty() {
        return Err(
            "Bitte suche angeben: Hersteller oder Katalog und Begriffe, z. B. „Belimo Drehantrieb 20 Nm“."
                .into(),
        );
    }
    let limit = match args.get("max_treffer").and_then(serde_json::Value::as_u64) {
        Some(0) | None => DEFAULT_HITS,
        Some(limit) => (limit as usize).min(tender_index::MAX_HITS),
    };
    let result = tender_index::search(server.web_texts(), &query)?;
    let catalogs: Vec<String> = result
        .catalogs
        .iter()
        .map(|catalog| {
            if catalog.manufacturer.is_empty() || catalog.manufacturer == catalog.name {
                catalog.name.clone()
            } else {
                format!("{} ({})", catalog.name, catalog.manufacturer)
            }
        })
        .collect();
    let mut out = format!(
        "AUSSCHREIBEN.DE, Katalogverzeichnisse für KI-Assistenten (Titel und Artikelnummer, ohne Langtext): {} in {}{}.",
        count(result.hits.len(), "Treffer", "Treffer"),
        catalogs.join(", "),
        if result.truncated {
            " – gekürzt, Suche verfeinern"
        } else {
            ""
        }
    );
    for hit in result.hits.iter().take(limit) {
        out.push_str(&format!("\n- {}", hit.title));
        if !hit.article.is_empty() {
            out.push_str(&format!(" · Art.-Nr. {}", hit.article));
        }
        out.push_str(&format!(
            " · {} › {} · {}",
            hit.catalog, hit.folder, hit.url
        ));
    }
    if result.hits.len() > limit {
        out.push_str(&format!(
            "\n… {} weitere (max_treffer erhöhen oder Begriffe ergänzen).",
            result.hits.len() - limit
        ));
    }
    if let Some(catalog) = result.catalogs.first() {
        out.push_str(&format!("\nKatalogseite: {}", catalog.page_url()));
    }
    out.push_str(
        "\nWeiter: ausschreiben_oeffnen mit url öffnet einen Treffer im Browserfenster für den Anwender; er exportiert dort als GAEB, danach stlb_uebernehmen (datei, quelle „AUSSCHREIBEN.DE“).",
    );
    Ok(out)
}

pub(crate) fn open(server: &mut Server, args: &Args) -> Result<String, String> {
    let url = args.opt_text("url")?.unwrap_or_default();
    let url = window_url(Site::Ausschreiben, url)?;
    let window = server.browser_window().ok_or_else(|| {
        format!(
            "Kein Browserfenster in diesem Server. Der Anwender kann die Seite im Browser öffnen: {url}"
        )
    })?;
    window.open(&url)
}

fn count(number: usize, one: &str, many: &str) -> String {
    format!("{number} {}", if number == 1 { one } else { many })
}
