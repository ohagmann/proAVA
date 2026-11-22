//! Writes the Kostenschätzung and Kostenberechnung of the synthetic demo
//! school as PDF to `PROAVA_OUT` (default /tmp/proava/kosten/), for a
//! visual check of the layout. Starts no other program.
//!
//!   cargo run -p proava-exchange --example kosten_druck

use proava_domain::cost_demo::demo_cost_estimate;
use proava_domain::{CostStage, Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};
use proava_print::{render_cost_estimate_pdf_counted, PrintCostOptions, PrintLook, PrintStyle};

fn main() {
    let out = std::env::var("PROAVA_OUT").unwrap_or_else(|_| "/tmp/proava/kosten".to_string());
    std::fs::create_dir_all(&out).expect("Ausgabeordner");
    let mut lv = Lv::new(
        "Gebäudeautomation",
        OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
    );
    for (code, title, price) in [
        ("481", "Automation", 91_500),
        ("482", "Schaltschränke", 24_800),
        ("484", "Kabel", 69_900),
    ] {
        let mut group = LvGroup::new(0, "", title);
        group.set_cost_group(Some(code.into()));
        lv.push_group(group);
        lv.push_position(Position::new(
            "",
            title,
            Decimal::ONE,
            "psch",
            Some(Decimal::from(price)),
        ));
    }
    lv.renumber();
    let mut project = Project::new(ProjectName::parse("Demo Schule").unwrap(), "2026-11-18", lv);
    project.info_mut().label = "Neubau und Sanierung Grundschule – Gebäudeautomation".into();
    let estimate = project.add_cost_estimate(demo_cost_estimate(CostStage::Estimate));
    let calculation = project.add_cost_estimate(demo_cost_estimate(CostStage::Calculation));
    for (name, id, cover, style) in [
        ("kostenschaetzung", estimate, false, PrintStyle::Classic),
        ("kostenberechnung", calculation, false, PrintStyle::Classic),
        (
            "kostenberechnung_deckblatt",
            calculation,
            true,
            PrintStyle::Classic,
        ),
        (
            "kostenberechnung_proava",
            calculation,
            false,
            PrintStyle::ProAva,
        ),
    ] {
        let options = PrintCostOptions {
            cover,
            ..PrintCostOptions::default()
        };
        let mut model = proava_exchange::cost_estimate_to_print(
            &project,
            Some(id),
            "18.11.2026",
            options,
            true,
        )
        .expect("Kostenermittlung");
        model.header.look = PrintLook {
            style,
            ..PrintLook::default()
        };
        let (bytes, pages) = render_cost_estimate_pdf_counted(&model).expect("PDF");
        let file = format!("{out}/{name}.pdf");
        std::fs::write(&file, bytes).expect("schreiben");
        println!("{file}: {pages} Seiten");
    }
}
