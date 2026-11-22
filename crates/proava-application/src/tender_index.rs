//! Suche in den Katalogverzeichnissen von AUSSCHREIBEN.DE.
//!
//! AUSSCHREIBEN.DE publishes plain-text lists of its catalogues „for AI
//! assistants“ (`/.well-known/ard.json` → `llms.txt`): `catalogs/llms.txt`
//! names every catalogue, `catalog/<id>/llms.txt` its folders and pages,
//! `catalog/<id>/llms-<n>.txt` every text with title, article number and the
//! address of its page. robots.txt allows these files; the search of the web
//! site (`/search/`, `searchTerm=`) and its internal interface (`/v2/`) are
//! closed to programs and are never asked ([`is_allowed`]). The lists carry
//! no long text and no GAEB file: the text is exported on its page in the
//! browser and arrives through the Downloads folder (see
//! `docs/FACHLICHES.md`).
//!
//! Before any list, [`search`] reads `robots.txt` (cached like the lists)
//! and asks only addresses it allows ([`robots_allows`]); should AUSSCHREIBEN.DE
//! close the lists one day, the search stops with a message.
//!
//! Everything here is pure except [`search`], which asks a [`WebTexts`] port.

use crate::ports::WebTexts;

/// List of all catalogues.
pub const CATALOGS_URL: &str = "https://ausschreiben.de/catalogs/llms.txt";
/// Rules for programs (read before every search, cached like the lists).
pub const ROBOTS_URL: &str = "https://ausschreiben.de/robots.txt";
/// Product token proAVA looks for in robots.txt (else the rules for `*`).
pub const ROBOTS_AGENT: &str = "proava";
/// Start page of the portal (for the browser).
pub const PORTAL_URL: &str = "https://ausschreiben.de";
/// Lists are asked again after this many days (cache of the adapter).
pub const MAX_AGE_DAYS: u32 = 7;
/// At most this many catalogues are opened per search.
pub const MAX_CATALOGS: usize = 4;
/// At most this many list pages are read per search (server load).
pub const MAX_PAGES: usize = 12;
/// At most this many hits are shown.
pub const MAX_HITS: usize = 300;

/// A catalogue of the portal (one line of `catalogs/llms.txt`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexCatalog {
    pub name: String,
    pub manufacturer: String,
    pub language: String,
    /// `…/catalog/<id>/llms.txt`.
    pub list_url: String,
}

impl IndexCatalog {
    /// The catalogue page for the browser (`…/catalog/<id>`).
    pub fn page_url(&self) -> String {
        self.list_url
            .strip_suffix("/llms.txt")
            .unwrap_or(&self.list_url)
            .to_string()
    }
}

/// A text of a catalogue (one line of `llms-<n>.txt`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexItem {
    /// Name of the catalogue (usually the manufacturer or its product line).
    pub catalog: String,
    /// Folder path in the catalogue, „Sensoren › Kanalsensoren“.
    pub folder: String,
    pub title: String,
    pub article: String,
    /// Page of the text on AUSSCHREIBEN.DE (export there).
    pub url: String,
}

/// Whether proAVA may ask this address: only the published lists on
/// `https://ausschreiben.de` (`catalogs/llms.txt`, `catalog/<id>/llms.txt`,
/// `catalog/<id>/llms-<n>.txt`), no query, nothing else.
pub fn is_allowed(url: &str) -> bool {
    let Some(path) = url.strip_prefix("https://ausschreiben.de/") else {
        return false;
    };
    if path.contains(['?', '#', '%', '\\']) || path.contains("..") {
        return false;
    }
    if path == "catalogs/llms.txt" || path == "llms.txt" || path == "robots.txt" {
        return true;
    }
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["catalog", id, file] => {
            let id_ok = !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
            let file_ok = *file == "llms.txt"
                || file
                    .strip_prefix("llms-")
                    .and_then(|rest| rest.strip_suffix(".txt"))
                    .is_some_and(|number| {
                        !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
                    });
            id_ok && file_ok
        }
        _ => false,
    }
}

/// A group of robots.txt: the agents it names and its rules (allow?, pattern).
type RobotsGroup = (Vec<String>, Vec<(bool, String)>);

