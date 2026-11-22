use std::collections::HashMap;
use std::sync::LazyLock;

use typst::diag::{FileError, FileResult, Severity, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World, WorldExt};
use typst_layout::PagedDocument;

use crate::PrintError;

static FONT_FILES: [&[u8]; 12] = [
    include_bytes!("../fonts/LiberationSans-Regular.ttf"),
    include_bytes!("../fonts/LiberationSans-Bold.ttf"),
    include_bytes!("../fonts/LiberationSans-Italic.ttf"),
    include_bytes!("../fonts/LiberationSans-BoldItalic.ttf"),
    include_bytes!("../fonts/LiberationSerif-Regular.ttf"),
    include_bytes!("../fonts/LiberationSerif-Bold.ttf"),
    include_bytes!("../fonts/LiberationSerif-Italic.ttf"),
    include_bytes!("../fonts/LiberationSerif-BoldItalic.ttf"),
    include_bytes!("../fonts/LiberationMono-Regular.ttf"),
    include_bytes!("../fonts/LiberationMono-Bold.ttf"),
    include_bytes!("../fonts/LiberationMono-Italic.ttf"),
    include_bytes!("../fonts/LiberationMono-BoldItalic.ttf"),
];

struct Shared {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

static SHARED: LazyLock<Shared> = LazyLock::new(|| {
    let fonts: Vec<Font> = FONT_FILES
        .iter()
        .flat_map(|data| Font::iter(Bytes::new(*data)))
        .collect();
    Shared {
        library: LazyHash::new(Library::default()),
        book: LazyHash::new(FontBook::from_fonts(&fonts)),
        fonts,
    }
});

/// The embedded font files (Liberation Sans, Serif, Mono).
pub(crate) fn font_files() -> &'static [&'static [u8]] {
    &FONT_FILES
}

const COMMON: &str = include_str!("../templates/common.typ");
const COVER: &str = include_str!("../templates/cover.typ");
const POINTS: &str = include_str!("../templates/points.typ");

pub fn file_id(path: &str) -> FileId {
    RootedPath::new(
        VirtualRoot::Project,
        VirtualPath::new(path).expect("valid virtual path"),
    )
    .intern()
}

/// In-memory world: one main template, the shared helpers, data and images.
pub struct PrintWorld {
    main: Source,
    /// Shared templates every printout may import (common.typ, cover.typ,
    /// points.typ).
    library: Vec<Source>,
    files: HashMap<FileId, Bytes>,
}

impl PrintWorld {
    pub fn new(main_template: &str) -> Self {
        Self {
            main: Source::new(file_id("/main.typ"), main_template.to_owned()),
            library: vec![
                Source::new(file_id("/common.typ"), COMMON.to_owned()),
                Source::new(file_id("/cover.typ"), COVER.to_owned()),
                Source::new(file_id("/points.typ"), POINTS.to_owned()),
            ],
            files: HashMap::new(),
        }
    }

    /// The same printout without PDF-Lesezeichen (tests: the print must not
    /// change with them).
    #[cfg(test)]
    pub fn without_bookmarks(mut self) -> Self {
        let id = file_id("/common.typ");
        for source in &mut self.library {
            if source.id() == id {
                *source = Source::new(
                    id,
                    format!(
                        "{COMMON}\n#let bookmark(bm) = none\n#let bookmark-at(level, text) = none\n#let bookmark-once(bm) = none\n"
                    ),
                );
            }
        }
        self
    }

    pub fn add_file(&mut self, path: &str, data: Vec<u8>) {
        self.files.insert(file_id(path), Bytes::new(data));
    }

    pub fn layout(&self) -> Result<PagedDocument, PrintError> {
        let document = lean_compile(self)
            .output
            .map_err(|errors| PrintError::Layout(self.describe(&errors)));
        crate::stage_done(crate::Stage::Layout);
        document
    }

    /// Tagged PDF (Typst default). poppler up to 25.x (pdfinfo, pdftotext,
    /// xreader) reports „Syntax Error: Suspects object is wrong type
    /// (boolean)“ for these files: krilla writes `/MarkInfo << /Marked true
    /// /Suspects false >>` as PDF/UA asks, and old poppler (Catalog::getMarkInfo)
    /// takes any `/Suspects` that is not `true` for the wrong type. Valid PDF,
    /// shown correctly, fixed in newer poppler (26.07 checked) – not a reason to
    /// switch tagging off.
    pub fn pdf(&self, document: &PagedDocument) -> Result<Vec<u8>, PrintError> {
        let pdf = typst_pdf::pdf(document, &typst_pdf::PdfOptions::default())
            .map_err(|errors| PrintError::Pdf(self.describe(&errors)));
        release_cache();
        crate::stage_done(crate::Stage::Pdf);
        pdf
    }

