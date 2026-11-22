//! Whole award run from the command line, for checks with real files:
//! LV from GAEB → bids → GAEB export, PDFs and project file.
//!
//! `PROAVA_LV=<lv.x83> PROAVA_BIDS="<a.x84>;<b.d84>" PROAVA_OUT=<folder>
//!  cargo run -p proava-exchange --example vergabe`
//!
//! The bid ranked first (or the one whose name contains `PROAVA_AWARD`)
//! gets the contract; its Auftrags-LV is written as X86, D86 and PDF.

use std::path::{Path, PathBuf};

use proava_application::{ComparisonPrint, ContractExport, ExchangeFiles, GaebExport, PdfDocument};
use proava_domain::{ComparisonOptions, PriceComparison, ProjectName};
use proava_exchange::GaebPdfExchange;

fn main() -> Result<(), String> {
    let lv_file = PathBuf::from(std::env::var("PROAVA_LV").map_err(|_| "PROAVA_LV fehlt")?);
    let out =
        PathBuf::from(std::env::var("PROAVA_OUT").unwrap_or_else(|_| "/tmp/proava/vergabe".into()));
    std::fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    let exchange = GaebPdfExchange;

    let (mut project, warnings) = exchange.import_lv(
        &lv_file,
        &ProjectName::parse("Vergabe").map_err(|error| error.to_string())?,
        "2026-11-17T18:00:00",
    )?;
    println!(
        "LV: {} Positionen, {} Titel, Maske {}",
        project.lv().position_count(),
        project.lv().group_count(),
        project.lv().oz_mask().pattern()
    );
    for warning in &warnings {
        println!("  LV: {}", shorten(warning));
    }

    for bid_file in std::env::var("PROAVA_BIDS")
        .unwrap_or_default()
        .split(';')
        .filter(|file| !file.is_empty())
    {
        let import = exchange.import_bid(&project, Path::new(bid_file))?;
        println!(
            "Angebot {} ({} Bieterangaben):",
            import.bid.name(),
            import
                .bid
                .complements()
                .values()
                .map(Vec::len)
                .sum::<usize>()
        );
        for warning in &import.warnings {
            println!("  {}", shorten(warning));
        }
        project.add_bid(import.bid);
    }

    let comparison =
        PriceComparison::build(project.lv(), project.bids(), ComparisonOptions::default());
    for summary in &comparison.bids {
        println!(
            "Rang {}: {} netto {} (+{} %)",
            summary.rank, summary.name, summary.net_after_discount, summary.above_lowest_percent
        );
    }

    let wanted = std::env::var("PROAVA_AWARD")
        .unwrap_or_default()
        .to_lowercase();
    let winner = comparison
        .bids
        .iter()
        .find(|summary| {
            if wanted.is_empty() {
                summary.rank == 1
            } else {
                summary.name.to_lowercase().contains(&wanted)
            }
        })
        .map(|summary| summary.bid);
    if let Some(winner) = winner {
        let total = project.award_bid(winner, "13.02.2026")?;
        let name = project
            .awarded_bid()
            .map(|bid| bid.name().to_string())
            .unwrap_or_default();
        println!("Auftrag an {name}: Auftragssumme netto {total}");
        for kind in [
            ContractExport::Xml,
            ContractExport::Datan,
            ContractExport::Pdf,
        ] {
            let file = out.join(format!("Auftrags-LV.{}", kind.extension()));
            match exchange.export_contract(&project, winner, kind, &file) {
                Ok(warnings) => {
                    for warning in warnings {
                        println!("  {}: {}", kind.extension(), shorten(&warning));
                    }
                }
                Err(error) => println!("  {}: FEHLER {}", kind.extension(), shorten(&error)),
            }
        }
        let texts: usize = project
            .awarded_bid()
            .map(|bid| bid.complements().values().map(Vec::len).sum())
            .unwrap_or(0);
        println!("  Bieterangaben (Textergänzungen): {texts}");
    }

    exchange.export_gaeb(
        &project,
        GaebExport::RequestForQuotation,
        &out.join("LV.X83"),
    )?;
    exchange.export_pdf(
        &project,
        PdfDocument::RequestForQuotation,
        &out.join("Angebotsaufforderung.pdf"),
    )?;
    exchange.export_price_comparison_pdf(
        &project,
        &ComparisonPrint::default(),
        &out.join("Preisspiegel.pdf"),
    )?;
    println!("geschrieben nach {}", out.display());
    Ok(())
}

fn shorten(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    if characters.len() > 220 {
        format!("{}…", characters[..220].iter().collect::<String>())
    } else {
        text.to_string()
    }
}
