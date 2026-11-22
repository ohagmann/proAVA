//! What proAVA and its helper program `proava-web` agree on.
//!
//! The helper shows AUSSCHREIBEN.DE (or STLB-Bau Online) in an embedded
//! browser window (wry: WebKitGTK on Linux, WebView2 on Windows). proAVA
//! itself stays pure Rust without a browser engine: it only starts the
//! helper and talks to it over stdin/stdout, one JSON object per line
//! ([`ToHelper`], [`FromHelper`]). Without the helper the system browser and
//! the Downloads folder do the same job (`docs/FACHLICHES.md`).
//!
//! Everything here is pure – no window, no process, no network – so it is
//! tested in the main workspace. The helper is a Cargo workspace of its own
//! (`apps/proava-web`) and links this crate by path; that keeps wry and the
//! browser engine out of the dependency tree of `proava` and `proava-mcp`.
//!
//! What the window deliberately does not do: no scripts in the pages, no
//! calls of the site's internal interface, no automatic clicks, searches or
//! form entries. The pages are shown as they are; the window only opens an
//! address, keeps its own toolbar, catches downloads and hands links to other
//! sites to the system browser.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// File name of the helper program (Windows: with `.exe`).
pub const HELPER_NAME: &str = "proava-web";

/// Longest message line that is read (a URL, a path, a status text).
pub const MAX_LINE: usize = 64 * 1024;

/// The web site shown in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Site {
    /// Manufacturer texts (free portal, no sign-in needed for texts).
    Ausschreiben,
    /// STLB-Bau Online (DIN Bauportal); the user signs in on the page himself.
    StlbBau,
}

impl Site {
    pub const ALL: [Site; 2] = [Site::Ausschreiben, Site::StlbBau];

    /// Key on the command line and name of the profile folder.
    pub fn key(self) -> &'static str {
        match self {
            Site::Ausschreiben => "ausschreiben",
            Site::StlbBau => "stlb-bau",
        }
    }

    pub fn from_key(key: &str) -> Option<Site> {
        Site::ALL.into_iter().find(|site| site.key() == key.trim())
    }

    /// Name in the window title and in messages.
    pub fn name(self) -> &'static str {
        match self {
            Site::Ausschreiben => "AUSSCHREIBEN.DE",
            Site::StlbBau => "STLB-Bau Online",
        }
    }

    /// Start page (button „Startseite“).
    pub fn start_url(self) -> &'static str {
        match self {
            Site::Ausschreiben => "https://ausschreiben.de",
            Site::StlbBau => "https://www.stlb-bau-online.de",
        }
    }

    /// Domains whose pages stay in the window, with their subdomains
    /// (`info.ausschreiben.de`).
    pub fn home_domains(self) -> &'static [&'static str] {
        match self {
            Site::Ausschreiben => &["ausschreiben.de"],
            Site::StlbBau => &["stlb-bau-online.de"],
        }
    }

    /// Hosts of the provider's sign-in service, exact names (no subdomains,
    /// https only): its pages stay in the window, so the session cookies land
    /// in the window's profile and not in the system browser. Kept narrow on
    /// purpose (`docs/FACHLICHES.md`).
    ///
    /// - STLB-Bau Online: „Login“ goes by two redirects on the site to the
    ///   OpenID-Connect service `auth.din-bauportal.de` and back to
    ///   `www.stlb-bau-online.de/signin-oidc`.
    /// - AUSSCHREIBEN.DE: the site itself sends the browser (by script, not by
    ///   a click) to the sign-in service of the operator; that stays anyway,
    ///   and links on its pages stay by the same-host rule of
    ///   [`navigation_from`].
    pub fn sign_in_hosts(self) -> &'static [&'static str] {
        match self {
            Site::Ausschreiben => &[],
            Site::StlbBau => &["auth.din-bauportal.de"],
        }
    }

    /// Whether `url` is a web page of the site itself (https or http).
    pub fn is_home(self, url: &str) -> bool {
        matches!(kind(self, url), Kind::Home)
    }
}

/// How a navigation came about – browsers report it differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// The user clicked a link in the page (WebKitGTK: link clicked with a
    /// user gesture).
    UserLink,
    /// The page asks for a new window or tab (`target="_blank"`,
    /// `window.open`).
    NewWindow,
    /// Anything else: redirects, sign-in steps, sent forms, frames inside the
    /// page, scripts – and under Windows every navigation of the page
    /// (WebView2 does not say whether a click caused it).
    Other,
}

