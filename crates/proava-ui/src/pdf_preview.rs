//! Vorschau der PDF-Ausschnitte im Langtext.
//!
//! A big vector page (A0 topology) is expensive to draw. So the editor never
//! draws the PDF itself: a preview is rendered once at a fitting width, in a
//! background thread (one for the whole program, one job after the other),
//! and kept as a texture. It is rendered again only when the shown width
//! grows noticeably. A frozen preview is the PNG stored in the project – no
//! rendering at all; a hidden preview is only a placeholder.
//!
//! The service lives in the egui context ([`ClipPreviews::of`]); the main
//! window installs it with the renderer of the exchange adapter. Changes of
//! the preview choices (einfrieren, ausblenden) are collected here and taken
//! into the project by the main window ([`ClipPreviews::take_updates`]).

use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};

use eframe::egui;
use proava_application::Pixels;
use proava_domain::{ClipRect, ImageAsset};

/// Renders a rectangle of a PDF page: (pdf, page, rect, width in px).
pub type Renderer = Arc<dyn Fn(&[u8], u32, ClipRect, u32) -> Result<Pixels, String> + Send + Sync>;

/// Width steps of a live preview (pixels): it is rendered again only when
/// the shown width needs the next step.
const STEP_PX: u32 = 400;
const MIN_PX: u32 = 400;
const MAX_PX: u32 = 2400;
/// Width of a frozen preview (pixels).
pub const FREEZE_PX: u32 = 1600;
/// Previews kept (textures); the oldest go first.
const MAX_ENTRIES: usize = 48;

/// Preview choices of one clip, for the project.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewUpdate {
    pub file: String,
    pub frozen: bool,
    pub hidden: bool,
    /// The frozen picture (PNG) with `frozen`.
    pub png: Option<Arc<Vec<u8>>>,
}

/// What the editor shows for a clip in this frame.
pub enum Shown {
    Picture(egui::TextureHandle),
    /// Being rendered; the older (smaller) picture if there is one.
    Waiting(Option<egui::TextureHandle>),
    Failed(String),
    /// No renderer installed (tests, other windows).
    Unavailable,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    file: String,
    fingerprint: u64,
    length: usize,
    page: u32,
    rect: [u32; 4],
    frozen: bool,
}

impl Key {
    fn of(asset: &ImageAsset, frozen: bool) -> Option<Self> {
        let clip = asset.clip.as_ref()?;
        Some(Self {
            file: asset.file.clone(),
            fingerprint: proava_domain::pdf_clip::fingerprint(&asset.bytes),
            length: asset.bytes.len(),
            page: clip.page,
            rect: [
                clip.rect.left.to_bits(),
                clip.rect.top.to_bits(),
                clip.rect.width.to_bits(),
                clip.rect.height.to_bits(),
            ],
            frozen,
        })
    }
}

#[derive(Default)]
struct Entry {
    texture: Option<egui::TextureHandle>,
    /// Width of `texture` in pixels.
    width_px: u32,
    /// Width of the job on its way.
    pending_px: Option<u32>,
    error: Option<String>,
    used: u64,
}

enum Job {
    Render {
        key: Key,
        pdf: Arc<Vec<u8>>,
        page: u32,
        rect: ClipRect,
        width_px: u32,
    },
    Decode {
        key: Key,
        png: Arc<Vec<u8>>,
    },
    Freeze {
        key: Key,
        pdf: Arc<Vec<u8>>,
        page: u32,
        rect: ClipRect,
        hidden: bool,
    },
}

struct Inner {
    ctx: egui::Context,
    renderer: Option<Renderer>,
    /// Run jobs at once in the calling thread (tests).
    inline: bool,
    sender: Option<mpsc::Sender<Job>>,
    entries: HashMap<Key, Entry>,
    clock: u64,
    all_hidden: bool,
    updates: Vec<PreviewUpdate>,
    toggle_all: Option<bool>,
    /// Renderings so far (PDF → pixels), for tests and the status line.
    renders: usize,
    /// Files being frozen right now.
    freezing: Vec<String>,
}

/// The preview service of one program window (shared via the egui context).
#[derive(Clone)]
pub struct ClipPreviews(Arc<Mutex<Inner>>);

