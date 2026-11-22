//! Try the views „Angebote“ and „Preisspiegel“ with the demo project.
//!
//! Environment: PROAVA_DEMO=large (13× LV, 8 bids) | empty (no bids),
//! PROAVA_VIEW=preisspiegel, PROAVA_THEME=dark.

use eframe::egui::{self, RichText};
use proava_application::ThemeChoice;
use proava_domain::Project;
use proava_ui::bids_view::BidsView;
use proava_ui::comparison_view::ComparisonView;
use proava_ui::demo::{demo_project, large_demo_project};
use proava_ui::theme::{self, Palette};
use proava_ui::view::ViewResponse;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Bids,
    Comparison,
}

struct Demo {
    project: Project,
    tab: Tab,
    theme: ThemeChoice,
    bids: BidsView,
    comparison: ComparisonView,
    dirty: bool,
    status: String,
}

impl eframe::App for Demo {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let cpu = frame.info().cpu_usage.unwrap_or_default() * 1000.0;
        theme::apply(ui.ctx(), self.theme);
        let palette = Palette::of(self.theme);

        egui::Panel::top("demo_top")
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .inner_margin(egui::Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("proAVA – Demo")
                            .size(14.0)
                            .color(palette.text),
                    );
                    ui.add_space(16.0);
                    ui.selectable_value(&mut self.tab, Tab::Bids, "Angebote");
                    ui.selectable_value(&mut self.tab, Tab::Comparison, "Preisspiegel");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if self.theme == ThemeChoice::Light {
                            "Dunkel"
                        } else {
                            "Hell"
                        };
                        if ui.button(label).clicked() {
                            self.theme = self.theme.flipped();
                        }
                    });
                });
            });

        egui::Panel::bottom("demo_status")
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .inner_margin(egui::Margin::symmetric(16, 5)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.status).size(11.0).color(palette.accent));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("Bild {cpu:.1} ms"))
                                .size(11.0)
                                .color(palette.text_dim),
                        );
                        ui.add_space(12.0);
                        let (text, color) = if self.dirty {
                            ("Ungesichert", palette.warn)
                        } else {
                            ("Gesichert", palette.text_dim)
                        };
                        ui.label(RichText::new(text).size(11.0).color(color));
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::symmetric(16, 14)),
            )
            .show(ui, |ui| {
                let response = match self.tab {
                    Tab::Bids => self.bids.ui(ui, &palette, &mut self.project),
                    Tab::Comparison => self.comparison.ui(ui, &palette, &mut self.project),
                };
                self.handle(response);
            });
    }
}

impl Demo {
    fn handle(&mut self, response: ViewResponse) {
        self.dirty |= response.dirty;
        if let Some(toast) = response.toast {
            self.status = toast;
        }
        for request in response.requests {
            self.status = format!("Anfrage an die Shell: {request:?}");
        }
    }
}

fn main() -> eframe::Result<()> {
    let project = match std::env::var("PROAVA_DEMO").as_deref() {
        Ok("large") => large_demo_project(13, 8),
        Ok("empty") => {
            let mut project = demo_project();
            let ids: Vec<_> = project.bids().iter().map(|bid| bid.id()).collect();
            for id in ids {
                project.remove_bid(id);
            }
            project
        }
        _ => demo_project(),
    };
    let tab = match std::env::var("PROAVA_VIEW").as_deref() {
        Ok("preisspiegel") => Tab::Comparison,
        _ => Tab::Bids,
    };
    let theme = match std::env::var("PROAVA_THEME").as_deref() {
        Ok("dark") => ThemeChoice::Dark,
        _ => ThemeChoice::Light,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 860.0])
            .with_title("proAVA – Angebote und Preisspiegel (Demo)"),
        ..Default::default()
    };
    eframe::run_native(
        "proava-bids-demo",
        options,
        Box::new(move |_| {
            Ok(Box::new(Demo {
                project,
                tab,
                theme,
                bids: BidsView::new(),
                comparison: ComparisonView::new(),
                dirty: false,
                status: String::new(),
            }))
        }),
    )
}