/// Whether `robots` (the text of robots.txt) lets `agent` read `path`
/// (`/catalog/x/llms-1.txt`, with query if any), after RFC 9309: the groups
/// naming the agent, else those for `*`; `*` matches any characters, `$` the
/// end; the longest matching rule wins, Allow before Disallow on a tie;
/// `/robots.txt` itself is always allowed.
pub fn robots_allows(robots: &str, agent: &str, path: &str) -> bool {
    if path == "/robots.txt" {
        return true;
    }
    let agent = agent.to_ascii_lowercase();
    // (agents of the group, rules of the group)
    let mut groups: Vec<RobotsGroup> = Vec::new();
    let mut in_rules = false;
    for line in robots.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim().to_string());
        match key.as_str() {
            "user-agent" => {
                if in_rules || groups.is_empty() {
                    groups.push((Vec::new(), Vec::new()));
                    in_rules = false;
                }
                if let Some(group) = groups.last_mut() {
                    group.0.push(value.to_ascii_lowercase());
                }
            }
            "allow" | "disallow" => {
                in_rules = true;
                if let Some(group) = groups.last_mut() {
                    if !value.is_empty() {
                        group.1.push((key == "allow", value));
                    }
                }
            }
            _ => {}
        }
    }
    let named: Vec<&RobotsGroup> = groups
        .iter()
        .filter(|(agents, _)| agents.contains(&agent))
        .collect();
    let chosen = if named.is_empty() {
        groups
            .iter()
            .filter(|(agents, _)| agents.iter().any(|name| name == "*"))
            .collect()
    } else {
        named
    };
    let mut best: Option<(usize, bool)> = None;
    for (allow, pattern) in chosen.iter().flat_map(|(_, rules)| rules) {
        if robots_pattern_matches(pattern, path) {
            let length = pattern.chars().count();
            let better = match best {
                None => true,
                Some((best_length, best_allow)) => {
                    length > best_length || (length == best_length && *allow && !best_allow)
                }
            };
            if better {
                best = Some((length, *allow));
            }
        }
    }
    best.is_none_or(|(_, allow)| allow)
}

/// A robots.txt path pattern: prefix match, `*` = any characters, `$` at
/// the end = the path ends there.
fn robots_pattern_matches(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(rest) => (rest, true),
        None => (pattern, false),
    };
    let pattern: Vec<char> = pattern.chars().collect();
    let path: Vec<char> = path.chars().collect();
    // match_from[i][j]: pattern[i..] matches a prefix (or, anchored, all) of path[j..].
    fn go(pattern: &[char], path: &[char], anchored: bool) -> bool {
        match pattern.split_first() {
            None => !anchored || path.is_empty(),
            Some(('*', rest)) => (0..=path.len()).any(|skip| go(rest, &path[skip..], anchored)),
            Some((first, rest)) => path
                .split_first()
                .is_some_and(|(c, tail)| c == first && go(rest, tail, anchored)),
        }
    }
    go(&pattern, &path, anchored)
}

/// A text page `https://ausschreiben.de/…/catalogItem/…` (opened in the
/// browser): no other host, no backslash, no spaces or quotes.
fn is_item_page(url: &str) -> bool {
    url.strip_prefix("https://ausschreiben.de/")
        .is_some_and(|path| {
            path.contains("catalogItem/")
                && !path.contains("..")
                && !path
                    .chars()
                    .any(|c| c == '\\' || c == '"' || c.is_whitespace() || c.is_control())
        })
}

/// `[text](url)` at the start of `line` (after „- “): (text, url, rest).
fn link(line: &str) -> Option<(&str, &str, &str)> {
    let line = line.trim_start().strip_prefix("- ")?.trim_start();
    let line = line.strip_prefix('[')?;
    let close = line.find("](")?;
    let text = &line[..close];
    let after = &line[close + 2..];
    let end = after.find(')')?;
    Some((text, &after[..end], &after[end + 1..]))
}

