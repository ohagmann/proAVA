//! Kostenverfolgung of the synthetic demo (`proava_ui::demo::tracking_demo_project`)
//! as PDF in several views – for a visual check of the printout.
//!
//! ```sh
//! cargo run -p proava --example kostenverfolgung_druck -- /tmp/Kostenverfolgung
//! ```

use std::path::PathBuf;

use proava_application::{ExchangeFiles, PrintJob, PrintKind, PrintStyle};
use proava_domain::{TrackDeviation, TrackOptions, TrackView};
use proava_exchange::GaebPdfExchange;

fn main() -> Result<(), String> {
    let folder = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Aufruf: kostenverfolgung_druck <Ordner>")?;
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let project = proava_ui::demo::tracking_demo_project();
    let exchange = GaebPdfExchange;
    let views = [
        (
            "Ebene2_Kostenvorgabe_Klassisch",
            TrackOptions::default(),
            PrintStyle::Classic,
            false,
        ),
        (
            "Ebene3_Vorstufe_Stand_proAVA",
            TrackOptions {
                level: 3,
                deviation: TrackDeviation::Previous,
                snapshot: Some(0),
                ..TrackOptions::default()
            },
            PrintStyle::ProAva,
            true,
        ),
        (
            "Vergabeeinheiten_brutto",
            TrackOptions {
                level: 3,
                view: TrackView::ByLv,
                gross: true,
                ..TrackOptions::default()
            },
            PrintStyle::Classic,
            false,
        ),
    ];
    for (name, options, style, cover) in views {
        let mut job = PrintJob::new(PrintKind::CostTracking);
        job.tracking = options;
        job.settings.style = Some(style);
        job.settings.cover = cover;
        let file = folder.join(format!("Kostenverfolgung_{name}.pdf"));
        let outcome = exchange.export_print(&project, &job, &file)?;
        println!("{}: {} Seiten", file.display(), outcome.pages);
    }
    Ok(())
}
