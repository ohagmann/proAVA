//! Aufmaßprüfung and Rechnungsfreigabe of every invoice of the synthetic
//! billing demo (`proava_ui::demo::billing_demo_project`) – for a visual
//! check of the printouts.
//!
//! ```sh
//! cargo run -p proava --example abrechnung_druck -- /tmp/Abrechnung
//! ```

use std::path::PathBuf;

use proava_application::{ExchangeFiles, PrintJob, PrintKind};
use proava_exchange::GaebPdfExchange;

fn main() -> Result<(), String> {
    let folder = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Aufruf: abrechnung_druck <Ordner>")?;
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let project = proava_ui::demo::billing_demo_project();
    let exchange = GaebPdfExchange;
    for invoice in project.billing().invoices() {
        for kind in [PrintKind::BillingCheck, PrintKind::Release] {
            let mut job = PrintJob::new(kind);
            job.invoice = Some(invoice.id);
            let file = folder.join(format!("{}_{}.pdf", invoice.number, kind.file_label()));
            let outcome = exchange.export_print(&project, &job, &file)?;
            println!("{}: {} Seiten", file.display(), outcome.pages);
        }
    }
    Ok(())
}
