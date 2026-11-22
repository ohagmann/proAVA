//! Manual check against real GAEB files that never enter the repository:
//! `PROAVA_LV=<LV file> PROAVA_BIDS="<bid>;<bid>" cargo test -p proava-exchange --test real_files -- --ignored --nocapture`

use std::path::Path;

use proava_application::ExchangeFiles;
use proava_domain::ProjectName;
use proava_exchange::GaebPdfExchange;

#[test]
#[ignore]
fn real_lv_and_bids() {
    let Ok(lv_file) = std::env::var("PROAVA_LV") else {
        eprintln!("PROAVA_LV nicht gesetzt");
        return;
    };
    let exchange = GaebPdfExchange;
    let (project, warnings) = exchange
        .import_lv(
            Path::new(&lv_file),
            &ProjectName::parse("Echt").unwrap(),
            "",
        )
        .unwrap();
    let lv = project.lv();
    println!(
        "LV: Maske {}, Schritt {}, {} Titel, {} Positionen, {} Hinweise, Summe {}",
        lv.oz_mask().pattern(),
        lv.position_step(),
        lv.group_count(),
        lv.position_count(),
        lv.note_count(),
        lv.net_total()
    );
    println!("Bilder im Projekt: {}", project.images().len());
    let surcharges = lv.surcharge_values();
    for position in lv.positions().filter(|position| position.is_surcharge()) {
        let value = surcharges.get(&position.id()).copied().unwrap_or_default();
        println!(
            "  Zuschlag {} ({}): {:?} % auf {} = {}",
            position.ordinal(),
            position
                .surcharge()
                .map(|s| s.kind.key())
                .unwrap_or_default(),
            position.unit_price(),
            value.base,
            value.amount
        );
    }
    for warning in &warnings {
        println!("  LV-Hinweis: {warning}");
    }
    for bid_file in std::env::var("PROAVA_BIDS")
        .unwrap_or_default()
        .split(';')
        .filter(|f| !f.is_empty())
    {
        match exchange.import_bid(&project, Path::new(bid_file)) {
            Ok(import) => {
                let net = import.bid.net_total(lv);
                println!(
                    "Angebot {}: {} Preise, netto {}, Nachlass {:?} = {}",
                    import.bid.name(),
                    import.bid.prices().len(),
                    net,
                    import.bid.discount(),
                    import.bid.discount_amount(net)
                );
                let values = import.bid.surcharge_values(lv);
                for position in lv.positions().filter(|position| position.is_surcharge()) {
                    let value = values.get(&position.id()).copied().unwrap_or_default();
                    println!(
                        "  Zuschlag {}: {:?} % auf {} = {}",
                        position.ordinal(),
                        import
                            .bid
                            .price(position.id())
                            .and_then(|price| price.unit_price),
                        value.base,
                        value.amount
                    );
                }
                for warning in &import.warnings {
                    println!("  {warning}");
                }
            }
            Err(error) => println!("Angebot {bid_file}: FEHLER {error}"),
        }
    }
}