/// What the window does with a navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    /// Load it in the window (for [`Request::NewWindow`]: in the same view).
    Stay,
    /// Do not load it here, open it in the system browser (or mail program).
    System,
    /// Do not load it at all (files, programs, unknown schemes).
    Refuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// http(s) page of the site.
    Home,
    /// https page of the provider's sign-in service ([`Site::sign_in_hosts`]).
    SignIn,
    /// http(s) page of another site (manufacturer pages, data sheets …).
    Web,
    /// Content of the page itself: `about:blank`, `blob:` of the site, `data:`.
    Inline,
    /// `mailto:`, `tel:` – for the mail program.
    Mail,
    /// Everything else (`file:`, `javascript:`, program schemes …).
    Other,
}

fn scheme(url: &str) -> Option<String> {
    let (scheme, _) = url.split_once(':')?;
    let valid = !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then(|| scheme.to_ascii_lowercase())
}

/// Host of an `http(s)://` address in lower case, without user part and port.
pub fn host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#', '\\']).next().unwrap_or_default();
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    let host = if let Some(inner) = host_port.strip_prefix('[') {
        inner.split(']').next().unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

fn on_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|front| front.ends_with('.'))
}

fn kind(site: Site, url: &str) -> Kind {
    let url = url.trim();
    if url.chars().any(|c| c.is_control() || c == ' ') {
        return Kind::Other;
    }
    match scheme(url).as_deref() {
        Some(scheme @ ("http" | "https")) => match host(url) {
            Some(host) if site.home_domains().iter().any(|d| on_domain(&host, d)) => Kind::Home,
            Some(host) if scheme == "https" && site.sign_in_hosts().contains(&host.as_str()) => {
                Kind::SignIn
            }
            Some(_) => Kind::Web,
            None => Kind::Other,
        },
        Some("about") => {
            let page = url[6..].to_ascii_lowercase();
            if page == "blank" || page == "srcdoc" {
                Kind::Inline
            } else {
                Kind::Other
            }
        }
        // blob:https://ausschreiben.de/… – only blobs of the site itself.
        Some("blob") => match kind(site, &url[5..]) {
            Kind::Home => Kind::Inline,
            _ => Kind::Other,
        },
        Some("data") => Kind::Inline,
        Some("mailto" | "tel") => Kind::Mail,
        _ => Kind::Other,
    }
}

/// The navigation rule of the window: pages of the site stay, a click on a
/// link to another site goes to the system browser, files and programs are
/// never loaded. Redirects, forms, frames and scripts keep the page as a
/// normal browser would (sign-in, cookie notices, embedded videos stay as
/// the site made them).
pub fn navigation(site: Site, url: &str, request: Request) -> Navigation {
    navigation_from(site, url, request, "")
}

/// [`navigation`] knowing the page the window shows now (`current`, empty =
/// none): pages of the site and of its sign-in service stay (also the
/// redirect that follows a click on „Login“, which WebKit reports as a
/// click); a click or a new window to another site goes to the system
/// browser – unless it stays on the host the window shows already (e.g.
/// „Kennwort vergessen?“ on the sign-in page the site sent the window to).
/// New windows that stay load in the same view, so they share the session.
pub fn navigation_from(site: Site, url: &str, request: Request, current: &str) -> Navigation {
    let target = kind(site, url);
    let same_host = || {
        target == Kind::Web
            && scheme(url).as_deref() == Some("https")
            && scheme(current).as_deref() == Some("https")
            && host(url).is_some()
            && host(url) == host(current)
    };
    match (request, target) {
        (_, Kind::Other) => Navigation::Refuse,
        (_, Kind::Home | Kind::SignIn) => Navigation::Stay,
        (Request::UserLink | Request::NewWindow, Kind::Inline) => Navigation::Stay,
        (Request::UserLink | Request::NewWindow, Kind::Web) if same_host() => Navigation::Stay,
        (Request::UserLink | Request::NewWindow, Kind::Web | Kind::Mail) => Navigation::System,
        (Request::Other, Kind::Web | Kind::Inline) => Navigation::Stay,
        // A page must not start the mail program on its own.
        (Request::Other, Kind::Mail) => Navigation::Refuse,
    }
}