/// Catalogues from `catalogs/llms.txt`:
/// `- [Belimo](https://ausschreiben.de/catalog/belimo/llms.txt): BELIMO Automation AG, German`.
pub fn parse_catalogs(text: &str) -> Vec<IndexCatalog> {
    let mut catalogs = Vec::new();
    for line in text.lines() {
        let Some((name, url, rest)) = link(line) else {
            continue;
        };
        if !url.ends_with("/llms.txt") || !url.contains("/catalog/") || !is_allowed(url) {
            continue;
        }
        let rest = rest.trim_start_matches(':').trim();
        let (manufacturer, language) = match rest.rsplit_once(',') {
            Some((manufacturer, language)) => (manufacturer.trim(), language.trim()),
            None => (rest, ""),
        };
        catalogs.push(IndexCatalog {
            name: name.trim().to_string(),
            manufacturer: manufacturer.to_string(),
            language: language.to_string(),
            list_url: url.to_string(),
        });
    }
    catalogs
}

/// Pages with the texts of a catalogue (`llms-1.txt` …) from its
/// `llms.txt`, each once, in order.
pub fn parse_pages(text: &str) -> Vec<String> {
    let mut pages: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(start) = rest.find("](") {
            let after = &rest[start + 2..];
            let Some(end) = after.find(')') else {
                break;
            };
            let url = &after[..end];
            let is_page = url
                .rsplit('/')
                .next()
                .is_some_and(|file| file.starts_with("llms-"));
            if is_page && is_allowed(url) && !pages.iter().any(|page| page == url) {
                pages.push(url.to_string());
            }
            rest = &after[end..];
        }
    }
    pages
}

/// Texts of one list page: `## Ordner`, `### Unterordner`, then
/// `- [Titel](…/catalogItem/…) – Product number 01APS-101`.
pub fn parse_items(text: &str, catalog: &str) -> Vec<IndexItem> {
    let mut items = Vec::new();
    let mut folders: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix('#') {
            let level = 1 + heading.chars().take_while(|c| *c == '#').count();
            let name = heading.trim_start_matches('#').trim();
            // „# Katalog – page 1 of 3“ is the page title, folders start at „##“.
            if level >= 2 {
                folders.truncate(level - 2);
                folders.push(name.to_string());
            }
            continue;
        }
        let Some((title, url, rest)) = link(trimmed) else {
            continue;
        };
        // The page opens with the system (Windows ShellExecute): only a web
        // page of AUSSCHREIBEN.DE, never a file or another address.
        if !is_item_page(url) {
            continue;
        }
        let rest = rest.trim().trim_start_matches(['–', '-']).trim();
        let article = rest
            .strip_prefix("Product number")
            .or_else(|| rest.strip_prefix("Artikelnummer"))
            .map(|number| number.trim_start_matches(':').trim().to_string())
            .unwrap_or_default();
        items.push(IndexItem {
            catalog: catalog.to_string(),
            folder: folders.join(" › "),
            title: title.trim().to_string(),
            article,
            url: url.to_string(),
        });
    }
    items
}

/// Words of a query, lower case, without very short ones.
fn words(query: &str) -> Vec<String> {
    query
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .map(|word| word.trim().to_lowercase())
        .filter(|word| word.chars().count() >= 2)
        .collect()
}

/// Words of name, manufacturer and language of a catalogue in lower case –
/// what a query can name („Belimo english“).
fn catalog_words(catalog: &IndexCatalog) -> Vec<String> {
    format!(
        "{} {} {}",
        catalog.name, catalog.manufacturer, catalog.language
    )
    .to_lowercase()
    .split(|c: char| !c.is_alphanumeric())
    .filter(|word| !word.is_empty())
    .map(str::to_string)
    .collect()
}

/// Whether a query word names the catalogue: a word of its name starts with
/// it. Numbers and words shorter than three letters never do („20 Nm“ must
/// not pick „Produktkatalog 2025“ or „Fernmelde…“).
fn names(word: &str, catalog_words: &[String]) -> bool {
    word.chars().count() >= 3
        && word.chars().any(char::is_alphabetic)
        && catalog_words.iter().any(|name| name.starts_with(word))
}

/// Which catalogues a query names and which words remain for the texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPlan {
    /// Indices into the catalogue list, best first (at most [`MAX_CATALOGS`]).
    pub catalogs: Vec<usize>,
    /// Words that did not name the catalogue: all must appear in a text.
    pub words: Vec<String>,
}