impl ClipPreviews {
    /// Installs the service with `renderer` (replaces an older one).
    pub fn install(ctx: &egui::Context, renderer: Option<Renderer>) -> Self {
        Self::install_with(ctx, renderer, false)
    }

    /// Like [`ClipPreviews::install`], but every job runs at once in the
    /// calling thread – for tests.
    pub fn install_inline(ctx: &egui::Context, renderer: Renderer) -> Self {
        Self::install_with(ctx, Some(renderer), true)
    }

    fn install_with(ctx: &egui::Context, renderer: Option<Renderer>, inline: bool) -> Self {
        let service = Self(Arc::new(Mutex::new(Inner {
            ctx: ctx.clone(),
            renderer,
            inline,
            sender: None,
            entries: HashMap::new(),
            clock: 0,
            all_hidden: false,
            updates: Vec::new(),
            toggle_all: None,
            renders: 0,
            freezing: Vec::new(),
        })));
        ctx.data_mut(|data| data.insert_temp(egui::Id::unique(SERVICE_ID), service.clone()));
        service
    }

    /// The service of this window; a service without renderer when none was
    /// installed (frozen previews still show).
    pub fn of(ctx: &egui::Context) -> Self {
        if let Some(service) =
            ctx.data(|data| data.get_temp::<ClipPreviews>(egui::Id::unique(SERVICE_ID)))
        {
            return service;
        }
        Self::install(ctx, None)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn has_renderer(&self) -> bool {
        self.lock().renderer.is_some()
    }

    /// Einstellung „Vorschau aller PDF-Ausschnitte ausblenden“.
    pub fn all_hidden(&self) -> bool {
        self.lock().all_hidden
    }

    pub fn set_all_hidden(&self, hidden: bool) {
        self.lock().all_hidden = hidden;
    }

    /// The editor asks to switch all previews on or off (the main window
    /// stores it in the settings).
    pub fn request_all_hidden(&self, hidden: bool) {
        let mut inner = self.lock();
        inner.all_hidden = hidden;
        inner.toggle_all = Some(hidden);
    }

    pub fn take_all_hidden_request(&self) -> Option<bool> {
        self.lock().toggle_all.take()
    }

    /// Number of renderings so far.
    pub fn renders(&self) -> usize {
        self.lock().renders
    }

    pub fn is_freezing(&self, file: &str) -> bool {
        self.lock().freezing.iter().any(|name| name == file)
    }

    /// Preview choices to take into the project.
    pub fn take_updates(&self) -> Vec<PreviewUpdate> {
        std::mem::take(&mut self.lock().updates)
    }

    /// Hide/show or unfreeze a clip (changes the project next frame).
    pub fn request(&self, update: PreviewUpdate) {
        self.lock().updates.push(update);
    }

    /// Freezes the preview of `asset`: renders it once at [`FREEZE_PX`] and
    /// hands the PNG to the project.
    pub fn freeze(&self, asset: &ImageAsset) {
        let Some(clip) = asset.clip.as_ref() else {
            return;
        };
        let Some(key) = Key::of(asset, true) else {
            return;
        };
        let job = Job::Freeze {
            key,
            pdf: Arc::new(asset.bytes.clone()),
            page: clip.page,
            rect: clip.rect,
            hidden: clip.preview_hidden,
        };
        {
            let mut inner = self.lock();
            if inner.renderer.is_none() || inner.freezing.contains(&asset.file) {
                return;
            }
            inner.freezing.push(asset.file.clone());
        }
        self.send(job);
    }

    /// The picture of a clip at about `wanted_px` pixels across. Never
    /// renders in the calling thread (unless installed inline) and never
    /// twice for the same width step.
    pub fn picture(&self, asset: &ImageAsset, wanted_px: u32) -> Shown {
        let Some(clip) = asset.clip.as_ref() else {
            return Shown::Failed(format!("„{}“ ist kein PDF-Ausschnitt.", asset.file));
        };
        let frozen_png = clip.frozen_png.clone().filter(|_| clip.preview_frozen);
        let Some(key) = Key::of(asset, frozen_png.is_some()) else {
            return Shown::Unavailable;
        };
        let job = {
            let mut inner = self.lock();
            inner.clock += 1;
            let clock = inner.clock;
            let has_renderer = inner.renderer.is_some();
            if frozen_png.is_none() && !has_renderer {
                return Shown::Unavailable;
            }
            if inner.entries.len() >= MAX_ENTRIES && !inner.entries.contains_key(&key) {
                if let Some(oldest) = inner
                    .entries
                    .iter()
                    .filter(|(_, entry)| entry.pending_px.is_none())
                    .min_by_key(|(_, entry)| entry.used)
                    .map(|(key, _)| key.clone())
                {
                    inner.entries.remove(&oldest);
                }
            }
            let entry = inner.entries.entry(key.clone()).or_default();
            entry.used = clock;
            if let Some(error) = &entry.error {
                return Shown::Failed(error.clone());
            }
            match frozen_png {
                Some(png) => {
                    if let Some(texture) = &entry.texture {
                        return Shown::Picture(texture.clone());
                    }
                    if entry.pending_px.is_some() {
                        return Shown::Waiting(None);
                    }
                    entry.pending_px = Some(0);
                    Job::Decode { key, png }
                }
                None => {
                    let step = wanted_px
                        .clamp(MIN_PX, MAX_PX)
                        .div_ceil(STEP_PX)
                        .saturating_mul(STEP_PX)
                        .min(MAX_PX);
                    // Big enough (a little blur is fine): keep it.
                    if let Some(texture) = entry
                        .texture
                        .as_ref()
                        .filter(|_| entry.width_px * 5 >= step * 4)
                    {
                        return Shown::Picture(texture.clone());
                    }
                    if entry.pending_px.is_some_and(|pending| pending >= step) {
                        return Shown::Waiting(entry.texture.clone());
                    }
                    entry.pending_px = Some(step);
                    Job::Render {
                        key,
                        pdf: Arc::new(asset.bytes.clone()),
                        page: clip.page,
                        rect: clip.rect,
                        width_px: step,
                    }
                }
            }
        };
        self.send(job);
        // Inline (tests): the result is there already.
        let inner = self.lock();
        let key = Key::of(asset, clip.preview_frozen && clip.frozen_png.is_some());
        match key.and_then(|key| inner.entries.get(&key)) {
            Some(Entry {
                error: Some(error), ..
            }) => Shown::Failed(error.clone()),
            Some(Entry {
                texture: Some(texture),
                pending_px: None,
                ..
            }) => Shown::Picture(texture.clone()),
            Some(entry) => Shown::Waiting(entry.texture.clone()),
            None => Shown::Waiting(None),
        }
    }

    fn send(&self, job: Job) {
        let (inline, renderer) = {
            let inner = self.lock();
            (inner.inline, inner.renderer.clone())
        };
        if inline {
            run_job(&self.0, renderer.as_ref(), job);
            return;
        }
        let mut inner = self.lock();
        if inner.sender.is_none() {
            let (sender, receiver) = mpsc::channel::<Job>();
            let shared = Arc::downgrade(&self.0);
            let renderer = inner.renderer.clone();
            let spawned = std::thread::Builder::new()
                .name("proava-pdf-vorschau".into())
                .spawn(move || {
                    for job in receiver {
                        let Some(shared) = shared.upgrade() else {
                            break;
                        };
                        run_job(&shared, renderer.as_ref(), job);
                    }
                });
            if spawned.is_err() {
                // No thread: show the error instead of rendering in the UI thread.
                drop(inner);
                finish_failed(
                    &self.0,
                    &job,
                    "Die Vorschau kann nicht gerechnet werden (kein Hintergrund-Thread).".into(),
                );
                return;
            }
            inner.sender = Some(sender);
        }
        if let Some(sender) = &inner.sender {
            let _ = sender.send(job);
        }
    }
}

const SERVICE_ID: &str = "proava-pdf-clip-previews";

/// Smooth when shown smaller than rendered (mipmaps): thin lines of a plan
/// stay calm instead of flickering.
const TEXTURE: egui::TextureOptions = egui::TextureOptions {
    magnification: egui::TextureFilter::Linear,
    minification: egui::TextureFilter::Linear,
    wrap_mode: egui::TextureWrapMode::ClampToEdge,
    mipmap_mode: Some(egui::TextureFilter::Linear),
};

fn key_of(job: &Job) -> &Key {
    match job {
        Job::Render { key, .. } | Job::Decode { key, .. } | Job::Freeze { key, .. } => key,
    }
}

fn finish_failed(shared: &Arc<Mutex<Inner>>, job: &Job, error: String) {
    let mut inner = shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Job::Freeze { key, .. } = job {
        inner.freezing.retain(|file| *file != key.file);
    }
    let entry = inner.entries.entry(key_of(job).clone()).or_default();
    entry.pending_px = None;
    entry.error = Some(error);
    inner.ctx.request_repaint();
}

fn color_image(pixels: &Pixels) -> Result<egui::ColorImage, String> {
    let expected = pixels.width as usize * pixels.height as usize * 4;
    if pixels.rgba.len() != expected || expected == 0 {
        return Err("Die Vorschau ist leer.".to_string());
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [pixels.width as usize, pixels.height as usize],
        &pixels.rgba,
    ))
}

