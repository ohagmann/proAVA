//! Eigenschaftstests: zufällige Folgen von Bearbeitungsschritten auf einem
//! Projekt – über dieselben Wege wie die Oberfläche (`lv_ops`, `struct_ops`,
//! `transfer`, Status, Bezüge, Gliederung, mehrere LVs, Angebote). Nach jedem
//! Schritt gelten die Invarianten aus [`check`].
//!
//! Feste Zufallssaat: reproduzierbar und schnell. Längere Läufe:
//! `PROAVA_PROPTEST_CASES=5000 PROAVA_PROPTEST_SEED=<Zahl> cargo test -p proava-ui --test eigenschaften_lv`
//! (siehe docs/BUILD.md).

mod common;

use std::collections::HashMap;

use common::invariants::{check_project, dump};

use proava_domain::transfer::{self, TransferOptions};
use proava_domain::{
    Address, Bid, BidPrice, Decimal, EditStatus, Lv, LvRow, MaskSpec, OzMask, Project, ProjectName,
    RowId,
};
use proava_ui::{lv_ops, struct_ops};
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};

const MASKS: [&str; 6] = [
    "11.22.PPPP",
    "11.PPPPI",
    "11.222.33.PPPP",
    "1.PPP",
    "LL.22.PPPP",
    "PPPP",
];
const STEPS: [u32; 4] = [1, 10, 100, 1000];

pub fn config(default_cases: u32) -> Config {
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
        max_shrink_iters: 4096,
        ..Config::default()
    }
}

#[derive(Debug, Clone)]
enum Op {
    Position(Index, bool),
    Title(Index),
    Subtitle(Index),
    Note(Index, bool),
    Performance(Index),
    Stlb(Index),
    Delete(Index),
    Step(Index, bool),
    Shift(Index, bool),
    Duplicate(Index),
    MoveTo(Index, Index),
    MoveRows(Index, usize, Index),
    Paste(Index, Index),
    RowNumber(Index, u16),
    Renumber,
    RenumberPositions,
    Status(Index, u8),
    LinkPerformance(Index, Index, bool),
    LinkRepetition(Index, Index, bool),
    Mask(usize, usize, u8),
    Prices(Index, u16, u32, u8),
    NewLv(usize),
    SwitchLv(Index),
    Transfer(Index, Index, Index),
    AddBid(u32),
    RemoveBid(Index),
    /// Dialog „Neu nummerieren“: nur Positionen, fertige behalten, nur der Titel der Auswahl.
    RenumberWith(Index, bool, bool, bool),
    /// Ausführungsbeschreibung für die gewählten Positionen anlegen.
    PerformanceFor(Vec<Index>),
    DuplicateLv(Index),
    MoveLv(Index, bool),
    RemoveLv(Index),
    /// Abstandshalter (Strg+L) davor oder danach.
    Spacer(Index, bool),
    /// Leere Hinweise in Abstandshalter umwandeln: alle oder die gewählten Zeilen.
    BlankNotesToSpacers(Vec<Index>, bool),
}

