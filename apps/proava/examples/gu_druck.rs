//! GU-Fall: the Kostenermittlung (Kosten-LV, DIN 276) of the synthetic demo
//! (`proava_ui::demo::gu_demo_project` – Kurztext positions KG 410–430 and the
//! Datenpunkt-Abschnitt 480) as PDF, Klassisch and proAVA – for a visual
//! check of the Zusammenstellung and the appendix „Datenpunkte“.
//!
//! ```sh
//! cargo run -p proava --example gu_druck -- /tmp/GU
//! cargo run -p proava --example gu_druck -- /tmp/GA gakurztext
//! ```
//!
//! With `gakurztext`: the GA as Kurztext with Aufteilungszeilen
//! (`proava_ui::demo::ga_kurztext_demo_project`).

use std::path::PathBuf;

use proava_application::{ExchangeFiles, PrintJob, PrintKind, PrintStyle};
use proava_exchange::GaebPdfExchange;

fn main() -> Result<(), String> {
    let folder = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Aufruf: gu_druck <Ordner>")?;
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let kurztext = std::env::args().nth(2).as_deref() == Some("gakurztext");
    let project = if kurztext {
        proava_ui::demo::ga_kurztext_demo_project()
    } else {
        proava_ui::demo::gu_demo_project()
    };
    let stem = if kurztext { "GA_Kurztext" } else { "GU" };
    let exchange = GaebPdfExchange;
    for (name, style) in [
        ("Klassisch", PrintStyle::Classic),
        ("proAVA", PrintStyle::ProAva),
    ] {
        let mut job = PrintJob::new(PrintKind::CostLv);
        job.settings.style = Some(style);
        let file = folder.join(format!("{stem}_Kostenberechnung_{name}.pdf"));
        let outcome = exchange.export_print(&project, &job, &file)?;
        println!("{}: {} Seiten", file.display(), outcome.pages);
    }
    Ok(())
}