/// An address proAVA or an assistant may open in the window: an https
/// page of the site, nothing else.
pub fn window_url(site: Site, url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(site.start_url().to_string());
    }
    if scheme(url).as_deref() == Some("https") && site.is_home(url) {
        Ok(url.to_string())
    } else {
        Err(format!(
            "Nur Seiten von {} ({}) lassen sich im Fenster öffnen: {url}",
            site.name(),
            site.home_domains().join(", ")
        ))
    }
}

/// Brake for pages that try to open many system browser windows: at most
/// [`SystemOpens::MAX`] per window and one per [`SystemOpens::GAP_SECONDS`].
#[derive(Debug, Clone, Default)]
pub struct SystemOpens {
    last: Option<f64>,
    count: usize,
}

impl SystemOpens {
    pub const MAX: usize = 30;
    pub const GAP_SECONDS: f64 = 1.0;

    /// Whether an open at `now` (seconds since any fixed start) may happen.
    pub fn allow(&mut self, now: f64) -> bool {
        if self.count >= Self::MAX {
            return false;
        }
        if self
            .last
            .is_some_and(|last| now - last < Self::GAP_SECONDS && now >= last)
        {
            return false;
        }
        self.last = Some(now);
        self.count += 1;
        true
    }
}

/// GAEB files and zip archives (as in proAVA's take-over dialog).
pub const CATALOG_EXTENSIONS: [&str; 11] = [
    "x81", "x82", "x83", "x84", "x86", "d81", "d82", "d83", "d84", "d86", "zip",
];

/// Whether proAVA can take the file over (GAEB or zip).
pub fn is_catalog_file(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| CATALOG_EXTENSIONS.contains(&extension.as_str()))
}

/// A file name the server suggested, made safe for every system: only the
/// last part of a path, no control characters, no characters Windows
/// forbids, no reserved device names, not empty, at most 150 characters
/// (the extension is kept).
pub fn safe_file_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name: String = last
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c => c,
        })
        .collect();
    name = name
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    if name.is_empty() {
        name = "Download".to_string();
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem[3..].chars().all(|c| c.is_ascii_digit()));
    if reserved {
        name.insert(0, '_');
    }
    const MAX_CHARS: usize = 150;
    if name.chars().count() > MAX_CHARS {
        let (stem, extension) = match name.rsplit_once('.') {
            Some((stem, extension)) if extension.chars().count() <= 10 => {
                (stem.to_string(), format!(".{extension}"))
            }
            _ => (name.clone(), String::new()),
        };
        let keep = MAX_CHARS - extension.chars().count();
        name = stem.chars().take(keep).collect::<String>() + &extension;
    }
    name
}

/// Where a download goes: `folder` / safe name, „Name (2).X81“ … when the
/// name is taken (`exists` asks the disk; tests hand in their own).
pub fn download_path(folder: &Path, suggested: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let name = safe_file_name(suggested);
    let first = folder.join(&name);
    if !exists(&first) {
        return first;
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_string(), format!(".{extension}")),
        _ => (name.clone(), String::new()),
    };
    (2..10_000)
        .map(|number| folder.join(format!("{stem} ({number}){extension}")))
        .find(|candidate| !exists(candidate))
        .unwrap_or(first)
}

/// proAVA → helper, one per line on the helper's stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToHelper {
    /// Load this address (only pages of the site, see [`window_url`]).
    Open { url: String },
    /// „Ziel im LV“ shown in the toolbar.
    Target { text: String },
    /// A short note in the toolbar, e.g. „2 Positionen übernommen“.
    Status { text: String },
    /// Bring the window to the front.
    Raise,
    /// Close the window and end.
    Close,
}

/// Helper → proAVA, one per line on the helper's stdout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromHelper {
    /// The window is open.
    Ready,
    /// A page was loaded (address and title, for the dialog).
    Page { url: String, title: String },
    /// A download was written completely.
    Downloaded { file: PathBuf, url: String },
    /// A download failed or was cancelled.
    DownloadFailed { url: String },
    /// The user closed the window; the helper ends.
    Closed,
    /// Something did not work (window, browser engine, profile in use …).
    Error { message: String },
}

