//! All printouts of the print dialog for a real LV, for a visual check:
//! every kind with default options plus a few variants, with page counts.
//!
//! `PROAVA_LV=<lv> PROAVA_BIDS="<a.x84>;<b.d84>" PROAVA_OUT=<folder>
//!  cargo run -p proava-exchange --example druck --release`
//!
//! The bid ranked first gets the award (for the Auftrags-LV). Only reads
//! the GAEB files; writes PDFs into `PROAVA_OUT`.

use std::path::{Path, PathBuf};

use proava_application::{ExchangeFiles, PrintJob, PrintKind};
use proava_domain::{ComparisonOptions, PriceComparison, ProjectName};
use proava_exchange::GaebPdfExchange;

fn main() -> Result<(), String> {
    let lv_file = PathBuf::from(std::env::var("PROAVA_LV").map_err(|_| "PROAVA_LV fehlt")?);
    let out =
        PathBuf::from(std::env::var("PROAVA_OUT").unwrap_or_else(|_| "/tmp/proava/print3".into()));
    std::fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    let exchange = GaebPdfExchange;
    let (mut project, _) = exchange.import_lv(
        &lv_file,
        &ProjectName::parse("Druck").map_err(|error| error.to_string())?,
        "2026-11-17T20:00:00",
    )?;
    for bid_file in std::env::var("PROAVA_BIDS")
        .unwrap_or_default()
        .split(';')
        .filter(|file| !file.is_empty())
    {
        let import = exchange.import_bid(&project, Path::new(bid_file))?;
        project.add_bid(import.bid);
    }
    let comparison =
        PriceComparison::build(project.lv(), project.bids(), ComparisonOptions::default());
    let first = comparison
        .bids
        .iter()
        .find(|summary| summary.rank == 1)
        .map(|summary| summary.bid);
    if let Some(first) = first {
        project.award_bid(first, "13.02.2026")?;
    }
    let first_title = project
        .lv()
        .rows()
        .iter()
        .find(|row| row.is_group())
        .map(|row| row.id());

    let mut jobs: Vec<(String, PrintJob)> = Vec::new();
    for kind in PrintKind::ALL {
        if kind.needs_bid() && first.is_none()
            || kind == PrintKind::PriceComparison && first.is_none()
        {
            continue;
        }
        if kind.needs_invoice() && project.billing().invoices().is_empty() {
            continue;
        }
        let mut job = PrintJob::new(kind);
        job.bid = first.filter(|_| kind.needs_bid());
        jobs.push((kind.key().to_string(), job));
    }
    let mut variant = PrintJob::new(PrintKind::PricedLv);
    variant.settings.long_texts = false;
    jobs.push(("bepreist_ohne_langtext".into(), variant));
    let mut variant = PrintJob::new(PrintKind::PricedLv);
    variant.settings.landscape = true;
    variant.settings.font_size_pt = 11;
    variant.settings.title_page_break = true;
    variant.settings.header_text = "Vorabzug – nicht zur Ausführung".into();
    variant.settings.footer_text = "Aufgestellt: Planungsbüro Muster".into();
    variant.settings.cover = true;
    variant.titles = first_title.into_iter().collect();
    jobs.push(("bepreist_quer_11pt_titel1".into(), variant));
    let mut variant = PrintJob::new(PrintKind::RequestForQuotation);
    variant.settings.notes = false;
    variant.settings.performance = false;
    jobs.push(("blanko_ohne_hinweise".into(), variant));

    // Logo as from Einstellungen ▸ Drucken (PROAVA_LOGO = PNG/JPEG).
    let logo = std::env::var("PROAVA_LOGO").ok().and_then(|file| {
        Some(proava_application::PdfLogo {
            file_name: Path::new(&file).file_name()?.to_string_lossy().to_string(),
            bytes: std::fs::read(&file).ok()?,
        })
    });
    // Druckstil for all jobs (PROAVA_STIL = klassisch | proava).
    let style = std::env::var("PROAVA_STIL")
        .ok()
        .and_then(|key| proava_application::PrintStyle::from_key(&key));
    for (name, mut job) in jobs {
        job.logo = logo.clone();
        if style.is_some() {
            job.settings.style = style;
        }
        let file = out.join(format!("{name}.pdf"));
        let start = std::time::Instant::now();
        match exchange.export_print(&project, &job, &file) {
            Ok(outcome) => println!(
                "{name}: {} Seiten ({:.1} s){}",
                outcome.pages,
                start.elapsed().as_secs_f32(),
                if outcome.warnings.is_empty() {
                    String::new()
                } else {
                    format!(" – {} Hinweise", outcome.warnings.len())
                }
            ),
            Err(error) => println!("{name}: FEHLER {error}"),
        }
    }
    Ok(())
}
