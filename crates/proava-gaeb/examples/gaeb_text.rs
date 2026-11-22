//! Prints the Kurz- and Langtext of one position as the codec reads it
//! (paragraphs separated by a blank line, list items with "- ").
//!
//! ```sh
//! cargo run -p proava-gaeb --example gaeb_text -- datei.x83 01.02.0030
//! ```

use proava_gaeb::{datan, xml, TextBlock};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, ordinal] = args.as_slice() else {
        eprintln!("Aufruf: gaeb_text <datei> <OZ>");
        std::process::exit(2);
    };
    let path = std::path::Path::new(file);
    let is_xml = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.to_ascii_lowercase().starts_with('x'));
    let doc = if is_xml {
        xml::read_file(path).map_err(|e| e.to_string())
    } else {
        datan::read_file(path)
            .map(|r| r.document)
            .map_err(|e| e.to_string())
    }
    .unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    let wanted: Vec<&str> = ordinal
        .split('.')
        .map(|p| p.trim_start_matches('0'))
        .collect();
    for (pieces, item) in doc.award.boq.items() {
        let key: Vec<&str> = pieces.iter().map(|p| p.trim_start_matches('0')).collect();
        if key != wanted {
            continue;
        }
        println!("{} – {}\n", pieces.join("."), item.short_text);
        for block in &item.long_text.blocks {
            match block {
                TextBlock::Paragraph { runs, .. } => {
                    println!(
                        "{}\n",
                        runs.iter().map(|r| r.text.as_str()).collect::<String>()
                    )
                }
                TextBlock::ListItem { runs, .. } => {
                    println!(
                        "- {}",
                        runs.iter().map(|r| r.text.as_str()).collect::<String>()
                    )
                }
                TextBlock::Table(rows) => println!("[Tabelle, {} Zeilen]\n", rows.rows.len()),
                TextBlock::Image { file_name, .. } => println!("[Bild {file_name}]\n"),
            }
        }
    }
}
