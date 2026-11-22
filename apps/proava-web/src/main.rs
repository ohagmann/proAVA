//! proava-web – das eingebaute Browserfenster von proAVA.
//!
//! Shows AUSSCHREIBEN.DE (or STLB-Bau Online) as it is, in a window with an
//! own toolbar (Zurück, Vor, Neu laden, Startseite, Adresse, „Im
//! System-Browser öffnen“, Ziel im LV). The pages run in the system's browser
//! engine (wry: WebKitGTK on Linux, WebView2 on Windows) with a profile of
//! their own, so cookies and the Merkliste stay between sessions.
//!
//! What it does for proAVA: downloads are caught – GAEB files and zips go to
//! a folder of their own and are reported on stdout ([`FromHelper`]), so the
//! take-over dialog reads them at once. proAVA steers the window over stdin
//! ([`ToHelper`]). The end of stdin closes the window (unless started with
//! `--allein`, by the MCP server), so it never outlives proAVA.
//!
//! What it deliberately does not do (`docs/FACHLICHES.md`): no script in
//! the pages (the toolbar is a page of its own), no calls of the site's
//! internal interface, no automatic clicks, searches or form entries, nothing
//! hidden. Links to other sites the user clicks go to the system browser.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::borrow::Cow;
use std::fs::File;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use proava_web_protocol::{
    decode, default_downloads, default_profile, download_path, encode, is_catalog_file, navigation,
    navigation_from, window_url, FromHelper, Launch, Navigation, Request, Site, SystemOpens,
    ToHelper,
};
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{Window, WindowBuilder};
use wry::http::{header::CONTENT_TYPE, Response};
use wry::{NewWindowResponse, PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder};

/// Height of the own toolbar (logical pixels).
const TOOLBAR_HEIGHT: f64 = 44.0;
/// The toolbar page (own page, served by the window itself).
const TOOLBAR_HTML: &str = include_str!("leiste.html");
/// GAEB files of earlier sessions in the own temp folder are removed after this.
const KEEP_DOWNLOADS: Duration = Duration::from_secs(3 * 24 * 3600);
/// After opening or „Raise“ the window asks for the keyboard focus this long
/// (as proAVA's own windows, `window_focus.rs`: Cinnamon keeps the focus in
/// the old window when a new one asks only once).
const ASK_FOCUS: Duration = Duration::from_millis(1500);

#[cfg(target_os = "linux")]
const TOOLBAR_URL: &str = "proava://localhost/";
#[cfg(not(target_os = "linux"))]
const TOOLBAR_URL: &str = "http://proava.localhost/";

/// What reaches the event loop from other threads and the browser engine.
#[derive(Debug)]
enum UserEvent {
    Command(ToHelper),
    InputEnded,
    Toolbar(String),
    /// A new window of the site: load it in this one.
    Load(String),
    OpenSystem(String),
    Title(String),
    Loaded(String),
    /// The address changed without a load (the site switches pages itself;
    /// only WebKitGTK reports it, WebView2 shows the address after loading).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Address(String),
    Downloaded {
        url: String,
        file: Option<PathBuf>,
    },
}

/// Everything the toolbar shows.
#[derive(Default)]
struct ToolbarState {
    url: String,
    target: String,
    status: String,
}

/// Writes one message to proAVA; a closed output is no error (alone,
/// started without proAVA).
fn send(message: &FromHelper) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", encode(message));
    let _ = out.flush();
}

/// Navigation log on stderr, only with `PROAVA_WEB_PROTOKOLL=1` (finding
/// out where a page goes, e.g. a sign-in; addresses can carry one-time
/// codes, so it is off unless asked for).
fn trace(line: impl FnOnce() -> String) {
    if std::env::var_os("PROAVA_WEB_PROTOKOLL").is_some() {
        eprintln!("[proava-web] {}", line());
    }
}

/// A folder only the user can read (`proava_web_protocol::make_private`);
/// an error is only reported, the folder is used anyway.
fn private_folder(folder: &Path) {
    if let Err(error) = proava_web_protocol::make_private(folder) {
        eprintln!("[proava-web] Rechte von {}: {error}", folder.display());
    }
}

