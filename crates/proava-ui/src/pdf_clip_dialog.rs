//! „Ausschnitt aus PDF einfügen …“: choose a PDF with the program's file
//! dialog, pick the page, draw a rectangle on the page preview (e.g. half a
//! topology without frame and title block), set the width in the long text.
//! The page preview is rendered once per page in a background thread.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use proava_application::pdf_clip::{DEFAULT_CLIP_WIDTH_MM, MAX_CLIP_WIDTH_MM};
use proava_application::{ExchangeFiles, Launcher, PdfSource, Pixels, PreparedClip};
use proava_domain::{ClipRect, PT_PER_MM};

use crate::file_dialog::{DialogResult, FileDialog};
use crate::theme::Palette;
use crate::widgets;

pub const TITLE: &str = "Ausschnitt aus PDF einfügen";
/// Pixels across of the page preview in the dialog.
const PAGE_PREVIEW_PX: u32 = 1800;

/// What the dialog gives back.
pub enum ClipDialogResult {
    Open,
    Cancelled,
    /// The clip for the project and its width in the long text (mm).
    Chosen(Box<PreparedClip>, f32),
}

/// A result computed in a background thread.
struct Background<T> {
    slot: Arc<Mutex<Option<T>>>,
}

impl<T: Send + 'static> Background<T> {
    fn spawn(ctx: &egui::Context, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let slot = Arc::new(Mutex::new(None));
        let target = slot.clone();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("proava-pdf-ausschnitt".into())
            .spawn(move || {
                let result = work();
                if let Ok(mut slot) = target.lock() {
                    *slot = Some(result);
                }
                repaint.request_repaint();
            });
        if spawned.is_err() {
            // Nothing started: the dialog keeps waiting and can be cancelled.
            ctx.request_repaint();
        }
        Self { slot }
    }

    fn take(&self) -> Option<T> {
        self.slot.lock().ok()?.take()
    }
}

enum Step {
    File(Box<FileDialog>),
    Crop(Box<Crop>),
}

struct Crop {
    source: Arc<PdfSource>,
    page: u32,
    rect: ClipRect,
    width_mm: f32,
    /// Page preview: the texture, or the job on its way.
    picture: Option<Result<egui::TextureHandle, String>>,
    job: Option<(u32, Background<Result<Pixels, String>>)>,
    /// Corner where the mouse drag started (page points).
    anchor: Option<(f32, f32)>,
    preparing: Option<Background<Result<PreparedClip, String>>>,
    error: Option<String>,
}

pub struct PdfClipDialog {
    step: Step,
    exchange: Arc<dyn ExchangeFiles>,
    /// Folder of the chosen PDF (to start there next time).
    pub folder: Option<PathBuf>,
}

impl PdfClipDialog {
    /// Starts with the file dialog in `start`.
    pub fn open(launcher: &Launcher, start: PathBuf) -> Self {
        let dialog = FileDialog::open(
            launcher,
            TITLE,
            "PDF wählen – danach Seite und Ausschnitt.",
            start,
            &["pdf"],
            false,
        );
        Self {
            step: Step::File(Box::new(dialog)),
            exchange: launcher.shared_exchange(),
            folder: None,
        }
    }

    /// Starts with a PDF already read (tests, drag and drop).
    pub fn with_pdf(launcher: &Launcher, name: &str, bytes: Vec<u8>) -> Result<Self, String> {
        let exchange = launcher.shared_exchange();
        let source = PdfSource::read(exchange.as_ref(), name, bytes)?;
        Ok(Self {
            step: Step::Crop(Box::new(Crop::new(source))),
            exchange,
            folder: None,
        })
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        launcher: &Launcher,
    ) -> ClipDialogResult {
        match &mut self.step {
            Step::File(dialog) => match dialog.show(ui, palette, launcher) {
                DialogResult::Open => ClipDialogResult::Open,
                DialogResult::Cancelled => ClipDialogResult::Cancelled,
                DialogResult::Chosen(paths) => {
                    let Some(path) = paths.into_iter().next() else {
                        return ClipDialogResult::Cancelled;
                    };
                    self.folder = path.parent().map(std::path::Path::to_path_buf);
                    let name = crate::file_dialog::file_name(&path);
                    let read = launcher
                        .read_file(&path)
                        .and_then(|bytes| PdfSource::read(self.exchange.as_ref(), &name, bytes));
                    match read {
                        Ok(source) => {
                            self.step = Step::Crop(Box::new(Crop::new(source)));
                            ui.ctx().request_repaint();
                            ClipDialogResult::Open
                        }
                        Err(error) => {
                            // Back to the file choice with the reason.
                            let start = self
                                .folder
                                .clone()
                                .unwrap_or_else(|| launcher.home_folder());
                            let mut again = Self::open(launcher, start);
                            again.folder = self.folder.clone();
                            *self = again;
                            if let Step::File(dialog) = &mut self.step {
                                dialog.set_error(error);
                            }
                            ClipDialogResult::Open
                        }
                    }
                }
            },
            Step::Crop(crop) => crop.show(ui, palette, &self.exchange),
        }
    }
}

