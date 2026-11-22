//! Synthetic Kostenermittlung for demos, the handbook and tests (no
//! customer data): a school with three Bauteile and a short plant list.

use rust_decimal::Decimal;

use crate::cost_estimate::{CostEstimate, CostSection, CostShare, CostStage, PointRow};

/// (Bauteil, [(Ort, Gewerk, Anlage, Bemerkung, AI, BI, AO, BO, AV, BV)])
type DemoRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    [u32; 6],
);

const DEMO: [(&str, &[DemoRow]); 3] = [
    (
        "Altbau",
        &[
            (
                "ASP01 · UG",
                "RLT",
                "RLT-01 Klassen",
                "8.000 m³/h, Erhitzer + WRG",
                [14, 22, 7, 9, 18, 2],
            ),
            (
                "ASP01 · UG",
                "RLT",
                "RLT-02 Verwaltung",
                "3.500 m³/h",
                [10, 16, 5, 7, 12, 2],
            ),
            (
                "ASP01 · UG",
                "HZG",
                "Fernwärme-Übergabe",
                "inkl. Zähler",
                [6, 6, 2, 2, 8, 0],
            ),
            (
                "ASP01 · UG",
                "HZG",
                "Heizkreise (4 Stück)",
                "statisch / RLT",
                [12, 16, 4, 8, 16, 4],
            ),
            (
                "ASP02 · EG",
                "SAN",
                "Trinkwarmwasser",
                "Zirkulation",
                [4, 6, 1, 3, 6, 0],
            ),
            (
                "ASP02 · EG",
                "ELT",
                "Meldungen NSHV",
                "Störmeldungen",
                [0, 24, 0, 0, 0, 0],
            ),
        ],
    ),
    (
        "Erweiterung",
        &[
            (
                "ASP03 · UG",
                "RLT",
                "RLT-11 Mensa",
                "6.000 m³/h, Kühler",
                [16, 24, 8, 10, 20, 4],
            ),
            (
                "ASP03 · UG",
                "KAE",
                "Kälteerzeugung",
                "Wärmepumpe reversibel",
                [8, 10, 3, 4, 14, 2],
            ),
            (
                "ASP03 · UG",
                "HZG",
                "Heizkreise (3 Stück)",
                "",
                [9, 12, 3, 6, 12, 3],
            ),
            (
                "ASP04 · 1.OG",
                "RA",
                "Raumautomation Klassen",
                "12 Räume, Bus",
                [0, 0, 0, 0, 96, 48],
            ),
        ],
    ),
    (
        "Sporthalle",
        &[
            (
                "ASP05 · UG",
                "RLT",
                "RLT-21 Halle",
                "12.000 m³/h",
                [14, 20, 7, 9, 16, 2],
            ),
            (
                "ASP05 · UG",
                "SAN",
                "Duschen TWW",
                "Legionellenschaltung",
                [5, 7, 2, 3, 6, 1],
            ),
            (
                "ASP05 · UG",
                "HZG",
                "Heizkreise (2 Stück)",
                "",
                [6, 8, 2, 4, 8, 2],
            ),
        ],
    ),
];

/// A Kostenschätzung (LPH 2) or Kostenberechnung (LPH 3) of the demo
/// school; the Berechnung splits the Sporthalle with its own shares.
pub fn demo_cost_estimate(stage: CostStage) -> CostEstimate {
    let mut estimate = CostEstimate::template(stage, "12.11.2026", "Planer GA");
    estimate.title = match stage {
        CostStage::Estimate => "Kostenschätzung LPH 2".to_string(),
        CostStage::Calculation => "Kostenberechnung LPH 3".to_string(),
    };
    if stage == CostStage::Estimate {
        estimate.date = "15.06.2026".to_string();
        estimate.rate_low = Some(Decimal::from(400));
        estimate.rate_high = Some(Decimal::from(500));
    }
    estimate.sections = DEMO
        .iter()
        .map(|(name, rows)| {
            let mut section = CostSection::new(name);
            section.rows = rows
                .iter()
                .map(|(location, trade, plant, remark, counts)| PointRow {
                    location: location.to_string(),
                    trade: trade.to_string(),
                    plant: plant.to_string(),
                    remark: remark.to_string(),
                    counts: counts.to_vec(),
                })
                .collect();
            section
        })
        .collect();
    if stage == CostStage::Calculation {
        // Sporthalle: weniger Management, mehr Leitungen (lange Wege).
        estimate.sections[2].shares = Some(
            [
                ("481", 38),
                ("482", 12),
                ("483", 6),
                ("484", 36),
                ("485", 5),
                ("489", 3),
            ]
            .iter()
            .map(|(code, percent)| CostShare::new(code, Decimal::from(*percent)))
            .collect(),
        );
    }
    estimate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Money;

    #[test]
    fn demo_adds_up() {
        let result = demo_cost_estimate(CostStage::Calculation).result();
        assert_eq!(result.sections.len(), 3);
        assert_eq!(result.check, Money::ZERO);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let estimate = demo_cost_estimate(CostStage::Estimate).result();
        assert_eq!(estimate.points, result.points);
        assert_eq!(
            estimate.total,
            Money::total(Decimal::from(450), Decimal::from(estimate.points))
        );
    }
}