/// A query names a manufacturer or catalogue („Belimo Drehantrieb 20 Nm“):
/// catalogues whose name or manufacturer contain the most query words; the
/// other words filter the texts. Err when no catalogue is named – the lists
/// carry titles per catalogue only, a search over all 900 catalogues stays
/// on the web site.
pub fn plan(catalogs: &[IndexCatalog], query: &str) -> Result<SearchPlan, String> {
    let all = words(query);
    if all.is_empty() {
        return Err(
            "Bitte Hersteller oder Katalog und Suchbegriffe eingeben, z. B. „Belimo Drehantrieb“."
                .into(),
        );
    }
    let mut scored: Vec<(usize, usize)> = catalogs
        .iter()
        .enumerate()
        .map(|(index, catalog)| {
            let name = catalog_words(catalog);
            let score = all.iter().filter(|word| names(word, &name)).count();
            (index, score)
        })
        .filter(|(_, score)| *score > 0)
        .collect();
    let best = scored.iter().map(|(_, score)| *score).max().unwrap_or(0);
    if best == 0 {
        return Err(
            "Kein Katalog dazu gefunden – bitte den Hersteller mit angeben (z. B. „Belimo Drehantrieb“). Eine Volltextsuche über alle Kataloge gibt es nur auf AUSSCHREIBEN.DE selbst."
                .into(),
        );
    }
    scored.retain(|(_, score)| *score == best);
    // Same catalogue in several languages: German only, unless the query
    // names the other language („Belimo english“).
    let german =
        |index: usize| catalogs[index].language.is_empty() || catalogs[index].language == "German";
    let names_language = |index: usize| {
        let language = catalogs[index].language.to_lowercase();
        !language.is_empty() && all.contains(&language)
    };
    if scored.iter().any(|(index, _)| german(*index)) {
        scored.retain(|(index, _)| german(*index) || names_language(*index));
    }
    // German catalogues first, then by name.
    scored.sort_by_key(|(index, _)| {
        let catalog = &catalogs[*index];
        (catalog.language != "German", catalog.name.to_lowercase())
    });
    let chosen: Vec<usize> = scored
        .iter()
        .take(MAX_CATALOGS)
        .map(|(index, _)| *index)
        .collect();
    let chosen_words: Vec<Vec<String>> = chosen
        .iter()
        .map(|index| catalog_words(&catalogs[*index]))
        .collect();
    let remaining = all
        .into_iter()
        .filter(|word| !chosen_words.iter().all(|name| names(word, name)))
        .collect();
    Ok(SearchPlan {
        catalogs: chosen,
        words: remaining,
    })
}

/// Whether a text matches all remaining words (title, article number,
/// folder; case does not matter). No words: every text of the catalogue.
pub fn matches(item: &IndexItem, words: &[String]) -> bool {
    let haystack = format!("{} {} {}", item.title, item.article, item.folder).to_lowercase();
    words.iter().all(|word| haystack.contains(word.as_str()))
}

/// Result of [`search`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchResult {
    pub hits: Vec<IndexItem>,
    /// Catalogues that were searched.
    pub catalogs: Vec<IndexCatalog>,
    /// More hits than shown, or pages left out (limits).
    pub truncated: bool,
}

/// Searches the published lists: catalogue list, then the pages of the named
/// catalogues (at most [`MAX_CATALOGS`] and [`MAX_PAGES`]), cached by the
/// adapter for [`MAX_AGE_DAYS`].
pub fn search(web: &dyn WebTexts, query: &str) -> Result<SearchResult, String> {
    let robots = web.get_text(ROBOTS_URL, MAX_AGE_DAYS).map_err(|error| {
        format!(
            "robots.txt von AUSSCHREIBEN.DE nicht lesbar – ohne die Regeln keine Suche ({error})."
        )
    })?;
    let fetch = |url: &str| fetch(web, &robots, url);
    let catalogs = parse_catalogs(&fetch(CATALOGS_URL)?);
    if catalogs.is_empty() {
        return Err("Die Katalogliste von AUSSCHREIBEN.DE ist leer oder nicht lesbar.".into());
    }
    let plan = plan(&catalogs, query)?;
    let mut result = SearchResult::default();
    let mut pages_read = 0;
    for index in &plan.catalogs {
        let catalog = &catalogs[*index];
        result.catalogs.push(catalog.clone());
        let pages = parse_pages(&fetch(&catalog.list_url)?);
        for page in pages {
            if pages_read >= MAX_PAGES {
                result.truncated = true;
                break;
            }
            pages_read += 1;
            for item in parse_items(&fetch(&page)?, &catalog.name) {
                if matches(&item, &plan.words) {
                    if result.hits.len() >= MAX_HITS {
                        result.truncated = true;
                        break;
                    }
                    result.hits.push(item);
                }
            }
        }
    }
    Ok(result)
}

