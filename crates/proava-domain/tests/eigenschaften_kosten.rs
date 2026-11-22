//! Eigenschaftstests der Kostenermittlung nach DIN 276 (docs/BUILD.md):
//! für jede Anlagenliste, jede Aufteilung und jeden Kennwert stimmen die
//! Summen (Bauteil, DP-Art, Gesamt), die Aufteilung auf KG 481–489 ergibt
//! bei 100 % centgenau die Gesamtsumme (Rundungsrest bei der größten KG,
//! jeder Anteil höchstens ein paar Cent neben dem genauen Wert), eine
//! Aufteilung ≠ 100 % zeigt ihre Lücke als Prüfsumme und Warnung, die
//! Spanne liegt um den Kennwert, „Kostenberechnung aus Kostenschätzung“
//! behält die Anlagenliste, DP-Arten hinzufügen/verschieben/entfernen
//! nehmen ihre Anzahlen mit – und auch absurde Werte (aus einer fremden
//! Datei, über den MCP-Server) bringen nichts zum Absturz.

use proava_domain::cost_estimate::split;
use proava_domain::{
    cost_control, CostEstimate, CostSection, CostShare, CostStage, Decimal, Money, PointKind,
    PointRow,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

fn config(default_cases: u32) -> Config {
    let cases = std::env::var("PROAVA_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default_cases);
    let seed = std::env::var("PROAVA_PROPTEST_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0x0A7A_2026);
    Config {
        cases,
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..Config::default()
    }
}

/// Upper bound of the inputs of the program (numbers::MAX_INPUT in the UI).
const INPUT: i64 = 100_000_000;

/// A price with up to three decimals, 0 … 99.999.999,999 (mostly small).
fn price() -> impl Strategy<Value = Decimal> {
    prop_oneof![
        4 => (0i64..2_000_000).prop_map(|milli| Decimal::new(milli, 3)),
        1 => (0i64..INPUT * 1000).prop_map(|milli| Decimal::new(milli, 3)),
        1 => Just(Decimal::ZERO),
    ]
}

/// Number of data points (mostly small, sometimes at the input limit).
fn count() -> impl Strategy<Value = u32> {
    prop_oneof![
        6 => 0u32..60,
        1 => 0u32..INPUT as u32,
        1 => Just(0u32),
    ]
}

/// A percentage with up to two decimals (also negative or above 100).
fn percent() -> impl Strategy<Value = Decimal> {
    prop_oneof![
        4 => (0i64..10_000).prop_map(|hundredths| Decimal::new(hundredths, 2)),
        1 => (-5_000i64..20_000).prop_map(|hundredths| Decimal::new(hundredths, 2)),
    ]
}

const CODES: [&str; 9] = ["481", "482", "483", "484", "485", "486", "489", "48", "xyz"];

/// Shares that add up to exactly 100 % (the last one takes the rest).
fn full_shares() -> impl Strategy<Value = Vec<CostShare>> {
    prop::collection::vec((0..CODES.len(), 0i64..5_000), 1..8).prop_map(|parts| {
        let mut shares: Vec<CostShare> = parts
            .iter()
            .map(|(code, hundredths)| CostShare::new(CODES[*code], Decimal::new(*hundredths, 2)))
            .collect();
        let taken: Decimal = shares[..shares.len() - 1]
            .iter()
            .map(|share| share.percent)
            .sum();
        shares.last_mut().unwrap().percent = Decimal::ONE_HUNDRED - taken;
        shares
    })
}

fn shares() -> impl Strategy<Value = Vec<CostShare>> {
    prop_oneof![
        3 => full_shares(),
        1 => prop::collection::vec((0..CODES.len(), percent()), 0..8).prop_map(|parts| {
            parts.into_iter().map(|(code, percent)| CostShare::new(CODES[code], percent)).collect()
        }),
    ]
}

fn estimate() -> impl Strategy<Value = CostEstimate> {
    (
        any::<bool>(),
        prop::collection::vec((0..4usize, price()), 0..7),
        prop::collection::vec(
            (
                prop::option::weighted(0.3, shares()),
                prop::collection::vec(prop::collection::vec(count(), 0..9), 0..6),
            ),
            0..4,
        ),
        price(),
        prop::option::of((price(), price())),
        any::<bool>(),
        shares(),
    )
        .prop_map(
            |(calculation, kinds, sections, rate, span, per_kind, shares)| {
                let stage = if calculation {
                    CostStage::Calculation
                } else {
                    CostStage::Estimate
                };
                let mut estimate = CostEstimate::template(stage, "18.11.2026", "Affe");
                let keys = ["AI", "BI", "AV", "ai"];
                estimate.kinds = kinds
                    .into_iter()
                    .map(|(key, price)| PointKind::new(keys[key], "Art", price))
                    .collect();
                let kinds = estimate.kinds.len();
                estimate.sections = sections
                    .into_iter()
                    .enumerate()
                    .map(|(index, (own, rows))| CostSection {
                        name: if index == 1 {
                            String::new()
                        } else {
                            format!("Bauteil {index}")
                        },
                        shares: own,
                        rows: rows
                            .into_iter()
                            .map(|mut counts| {
                                counts.resize(kinds, 0);
                                PointRow {
                                    plant: "Anlage".into(),
                                    counts,
                                    ..PointRow::default()
                                }
                            })
                            .collect(),
                    })
                    .collect();
                estimate.rate = rate;
                (estimate.rate_low, estimate.rate_high) =
                    span.map_or((None, None), |(low, high)| (Some(low), Some(high)));
                estimate.rate_per_kind = per_kind;
                estimate.shares = shares;
                estimate
            },
        )
}

proptest! {
    #![proptest_config(config(400))]

    /// Summen je Bauteil, je DP-Art und gesamt; Aufteilung und Prüfsumme.
    #[test]
    fn sums_split_and_check_add_up(estimate in estimate()) {
        let result = estimate.result();
        prop_assert_eq!(result.sections.len(), estimate.sections.len());
        let mut total = Money::ZERO;
        let mut points = 0u64;
        for (index, (section, sums)) in estimate.sections.iter().zip(&result.sections).enumerate() {
            let section_points: u64 = section.rows.iter().map(PointRow::points).sum();
            prop_assert_eq!(sums.points, section_points);
            for (kind, sum) in sums.kinds.iter().enumerate() {
                let count: u64 = section.rows.iter().map(|row| u64::from(row.count(kind))).sum();
                prop_assert_eq!(sum.count, count);
                prop_assert_eq!(sum.amount, Money::total(estimate.price_of(kind), Decimal::from(count)));
            }
            let expected = match (estimate.stage, estimate.rate_per_kind) {
                (CostStage::Estimate, false) => Money::total(estimate.rate, Decimal::from(section_points)),
                _ => sums.kinds.iter().map(|kind| kind.amount).sum(),
            };
            prop_assert_eq!(sums.total, expected);
            total += sums.total;
            points += sums.points;
            match estimate.stage {
                CostStage::Estimate => {
                    prop_assert!(sums.shares.is_empty());
                    prop_assert_eq!(sums.check, Money::ZERO);
                }
                CostStage::Calculation => {
                    let shares = estimate.shares_of(index);
                    prop_assert_eq!(sums.shares.len(), shares.len());
                    let split_sum: Money = sums.shares.iter().map(|share| share.amount).sum();
                    prop_assert_eq!(sums.check, split_sum - sums.total);
                    let percent: Decimal = shares.iter().map(|share| share.percent).sum();
                    prop_assert_eq!(sums.percent_sum, percent);
                    if percent == Decimal::ONE_HUNDRED {
                        prop_assert_eq!(sums.check, Money::ZERO, "100 % ergibt centgenau die Gesamtsumme");
                        // Every part within a few cents of the exact value.
                        let slack = Decimal::new(shares.len() as i64 + 1, 2);
                        for share in &sums.shares {
                            let exact = sums.total.to_decimal() * share.percent / Decimal::ONE_HUNDRED;
                            prop_assert!(
                                (share.amount.to_decimal() - exact).abs() <= slack,
                                "KG {} {} % von {}: {} statt {}", share.code, share.percent, sums.total, share.amount, exact
                            );
                        }
                    } else {
                        let label = match section.name.trim() {
                            "" => format!("Bauteil {}", index + 1),
                            name => format!("Bauteil „{name}“"),
                        };
                        prop_assert!(
                            result.warnings.iter().any(|warning| warning.starts_with(&label) && warning.contains("statt 100 %")),
                            "keine Warnung für {}: {:?}", label, result.warnings
                        );
                    }
                }
            }
            // The span lies around the Kennwert.
            if estimate.has_span() {
                let (low, high) = (sums.low.unwrap(), sums.high.unwrap());
                prop_assert_eq!(low, Money::total(estimate.rate_low.unwrap(), Decimal::from(section_points)));
                prop_assert_eq!(high, Money::total(estimate.rate_high.unwrap(), Decimal::from(section_points)));
                if estimate.rate_low <= Some(estimate.rate) && Some(estimate.rate) <= estimate.rate_high {
                    prop_assert!(low <= sums.total && sums.total <= high);
                }
            } else {
                prop_assert!(sums.low.is_none() && sums.high.is_none());
            }
        }
        prop_assert_eq!(result.total, total);
        prop_assert_eq!(result.points, points);
        for (kind, sum) in result.kinds.iter().enumerate() {
            prop_assert_eq!(sum.count, result.sections.iter().map(|section| section.kinds[kind].count).sum::<u64>());
            prop_assert_eq!(sum.amount, result.sections.iter().map(|section| section.kinds[kind].amount).sum::<Money>());
        }
        let groups: Money = result.groups.iter().map(|group| group.amount).sum();
        prop_assert_eq!(result.check, groups - result.total);
        match estimate.stage {
            CostStage::Estimate => {
                prop_assert_eq!(result.groups.len(), 1);
                prop_assert_eq!(result.groups[0].code.as_str(), "480");
                prop_assert_eq!(result.groups[0].amount, result.total);
            }
            CostStage::Calculation => {
                // Per cost group: the sum of that code over all Bauteile.
                for group in &result.groups {
                    let sum: Money = result
                        .sections
                        .iter()
                        .flat_map(|section| &section.shares)
                        .filter(|share| share.code == group.code)
                        .map(|share| share.amount)
                        .sum();
                    prop_assert_eq!(group.amount, sum, "KG {}", group.code);
                }
                let all_full = (0..estimate.sections.len()).all(|index| {
                    estimate.shares_of(index).iter().map(|share| share.percent).sum::<Decimal>() == Decimal::ONE_HUNDRED
                });
                if all_full {
                    prop_assert_eq!(result.check, Money::ZERO);
                }
            }
        }
    }

    /// Aufteilung allein: bei genau 100 % ist die Summe der Anteile die Gesamtsumme.
    #[test]
    fn split_of_any_total_is_exact(total in -100_000_000_000_000i64..100_000_000_000_000, shares in full_shares()) {
        let total = Money::from_cents(total);
        let lines = split(total, &shares);
        prop_assert_eq!(lines.len(), shares.len());
        prop_assert_eq!(lines.iter().map(|line| line.amount).sum::<Money>(), total);
        // The rest goes to the first of the largest shares only.
        let largest = shares.iter().map(|share| share.percent).max().unwrap();
        let first = shares.iter().position(|share| share.percent == largest).unwrap();
        for (index, (line, share)) in lines.iter().zip(&shares).enumerate() {
            if index != first {
                prop_assert_eq!(line.amount, total.percent(share.percent));
            }
        }
    }

    /// „Kostenberechnung aus Kostenschätzung“ behält Bauteile, Zeilen und Datenpunkte.
    #[test]
    fn calculation_from_an_estimate_keeps_the_plant_list(estimate in estimate()) {
        let calculation = estimate.to_calculation("19.11.2026", "Affe");
        prop_assert_eq!(calculation.stage, CostStage::Calculation);
        prop_assert_eq!(&calculation.sections, &estimate.sections);
        prop_assert_eq!(calculation.kinds.len(), estimate.kinds.len());
        for (new, old) in calculation.kinds.iter().zip(&estimate.kinds) {
            prop_assert_eq!(&new.key, &old.key);
        }
        prop_assert_eq!(calculation.result().points, estimate.result().points);
    }

    /// DP-Arten hinzufügen, verschieben und entfernen nehmen ihre Anzahlen mit.
    #[test]
    fn kind_edits_carry_their_counts(
        estimate in estimate(),
        edits in prop::collection::vec((0u8..3, 0usize..10, any::<bool>()), 0..12),
    ) {
        let mut estimate = estimate;
        // Points per kind key as a multiset that every edit must keep in step.
        let columns = |estimate: &CostEstimate| -> Vec<Vec<u32>> {
            (0..estimate.kinds.len())
                .map(|kind| estimate.sections.iter().flat_map(|section| &section.rows).map(|row| row.count(kind)).collect())
                .collect()
        };
        for (what, index, left) in edits {
            let before = columns(&estimate);
            match what {
                0 => {
                    let at = estimate.add_kind(PointKind::new("MV", "Mehrstufenwert", Decimal::ONE));
                    prop_assert_eq!(at, before.len());
                    let after = columns(&estimate);
                    prop_assert_eq!(&after[..before.len()], &before[..]);
                    prop_assert!(after[at].iter().all(|count| *count == 0));
                }
                1 => {
                    let moved = estimate.move_kind(index, left);
                    let after = columns(&estimate);
                    if moved {
                        let other = if left { index - 1 } else { index + 1 };
                        let mut expected = before.clone();
                        expected.swap(index, other);
                        prop_assert_eq!(after, expected);
                    } else {
                        prop_assert_eq!(after, before);
                    }
                }
                _ => {
                    let removed = estimate.remove_kind(index);
                    let after = columns(&estimate);
                    let mut expected = before.clone();
                    if removed.is_some() {
                        expected.remove(index);
                    }
                    prop_assert_eq!(after, expected);
                }
            }
        }
    }
}

/// Values the program never accepts at the keyboard, but a foreign or damaged
/// project file, a CSV or the MCP server can bring: nothing panics.
fn absurd_price() -> impl Strategy<Value = Decimal> {
    prop_oneof![
        Just(Decimal::MAX),
        Just(Decimal::MIN),
        Just(Decimal::new(i64::MAX, 0)),
        Just(Decimal::new(i64::MAX, 2)),
        Just(Decimal::new(1, 28)),
        (any::<i64>(), 0u32..6).prop_map(|(mantissa, scale)| Decimal::new(mantissa, scale)),
        price(),
    ]
}

fn absurd_estimate() -> impl Strategy<Value = CostEstimate> {
    (
        estimate(),
        prop::collection::vec(absurd_price(), 0..7),
        absurd_price(),
        prop::option::of((absurd_price(), absurd_price())),
        prop::collection::vec(absurd_price(), 0..7),
        prop::collection::vec(
            prop_oneof![Just(u32::MAX), Just(u32::MAX / 2), any::<u32>()],
            0..8,
        ),
    )
        .prop_map(|(mut estimate, prices, rate, span, percents, counts)| {
            for (kind, price) in estimate.kinds.iter_mut().zip(prices) {
                kind.unit_price = price;
            }
            estimate.rate = rate;
            (estimate.rate_low, estimate.rate_high) =
                span.map_or((None, None), |(low, high)| (Some(low), Some(high)));
            for (share, percent) in estimate.shares.iter_mut().zip(percents) {
                share.percent = percent;
            }
            let kinds = estimate.kinds.len();
            if let Some(section) = estimate.sections.first_mut() {
                for _ in 0..3 {
                    let mut row = PointRow::new("RLT", "Groß");
                    row.counts = counts.clone();
                    row.counts.resize(kinds, 0);
                    section.rows.push(row);
                }
            }
            estimate
        })
}

proptest! {
    #![proptest_config(config(300))]

    #[test]
    fn absurd_values_never_panic(estimate in absurd_estimate()) {
        let result = estimate.result();
        let _ = estimate.warnings();
        let _ = estimate.to_calculation("", "").result();
        let mut project = proava_domain::Project::new(
            proava_domain::ProjectName::parse("Kosten").unwrap(),
            "2026-11-18",
            proava_domain::Lv::new("LV", proava_domain::OzMask::parse(proava_domain::OzMask::DEFAULT_PATTERN).unwrap()),
        );
        project.add_cost_estimate(estimate.clone());
        let _ = cost_control(&project, &estimate);
        // Prices, counts and the Kennwert are not negative here: no sum may
        // come out negative (an overflow that wrapped round).
        let negative = estimate.kinds.iter().any(|kind| kind.unit_price.is_sign_negative())
            || estimate.rate.is_sign_negative();
        if !negative {
            prop_assert!(result.total.cents() >= 0, "Summe übergelaufen: {}", result.total);
            for section in &result.sections {
                prop_assert!(section.total.cents() >= 0, "Bauteilsumme übergelaufen: {}", section.total);
            }
        }
    }
}