fn fail(message: String) -> ! {
    eprintln!("[proava-web] {message}");
    send(&FromHelper::Error { message });
    std::process::exit(2);
}

fn data_folder() -> PathBuf {
    directories::ProjectDirs::from_path(PathBuf::from("proava"))
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("proava-daten"))
}

fn user_downloads() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(Path::to_path_buf))
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().join("Downloads")))
        .unwrap_or_else(std::env::temp_dir)
}

/// Removes GAEB files and zips older than [`KEEP_DOWNLOADS`] from the own
/// temp folder (never from the user's Downloads folder).
fn tidy(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > KEEP_DOWNLOADS);
        if old && path.is_file() && is_catalog_file(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// One window per site and profile: a second start reports an error and ends.
fn lock_profile(profile: &Path, site: Site) -> File {
    // The operating system holds the lock; a lock file left by an ended
    // window does not count (proava_web_protocol::lock_profile).
    match proava_web_protocol::lock_profile(profile) {
        Ok(Some(file)) => file,
        Ok(None) => fail(format!(
            "Ein Fenster für {} ist schon offen – bitte dort weiterarbeiten.",
            site.name()
        )),
        Err(error) => fail(format!("Profil nicht nutzbar: {error}")),
    }
}

/// Opens an address with the system (browser, mail program). Only
/// http(s) and mailto/tel get here (see [`navigation`]). With
/// `PROAVA_WEB_OHNE_SYSTEMBROWSER=1` (test screens) it is only logged – a
/// system browser started there would open in the user's running browser.
fn open_system(url: &str) {
    if std::env::var_os("PROAVA_WEB_OHNE_SYSTEMBROWSER").is_some() {
        eprintln!("[proava-web] System-Browser (abgeschaltet): {url}");
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let wide = |text: &str| {
            std::ffi::OsStr::new(text)
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<u16>>()
        };
        let verb = wide("open");
        let target = wide(url);
        // SAFETY: both strings are NUL-terminated UTF-16 and live during the call.
        unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        match std::process::Command::new(program).arg(url).spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(error) => eprintln!("[proava-web] {program} {url}: {error}"),
        }
    }
}

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let started_by_hand = arguments.is_empty();
    let mut launch = Launch::parse(arguments).unwrap_or_else(|error| fail(error));
    // Started by hand (double click): there is no proAVA to wait for.
    launch.standalone |= started_by_hand;
    let site = launch.site;

    let profile = launch
        .profile
        .clone()
        .unwrap_or_else(|| default_profile(&data_folder(), site));
    let own_downloads = launch.downloads.is_none() && !launch.standalone;
    let downloads = match &launch.downloads {
        Some(folder) => folder.clone(),
        None if launch.standalone => user_downloads(),
        None => default_downloads(&std::env::temp_dir(), site),
    };
    // Profile of the toolbar page, next to the site's profile.
    let toolbar_profile = profile.with_file_name(format!("{}-leiste", site.key()));
    for folder in [&profile, &toolbar_profile, &downloads] {
        if let Err(error) = std::fs::create_dir_all(folder) {
            fail(format!(
                "Ordner {} nicht anlegbar: {error}",
                folder.display()
            ));
        }
    }
    // Cookies and stored data of the session: only for the user.
    private_folder(&profile);
    private_folder(&toolbar_profile);
    if launch.profile.is_none() {
        if let Some(parent) = profile.parent() {
            private_folder(parent);
        }
    }
    let _lock = lock_profile(&profile, site);
    if own_downloads {
        tidy(&downloads);
    }
    // Other files (data sheets, PDF) go where a browser puts them.
    let other_downloads = if launch.standalone {
        downloads.clone()
    } else {
        user_downloads()
    };

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    read_input(proxy.clone());

    let window = WindowBuilder::new()
        .with_title(format!("{} – proAVA", site.name()))
        .with_inner_size(LogicalSize::new(1280.0, 880.0))
        .with_min_inner_size(LogicalSize::new(640.0, 420.0))
        .build(&event_loop)
        .unwrap_or_else(|error| fail(format!("Fenster lässt sich nicht öffnen: {error}")));
    #[cfg(target_os = "linux")]
    set_icon(&window);

    // Linux: toolbar and page one above the other in the window's own box –
    // GTK lays them out and resizes them (two web views in a `gtk::Fixed`
    // took each other's clicks). Windows: child windows with bounds.
    #[cfg(target_os = "linux")]
    let container = {
        use tao::platform::unix::WindowExtUnix;
        window
            .default_vbox()
            .unwrap_or_else(|| fail("Fensterbereich fehlt".into()))
            .clone()
    };
    let build = |builder: WebViewBuilder| -> wry::Result<WebView> {
        #[cfg(target_os = "linux")]
        {
            use wry::WebViewBuilderExtUnix;
            builder.build_gtk(&container)
        }
        #[cfg(not(target_os = "linux"))]
        {
            builder.build_as_child(&window)
        }
    };

    let (toolbar_rect, page_rect) = layout(&window);
    let mut page_context = WebContext::new(Some(profile.clone()));
    // The page the window shows now (for the same-host rule of
    // `navigation_from`), updated when a page is committed or loaded.
    let current = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let page = {
        let builder = WebViewBuilder::new_with_web_context(&mut page_context)
            .with_bounds(page_rect)
            .with_url(launch.start_url())
            .with_focused(true)
            .with_hotkeys_zoom(true)
            .with_new_window_req_handler({
                let proxy = proxy.clone();
                let current = current.clone();
                // A new window that stays loads in this view: same profile,
                // same session (sign-in in a pop-up keeps its cookies).
                move |url, _features| {
                    let decided =
                        navigation_from(site, &url, Request::NewWindow, &current.borrow());
                    trace(|| format!("neues Fenster {url} → {decided:?}"));
                    match decided {
                        Navigation::Stay => {
                            let _ = proxy.send_event(UserEvent::Load(url));
                        }
                        Navigation::System => {
                            let _ = proxy.send_event(UserEvent::OpenSystem(url));
                        }
                        Navigation::Refuse => {}
                    }
                    NewWindowResponse::Deny
                }
            })
            .with_download_started_handler({
                let downloads = downloads.clone();
                move |_url, path| {
                    let suggested = path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let folder = if is_catalog_file(Path::new(&suggested)) {
                        &downloads
                    } else {
                        &other_downloads
                    };
                    *path = download_path(folder, &suggested, Path::exists);
                    true
                }
            })
            .with_download_completed_handler({
                let proxy = proxy.clone();
                move |url, file, success| {
                    let _ = proxy.send_event(UserEvent::Downloaded {
                        url,
                        file: file.filter(|_| success),
                    });
                }
            })
            .with_document_title_changed_handler({
                let proxy = proxy.clone();
                move |title| {
                    let _ = proxy.send_event(UserEvent::Title(title));
                }
            })
            .with_on_page_load_handler({
                let proxy = proxy.clone();
                move |event, url| {
                    if matches!(event, PageLoadEvent::Finished) {
                        let _ = proxy.send_event(UserEvent::Loaded(url));
                    }
                }
            });
        // WebView2 only reports navigations of the page itself (not of its
        // frames) and does not say whether a click caused them: every one
        // counts as `Request::Other` – files and programs stay out, links to
        // other sites in a new tab go to the system browser (above).
        #[cfg(not(target_os = "linux"))]
        let builder = builder.with_navigation_handler({
            let current = current.clone();
            move |url| {
                navigation_from(site, &url, Request::Other, &current.borrow()) == Navigation::Stay
            }
        });
        build(builder).unwrap_or_else(|error| {
            fail(format!(
                "{} lässt sich nicht anzeigen: {error}",
                site.name()
            ))
        })
    };
    #[cfg(target_os = "linux")]
    watch_navigation(&page, site, proxy.clone(), current.clone());

    let mut toolbar_context = WebContext::new(Some(toolbar_profile.clone()));
    let toolbar = {
        let proxy = proxy.clone();
        build(
            WebViewBuilder::new_with_web_context(&mut toolbar_context)
                .with_bounds(toolbar_rect)
                .with_custom_protocol("proava".into(), |_id, _request| {
                    Response::builder()
                        .header(CONTENT_TYPE, "text/html; charset=utf-8")
                        .body(Cow::Borrowed(TOOLBAR_HTML.as_bytes()))
                        .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
                })
                .with_ipc_handler(move |request| {
                    let _ = proxy.send_event(UserEvent::Toolbar(request.body().clone()));
                })
                .with_navigation_handler(|url| url.starts_with(TOOLBAR_URL))
                .with_url(TOOLBAR_URL),
        )
        .unwrap_or_else(|error| fail(format!("Leiste lässt sich nicht anzeigen: {error}")))
    };
    #[cfg(target_os = "linux")]
    {
        // Built after the page, so its input area lies above the page's
        // only where it is drawn (the other way round, clicks into the page
        // went to the toolbar), then moved to the top with a fixed height.
        use gtk::prelude::*;
        use wry::WebViewExtUnix;
        let widget = toolbar.webview();
        widget.set_size_request(-1, TOOLBAR_HEIGHT as i32);
        container.set_child_packing(&widget, false, true, 0, gtk::PackType::Start);
        container.reorder_child(&widget, 0);
    }

    let mut state = ToolbarState {
        target: launch.target.clone().unwrap_or_default(),
        ..ToolbarState::default()
    };
    let mut system_opens = SystemOpens::default();
    let started = Instant::now();
    let standalone = launch.standalone;
    let mut title = String::new();
    let mut ask_focus_until: Option<Instant> = None;
    // The browser contexts live as long as the window.
    let contexts = (toolbar_context, page_context);
    send(&FromHelper::Ready);

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        let _ = &contexts;
        let mut refresh = false;
        match event {
            Event::NewEvents(StartCause::Init) => {
                ask_focus_until = Some(Instant::now() + ASK_FOCUS)
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(true),
                ..
            } => ask_focus_until = None,
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                // Linux: the box resizes both itself.
                if cfg!(not(target_os = "linux")) {
                    let (toolbar_rect, page_rect) = layout(&window);
                    let _ = toolbar.set_bounds(toolbar_rect);
                    let _ = page.set_bounds(page_rect);
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                send(&FromHelper::Closed);
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(event) => match event {
                UserEvent::Command(ToHelper::Open { url }) => match window_url(site, &url) {
                    Ok(url) => {
                        let _ = page.load_url(&url);
                        raise(&window);
                        ask_focus_until = Some(Instant::now() + ASK_FOCUS);
                    }
                    Err(error) => send(&FromHelper::Error { message: error }),
                },
                UserEvent::Command(ToHelper::Target { text }) => {
                    state.target = text;
                    refresh = true;
                }
                UserEvent::Command(ToHelper::Status { text }) => {
                    state.status = text;
                    refresh = true;
                }
                UserEvent::Command(ToHelper::Raise) => {
                    raise(&window);
                    ask_focus_until = Some(Instant::now() + ASK_FOCUS);
                }
                UserEvent::Command(ToHelper::Close) => *control_flow = ControlFlow::Exit,
                UserEvent::InputEnded => {
                    if !standalone {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                UserEvent::Toolbar(button) => {
                    match button.as_str() {
                        "back" => {
                            let _ = page.go_back();
                        }
                        "forward" => {
                            let _ = page.go_forward();
                        }
                        "reload" => {
                            let _ = page.reload();
                        }
                        "home" => {
                            let _ = page.load_url(site.start_url());
                        }
                        // „Abmelden“ (asked in the toolbar page): cookies,
                        // storage and cache of this site's profile go.
                        "logout" => {
                            let _ = page.clear_all_browsing_data();
                            let _ = page.load_url(site.start_url());
                            state.status = format!(
                                "Abgemeldet: Anmeldung, Cookies und gespeicherte Daten von {} gelöscht.",
                                site.name()
                            );
                        }
                        "system" => {
                            if let Ok(url) = page.url() {
                                if navigation(site, &url, Request::UserLink) != Navigation::Refuse
                                    && (url.starts_with("https://") || url.starts_with("http://"))
                                {
                                    open_system(&url);
                                }
                            }
                        }
                        _ => {}
                    }
                    refresh = true;
                }
                UserEvent::Load(url) => {
                    let _ = page.load_url(&url);
                }
                UserEvent::OpenSystem(url) => {
                    if system_opens.allow(started.elapsed().as_secs_f64()) {
                        open_system(&url);
                        state.status = format!(
                            "Im System-Browser geöffnet: {}",
                            proava_web_protocol::host(&url).unwrap_or_else(|| url.clone())
                        );
                        refresh = true;
                    }
                }
                UserEvent::Title(text) => {
                    title = text;
                    window.set_title(&window_title(site, &title));
                }
                UserEvent::Loaded(url) => {
                    *current.borrow_mut() = url.clone();
                    state.url = url.clone();
                    send(&FromHelper::Page {
                        url,
                        title: title.clone(),
                    });
                    refresh = true;
                }
                UserEvent::Address(url) => {
                    state.url = url;
                    refresh = true;
                }
                UserEvent::Downloaded { url, file } => {
                    match file {
                        Some(file) => {
                            let name = file
                                .file_name()
                                .map(|name| name.to_string_lossy().to_string())
                                .unwrap_or_default();
                            if is_catalog_file(&file) {
                                state.status = if standalone {
                                    format!("Gespeichert: {name} (in {})", downloads.display())
                                } else {
                                    format!("{name} geht an proAVA …")
                                };
                                send(&FromHelper::Downloaded { file, url });
                            } else {
                                let folder =
                                    file.parent().map(Path::to_path_buf).unwrap_or_default();
                                state.status =
                                    format!("Gespeichert: {name} (in {})", folder.display());
                            }
                        }
                        None => {
                            state.status = "Download abgebrochen.".into();
                            send(&FromHelper::DownloadFailed { url });
                        }
                    }
                    refresh = true;
                }
            },
            _ => {}
        }
        if refresh {
            show_state(&toolbar, &page, &state, standalone);
        }
        if let Some(until) = ask_focus_until {
            if Instant::now() < until && !matches!(*control_flow, ControlFlow::Exit) {
                window.set_focus();
                *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50));
            } else {
                ask_focus_until = None;
            }
        }
    });
}

