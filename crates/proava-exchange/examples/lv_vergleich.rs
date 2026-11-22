//! LV-Vergleich zweier GAEB-Dateien auf der Kommandozeile (nur lesen):
//!
//! ```sh
//! cargo run -p proava-exchange --example lv_vergleich -- ALT.X83 NEU.X83
//! ```

use std::path::Path;

use proava_application::ExchangeFiles;
use proava_domain::lv_diff::compare;
use proava_domain::ProjectName;
use proava_exchange::GaebPdfExchange;

fn old_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (old, new, pdf) = match args.as_slice() {
        [old, new] => (old, new, None),
        [old, new, pdf] => (old, new, Some(pdf)),
        _ => return Err("Aufruf: lv_vergleich ALT NEU [AUSGABE.pdf]".to_string()),
    };
    let read = |file: &str| {
        GaebPdfExchange
            .import_lv(
                Path::new(file),
                &ProjectName::parse("Vergleich").unwrap(),
                "2026-11-17T20:00:00",
            )
            .map(|(project, _)| project)
    };
    let (old, new) = (read(old)?, read(new)?);
    let _ = &args;
    let diff = compare(old.lv(), new.lv());
    println!("{}", diff.summary());
    println!("{}", diff.text());
    if let Some(pdf) = pdf {
        GaebPdfExchange.export_change_list_pdf(&new, old_name(&args[0]), &diff, Path::new(pdf))?;
        println!("PDF: {pdf}");
    }
    Ok(())
}