/// One message as one line of JSON (without the line break).
pub fn encode<T: Serialize>(message: &T) -> String {
    // Serializing these plain enums cannot fail; a line break never occurs
    // in serde_json's compact output.
    serde_json::to_string(message).unwrap_or_default()
}

/// One line back into a message; too long or unknown lines are errors.
pub fn decode<T: DeserializeOwned>(line: &str) -> Result<T, String> {
    if line.len() > MAX_LINE {
        return Err(format!("Nachricht zu lang ({} Bytes)", line.len()));
    }
    serde_json::from_str(line.trim()).map_err(|error| format!("Nachricht unlesbar: {error}"))
}

/// How the helper is started (command line, see [`Launch::arguments`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub site: Site,
    /// First page; `None` or not allowed = the start page of the site.
    pub url: Option<String>,
    /// „Ziel im LV“ from the start.
    pub target: Option<String>,
    /// Folder for downloads; `None` = the helper's own temp folder (with
    /// proAVA) or the user's Downloads folder (alone).
    pub downloads: Option<PathBuf>,
    /// Profile (cookies, local storage, e.g. the Merkliste); `None` = the
    /// default under proAVA's data folder.
    pub profile: Option<PathBuf>,
    /// Started without proAVA (from the MCP server): keeps running when its
    /// input ends; otherwise the end of input closes the window, so it never
    /// outlives proAVA.
    pub standalone: bool,
}

impl Launch {
    pub fn new(site: Site) -> Self {
        Self {
            site,
            url: None,
            target: None,
            downloads: None,
            profile: None,
            standalone: false,
        }
    }

    /// Command-line arguments (after the program name).
    pub fn arguments(&self) -> Vec<OsString> {
        let mut arguments: Vec<OsString> = vec!["--seite".into(), self.site.key().into()];
        if let Some(url) = &self.url {
            arguments.extend(["--url".into(), url.into()]);
        }
        if let Some(target) = &self.target {
            arguments.extend(["--ziel".into(), target.into()]);
        }
        if let Some(downloads) = &self.downloads {
            arguments.extend(["--downloads".into(), downloads.into()]);
        }
        if let Some(profile) = &self.profile {
            arguments.extend(["--profil".into(), profile.into()]);
        }
        if self.standalone {
            arguments.push("--allein".into());
        }
        arguments
    }

    /// Reads the arguments of [`Launch::arguments`].
    pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut launch = Launch::new(Site::Ausschreiben);
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            let argument = argument.to_string_lossy().to_string();
            let mut value = |name: &str| {
                arguments
                    .next()
                    .ok_or_else(|| format!("{name} braucht einen Wert"))
            };
            match argument.as_str() {
                "--seite" => {
                    let key = value("--seite")?.to_string_lossy().to_string();
                    launch.site =
                        Site::from_key(&key).ok_or_else(|| format!("Unbekannte Seite: {key}"))?;
                }
                "--url" => launch.url = Some(value("--url")?.to_string_lossy().to_string()),
                "--ziel" => launch.target = Some(value("--ziel")?.to_string_lossy().to_string()),
                "--downloads" => launch.downloads = Some(PathBuf::from(value("--downloads")?)),
                "--profil" => launch.profile = Some(PathBuf::from(value("--profil")?)),
                "--allein" => launch.standalone = true,
                other => return Err(format!("Unbekannte Angabe: {other}")),
            }
        }
        Ok(launch)
    }

    /// The first page: the given address if it is a page of the site, else
    /// the start page.
    pub fn start_url(&self) -> String {
        self.url
            .as_deref()
            .and_then(|url| window_url(self.site, url).ok())
            .unwrap_or_else(|| self.site.start_url().to_string())
    }
}

/// The helper next to a proAVA program file (same folder, `.exe` when the
/// program has one).
pub fn helper_program(program: &Path) -> PathBuf {
    let windows = program
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
    let name = if windows {
        format!("{HELPER_NAME}.exe")
    } else {
        HELPER_NAME.to_string()
    };
    program
        .parent()
        .map(|folder| folder.join(&name))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Whether `program` is the helper (file name `proava-web`, `.exe` allowed).
pub fn is_helper_program(program: &Path) -> bool {
    program
        .file_stem()
        .is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case(HELPER_NAME))
}