    fn describe(&self, diagnostics: &[SourceDiagnostic]) -> String {
        diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| {
                let mut text = d.message.to_string();
                if let Some(id) = d.span.id() {
                    if let (Ok(source), Some(range)) = (self.source(id), self.range(d.span)) {
                        let line = source.text()[..range.start.min(source.text().len())]
                            .matches('\n')
                            .count()
                            + 1;
                        text.push_str(&format!(" ({:?}, Zeile {line})", id.vpath()));
                    }
                }
                for hint in &d.hints {
                    text.push_str(&format!("; {}", hint.v));
                }
                text
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl World for PrintWorld {
    fn library(&self) -> &LazyHash<Library> {
        &SHARED.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &SHARED.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else if let Some(source) = self.library.iter().find(|source| source.id() == id) {
            Ok(source.clone())
        } else if self.files.contains_key(&id) {
            Err(FileError::NotSource)
        } else {
            Err(not_found(id))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main.id() {
            return Ok(Bytes::from_string(self.main.clone()));
        }
        if let Some(source) = self.library.iter().find(|source| source.id() == id) {
            return Ok(Bytes::from_string(source.clone()));
        }
        self.files.get(&id).cloned().ok_or_else(|| not_found(id))
    }

    fn font(&self, index: usize) -> Option<Font> {
        SHARED.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Typst keeps every intermediate result of a layout in a global cache for
/// the next compile; proAVA prints rarely and big, so the cache goes after
/// the PDF (else a long-running program or MCP server keeps the layout of
/// every printout: several GB after a few big LVs). Freeing takes about 0.1 s
/// per 1000 positions – on a helper thread, the printout does not wait. The
/// cache is behind locks: a layout running meanwhile only finds fewer cached
/// results.
pub(crate) fn release_cache() {
    std::thread::spawn(|| typst::comemo::evict(0));
}

/// `typst::compile` for paged output with less memory, otherwise the same
/// steps (Typst 0.15.1, `compile_impl`): evaluate the main file once, lay out
/// until the introspections stabilize, at most `MAX_ITERS` times, then promote
/// delayed errors. Two differences: of an earlier pass only the introspector of
/// the last one is kept, not the pages (Typst keeps all documents to explain a
/// missing convergence), and between the passes the cache drops what the last
/// pass did not use. A big LV with Übertrag needs
/// four or five passes – kept in full, they were most of the memory.
fn lean_compile(
    world: &dyn World,
) -> typst::diag::Warned<typst::diag::SourceResult<PagedDocument>> {
    let mut sink = typst::engine::Sink::new();
    let output = lean_compile_impl(world, &mut sink);
    typst::diag::Warned {
        output,
        warnings: sink.warnings(),
    }
}

fn lean_compile_impl(
    world: &dyn World,
    sink: &mut typst::engine::Sink,
) -> typst::diag::SourceResult<PagedDocument> {
    use typst::comemo::{Constraint, Track};
    use typst::engine::{Engine, Route, Sink, Traced};
    use typst::foundations::{Output, StyleChain, Target, TargetElem};
    use typst::introspection::{EmptyIntrospector, Introspector, MAX_ITERS};
    use typst::utils::Protected;

    let world = world.track();
    let traced = Traced::default();
    let traced = traced.track();
    let library = world.library();
    let base = StyleChain::new(&library.styles);
    let target = TargetElem::target.set(Target::Paged).wrap();
    let styles = base.chain(&target);
    let main = world.source(world.main()).map_err(|error| {
        typst::ecow::eco_vec![SourceDiagnostic::error(
            typst::syntax::Span::detached(),
            typst::ecow::EcoString::from(error)
        )]
    })?;
    let content = typst_eval::eval(
        world,
        library,
        traced,
        sink.track_mut(),
        Route::default().track(),
        &main,
    )?
    .content();

    let empty = EmptyIntrospector;
    // Only the introspector of the previous pass, not its pages.
    let mut previous: Option<std::sync::Arc<typst_layout::PagedIntrospector>> = None;
    let mut pass = 1;
    let document = loop {
        let constraint = Constraint::new();
        let mut subsink = Sink::new();
        let document = {
            let introspector: &dyn Introspector = match &previous {
                Some(introspector) => &**introspector,
                None => &empty,
            };
            let mut engine = Engine {
                library,
                world,
                introspector: Protected::new(introspector.track_with(&constraint)),
                traced,
                sink: subsink.track_mut(),
                route: Route::default(),
            };
            PagedDocument::create(&mut engine, &content, styles)?
        };
        let stable = constraint.validate(Output::introspector(&document));
        if stable || pass == MAX_ITERS {
            sink.extend_from_sink(subsink);
            if !stable {
                sink.warn(typst::diag::SourceDiagnostic::warning(
                    typst::syntax::Span::detached(),
                    "document did not converge within five attempts",
                ));
            }
            break document;
        }
        // The pages of this pass go, its introspector stays for the next one;
        // cached results this pass did not use go too.
        previous = Some(document.introspector().clone());
        drop(document);
        typst::comemo::evict(1);
        pass += 1;
    };

    // Promote delayed errors.
    let delayed = sink.delayed();
    if !delayed.is_empty() {
        return Err(delayed);
    }
    Ok(document)
}

fn not_found(id: FileId) -> FileError {
    FileError::NotFound(id.vpath().get_without_slash().into())
}