/// „Seitentitel – AUSSCHREIBEN.DE – proAVA“, the site only once.
fn window_title(site: Site, page_title: &str) -> String {
    let page_title = page_title.trim();
    if page_title.is_empty() {
        format!("{} – proAVA", site.name())
    } else if page_title
        .to_lowercase()
        .contains(&site.name().to_lowercase())
    {
        format!("{page_title} – proAVA")
    } else {
        format!("{page_title} – {} – proAVA", site.name())
    }
}

/// Rectangles of toolbar and page in the window (logical pixels).
fn layout(window: &Window) -> (Rect, Rect) {
    let size = window.inner_size().to_logical::<f64>(window.scale_factor());
    let toolbar = Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: LogicalSize::new(size.width, TOOLBAR_HEIGHT).into(),
    };
    let page = Rect {
        position: LogicalPosition::new(0.0, TOOLBAR_HEIGHT).into(),
        size: LogicalSize::new(size.width, (size.height - TOOLBAR_HEIGHT).max(1.0)).into(),
    };
    (toolbar, page)
}

fn raise(window: &Window) {
    window.set_visible(true);
    window.set_minimized(false);
    window.set_focus();
}

/// Writes the state into the own toolbar page (never into the web site).
fn show_state(toolbar: &WebView, page: &WebView, state: &ToolbarState, standalone: bool) {
    let hint = if standalone {
        "Als GAEB exportieren – die Datei landet im Download-Ordner."
    } else {
        "Als GAEB exportieren – proAVA übernimmt die Datei."
    };
    let json = serde_json::json!({
        "url": state.url,
        "back": page.can_go_back().unwrap_or(false),
        "forward": page.can_go_forward().unwrap_or(false),
        "target": state.target,
        "hint": hint,
        "status": state.status,
    });
    let _ = toolbar.evaluate_script(&format!("window.proava && window.proava.state({json})"));
}

