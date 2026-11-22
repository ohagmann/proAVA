//! Protocol lists (import protocol, preview, hints): one fact per line,
//! numbers on one right edge, paths whole as long as the window allows –
//! only then shortened in the middle, the full path as tooltip.

use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use proava_application::{Protocol, ProtocolEntry, ThemeChoice};
use proava_ui::protocol_view;
use proava_ui::theme::Palette;

const PATH: &str = "/Projekte/Musterstadt/Rathaus 3 Sanierung Gebäudeautomation/Ausschreibung/Leistungsverzeichnisse/2026/Rathaus 3 Gebäudeautomation LV 01";

fn protocol() -> Protocol {
    let mut protocol = Protocol::new("Importprotokoll Rathaus3.X82");
    protocol.push(ProtocolEntry::text("Datei", "Rathaus3.X82"));
    protocol.push(ProtocolEntry::count("Positionen", 171));
    protocol.push(ProtocolEntry::number("Summe netto", "290.023,58 €"));
    protocol.push(ProtocolEntry::path(
        "Speicherort",
        std::path::Path::new(PATH),
    ));
    protocol.push(
        ProtocolEntry::count("Verworfen oder umgewandelt", 1)
            .with_details(vec!["Ein Befund".into()]),
    );
    protocol
}

/// The protocol as a dialog in a window of `width` × 800; returns the
/// harness and the width the dialog asked for.
fn harness(width: f32) -> (Harness<'static, f32>, f32) {
    let protocol = protocol();
    let palette = Palette::of(ThemeChoice::Light);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(width, 800.0))
        .build_ui_state(
            move |ui, asked: &mut f32| {
                *asked = protocol_view::dialog_width(ui, protocol.entries.iter(), 620.0);
                let mut copied = false;
                protocol_view::dialog(ui, &palette, &protocol, &mut copied);
            },
            0.0,
        );
    harness.run_steps(4);
    let asked = *harness.state();
    (harness, asked)
}

fn texts(harness: &Harness<'_, f32>) -> Vec<String> {
    harness
        .query_all_by(|_| true)
        .filter_map(|node| {
            let node = node.accesskit_node();
            node.label().or_else(|| node.value())
        })
        .collect()
}

#[test]
fn a_wide_window_shows_the_whole_path_on_one_line() {
    let (harness, asked) = harness(1600.0);
    assert!(asked > 620.0, "breiter als die Vorgabe: {asked}");
    assert!(asked <= 1600.0 * 0.9, "höchstens 90 %: {asked}");
    let path = harness.query_by_label(PATH).expect("Pfad ganz");
    let caption = harness.query_by_label("Speicherort").expect("Bezeichnung");
    assert!(
        caption.rect().right() <= path.rect().left(),
        "Bezeichnung links vom Wert"
    );
    assert!(path.rect().height() < 24.0, "eine Zeile: {:?}", path.rect());
    assert!(!texts(&harness).iter().any(|text| text.contains('…')));
}

#[test]
fn a_narrow_window_shortens_the_path_in_the_middle() {
    let (harness, asked) = harness(800.0);
    assert!(asked <= 800.0 * 0.9, "{asked}");
    let shortened = texts(&harness)
        .into_iter()
        .find(|text| text.contains('…'))
        .expect("gekürzter Pfad");
    assert!(shortened.starts_with("/Projekte/"), "{shortened}");
    assert!(
        shortened.ends_with("LV 01"),
        "Ende bleibt lesbar: {shortened}"
    );
    assert!(harness.query_by_label(PATH).is_none(), "nicht ganz");
}

#[test]
fn numbers_share_one_right_edge_and_details_stand_below() {
    let (harness, _) = harness(1600.0);
    let count = harness.query_by_label("171").expect("Anzahl").rect();
    let sum = harness
        .query_by_label("290.023,58 €")
        .expect("Summe")
        .rect();
    assert!(
        (count.right() - sum.right()).abs() < 1.5,
        "{count:?} {sum:?}"
    );
    let file = harness
        .query_by_label("Rathaus3.X82")
        .expect("Datei")
        .rect();
    assert!(
        (file.left() - sum.left()).abs() > 10.0 || file.left() <= sum.left(),
        "Text linksbündig"
    );
    let detail = harness
        .query_by_label("– Ein Befund")
        .expect("Einzelheit")
        .rect();
    let line = harness
        .query_by_label("Verworfen oder umgewandelt")
        .expect("Zeile")
        .rect();
    assert!(detail.top() > line.top(), "darunter");
}