fn fetch(web: &dyn WebTexts, robots: &str, url: &str) -> Result<String, String> {
    if !is_allowed(url) {
        return Err(format!("Adresse nicht freigegeben: {url}"));
    }
    let path = url.strip_prefix("https://ausschreiben.de").unwrap_or(url);
    if !robots_allows(robots, ROBOTS_AGENT, path) {
        return Err(format!(
            "robots.txt von AUSSCHREIBEN.DE sperrt {url} für Programme – die Suche geht dann nur auf der Website selbst."
        ));
    }
    web.get_text(url, MAX_AGE_DAYS)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    // Synthetic lists in the published layout; names and numbers made up.
    const CATALOGS: &str = "# Catalogs on AUSSCHREIBEN.DE\n\n> Catalog files for AI assistants.\n\n## Catalogs\n\n- [Muster Antriebe](https://ausschreiben.de/catalog/muster_antriebe/llms.txt): Muster Antriebe GmbH, German\n- [Muster Antriebe english](https://ausschreiben.de/catalog/muster_antriebe_en/llms.txt): Muster Antriebe GmbH, English\n- [Beispiel Fühler](https://ausschreiben.de/catalog/beispiel/llms.txt): Beispiel Sensorik AG, German\n- [Fremd](https://example.com/catalog/x/llms.txt): Fremd GmbH, German\n";
    const CATALOG: &str = "# Muster Antriebe\n\n## Contents\n\n- Antriebe\n  - Drehantriebe (2 items): [page 1](https://ausschreiben.de/catalog/muster_antriebe/llms-1.txt)\n  - Hubantriebe (1 item): [page 2](https://ausschreiben.de/catalog/muster_antriebe/llms-2.txt)\n  - Zubehör (1 item): [page 1](https://ausschreiben.de/catalog/muster_antriebe/llms-1.txt)\n";
    const PAGE1: &str = "# Muster Antriebe – page 1 of 2\n\nTender texts …\n\n## Antriebe\n\n### Drehantriebe\n\n- [Drehantrieb 20 Nm, stetig](https://ausschreiben.de/catalog/muster_antriebe/catalogItem/aaaa-1) – Product number MA-20-SR\n- [Drehantrieb 10 Nm, Auf-Zu](https://ausschreiben.de/catalog/muster_antriebe/catalogItem/aaaa-2) – Product number MA-10-AZ\n\n### Zubehör\n\n- [Hilfsschalter](https://ausschreiben.de/catalog/muster_antriebe/catalogItem/aaaa-3)\n";
    const PAGE2: &str = "# Muster Antriebe – page 2 of 2\n\n## Antriebe\n\n### Hubantriebe\n\n- [Hubantrieb 500 N, stetig](https://ausschreiben.de/catalog/muster_antriebe/catalogItem/bbbb-1) – Product number MH-500\n";
    /// Synthetic rules in the form of the site's robots.txt (search and
    /// internal interface closed, lists open).
    const ROBOTS: &str = "User-agent: *\nDisallow: /*searchTerm=\nDisallow: /catalog/*?*\nDisallow: /search/\nDisallow: /v2/\nAllow: /catalog/*?mobileView=CatalogTree\n";

    struct Fake {
        files: HashMap<&'static str, &'static str>,
        asked: Mutex<Vec<String>>,
    }

    impl WebTexts for Fake {
        fn get_text(&self, url: &str, max_age_days: u32) -> Result<String, String> {
            assert_eq!(max_age_days, MAX_AGE_DAYS);
            self.asked.lock().unwrap().push(url.to_string());
            self.files
                .get(url)
                .map(|text| text.to_string())
                .ok_or_else(|| format!("nicht gefunden: {url}"))
        }
    }

    fn fake() -> Fake {
        Fake {
            files: HashMap::from([
                (ROBOTS_URL, ROBOTS),
                (CATALOGS_URL, CATALOGS),
                (
                    "https://ausschreiben.de/catalog/muster_antriebe/llms.txt",
                    CATALOG,
                ),
                (
                    "https://ausschreiben.de/catalog/muster_antriebe/llms-1.txt",
                    PAGE1,
                ),
                (
                    "https://ausschreiben.de/catalog/muster_antriebe/llms-2.txt",
                    PAGE2,
                ),
            ]),
            asked: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn only_the_published_lists_may_be_asked() {
        assert!(is_allowed(CATALOGS_URL));
        assert!(is_allowed(
            "https://ausschreiben.de/catalog/belimo/llms.txt"
        ));
        assert!(is_allowed(
            "https://ausschreiben.de/catalog/belimo/llms-12.txt"
        ));
        assert!(!is_allowed(
            "https://ausschreiben.de/search/?searchTerm=Antrieb"
        ));
        assert!(!is_allowed("https://ausschreiben.de/v2/catalog/belimo"));
        assert!(!is_allowed("https://ausschreiben.de/catalog/belimo?x=1"));
        assert!(!is_allowed(
            "https://ausschreiben.de/catalog/belimo/catalogItem/aaaa"
        ));
        assert!(!is_allowed(
            "https://ausschreiben.de/catalog/../v2/llms.txt"
        ));
        assert!(!is_allowed("http://ausschreiben.de/catalogs/llms.txt"));
        assert!(!is_allowed("https://example.com/catalogs/llms.txt"));
        assert!(!is_allowed(
            "https://ausschreiben.de/catalog/belimo/llms-x.txt"
        ));
    }

    #[test]
    fn robots_txt_is_obeyed() {
        let allows = |path: &str| robots_allows(ROBOTS, ROBOTS_AGENT, path);
        assert!(allows("/catalogs/llms.txt"));
        assert!(allows("/catalog/belimo/llms-1.txt"));
        assert!(!allows("/search/?q=x"));
        assert!(!allows("/catalog/belimo?searchTerm=Antrieb"));
        assert!(!allows("/v2/catalog"));
        assert!(
            allows("/catalog/belimo?mobileView=CatalogTree"),
            "längere Allow-Regel"
        );
        assert!(allows("/robots.txt"));
        // A group for proAVA replaces the one for everybody.
        let own = "User-agent: *\nDisallow: /\n\nUser-agent: proAVA\nUser-agent: other\nDisallow: /catalog/\nAllow: /catalogs/\n";
        assert!(robots_allows(own, ROBOTS_AGENT, "/catalogs/llms.txt"));
        assert!(!robots_allows(own, ROBOTS_AGENT, "/catalog/x/llms.txt"));
        assert!(
            !robots_allows(own, "fremd", "/catalogs/llms.txt"),
            "für * alles zu"
        );
        // `$` and comments, empty Disallow.
        let end = "User-agent: * # alle\nDisallow: /*.txt$\nDisallow:\n";
        assert!(!robots_allows(end, ROBOTS_AGENT, "/llms.txt"));
        assert!(robots_allows(end, ROBOTS_AGENT, "/llms.txt?x"));
        assert!(
            robots_allows("", ROBOTS_AGENT, "/catalogs/llms.txt"),
            "keine Regeln"
        );

        // Closed lists stop the search with a message.
        let closed = Fake {
            files: HashMap::from([
                (ROBOTS_URL, "User-agent: *\nDisallow: /catalogs/\n"),
                (CATALOGS_URL, CATALOGS),
            ]),
            asked: Mutex::new(Vec::new()),
        };
        let error = search(&closed, "Muster").unwrap_err();
        assert!(error.contains("robots.txt"), "{error}");
        assert_eq!(
            closed.asked.lock().unwrap().len(),
            1,
            "nur robots.txt gelesen"
        );
        let without = Fake {
            files: HashMap::new(),
            asked: Mutex::new(Vec::new()),
        };
        assert!(search(&without, "Muster")
            .unwrap_err()
            .contains("robots.txt"));
    }

    /// Text pages open with the system (Windows ShellExecute): only web pages
    /// of AUSSCHREIBEN.DE, never a file, a program or another host.
    #[test]
    fn only_web_pages_of_the_site_are_hits() {
        let page = |url: &str| format!("## A\n\n- [Text]({url}) – Product number 1\n");
        let hits = |url: &str| parse_items(&page(url), "K").len();
        assert_eq!(
            hits("https://ausschreiben.de/catalog/k/catalogItem/aaaa-1"),
            1
        );
        assert_eq!(hits("C:\\Users\\x\\catalogItem\\start.bat"), 0);
        assert_eq!(hits("file:///C:/catalogItem/start.bat"), 0);
        assert_eq!(hits("\\\\server\\share\\catalogItem\\x.exe"), 0);
        assert_eq!(hits("https://example.com/catalogItem/aaaa"), 0);
        assert_eq!(hits("http://ausschreiben.de/catalog/k/catalogItem/aaaa"), 0);
        assert_eq!(
            hits("https://ausschreiben.de/catalog/k/catalogItem/..\\x"),
            0
        );
        assert_eq!(
            hits("https://ausschreiben.de/catalog/k/catalogItem/a\"b"),
            0
        );
    }

    #[test]
    fn lists_are_read() {
        let catalogs = parse_catalogs(CATALOGS);
        assert_eq!(catalogs.len(), 3, "fremde Adresse übergangen");
        assert_eq!(catalogs[0].name, "Muster Antriebe");
        assert_eq!(catalogs[0].manufacturer, "Muster Antriebe GmbH");
        assert_eq!(catalogs[0].language, "German");
        assert_eq!(
            catalogs[0].page_url(),
            "https://ausschreiben.de/catalog/muster_antriebe"
        );
        assert_eq!(
            parse_pages(CATALOG),
            vec![
                "https://ausschreiben.de/catalog/muster_antriebe/llms-1.txt".to_string(),
                "https://ausschreiben.de/catalog/muster_antriebe/llms-2.txt".to_string(),
            ]
        );
        let items = parse_items(PAGE1, "Muster Antriebe");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].folder, "Antriebe › Drehantriebe");
        assert_eq!(items[0].article, "MA-20-SR");
        assert_eq!(items[2].folder, "Antriebe › Zubehör");
        assert_eq!(items[2].article, "");
    }

    #[test]
    fn a_query_names_the_catalogue_and_filters_its_texts() {
        let catalogs = parse_catalogs(CATALOGS);
        let plan = plan(&catalogs, "Muster stetig").unwrap();
        assert_eq!(plan.catalogs, vec![0], "nur der deutsche Katalog");
        assert_eq!(plan.words, vec!["stetig".to_string()]);
        let english = super::plan(&catalogs, "Muster english").unwrap();
        assert_eq!(english.catalogs, vec![1], "Sprache genannt");
        assert!(english.words.is_empty());
        assert!(
            super::plan(&catalogs, "Drehantrieb").is_err(),
            "kein Hersteller"
        );
        assert!(super::plan(&catalogs, " ").is_err());
        let numbers = super::plan(&catalogs, "Muster 20 Nm").unwrap();
        assert_eq!(numbers.catalogs, vec![0]);
        assert_eq!(numbers.words, vec!["20".to_string(), "nm".to_string()]);
        let prefix = super::plan(&catalogs, "Beisp Fühler").unwrap();
        assert_eq!(prefix.catalogs, vec![2], "Wortanfang genügt");

        let web = fake();
        let result = search(&web, "Muster Antriebe GmbH German stetig").unwrap();
        let titles: Vec<&str> = result.hits.iter().map(|hit| hit.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["Drehantrieb 20 Nm, stetig", "Hubantrieb 500 N, stetig"]
        );
        assert_eq!(result.catalogs.len(), 1);
        assert!(!result.truncated);
        let asked = web.asked.lock().unwrap().clone();
        assert_eq!(
            asked.len(),
            5,
            "robots.txt, Liste, Katalog, zwei Seiten: {asked:?}"
        );
        assert_eq!(asked[0], ROBOTS_URL, "robots.txt zuerst");
        assert!(asked.iter().all(|url| is_allowed(url)));

        let by_article = search(&fake(), "Muster Antriebe GmbH German MA-10").unwrap();
        assert_eq!(by_article.hits.len(), 1);
        assert_eq!(by_article.hits[0].title, "Drehantrieb 10 Nm, Auf-Zu");
    }
}
