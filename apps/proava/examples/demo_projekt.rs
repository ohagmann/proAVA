//! Writes the synthetic demo project (`proava_ui::demo`, four bids) as a
//! `.proava` file – for screenshots of the Handbuch and for trying things out.
//!
//! ```sh
//! cargo run -p proava --example demo_projekt -- /tmp/Demo/Demo.proava
//! cargo run -p proava --example demo_projekt -- /tmp/Demo/Abrechnung.proava abrechnung
//! cargo run -p proava --example demo_projekt -- /tmp/Demo/Kosten.proava kosten
//! cargo run -p proava --example demo_projekt -- /tmp/Demo/Voll.proava voll
//! cargo run -p proava --example demo_projekt -- /tmp/Demo/Verfolgung.proava kostenverfolgung
//! cargo run -p proava --example demo_projekt -- /tmp/Demo/GU.proava gu
//! cargo run -p proava --example demo_projekt -- /tmp/Demo/GA.proava gakurztext
//! ```
//!
//! With `abrechnung`: the demo with contract, change order and invoices.
//! With `kosten`: the demo with a Kostenschätzung and a Kostenberechnung.
//! With `voll`: everything at once (STLB-Bau, lists, second LV with
//! Pauschalsumme, Zuschlag and Schlussbemerkung, Kosten-LV) – as the Affentest.
//! With `kostenverfolgung`: several trades and GA with budget, bids,
//! contracts, change orders and invoices (Reiter Kostenverfolgung).
//! With `gu`: GU-Fall – one Kostenberechnung KG 400 as Kosten-LV with
//! Kurztext positions (KG 410–430) and the Datenpunkt-Abschnitt 480.
//! With `gakurztext`: the GA as Kurztext – Datenpunkte as Rechengrundlage and
//! Aufteilungszeilen KG 481–489 below them.

use std::path::PathBuf;

use proava_application::ProjectRepository;
use proava_infrastructure::XmlProjectFile;

fn main() -> Result<(), String> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Aufruf: demo_projekt <Datei.proava>")?;
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let project = match std::env::args().nth(2).as_deref() {
        Some("abrechnung") => proava_ui::demo::billing_demo_project(),
        Some("kosten") => proava_ui::demo::cost_demo_project(),
        Some("kostenverfolgung") => proava_ui::demo::tracking_demo_project(),
        Some("kostenlv") => proava_ui::demo::cost_lv_demo_project(),
        Some("gu") => proava_ui::demo::gu_demo_project(),
        Some("gakurztext") => proava_ui::demo::ga_kurztext_demo_project(),
        Some("voll") => proava_ui::demo::full_demo_project(),
        _ => proava_ui::demo::demo_project(),
    };
    XmlProjectFile::new().save_project(&path, &project)?;
    println!("geschrieben: {}", path.display());
    Ok(())
}