fn op() -> impl Strategy<Value = Op> {
    let i = || any::<Index>();
    prop_oneof![
        8 => (i(), any::<bool>()).prop_map(|(a, b)| Op::Position(a, b)),
        4 => i().prop_map(Op::Title),
        3 => i().prop_map(Op::Subtitle),
        2 => (i(), any::<bool>()).prop_map(|(a, b)| Op::Note(a, b)),
        2 => i().prop_map(Op::Performance),
        1 => i().prop_map(Op::Stlb),
        3 => i().prop_map(Op::Delete),
        3 => (i(), any::<bool>()).prop_map(|(a, b)| Op::Step(a, b)),
        2 => (i(), any::<bool>()).prop_map(|(a, b)| Op::Shift(a, b)),
        2 => i().prop_map(Op::Duplicate),
        2 => (i(), i()).prop_map(|(a, b)| Op::MoveTo(a, b)),
        2 => (i(), 1usize..4, i()).prop_map(|(a, n, b)| Op::MoveRows(a, n, b)),
        2 => (i(), i()).prop_map(|(a, b)| Op::Paste(a, b)),
        2 => (i(), 1u16..1200).prop_map(|(a, n)| Op::RowNumber(a, n)),
        1 => Just(Op::Renumber),
        1 => Just(Op::RenumberPositions),
        3 => (i(), 0u8..3).prop_map(|(a, s)| Op::Status(a, s)),
        2 => (i(), i(), any::<bool>()).prop_map(|(a, b, c)| Op::LinkPerformance(a, b, c)),
        2 => (i(), i(), any::<bool>()).prop_map(|(a, b, c)| Op::LinkRepetition(a, b, c)),
        1 => (0..MASKS.len(), 0..STEPS.len(), 0u8..3).prop_map(|(m, s, n)| Op::Mask(m, s, n)),
        3 => (i(), 0u16..500, 0u32..500_000, 0u8..6).prop_map(|(a, q, p, k)| Op::Prices(a, q, p, k)),
        1 => (0..MASKS.len()).prop_map(Op::NewLv),
        1 => i().prop_map(Op::SwitchLv),
        1 => (i(), i(), i()).prop_map(|(a, b, c)| Op::Transfer(a, b, c)),
        1 => (0u32..1000).prop_map(Op::AddBid),
        1 => i().prop_map(Op::RemoveBid),
        2 => (i(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(|(a, b, c, d)| Op::RenumberWith(a, b, c, d)),
        1 => prop::collection::vec(i(), 1..4).prop_map(Op::PerformanceFor),
        1 => i().prop_map(Op::DuplicateLv),
        1 => (i(), any::<bool>()).prop_map(|(a, b)| Op::MoveLv(a, b)),
        1 => i().prop_map(Op::RemoveLv),
        2 => (i(), any::<bool>()).prop_map(|(a, b)| Op::Spacer(a, b)),
        1 => (prop::collection::vec(i(), 1..4), any::<bool>()).prop_map(|(a, b)| Op::BlankNotesToSpacers(a, b)),
    ]
}

fn pick(index: &Index, len: usize) -> Option<usize> {
    (len > 0).then(|| index.index(len))
}

fn new_lv(mask: &str) -> Lv {
    Lv::new("Zufall", OzMask::parse(mask).expect("Maske"))
}

/// Ordinals of finished positions before a step (they must stay), per LV
/// by its session key (LVs move and go).
fn locked_ordinals(project: &Project) -> HashMap<u64, HashMap<RowId, String>> {
    project
        .lvs()
        .enumerate()
        .map(|(index, lv)| {
            let locked = lv
                .positions()
                .filter(|position| position.is_locked())
                .map(|position| (position.id(), position.ordinal().to_string()))
                .collect();
            (project.lv_key(index).unwrap(), locked)
        })
        .collect()
}

/// Applies one step like the UI does (lock check before deleting, moving,
/// shifting – see `LvEditor::check_lock`). Errors are fine; they must not
/// leave a broken LV behind.
fn apply(project: &mut Project, op: &Op) {
    let len = project.lv().rows().len();
    let row_id = |project: &Project, index: usize| project.lv().rows()[index].id();
    match op {
        Op::Position(at, before) => {
            let _ = lv_ops::insert_position(project.lv_mut(), pick(at, len), *before);
        }
        Op::Title(at) => {
            let _ = struct_ops::insert_title(project.lv_mut(), pick(at, len));
        }
        Op::Subtitle(at) => {
            let _ = struct_ops::insert_subtitle(project.lv_mut(), pick(at, len));
        }
        Op::Note(at, before) => {
            let _ = lv_ops::insert_note(project.lv_mut(), pick(at, len), *before);
        }
        Op::Performance(at) => {
            let _ = struct_ops::insert_performance(project.lv_mut(), pick(at, len));
        }
        Op::Stlb(at) => {
            let _ = lv_ops::insert_stlb(project.lv_mut(), pick(at, len));
        }
        Op::Delete(at) => {
            if let Some(index) = pick(at, len) {
                if project.lv().ensure_editable_rows(&[index]).is_ok() {
                    lv_ops::delete(project.lv_mut(), index);
                }
            }
        }
        Op::Step(at, up) => {
            if let Some(index) = pick(at, len) {
                if project.lv().ensure_editable_rows(&[index]).is_ok() {
                    lv_ops::move_step(project.lv_mut(), index, *up);
                }
            }
        }
        Op::Shift(at, deeper) => {
            if let Some(index) = pick(at, len) {
                if project.lv().ensure_editable_rows(&[index]).is_ok() {
                    lv_ops::shift(project.lv_mut(), index, *deeper);
                }
            }
        }
        Op::Duplicate(at) => {
            if let Some(index) = pick(at, len) {
                let _ = lv_ops::duplicate(project.lv_mut(), index);
            }
        }
        Op::MoveTo(from, target) => {
            if let (Some(from), Some(target)) = (pick(from, len), pick(target, len + 1)) {
                if project.lv().ensure_editable_rows(&[from]).is_ok() {
                    lv_ops::move_to(project.lv_mut(), from, target);
                }
            }
        }
        Op::MoveRows(start, count, target) => {
            if let (Some(start), Some(target)) = (pick(start, len), pick(target, len + 1)) {
                let end = (start + count).min(len);
                let _ = struct_ops::move_rows_to(project.lv_mut(), start..end, target);
            }
        }
        Op::Paste(from, at) => {
            if let (Some(from), Some(at)) = (pick(from, len), pick(at, len)) {
                let rows = lv_ops::copy_block(project.lv(), from);
                let _ = lv_ops::paste(project.lv_mut(), Some(at), &rows);
            }
        }
        Op::RowNumber(at, number) => {
            if let Some(index) = pick(at, len) {
                let _ = project.lv_mut().set_row_number(index, &number.to_string());
            }
        }
        Op::Renumber => project.lv_mut().renumber(),
        Op::RenumberPositions => project.lv_mut().renumber_positions(),
        Op::Status(at, status) => {
            if let Some(index) = pick(at, len) {
                let status =
                    [EditStatus::Open, EditStatus::InProgress, EditStatus::Done][*status as usize];
                let id = row_id(project, index);
                project.lv_mut().set_status(&[id], status);
            }
        }
        Op::LinkPerformance(position, performance, clear) => {
            let positions: Vec<RowId> = project.lv().positions().map(|p| p.id()).collect();
            let performances: Vec<RowId> = project.lv().performances().map(|p| p.id()).collect();
            if let Some(position) = pick(position, positions.len()) {
                let target = if *clear {
                    None
                } else {
                    pick(performance, performances.len()).map(|index| performances[index])
                };
                let ids = [positions[position]];
                if project.lv().ensure_editable(&ids).is_ok() {
                    let _ = struct_ops::link_many(project.lv_mut(), &ids, target);
                }
            }
        }
        Op::LinkRepetition(position, base, clear) => {
            let positions: Vec<RowId> = project.lv().positions().map(|p| p.id()).collect();
            if let Some(position) = pick(position, positions.len()) {
                let target = if *clear {
                    None
                } else {
                    pick(base, positions.len()).map(|index| positions[index])
                };
                let _ = project
                    .lv_mut()
                    .link_repetition(positions[position], target);
            }
        }
        Op::Mask(mask, step, numbering) => {
            let spec = MaskSpec::of(&OzMask::parse(MASKS[*mask]).unwrap(), &[]);
            let numbering = [
                struct_ops::MaskNumbering::All,
                struct_ops::MaskNumbering::KeepTitles,
                struct_ops::MaskNumbering::Unchanged,
            ][*numbering as usize];
            let _ = struct_ops::apply_mask(project.lv_mut(), &spec, STEPS[*step], numbering);
        }
        Op::Prices(at, quantity, cents, kind) => {
            if let Some(index) = pick(at, len) {
                if project.lv().ensure_editable_rows(&[index]).is_err() {
                    return;
                }
                if let Some(position) = project
                    .lv_mut()
                    .row_mut(index)
                    .and_then(LvRow::as_position_mut)
                {
                    position.set_quantity(Decimal::new(*quantity as i64, 1));
                    position.set_unit_price((*cents > 0).then(|| Decimal::new(*cents as i64, 2)));
                    use proava_domain::PositionKind::*;
                    position.set_kind(match kind {
                        0 => Normal,
                        1 => Provisional { with_total: true },
                        2 => Provisional { with_total: false },
                        3 => Supplement,
                        4 => Base { group: 1 },
                        _ => Alternative {
                            group: 1,
                            serial: 1,
                        },
                    });
                }
            }
        }
        Op::NewLv(mask) => {
            let mut lv = new_lv(MASKS[*mask]);
            lv.set_number(format!("{}", project.lv_count() + 1));
            let index = project.add_lv(lv);
            project.set_active_lv(index);
        }
        Op::SwitchLv(at) => {
            if let Some(index) = pick(at, project.lv_count()) {
                project.set_active_lv(index);
            }
        }
        Op::Transfer(source, row, at) => {
            let Some(source) = pick(source, project.lv_count()) else {
                return;
            };
            let lv = project.lv_at(source).unwrap().clone();
            let Some(row) = pick(row, lv.rows().len()) else {
                return;
            };
            let ids = [lv.rows()[row].id()];
            let collected = transfer::collect(project, &lv, &ids, TransferOptions::default());
            let _ = transfer::insert(project, pick(at, len), &collected);
        }
        Op::AddBid(seed) => {
            let mut bid = Bid::new(Address::named(format!("Bieter {seed}")), "zufall.X84");
            let ids: Vec<RowId> = project.lv().positions().map(|p| p.id()).collect();
            for (offset, id) in ids.iter().enumerate() {
                let cents = (*seed as i64 * 7 + offset as i64 * 131) % 90_000;
                bid.set_price(
                    *id,
                    BidPrice {
                        unit_price: Some(Decimal::new(cents, 2)),
                        ..BidPrice::default()
                    },
                );
            }
            project.add_bid(bid);
        }
        Op::RemoveBid(at) => {
            let ids: Vec<_> = project.bids().iter().map(Bid::id).collect();
            if let Some(index) = pick(at, ids.len()) {
                project.remove_bid(ids[index]);
            }
        }
        Op::RenumberWith(at, positions_only, keep_done, only_scope) => {
            // As the dialog: the scope is the title of the selection.
            let scope = struct_ops::renumber_scope(project.lv(), pick(at, len));
            let mode = proava_domain::RenumberMode {
                positions_only: *positions_only,
                keep_done: *keep_done,
                scope: scope.filter(|_| *only_scope),
            };
            project.lv_mut().renumber_with(mode);
        }
        Op::PerformanceFor(picks) => {
            let positions: Vec<RowId> = project.lv().positions().map(|p| p.id()).collect();
            let ids: Vec<RowId> = picks
                .iter()
                .filter_map(|at| pick(at, positions.len()).map(|index| positions[index]))
                .collect();
            if project.lv().ensure_editable(&ids).is_ok() {
                let _ = struct_ops::insert_performance_for(project.lv_mut(), &ids);
            }
        }
        Op::DuplicateLv(at) => {
            if let Some(index) = pick(at, project.lv_count()) {
                if let Some(copy) = project.duplicate_lv(index) {
                    let number = format!("{}", project.lv_count());
                    project.lv_at_mut(copy).unwrap().set_number(number);
                }
            }
        }
        Op::MoveLv(at, up) => {
            if let Some(index) = pick(at, project.lv_count()) {
                project.move_lv(index, *up);
            }
        }
        Op::RemoveLv(at) => {
            if let Some(index) = pick(at, project.lv_count()) {
                let _ = project.remove_lv(index);
            }
        }
        Op::Spacer(at, before) => {
            let spacers = project.lv().spacer_count();
            if let Ok(index) = lv_ops::insert_spacer(project.lv_mut(), pick(at, len), *before) {
                assert!(
                    project.lv().rows()[index].is_spacer(),
                    "Abstandshalter an {index}"
                );
                assert_eq!(project.lv().spacer_count(), spacers + 1);
            }
        }
        Op::BlankNotesToSpacers(picks, all) => {
            let ids: Vec<RowId> = if *all {
                project.lv().blank_notes()
            } else {
                picks
                    .iter()
                    .filter_map(|at| pick(at, len))
                    .map(|index| row_id(project, index))
                    .collect()
            };
            let blank = project
                .lv()
                .blank_notes()
                .iter()
                .filter(|id| ids.contains(id))
                .count();
            let (notes, spacers, rows) =
                (project.lv().note_count(), project.lv().spacer_count(), len);
            let net = project.lv().net_total();
            let ordinals: Vec<String> = project
                .lv()
                .rows()
                .iter()
                .map(|row| row.ordinal().to_string())
                .collect();
            let changed = project.lv_mut().blank_notes_to_spacers(&ids);
            // Same places and ids, nothing else changes.
            assert_eq!(changed, blank, "nur leere Hinweise");
            assert_eq!(project.lv().note_count(), notes - changed);
            assert_eq!(project.lv().spacer_count(), spacers + changed);
            assert_eq!(project.lv().rows().len(), rows);
            assert_eq!(project.lv().net_total(), net);
            let after: Vec<String> = project
                .lv()
                .rows()
                .iter()
                .map(|row| row.ordinal().to_string())
                .collect();
            assert_eq!(after, ordinals, "OZ bleiben");
            for id in &ids {
                if project.lv().index_of(*id).is_none() {
                    panic!("Zeile {id:?} verschwunden");
                }
            }
        }
    }
}

/// Everything that must hold after every step (see `common::invariants`),
/// and finished positions keep their number.
fn check(
    project: &Project,
    before: &HashMap<u64, HashMap<RowId, String>>,
    op: &Op,
) -> Result<(), String> {
    check_project(project)?;
    for (number, lv) in project.lvs().enumerate() {
        // Unless the step changed the status itself or the user chose to
        // renumber finished positions too.
        let allowed = matches!(op, Op::Status(..) | Op::RenumberWith(_, _, false, _));
        let locked = project.lv_key(number).and_then(|key| before.get(&key));
        if let (Some(locked), false) = (locked, allowed) {
            for (id, ordinal) in locked {
                if let Some(position) = lv.position_by_id(*id) {
                    if position.is_locked() && position.ordinal() != ordinal {
                        return Err(format!(
                            "LV {number}: fertige Position {id:?} hat ihre OZ geändert: {ordinal} → {}",
                            position.ordinal()
                        ));
                    }
                } else {
                    return Err(format!(
                        "LV {number}: fertige Position {id:?} ist verschwunden"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn start_project(mask: usize, step: usize) -> Project {
    let mut lv = new_lv(MASKS[mask]);
    lv.set_position_step(STEPS[step]);
    lv.set_number("1");
    Project::new(
        ProjectName::parse("Zufall").unwrap(),
        "2026-11-17T20:00:00",
        lv,
    )
}

/// Runs `ops` on a new project and checks the invariants after every step.
fn run_sequence(mask: usize, step: usize, ops: &[Op]) -> Result<(), TestCaseError> {
    {
        let mut project = start_project(mask, step);
        for (number, op) in ops.iter().enumerate() {
            let before = locked_ordinals(&project);
            let snapshot = project.lv().clone();
            apply(&mut project, op);
            if let Err(error) = check(&project, &before, op) {
                prop_assert!(
                    false,
                    "nach Schritt {number} {op:?}: {error}\nvorher:\n{}nachher:\n{}",
                    dump(&snapshot),
                    dump(project.lv())
                );
            }
        }
        if std::env::var_os("PROAVA_PROPTEST_STATS").is_some() {
            let rows: usize = project.lvs().map(|lv| lv.rows().len()).sum();
            let done: usize = project
                .lvs()
                .map(|lv| lv.positions().filter(|p| p.is_locked()).count())
                .sum();
            eprintln!(
                "STAT ops={} lvs={} rows={rows} done={done} auto={}",
                ops.len(),
                project.lv_count(),
                project.lv().is_auto_numbered()
            );
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(config(96))]

    #[test]
    fn edit_sequences_keep_the_lv_consistent(
        mask in 0..MASKS.len(),
        step in 0..STEPS.len(),
        ops in prop::collection::vec(op(), 1..120),
    ) {
        run_sequence(mask, step, &ops)?;
    }
}

proptest! {
    #![proptest_config(config(300))]

    /// Teurer Lauf (`cargo test -- --ignored lang_`): lange Folgen, große LVs.
    #[test]
    #[ignore]
    fn lang_edit_sequences_keep_the_lv_consistent(
        mask in 0..MASKS.len(),
        step in 0..STEPS.len(),
        ops in prop::collection::vec(op(), 200..600),
    ) {
        run_sequence(mask, step, &ops)?;
    }
}
