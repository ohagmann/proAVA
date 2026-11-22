//! „Ausschnitt aus PDF einfügen“ im Langtext: die Vorschau wird einmal
//! gerechnet, nicht je Bild; „Vorschau ausblenden“ und „Vorschau
//! einfrieren“ landen im Projekt und überstehen Speichern und Laden.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::*;
use proava_application::{Pixels, ProjectRepository};
use proava_domain::{
    ClipRect, Lv, LvRow, OzMask, PdfClip, Position, Project, ProjectName, RichText, TextBlock,
};
use proava_infrastructure::XmlProjectFile;
use proava_ui::pdf_preview::{ClipPreviews, Renderer};
use proava_ui::StartFile;

/// Project with one position whose long text shows half of page 1 of the
/// synthetic vector PDF.
fn project_with_clip() -> Project {
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_position(Position::new(
        "",
        "Systemtopologie",
        "1".parse().unwrap(),
        "psch",
        None,
    ));
    lv.renumber();
    let mut project = Project::new(ProjectName::parse("Ausschnitt").unwrap(), "2026-11-18", lv);
    let pdf = proava_print::sample_vector_pdf();
    let file = project.add_pdf_clip(
        "plan.pdf",
        pdf,
        PdfClip::new(
            1,
            (566.93, 283.46),
            ClipRect::new(0.0, 0.0, 283.46, 283.46),
            "plan.pdf",
        ),
    );
    let mut text = RichText::plain("Topologie:");
    text.push(TextBlock::Image {
        file,
        width_mm: Some(120.0),
    });
    if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(0) {
        position.set_long_text(text);
    }
    project
}

/// Renders white pixels and counts the calls (instead of the PDF rasterizer).
fn counting_renderer() -> (Renderer, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let renderer: Renderer = Arc::new(move |_, _, rect, width| {
        seen.fetch_add(1, Ordering::SeqCst);
        let height = ((width as f32) * rect.aspect()).round().max(1.0) as u32;
        Ok(Pixels {
            width,
            height,
            rgba: vec![255; (width * height * 4) as usize],
        })
    });
    (renderer, count)
}

/// The program with the project open, tall enough that the long text with
/// the clip and its switches is on screen.
fn tall_harness(folder: &std::path::Path, file: std::path::PathBuf) -> UiHarness {
    let app = proava_ui::App::new(launcher(folder), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1400.0, 1700.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    harness
}

fn clip_of(project: &Project) -> PdfClip {
    project.image("plan.pdf").unwrap().clip.clone().unwrap()
}

#[test]
fn clip_preview_is_rendered_once_and_its_switches_are_kept() {
    let folder = TempDir::new("ausschnitt");
    let file = save(&folder.0, &project_with_clip());
    let mut harness = tall_harness(&folder.0, file.clone());
    let (renderer, count) = counting_renderer();
    // In the test the preview is rendered at once (no background thread).
    ClipPreviews::install_inline(&harness.ctx, renderer);
    select_first_position(&mut harness);
    harness.run_steps(40);
    assert!(
        shows_part(&harness, "PDF-Ausschnitt · plan.pdf"),
        "Ausschnitt im Langtext"
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "einmal gerechnet, nicht je Bild"
    );

    // Ausblenden: only a placeholder, nothing more is rendered.
    click(&mut harness, "Vorschau ausblenden");
    harness.run_steps(20);
    assert!(
        clip_of(project(&harness)).preview_hidden,
        "Schalter im Projekt"
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // Einblenden and einfrieren: one rendering for the frozen picture.
    click(&mut harness, "Vorschau ausblenden");
    click(&mut harness, "Vorschau einfrieren");
    harness.run_steps(20);
    let clip = clip_of(project(&harness));
    assert!(clip.preview_frozen && !clip.preview_hidden);
    assert!(clip.frozen_png.is_some(), "Bild der eingefrorenen Vorschau");
    let renders = count.load(Ordering::SeqCst);
    harness.run_steps(20);
    assert_eq!(
        count.load(Ordering::SeqCst),
        renders,
        "eingefroren: nichts wird mehr gerechnet"
    );

    press_with(&mut harness, Modifiers::COMMAND, Key::S);
    harness.run_steps(10);
    let loaded = XmlProjectFile::new().load_project(&file).unwrap();
    let clip = clip_of(&loaded);
    assert!(
        clip.preview_frozen && clip.frozen_png.is_some(),
        "eingefroren gespeichert"
    );
}

/// Die Einstellung „Vorschau aller PDF-Ausschnitte ausblenden“: kein Rechnen,
/// gemerkt in den Einstellungen.
#[test]
fn all_clip_previews_can_be_hidden() {
    let folder = TempDir::new("ausschnitt-alle");
    let file = save(&folder.0, &project_with_clip());
    let launcher = launcher(&folder.0);
    launcher.set_clip_previews_hidden(true);
    let mut harness = harness(&folder.0, Some(StartFile::Project(file)));
    let (renderer, count) = counting_renderer();
    ClipPreviews::install_inline(&harness.ctx, renderer);
    select_first_position(&mut harness);
    harness.run_steps(30);
    assert!(shows_part(&harness, "PDF-Ausschnitt · plan.pdf"));
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "ausgeblendet: nichts gerechnet"
    );
    let settings = std::fs::read_to_string(folder.0.join("settings.json")).unwrap();
    assert!(
        settings.contains("\"clip_previews_hidden\": true"),
        "{settings}"
    );
}