/// Default profile folder below proAVA's local data folder.
pub fn default_profile(data_folder: &Path, site: Site) -> PathBuf {
    data_folder.join("webfenster").join(site.key())
}

/// Default download folder below the temp folder (with proAVA).
pub fn default_downloads(temp_folder: &Path, site: Site) -> PathBuf {
    temp_folder.join("proava-web").join(site.key())
}

/// Makes a folder of the window's profile private to the user (Unix 0700;
/// the profile holds the session cookies). An existing folder made by an
/// older version (0775) is tightened the same way and used as it is; if
/// that fails (folder of another user), the window still uses it and only
/// reports the error. Windows: no-op, the profile lies in the user's own
/// `%LOCALAPPDATA%`.
pub fn make_private(folder: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    {
        let _ = folder;
        Ok(())
    }
}

/// Name of the lock file in the profile folder.
pub const PROFILE_LOCK: &str = "proava-web.lock";

/// Locks the profile for one window. The lock is held by the operating
/// system (`flock`/`LockFileEx` through [`std::fs::File::try_lock`]) and ends
/// with the process: a lock file left behind by a crashed or killed window is
/// no obstacle, it is simply locked again. `Ok(None)`: a running window holds
/// the profile. A file system without locks (some network drives) does not
/// stop the window either – `Ok(Some(..))` without a lock.
pub fn lock_profile(profile: &Path) -> std::io::Result<Option<std::fs::File>> {
    let file = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(profile.join(PROFILE_LOCK))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(_)) => Ok(Some(file)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn messages_go_both_ways_one_line_each() {
        let to = [
            ToHelper::Open {
                url: "https://ausschreiben.de/catalog/x/catalogItem/a-1".into(),
            },
            ToHelper::Target {
                text: "einfügen hinter 01.0010 „Fühler“\nzweite Zeile".into(),
            },
            ToHelper::Status {
                text: "2 Positionen übernommen".into(),
            },
            ToHelper::Raise,
            ToHelper::Close,
        ];
        for message in &to {
            let line = encode(message);
            assert!(!line.contains('\n'), "eine Zeile: {line}");
            assert_eq!(&decode::<ToHelper>(&line).unwrap(), message);
        }
        let from = [
            FromHelper::Ready,
            FromHelper::Page {
                url: "https://ausschreiben.de".into(),
                title: "Start".into(),
            },
            FromHelper::Downloaded {
                file: PathBuf::from("/tmp/proava-web/ausschreiben/Belimo.X81"),
                url: "blob:https://ausschreiben.de/0e1f".into(),
            },
            FromHelper::DownloadFailed { url: "x".into() },
            FromHelper::Closed,
            FromHelper::Error {
                message: "Profil belegt".into(),
            },
        ];
        for message in &from {
            assert_eq!(&decode::<FromHelper>(&encode(message)).unwrap(), message);
        }
        assert_eq!(encode(&ToHelper::Raise), r#"{"type":"raise"}"#);
        assert_eq!(
            decode::<FromHelper>(r#"{"type":"downloaded","file":"/a/b.X83","url":"u"}"#).unwrap(),
            FromHelper::Downloaded {
                file: PathBuf::from("/a/b.X83"),
                url: "u".into()
            }
        );
        assert!(decode::<ToHelper>(r#"{"type":"klick","x":1}"#).is_err());
        assert!(decode::<ToHelper>("kein json").is_err());
        let long = format!(r#"{{"type":"open","url":"{}"}}"#, "a".repeat(MAX_LINE));
        assert!(decode::<ToHelper>(&long).unwrap_err().contains("zu lang"));
    }

    #[test]
    fn pages_of_the_site_stay_links_to_others_go_to_the_system_browser() {
        let site = Site::Ausschreiben;
        let nav = |url: &str, request| navigation(site, url, request);
        for url in [
            "https://ausschreiben.de",
            "https://ausschreiben.de/catalog/belimo/catalogItem/aaaa-1",
            "https://info.ausschreiben.de/partner/",
            "https://AUSSCHREIBEN.DE:443/x",
            "http://ausschreiben.de/",
            "about:blank",
            "blob:https://ausschreiben.de/0e1f-22",
        ] {
            assert_eq!(nav(url, Request::UserLink), Navigation::Stay, "{url}");
            assert_eq!(nav(url, Request::NewWindow), Navigation::Stay, "{url}");
            assert_eq!(nav(url, Request::Other), Navigation::Stay, "{url}");
        }
        for url in [
            "https://www.belimo.com/de/produkte",
            "https://ausschreiben.de.example.com/",
            "https://evilausschreiben.de/",
            "https://ausschreiben.de@example.com/",
            "mailto:info@example.com",
        ] {
            assert_eq!(nav(url, Request::UserLink), Navigation::System, "{url}");
            assert_eq!(nav(url, Request::NewWindow), Navigation::System, "{url}");
        }
        // Redirects, sign-in, frames and scripts: as in a normal browser …
        assert_eq!(
            nav("https://www.youtube-nocookie.com/embed/x", Request::Other),
            Navigation::Stay
        );
        // … but no mail program and no files from the page itself.
        assert_eq!(nav("mailto:a@b.de", Request::Other), Navigation::Refuse);
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ms-msdt:/id",
            "blob:https://example.com/0e1f",
            "about:config",
            "https://ausschreiben.de/a b",
            "https:///nur-pfad",
            "",
        ] {
            for request in [Request::UserLink, Request::NewWindow, Request::Other] {
                assert_eq!(nav(url, request), Navigation::Refuse, "{url} {request:?}");
            }
        }
        assert_eq!(
            navigation(
                Site::StlbBau,
                "https://www.stlb-bau-online.de/lv",
                Request::UserLink
            ),
            Navigation::Stay
        );
        assert_eq!(
            navigation(Site::StlbBau, "https://ausschreiben.de", Request::UserLink),
            Navigation::System
        );
    }

    /// STLB-Bau Online signs in over its OpenID-Connect service (observed
    /// 18.11.2026): the whole way stays in the window, so the session cookies
    /// end up in its profile; everything else of other sites goes out.
    #[test]
    fn the_sign_in_of_the_provider_stays_in_the_window() {
        let site = Site::StlbBau;
        let start = "https://www.stlb-bau-online.de/";
        let nav = |url: &str, request| navigation_from(site, url, request, start);
        // Click on „Login“, then the redirects (WebKit: link clicked, gesture).
        for url in [
            "https://www.stlb-bau-online.de/login",
            "https://www.stlb-bau-online.de/FederatedAccount/Login?returnUrl=%2F",
            "https://auth.din-bauportal.de/connect/authorize?client_id=x&redirect_uri=https%3A%2F%2Fwww.stlb-bau-online.de%2Fsignin-oidc&response_type=code",
            "https://auth.din-bauportal.de/Account/ForgotPassword",
        ] {
            assert_eq!(nav(url, Request::UserLink), Navigation::Stay, "{url}");
            assert_eq!(nav(url, Request::NewWindow), Navigation::Stay, "Popup gleiche Sitzung: {url}");
            assert_eq!(nav(url, Request::Other), Navigation::Stay, "{url}");
        }
        // Back to the site after signing in (form or redirect).
        assert_eq!(
            navigation_from(
                site,
                "https://www.stlb-bau-online.de/signin-oidc",
                Request::Other,
                "https://auth.din-bauportal.de/Account/Login"
            ),
            Navigation::Stay
        );
        // Narrow: exact host, https only, no look-alikes, no other hosts of
        // the provider (shop, help) – those go to the system browser.
        for url in [
            "http://auth.din-bauportal.de/connect/authorize",
            "https://auth.din-bauportal.de.example.com/",
            "https://evil-auth.din-bauportal.de/",
            "https://x.auth.din-bauportal.de/",
            "https://www.din-bauportal.de/shop",
            "https://example.com/hilfe",
        ] {
            assert_eq!(nav(url, Request::UserLink), Navigation::System, "{url}");
            assert_eq!(nav(url, Request::NewWindow), Navigation::System, "{url}");
        }
        assert!(Site::Ausschreiben.sign_in_hosts().is_empty());
        assert!(
            !site.is_home("https://auth.din-bauportal.de/"),
            "kein Startziel von außen"
        );
        assert!(window_url(site, "https://auth.din-bauportal.de/").is_err());
    }

    /// A sign-in service the site sends the window to by script stays, and so
    /// do the links on its pages to the same host („Kennwort vergessen?“);
    /// links from there to other sites go out. Host made up.
    #[test]
    fn links_on_the_page_shown_stay_on_its_host() {
        let site = Site::Ausschreiben;
        let sign_in = "https://anmeldung.example.net/signin/authorize?client_id=x";
        assert_eq!(
            navigation_from(site, sign_in, Request::Other, "https://ausschreiben.de/"),
            Navigation::Stay,
            "per Skript von der Seite"
        );
        let reset = "https://anmeldung.example.net/reset/authorize";
        assert_eq!(
            navigation_from(site, reset, Request::UserLink, sign_in),
            Navigation::Stay
        );
        assert_eq!(
            navigation_from(site, reset, Request::NewWindow, sign_in),
            Navigation::Stay
        );
        assert_eq!(
            navigation_from(
                site,
                "https://example.com/datenschutz",
                Request::UserLink,
                sign_in
            ),
            Navigation::System
        );
        // From a page of the site, the same click goes out (no list entry).
        assert_eq!(
            navigation_from(site, reset, Request::UserLink, "https://ausschreiben.de/"),
            Navigation::System
        );
        // Same host needs https on both sides.
        assert_eq!(
            navigation_from(
                site,
                "http://anmeldung.example.net/x",
                Request::UserLink,
                sign_in
            ),
            Navigation::System
        );
        assert_eq!(
            navigation_from(site, reset, Request::UserLink, ""),
            Navigation::System,
            "ohne bekannte Seite"
        );
    }

    #[test]
    fn only_https_pages_of_the_site_may_be_opened_from_outside() {
        assert_eq!(
            window_url(Site::Ausschreiben, "").unwrap(),
            "https://ausschreiben.de"
        );
        assert!(window_url(
            Site::Ausschreiben,
            "https://ausschreiben.de/catalog/belimo/catalogItem/aaaa-1"
        )
        .is_ok());
        for url in [
            "http://ausschreiben.de/",
            "https://example.com/",
            "file:///tmp/x.html",
            "blob:https://ausschreiben.de/x",
        ] {
            assert!(window_url(Site::Ausschreiben, url).is_err(), "{url}");
        }
        assert_eq!(
            host("https://user:pw@Host.DE:8080/x?y#z").unwrap(),
            "host.de"
        );
        assert_eq!(host("https://[::1]:443/").unwrap(), "::1");
        assert_eq!(host("https://a.de\\@b.de/").unwrap(), "a.de");
    }

    #[test]
    fn a_page_cannot_flood_the_system_browser() {
        let mut opens = SystemOpens::default();
        assert!(opens.allow(10.0));
        assert!(!opens.allow(10.5), "zu schnell");
        assert!(opens.allow(11.2));
        let mut allowed = 2;
        let mut now = 13.0;
        while opens.allow(now) {
            allowed += 1;
            now += 2.0;
        }
        assert_eq!(allowed, SystemOpens::MAX);
    }

    #[test]
    fn downloads_get_safe_unique_names() {
        assert_eq!(safe_file_name("Belimo LV.X81"), "Belimo LV.X81");
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("C:\\Windows\\x.exe"), "x.exe");
        assert_eq!(safe_file_name("a<b>:c|d?e*.x83"), "a_b__c_d_e_.x83");
        assert_eq!(safe_file_name("  .versteckt. "), "versteckt");
        assert_eq!(safe_file_name(""), "Download");
        assert_eq!(safe_file_name("..."), "Download");
        assert_eq!(safe_file_name("CON.X81"), "_CON.X81");
        assert_eq!(safe_file_name("com1"), "_com1");
        assert_eq!(safe_file_name("Zeile\nZwei.d83"), "ZeileZwei.d83");
        let long = safe_file_name(&format!("{}.X81", "ä".repeat(400)));
        assert_eq!(long.chars().count(), 150);
        assert!(long.ends_with(".X81"));

        let folder = Path::new("/dl");
        let taken: HashSet<PathBuf> = [folder.join("LV.X81"), folder.join("LV (2).X81")]
            .into_iter()
            .collect();
        let exists = |path: &Path| taken.contains(path);
        assert_eq!(
            download_path(folder, "LV.X81", exists),
            folder.join("LV (3).X81")
        );
        assert_eq!(
            download_path(folder, "Neu.zip", exists),
            folder.join("Neu.zip")
        );
        assert!(is_catalog_file(Path::new("/dl/LV (3).X81")));
        assert!(is_catalog_file(Path::new("/dl/merkliste.ZIP")));
        assert!(!is_catalog_file(Path::new("/dl/Datenblatt.pdf")));
        assert!(!is_catalog_file(Path::new("/dl/X81")));
    }

    #[test]
    fn the_command_line_goes_both_ways() {
        let launch = Launch {
            site: Site::StlbBau,
            url: Some("https://www.stlb-bau-online.de/x".into()),
            target: Some("Platzhalter 01.0020 „RBG“".into()),
            downloads: Some(PathBuf::from("/tmp/dl")),
            profile: Some(PathBuf::from(
                "/home/u/.local/share/proava/webfenster/stlb-bau",
            )),
            standalone: true,
        };
        assert_eq!(Launch::parse(launch.arguments()).unwrap(), launch);
        let plain = Launch::new(Site::Ausschreiben);
        assert_eq!(plain.arguments(), vec!["--seite", "ausschreiben"]);
        assert_eq!(Launch::parse(plain.arguments()).unwrap(), plain);
        assert!(Launch::parse(["--seite".into()]).is_err());
        assert!(Launch::parse(["--seite".into(), "google".into()]).is_err());
        assert!(Launch::parse(["--remote-debugging".into()]).is_err());

        let foreign = Launch {
            url: Some("https://example.com/".into()),
            ..Launch::new(Site::Ausschreiben)
        };
        assert_eq!(foreign.start_url(), "https://ausschreiben.de");
        assert_eq!(plain.start_url(), "https://ausschreiben.de");
    }

    #[test]
    fn helper_lies_next_to_the_program() {
        assert_eq!(
            helper_program(Path::new("/usr/lib/proava/proava")),
            PathBuf::from("/usr/lib/proava/proava-web")
        );
        assert_eq!(
            helper_program(Path::new("C:/Programme/proAVA/proava.exe")),
            PathBuf::from("C:/Programme/proAVA/proava-web.exe")
        );
        assert!(is_helper_program(Path::new("/x/proava-web")));
        assert!(is_helper_program(Path::new("C:/x/ProAVA-Web.exe")));
        assert!(!is_helper_program(Path::new("/x/proava")));
        assert!(!is_helper_program(Path::new("/x/proava-web-1a2b")));
        assert_eq!(
            default_profile(Path::new("/d"), Site::Ausschreiben),
            PathBuf::from("/d/webfenster/ausschreiben")
        );
        assert_eq!(
            default_downloads(Path::new("/tmp"), Site::StlbBau),
            PathBuf::from("/tmp/proava-web/stlb-bau")
        );
    }

    /// Profile folders of an older version (0775, 23:11 on 18.11.2026) are
    /// tightened to 0700 and stay usable – nothing in them is lost.
    #[cfg(unix)]
    #[test]
    fn an_old_open_profile_folder_is_made_private_and_kept() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::Builder::new()
            .prefix("proava-web-profil-")
            .tempdir()
            .unwrap();
        let profile = folder.path().join("stlb-bau");
        std::fs::create_dir(&profile).unwrap();
        std::fs::write(profile.join("cookies"), "# alt\n").unwrap();
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o775)).unwrap();
        make_private(&profile).unwrap();
        let mode = std::fs::metadata(&profile).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        assert_eq!(
            std::fs::read_to_string(profile.join("cookies")).unwrap(),
            "# alt\n"
        );
        assert!(lock_profile(&profile).unwrap().is_some(), "nutzbar");
    }

    /// A lock file left behind by a window that ended (crash, kill, old
    /// version) does not block the next window; a running window does, and
    /// after it ends the profile is free again.
    #[test]
    fn a_left_over_lock_file_does_not_block_a_running_window_does() {
        let folder = tempfile::Builder::new()
            .prefix("proava-web-sperre-")
            .tempdir()
            .unwrap();
        let profile = folder.path();
        std::fs::write(profile.join(PROFILE_LOCK), "").unwrap();
        let first = lock_profile(profile)
            .unwrap()
            .expect("liegengebliebene Sperrdatei");
        assert!(
            lock_profile(profile).unwrap().is_none(),
            "zweites Fenster, solange das erste läuft"
        );
        drop(first);
        assert!(lock_profile(profile).unwrap().is_some(), "wieder frei");
    }
}