/// Reads [`ToHelper`] lines from proAVA on a thread of its own.
fn read_input(proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }
            match decode::<ToHelper>(&line) {
                Ok(command) => {
                    if proxy.send_event(UserEvent::Command(command)).is_err() {
                        return;
                    }
                }
                Err(error) => eprintln!("[proava-web] {error}"),
            }
        }
        let _ = proxy.send_event(UserEvent::InputEnded);
    });
}

/// Linux: the navigation rule on WebKit's own decision, which knows whether
/// the user clicked a link (wry's handler does not, and also sees the frames
/// inside a page). A clicked link to another site goes to the system browser;
/// redirects, sign-in, forms, frames and scripts stay as in a browser; files
/// and programs never load. A GAEB file the page would show as text is
/// downloaded instead.
#[cfg(target_os = "linux")]
fn watch_navigation(
    page: &WebView,
    site: Site,
    proxy: EventLoopProxy<UserEvent>,
    current: std::rc::Rc<std::cell::RefCell<String>>,
) {
    use gtk::glib::Cast;
    use webkit2gtk::{
        LoadEvent, NavigationPolicyDecision, NavigationPolicyDecisionExt, NavigationType,
        PolicyDecisionExt, PolicyDecisionType, ResponsePolicyDecision, ResponsePolicyDecisionExt,
        URIRequestExt, URIResponseExt, WebViewExt,
    };
    use wry::WebViewExtUnix;

    let view = page.webview();
    {
        let proxy = proxy.clone();
        view.connect_uri_notify(move |view| {
            if let Some(url) = view.uri() {
                trace(|| format!("Adresse {url}"));
                let _ = proxy.send_event(UserEvent::Address(url.to_string()));
            }
        });
    }
    {
        let current = current.clone();
        view.connect_load_changed(move |view, event| {
            if event == LoadEvent::Committed {
                if let Some(url) = view.uri() {
                    *current.borrow_mut() = url.to_string();
                }
            }
        });
    }
    view.connect_decide_policy(move |_view, decision, kind| match kind {
        PolicyDecisionType::NavigationAction => {
            let Some(action) = decision
                .downcast_ref::<NavigationPolicyDecision>()
                .and_then(|decision| decision.navigation_action())
            else {
                return false;
            };
            let Some(url) = action.request().and_then(|request| request.uri()) else {
                return false;
            };
            let request = if action.navigation_type() == NavigationType::LinkClicked
                && action.is_user_gesture()
            {
                Request::UserLink
            } else {
                Request::Other
            };
            let decided = navigation_from(site, &url, request, &current.borrow());
            trace(|| {
                format!(
                    "Navigation {:?} Geste={} Weiterleitung={} {url} → {decided:?}",
                    action.navigation_type(),
                    action.is_user_gesture(),
                    action.clone().is_redirect()
                )
            });
            match decided {
                Navigation::Stay => false,
                Navigation::System => {
                    decision.ignore();
                    let _ = proxy.send_event(UserEvent::OpenSystem(url.to_string()));
                    true
                }
                Navigation::Refuse => {
                    decision.ignore();
                    true
                }
            }
        }
        PolicyDecisionType::Response => {
            let Some(response) = decision.downcast_ref::<ResponsePolicyDecision>() else {
                return false;
            };
            let name = response
                .response()
                .and_then(|response| {
                    response
                        .suggested_filename()
                        .filter(|name| !name.is_empty())
                        .or_else(|| response.uri())
                })
                .map(|name| name.to_string())
                .unwrap_or_default();
            let name = name.split(['?', '#']).next().unwrap_or_default();
            if is_catalog_file(Path::new(name)) {
                decision.download();
                return true;
            }
            false
        }
        _ => false,
    });
}

/// Linux: the proAVA symbol in the window list.
#[cfg(target_os = "linux")]
fn set_icon(window: &Window) {
    use gtk::gdk_pixbuf::PixbufLoader;
    use gtk::prelude::*;
    use tao::platform::unix::WindowExtUnix;
    const ICON: &[u8] = include_bytes!("../../../assets/icons/proava-64.png");
    let loader = PixbufLoader::new();
    if loader.write(ICON).is_ok() && loader.close().is_ok() {
        if let Some(pixbuf) = loader.pixbuf() {
            window.gtk_window().set_icon(Some(&pixbuf));
        }
    }
}