fn run_job(shared: &Arc<Mutex<Inner>>, renderer: Option<&Renderer>, job: Job) {
    let ctx = shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .ctx
        .clone();
    match &job {
        Job::Render {
            key,
            pdf,
            page,
            rect,
            width_px,
        } => {
            // Still wanted at this width (the user may have scrolled on)?
            {
                let inner = shared
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if inner.entries.get(key).and_then(|entry| entry.pending_px) != Some(*width_px) {
                    return;
                }
            }
            let Some(renderer) = renderer else {
                finish_failed(shared, &job, "Keine Vorschau verfügbar.".into());
                return;
            };
            let result =
                renderer(pdf, *page, *rect, *width_px).and_then(|pixels| color_image(&pixels));
            let mut inner = shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            inner.renders += 1;
            match result {
                Ok(image) => {
                    let width = image.size[0] as u32;
                    let texture =
                        ctx.load_texture(format!("pdf-clip-{}-{width}", key.file), image, TEXTURE);
                    if let Some(entry) = inner.entries.get_mut(key) {
                        entry.texture = Some(texture);
                        entry.width_px = width.max(*width_px);
                        entry.pending_px = None;
                        entry.error = None;
                    }
                    ctx.request_repaint();
                }
                Err(error) => {
                    drop(inner);
                    finish_failed(shared, &job, error);
                }
            }
        }
        Job::Decode { key, png } => {
            let result = crate::images::decode_preview_width(png, FREEZE_PX);
            match result {
                Ok(image) => {
                    let width = image.size[0] as u32;
                    let texture =
                        ctx.load_texture(format!("pdf-clip-frozen-{}", key.file), image, TEXTURE);
                    let mut inner = shared
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let entry = inner.entries.entry(key.clone()).or_default();
                    entry.texture = Some(texture);
                    entry.width_px = width;
                    entry.pending_px = None;
                    ctx.request_repaint();
                }
                Err(error) => {
                    finish_failed(shared, &job, format!("Eingefrorene Vorschau: {error}"))
                }
            }
        }
        Job::Freeze {
            key,
            pdf,
            page,
            rect,
            hidden,
        } => {
            let Some(renderer) = renderer else {
                finish_failed(shared, &job, "Keine Vorschau verfügbar.".into());
                return;
            };
            let result = renderer(pdf, *page, *rect, FREEZE_PX).and_then(|pixels| {
                let image = color_image(&pixels)?;
                let png = crate::images::encode_png(&image)?;
                Ok((image, png))
            });
            let mut inner = shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            inner.renders += 1;
            inner.freezing.retain(|file| *file != key.file);
            match result {
                Ok((image, png)) => {
                    let width = image.size[0] as u32;
                    let texture =
                        ctx.load_texture(format!("pdf-clip-frozen-{}", key.file), image, TEXTURE);
                    let entry = inner.entries.entry(key.clone()).or_default();
                    entry.texture = Some(texture);
                    entry.width_px = width;
                    entry.pending_px = None;
                    inner.updates.push(PreviewUpdate {
                        file: key.file.clone(),
                        frozen: true,
                        hidden: *hidden,
                        png: Some(Arc::new(png)),
                    });
                    ctx.request_repaint();
                }
                Err(error) => {
                    drop(inner);
                    finish_failed(shared, &job, error);
                }
            }
        }
    }
}