impl Crop {
    fn new(source: PdfSource) -> Self {
        let (width, height) = source.page_size(1).unwrap_or((595.0, 842.0));
        let rect = ClipRect::page(width, height);
        Self {
            width_mm: DEFAULT_CLIP_WIDTH_MM.min(rect.width_mm()).max(10.0),
            source: Arc::new(source),
            page: 1,
            rect,
            picture: None,
            job: None,
            anchor: None,
            preparing: None,
            error: None,
        }
    }

    fn page_size(&self) -> (f32, f32) {
        self.source.page_size(self.page).unwrap_or((595.0, 842.0))
    }

    /// Starts (once per page) the rendering of the page preview.
    fn ensure_picture(&mut self, ctx: &egui::Context, exchange: &Arc<dyn ExchangeFiles>) {
        if let Some((page, job)) = &self.job {
            if *page == self.page {
                if let Some(result) = job.take() {
                    self.picture = Some(result.and_then(|pixels| {
                        let size = [pixels.width as usize, pixels.height as usize];
                        if pixels.rgba.len() != size[0] * size[1] * 4 {
                            return Err("Die Vorschau ist leer.".to_string());
                        }
                        let image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels.rgba);
                        Ok(ctx.load_texture(
                            "pdf-clip-dialog-page",
                            image,
                            egui::TextureOptions::LINEAR
                                .with_mipmap_mode(Some(egui::TextureFilter::Linear)),
                        ))
                    }));
                    self.job = None;
                }
                return;
            }
        }
        if self.picture.is_some() {
            return;
        }
        let (width, height) = self.page_size();
        let source = self.source.clone();
        let exchange = exchange.clone();
        let page = self.page;
        // Long side about PAGE_PREVIEW_PX.
        let width_px = if width >= height {
            PAGE_PREVIEW_PX
        } else {
            ((PAGE_PREVIEW_PX as f32) * width / height).round() as u32
        };
        let job = Background::spawn(ctx, move || {
            exchange.render_pdf_clip(&source.bytes, page, ClipRect::page(width, height), width_px)
        });
        self.job = Some((page, job));
    }

    fn set_page(&mut self, page: u32) {
        if page == self.page || self.source.page_size(page).is_none() {
            return;
        }
        self.page = page;
        let (width, height) = self.page_size();
        self.rect = ClipRect::page(width, height);
        self.width_mm = DEFAULT_CLIP_WIDTH_MM.min(self.rect.width_mm()).max(10.0);
        self.picture = None;
        self.anchor = None;
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        exchange: &Arc<dyn ExchangeFiles>,
    ) -> ClipDialogResult {
        let ctx = ui.ctx().clone();
        self.ensure_picture(&ctx, exchange);
        if let Some(preparing) = &self.preparing {
            if let Some(result) = preparing.take() {
                self.preparing = None;
                match result {
                    Ok(prepared) => {
                        return ClipDialogResult::Chosen(Box::new(prepared), self.width_mm)
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        let screen = ctx.content_rect();
        let width = (screen.width() - 60.0).clamp(560.0, 1100.0);
        let mut cancelled = false;
        let mut confirmed = false;
        let (_, close) = widgets::modal(ui, palette, width, |ui| {
            let pages = self.source.pages.len();
            widgets::title(
                ui,
                palette,
                TITLE,
                "Mit der Maus ein Rechteck aufziehen, z. B. nur die Zeichnung ohne Rahmen und Plankopf. \
                 Im proAVA-PDF bleibt der Ausschnitt Vektorgrafik.",
            );
            crate::widgets::row_wrapped(ui, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} · {} · {}",
                        self.source.name,
                        crate::format::count(pages, "Seite", "Seiten"),
                        proava_domain::size_text(self.source.bytes.len())
                    ))
                    .color(palette.text_dim),
                );
                if pages > 1 {
                    ui.add_space(12.0);
                    ui.label("Seite");
                    let mut page = self.page;
                    ui.add(egui::DragValue::new(&mut page).range(1..=pages as u32));
                    ui.label(format!("von {pages}"));
                    if page != self.page {
                        self.set_page(page);
                    }
                }
                let (w, h) = self.page_size();
                ui.add_space(12.0);
                ui.label(RichText::new(proava_domain::paper_name(w, h)).color(palette.text_dim));
            });
            ui.add_space(6.0);
            let height = (screen.height() - 330.0).clamp(220.0, 760.0);
            self.page_view(ui, palette, Vec2::new(ui.available_width(), height));
            ui.add_space(6.0);
            self.fields(ui, palette);
            if let Some(error) = &self.error {
                ui.label(RichText::new(error).color(palette.error));
            }
            ui.add_space(6.0);
            if self.preparing.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Ausschnitt wird übernommen …");
                });
            }
            let (cancel, confirm) = widgets::dialog_buttons(ui, palette, "Einfügen");
            cancelled = cancel;
            confirmed = confirm;
        });
        if cancelled || (close && self.preparing.is_none()) {
            return ClipDialogResult::Cancelled;
        }
        if confirmed && self.preparing.is_none() {
            if self.rect.is_valid() {
                self.error = None;
                let source = self.source.clone();
                let exchange = exchange.clone();
                let (page, rect) = (self.page, self.rect);
                self.preparing = Some(Background::spawn(&ctx, move || {
                    proava_application::prepare_clip(exchange.as_ref(), &source, page, rect)
                }));
            } else {
                self.error = Some("Der Ausschnitt ist zu klein (mindestens 5 × 5 mm).".to_string());
            }
        }
        ClipDialogResult::Open
    }

    /// The page with the rectangle; dragging draws a new one.
    fn page_view(&mut self, ui: &mut egui::Ui, palette: &Palette, room: Vec2) {
        let (page_w, page_h) = self.page_size();
        let scale = (room.x / page_w).min(room.y / page_h).max(0.01);
        let size = Vec2::new(page_w * scale, page_h * scale);
        let (outer, _) = ui.allocate_exact_size(Vec2::new(room.x, size.y), Sense::hover());
        let area = Rect::from_min_size(
            Pos2::new(outer.center().x - size.x / 2.0, outer.min.y),
            size,
        );
        let response = ui.interact(
            area,
            ui.scope_id().with("pdf-clip-page"),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(outer);
        painter.rect_filled(area, 0.0, Color32::WHITE);
        match &self.picture {
            Some(Ok(texture)) => {
                painter.image(
                    texture.id(),
                    area,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            Some(Err(error)) => {
                painter.text(
                    area.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("Vorschau nicht möglich: {error}"),
                    egui::FontId::proportional(13.0),
                    palette.error,
                );
            }
            None => {
                painter.text(
                    area.center(),
                    egui::Align2::CENTER_CENTER,
                    "Seite wird gerechnet …",
                    egui::FontId::proportional(14.0),
                    palette.text_dim,
                );
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        painter.rect_stroke(
            area,
            0.0,
            Stroke::new(1.0, palette.line),
            egui::StrokeKind::Outside,
        );

        let to_page = |pos: Pos2| {
            (
                ((pos.x - area.min.x) / scale).clamp(0.0, page_w),
                ((pos.y - area.min.y) / scale).clamp(0.0, page_h),
            )
        };
        if response.drag_started() {
            // Where the button went down (the drag starts a few pixels later).
            let origin = ui.input(|input| input.pointer.press_origin());
            self.anchor = origin
                .or_else(|| response.interact_pointer_pos())
                .map(to_page);
        }
        if response.dragged() {
            if let (Some(anchor), Some(pos)) = (self.anchor, response.interact_pointer_pos()) {
                let (x, y) = to_page(pos);
                self.rect = ClipRect::new(
                    anchor.0.min(x),
                    anchor.1.min(y),
                    (x - anchor.0).abs(),
                    (y - anchor.1).abs(),
                );
            }
        }
        if response.drag_stopped() {
            self.anchor = None;
            if !self.rect.is_valid() {
                self.rect = ClipRect::page(page_w, page_h);
            }
            self.width_mm = self.width_mm.min(self.rect.width_mm()).max(10.0);
        }

        // Shade what stays outside, frame the rectangle.
        let chosen = Rect::from_min_size(
            Pos2::new(
                area.min.x + self.rect.left * scale,
                area.min.y + self.rect.top * scale,
            ),
            Vec2::new(self.rect.width * scale, self.rect.height * scale),
        );
        let shade = Color32::from_black_alpha(90);
        for part in [
            Rect::from_min_max(area.min, Pos2::new(area.max.x, chosen.min.y)),
            Rect::from_min_max(Pos2::new(area.min.x, chosen.max.y), area.max),
            Rect::from_min_max(
                Pos2::new(area.min.x, chosen.min.y),
                Pos2::new(chosen.min.x, chosen.max.y),
            ),
            Rect::from_min_max(
                Pos2::new(chosen.max.x, chosen.min.y),
                Pos2::new(area.max.x, chosen.max.y),
            ),
        ] {
            if part.is_positive() {
                painter.rect_filled(part, 0.0, shade);
            }
        }
        painter.rect_stroke(
            chosen,
            0.0,
            Stroke::new(2.0, palette.accent),
            egui::StrokeKind::Middle,
        );
        response.on_hover_cursor(egui::CursorIcon::Crosshair);
    }

    /// Rectangle in mm, width in the long text, size note.
    fn fields(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        let (page_w, page_h) = self.page_size();
        let mut left = self.rect.left / PT_PER_MM;
        let mut top = self.rect.top / PT_PER_MM;
        let mut width = self.rect.width_mm();
        let mut height = self.rect.height_mm();
        let mut changed = false;
        crate::widgets::row_wrapped(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (label, value, max) in [
                ("Links", &mut left, page_w / PT_PER_MM),
                ("Oben", &mut top, page_h / PT_PER_MM),
                ("Breite", &mut width, page_w / PT_PER_MM),
                ("Höhe", &mut height, page_h / PT_PER_MM),
            ] {
                ui.label(label);
                changed |= ui
                    .add(
                        egui::DragValue::new(value)
                            .range(0.0..=max)
                            .speed(1.0)
                            .max_decimals(0)
                            .suffix(" mm"),
                    )
                    .changed();
            }
            if ui.button("Ganze Seite").clicked() {
                self.rect = ClipRect::page(page_w, page_h);
                self.width_mm = self.width_mm.min(self.rect.width_mm()).max(10.0);
            }
        });
        if changed {
            self.rect = ClipRect::new(
                left * PT_PER_MM,
                top * PT_PER_MM,
                width * PT_PER_MM,
                height * PT_PER_MM,
            )
            .clamped(page_w, page_h);
        }
        crate::widgets::row_wrapped(ui, |ui| {
            ui.label("Breite im Langtext");
            ui.add(
                egui::DragValue::new(&mut self.width_mm)
                    .range(10.0..=MAX_CLIP_WIDTH_MM)
                    .speed(1.0)
                    .max_decimals(0)
                    .suffix(" mm"),
            );
            let scale = self.rect.width_mm() / self.width_mm.max(1.0);
            let ratio = if scale >= 1.0 {
                format!("verkleinert 1 : {}", format_scale(scale))
            } else {
                format!("vergrößert {} : 1", format_scale(1.0 / scale))
            };
            let stored = if self.source.pages.len() > 1 {
                "ins Projekt kommt nur diese Seite".to_string()
            } else {
                format!(
                    "im Projekt {}",
                    proava_domain::size_text(self.source.bytes.len())
                )
            };
            ui.label(
                RichText::new(format!(
                    "Ausschnitt {} · im Text {:.0} × {:.0} mm ({ratio}) · {stored}",
                    self.rect.size_text(),
                    self.width_mm,
                    self.width_mm * self.rect.aspect(),
                ))
                .size(12.0)
                .color(palette.text_dim),
            );
        });
    }
}

fn format_scale(value: f32) -> String {
    if value >= 10.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}").replace('.', ",")
    }
}