/// Takes the collected preview choices into `project`; true when it changed.
pub fn apply_updates(ctx: &egui::Context, project: &mut proava_domain::Project) -> bool {
    let service = ClipPreviews::of(ctx);
    let mut changed = false;
    for update in service.take_updates() {
        changed |= project.set_clip_preview(&update.file, update.frozen, update.hidden, update.png);
    }
    changed
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use proava_domain::{PdfClip, Project};

    use super::*;

    fn asset() -> ImageAsset {
        let mut project = Project::new(
            proava_domain::ProjectName::parse("P").unwrap(),
            "",
            proava_domain::Lv::new("LV", proava_domain::OzMask::parse("11.PPPP").unwrap()),
        );
        let name = project.add_pdf_clip(
            "plan.pdf",
            b"%PDF-1.7 synthetisch".to_vec(),
            PdfClip::new(
                1,
                (600.0, 400.0),
                ClipRect::new(0.0, 0.0, 300.0, 200.0),
                "plan.pdf",
            ),
        );
        project.image(&name).unwrap().clone()
    }

    fn counting() -> (Renderer, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let renderer: Renderer = Arc::new(move |_, _, rect, width| {
            seen.fetch_add(1, Ordering::SeqCst);
            let height = ((width as f32) * rect.aspect()).round() as u32;
            Ok(Pixels {
                width,
                height,
                rgba: vec![255; (width * height * 4) as usize],
            })
        });
        (renderer, count)
    }

    #[test]
    fn rendered_once_per_width_step() {
        let ctx = egui::Context::default();
        let (renderer, count) = counting();
        let service = ClipPreviews::install_inline(&ctx, renderer);
        let asset = asset();
        for _ in 0..50 {
            assert!(matches!(service.picture(&asset, 700), Shown::Picture(_)));
        }
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "einmal gerechnet, nicht je Frame"
        );
        // A little wider, same step (800 px): the picture is big enough.
        assert!(matches!(service.picture(&asset, 790), Shown::Picture(_)));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        // Wider: once more per step, narrower: never.
        assert!(matches!(service.picture(&asset, 1700), Shown::Picture(_)));
        for width in [300, 800, 1600, 2000] {
            assert!(matches!(service.picture(&asset, width), Shown::Picture(_)));
        }
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(service.renders(), 2);
    }

    #[test]
    fn frozen_preview_is_never_rendered() {
        let ctx = egui::Context::default();
        let (renderer, count) = counting();
        let service = ClipPreviews::install_inline(&ctx, renderer);
        let mut asset = asset();
        service.freeze(&asset);
        assert_eq!(count.load(Ordering::SeqCst), 1, "Einfrieren rechnet einmal");
        let updates = service.take_updates();
        assert_eq!(updates.len(), 1);
        assert!(updates[0].frozen && updates[0].png.is_some());
        let clip = asset.clip.as_mut().unwrap();
        clip.preview_frozen = true;
        clip.frozen_png = updates[0].png.clone();
        for width in [400, 1200, 2400] {
            assert!(matches!(service.picture(&asset, width), Shown::Picture(_)));
        }
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "eingefroren: kein neues Rechnen"
        );
    }

    #[test]
    fn the_service_stays_in_the_context() {
        let ctx = egui::Context::default();
        let (renderer, _) = counting();
        ClipPreviews::install(&ctx, Some(renderer));
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
            output.textures_delta.clear();
            assert!(
                ClipPreviews::of(&ctx).has_renderer(),
                "derselbe Dienst über mehrere Bilder"
            );
        }
    }

    #[test]
    fn without_renderer_only_frozen_previews_show() {
        let ctx = egui::Context::default();
        let service = ClipPreviews::of(&ctx);
        assert!(matches!(service.picture(&asset(), 800), Shown::Unavailable));
    }
}
