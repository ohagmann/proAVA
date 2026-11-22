//! German spell checking for proAVA texts.
//!
//! The Hunspell dictionaries (e.g. `de_DE.aff`/`.dic`, GPL/LGPL licensed) are
//! read from the system at run time and never built into the program; the
//! checker is [`spellbook`] (MPL-2.0). On top come an own list of building
//! automation terms ([`GA_WORDS`]), the user's personal words and words
//! ignored for the session. Checking is cached, suggestions are computed on
//! request only (they take some 50–150 ms).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Own list of building automation terms (one per line, `#` = comment).
pub const GA_WORDS: &str = include_str!("../data/ga_fachwoerter.txt");

/// Endings a listed word may carry and still count as known.
const ENDINGS: [&str; 8] = ["", "e", "en", "n", "s", "es", "er", "ern"];

/// Dictionaries tried, in this order.
const LANGUAGES: [&str; 4] = ["de_DE", "de_DE_frami", "de_AT", "de_CH"];

/// Folders searched for Hunspell dictionaries (besides the chosen one and the
/// `dictionaries` folder next to the program).
const SYSTEM_FOLDERS: [&str; 5] = [
    "/usr/share/hunspell",
    "/usr/share/myspell",
    "/usr/share/myspell/dicts",
    "/usr/local/share/hunspell",
    "/Library/Spelling",
];

/// Where a usable dictionary is: `.aff` and `.dic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryFiles {
    pub aff: PathBuf,
    pub dic: PathBuf,
}

/// Program folders of Windows that may hold LibreOffice or OpenOffice with
/// their German dictionaries (`share\extensions\dict-de`, `de_DE_frami`).
fn office_folders(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
        let Some(root) = env(variable).map(PathBuf::from) else {
            continue;
        };
        for office in ["LibreOffice", "OpenOffice 4", "OpenOffice.org 3"] {
            let folder = root
                .join(office)
                .join("share")
                .join("extensions")
                .join("dict-de");
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
    }
    folders
}

/// Looks for a German Hunspell dictionary in `preferred` (folder chosen in the
/// settings), next to the program, in the usual system folders and – on
/// Windows – in LibreOffice/OpenOffice.
pub fn find_dictionary(preferred: Option<&Path>) -> Option<DictionaryFiles> {
    let mut folders: Vec<PathBuf> = Vec::new();
    if let Some(folder) = preferred {
        folders.push(folder.to_path_buf());
    }
    if let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        folders.push(exe.join("dictionaries"));
        folders.push(exe.join("Wörterbücher"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        folders.push(PathBuf::from(home).join("Library/Spelling"));
    }
    folders.extend(SYSTEM_FOLDERS.iter().map(PathBuf::from));
    folders.extend(office_folders(|name| std::env::var_os(name)));
    find_in(&folders)
}

/// The first folder with a German `.aff`/`.dic` pair (de_DE before
/// de_DE_frami, de_AT, de_CH).
fn find_in(folders: &[PathBuf]) -> Option<DictionaryFiles> {
    for folder in folders {
        for language in LANGUAGES {
            let aff = folder.join(format!("{language}.aff"));
            let dic = folder.join(format!("{language}.dic"));
            if aff.is_file() && dic.is_file() {
                return Some(DictionaryFiles { aff, dic });
            }
        }
    }
    None
}

/// Text of a dictionary file: UTF-8, or Latin-1/ISO-8859-15 as declared by
/// `SET` in the affix file.
fn decode(bytes: &[u8], latin: bool) -> String {
    if latin {
        bytes
            .iter()
            .map(|&byte| match byte {
                // ISO-8859-15 differs in a few places; the euro sign matters.
                0xA4 => '€',
                _ => byte as char,
            })
            .collect()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// A word of a text: char range and the word itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub chars: Range<usize>,
    pub text: String,
}

/// Words worth checking: letters with inner hyphens or apostrophes; runs
/// with digits (Pt1000), all-capital abbreviations (DDC, KNX), single
/// letters, words cut with a hyphen (Lüftungs- und …) and parts of
/// addresses (www.…, name@…) are left out.
pub fn words(text: &str) -> Vec<Word> {
    let chars: Vec<char> = text.chars().collect();
    let joins = |c: char| matches!(c, '-' | '\'' | '’' | '\u{ad}');
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_alphanumeric() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len()
            && (chars[i].is_alphanumeric()
                || (joins(chars[i]) && chars.get(i + 1).is_some_and(|c| c.is_alphanumeric())))
        {
            i += 1;
        }
        let end = i;
        let token: String = chars[start..end]
            .iter()
            .filter(|&&c| c != '\u{ad}')
            .collect();
        let glued = |at: Option<char>, next: Option<char>| {
            matches!(at, Some('.' | '@' | '\\' | '_' | ':'))
                && next.is_some_and(|c| c.is_alphanumeric())
        };
        let before = start.checked_sub(1).map(|k| chars[k]);
        let before2 = start.checked_sub(2).map(|k| chars[k]);
        let address = glued(chars.get(end).copied(), chars.get(end + 1).copied())
            || (matches!(before, Some('.' | '@' | '\\' | '_' | ':'))
                && before2.is_some_and(|c| c.is_alphanumeric()));
        // Ergänzungsstrich: „Lüftungs- und Heizungsanlage“, „Heizungs- und -technik“.
        let truncated = chars.get(end) == Some(&'-') || before == Some('-');
        // Platzhalter der Briefvorlagen: „{abgabetermin}“.
        let placeholder = before == Some('{') && chars.get(end) == Some(&'}');
        let letters = token.chars().filter(|c| c.is_alphabetic()).count();
        let has_digit = token.chars().any(|c| c.is_numeric());
        let all_caps = token
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(|c| c.is_uppercase());
        if !address && !truncated && !placeholder && !has_digit && letters >= 2 && !all_caps {
            out.push(Word {
                chars: start..end,
                text: token,
            });
        }
    }
    out
}

fn lower_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The checker: dictionary, lists and cache. All methods take `&self`, so
/// one checker can be shared between threads (`Arc<Speller>`).
pub struct Speller {
    dictionary: Option<spellbook::Dictionary>,
    files: Option<DictionaryFiles>,
    listed: HashSet<String>,
    personal: Mutex<HashSet<String>>,
    ignored: Mutex<HashSet<String>>,
    cache: Mutex<HashMap<String, bool>>,
}

/// Why no dictionary could be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    NotFound,
    Unreadable(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotFound => write!(
                f,
                "Kein deutsches Wörterbuch gefunden (de_DE.aff/.dic). Unter Linux das Paket \
                 „hunspell-de-de“ installieren. Unter Windows nimmt proAVA die Wörterbücher \
                 von LibreOffice, wenn es installiert ist; sonst die Dateien in den Ordner \
                 „dictionaries“ neben dem Programm legen oder einen Ordner in den \
                 Einstellungen wählen."
            ),
            LoadError::Unreadable(message) => write!(f, "Wörterbuch nicht lesbar: {message}"),
        }
    }
}

impl Speller {
    /// Loads the dictionary files with the personal words.
    pub fn load(files: &DictionaryFiles, personal: &[String]) -> Result<Self, LoadError> {
        let aff_bytes =
            std::fs::read(&files.aff).map_err(|e| LoadError::Unreadable(e.to_string()))?;
        let dic_bytes =
            std::fs::read(&files.dic).map_err(|e| LoadError::Unreadable(e.to_string()))?;
        let head = String::from_utf8_lossy(&aff_bytes[..aff_bytes.len().min(4096)]).to_string();
        let set = head
            .lines()
            .find_map(|line| line.trim().strip_prefix("SET "))
            .unwrap_or("UTF-8")
            .trim()
            .to_ascii_uppercase();
        let latin = set.starts_with("ISO8859") || set.starts_with("ISO-8859");
        let aff: String = decode(&aff_bytes, latin)
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("SET ") {
                    "SET UTF-8"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let dic = decode(&dic_bytes, latin);
        let dictionary = spellbook::Dictionary::new(&aff, &dic)
            .map_err(|e| LoadError::Unreadable(format!("{e:?}")))?;
        let mut speller = Self::from_dictionary(Some(dictionary), personal);
        speller.files = Some(files.clone());
        Ok(speller)
    }

    /// Finds and loads a German dictionary.
    pub fn find_and_load(preferred: Option<&Path>, personal: &[String]) -> Result<Self, LoadError> {
        let files = find_dictionary(preferred).ok_or(LoadError::NotFound)?;
        Self::load(&files, personal)
    }

    /// Checker from given affix/dictionary text (tests, other sources).
    pub fn from_text(aff: &str, dic: &str, personal: &[String]) -> Result<Self, LoadError> {
        let dictionary = spellbook::Dictionary::new(aff, dic)
            .map_err(|e| LoadError::Unreadable(format!("{e:?}")))?;
        Ok(Self::from_dictionary(Some(dictionary), personal))
    }

    fn from_dictionary(dictionary: Option<spellbook::Dictionary>, personal: &[String]) -> Self {
        let listed = GA_WORDS
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect();
        Self {
            dictionary,
            files: None,
            listed,
            personal: Mutex::new(personal.iter().map(|w| w.trim().to_string()).collect()),
            ignored: Mutex::new(HashSet::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn files(&self) -> Option<&DictionaryFiles> {
        self.files.as_ref()
    }

    /// Word in one of our lists (with the usual endings).
    fn in_lists(&self, word: &str) -> bool {
        let personal = self.personal.lock().expect("personal words");
        let ignored = self.ignored.lock().expect("ignored words");
        let known = |candidate: &str| {
            self.listed.contains(candidate)
                || personal.contains(candidate)
                || ignored.contains(candidate)
        };
        if ignored.contains(word) || personal.contains(word) {
            return true;
        }
        ENDINGS.iter().any(|ending| {
            word.strip_suffix(ending)
                .is_some_and(|stem| !stem.is_empty() && (known(stem) || known(&lower_first(stem))))
        })
    }

    fn dictionary_knows(&self, word: &str) -> bool {
        let Some(dictionary) = &self.dictionary else {
            return true;
        };
        dictionary.check(word) || {
            // Capitalised at the start of a sentence.
            let lower = lower_first(word);
            lower != word && dictionary.check(&lower)
        }
    }

    /// Whether `word` is spelled correctly.
    pub fn check(&self, word: &str) -> bool {
        if let Some(known) = self.cache.lock().expect("cache").get(word) {
            return *known;
        }
        let normalized = word.replace('’', "'");
        let known = self.in_lists(&normalized)
            || self.dictionary_knows(&normalized)
            || (normalized.contains('-')
                && normalized
                    .split('-')
                    .filter(|part| part.chars().count() >= 2)
                    .all(|part| {
                        part.chars().all(|c| !c.is_lowercase())
                            || self.in_lists(part)
                            || self.dictionary_knows(part)
                    }));
        self.cache
            .lock()
            .expect("cache")
            .insert(word.to_string(), known);
        known
    }

    /// Unknown words of a text (char ranges).
    pub fn unknown_words(&self, text: &str) -> Vec<Word> {
        words(text)
            .into_iter()
            .filter(|word| !self.check(&word.text))
            .collect()
    }

    /// Up to `limit` suggestions (slow: call on request only). Words of
    /// our lists (GA terms, personal words, with the usual endings) a typo
    /// or two away come first – the dictionary does not know them.
    pub fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
        let mut out = self.list_suggestions(word);
        if let Some(dictionary) = &self.dictionary {
            let mut more = Vec::new();
            dictionary.suggest(word, &mut more);
            for suggestion in more {
                if !out.contains(&suggestion) {
                    out.push(suggestion);
                }
            }
        }
        out.retain(|s| s != word);
        out.truncate(limit);
        out
    }

    /// Listed words close to `word`, nearest first.
    fn list_suggestions(&self, word: &str) -> Vec<String> {
        let length = word.chars().count();
        if length < 4 {
            return Vec::new();
        }
        let allowed = if length >= 8 { 2 } else { 1 };
        let lower = word.to_lowercase();
        let personal = self.personal.lock().expect("personal words").clone();
        let mut found: Vec<(usize, String)> = Vec::new();
        for stem in self.listed.iter().chain(personal.iter()) {
            // Only the nearest form of each listed word (no „…montagee“).
            let best = ENDINGS
                .iter()
                .map(|ending| {
                    let candidate = format!("{stem}{ending}");
                    (edit_distance(&lower, &candidate.to_lowercase()), candidate)
                })
                .min_by_key(|(distance, candidate)| (*distance, candidate.chars().count()));
            if let Some((distance, candidate)) = best {
                if distance <= allowed && !found.iter().any(|(_, known)| *known == candidate) {
                    found.push((distance, candidate));
                }
            }
        }
        found.sort();
        found.into_iter().map(|(_, candidate)| candidate).collect()
    }

    /// Adds a word to the personal list (the caller stores the list).
    pub fn add_personal(&self, word: &str) {
        self.personal
            .lock()
            .expect("personal words")
            .insert(word.trim().to_string());
        self.cache.lock().expect("cache").clear();
    }

    /// Accepts a word until the program ends.
    pub fn ignore(&self, word: &str) {
        self.ignored
            .lock()
            .expect("ignored words")
            .insert(word.trim().to_string());
        self.cache.lock().expect("cache").clear();
    }

    pub fn personal_words(&self) -> Vec<String> {
        let mut words: Vec<String> = self
            .personal
            .lock()
            .expect("personal words")
            .iter()
            .cloned()
            .collect();
        words.sort();
        words
    }
}

/// Edits (insert, delete, replace, swap two neighbours) from `a` to `b`.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > 2 {
        return 3;
    }
    let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in table.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in table[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (table[i - 1][j] + 1)
                .min(table[i][j - 1] + 1)
                .min(table[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(table[i - 2][j - 2] + 1);
            }
            table[i][j] = best;
        }
    }
    table[a.len()][b.len()]
}

/// Unknown words of many texts: word → (count, places), places in the order
/// they were given and each listed once.
pub fn collect_unknown<'a>(
    speller: &Speller,
    texts: impl IntoIterator<Item = (String, &'a str)>,
) -> BTreeMap<String, (usize, Vec<String>)> {
    let mut found: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for (place, text) in texts {
        for word in speller.unknown_words(text) {
            let entry = found.entry(word.text).or_default();
            entry.0 += 1;
            if !entry.1.contains(&place) {
                entry.1.push(place.clone());
            }
        }
    }
    found
}

#[cfg(test)]
mod office_tests {
    use std::path::PathBuf;

    use super::{find_in, office_folders};

    #[test]
    fn libreoffice_dictionaries_under_the_windows_program_folders() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-spell-office-")
            .tempdir()
            .unwrap();
        let root = scratch.path().to_path_buf();
        let env = |name: &str| match name {
            "ProgramW6432" | "ProgramFiles" => Some(root.join("Programme").into_os_string()),
            "ProgramFiles(x86)" => Some(root.join("Programme (x86)").into_os_string()),
            _ => None,
        };
        let folders = office_folders(env);
        let libre = root
            .join("Programme")
            .join("LibreOffice")
            .join("share")
            .join("extensions")
            .join("dict-de");
        assert_eq!(folders[0], libre);
        assert_eq!(
            folders.len(),
            6,
            "doppelte Programmordner nur einmal: {folders:?}"
        );
        assert!(
            office_folders(|_| None).is_empty(),
            "unter Linux keine Einträge"
        );

        // LibreOffice bringt de_DE_frami mit.
        std::fs::create_dir_all(&libre).unwrap();
        std::fs::write(libre.join("de_DE_frami.aff"), "SET UTF-8\n").unwrap();
        std::fs::write(libre.join("de_DE_frami.dic"), "1\nHaus\n").unwrap();
        let found = find_in(&folders).expect("Wörterbuch von LibreOffice");
        assert_eq!(found.dic, libre.join("de_DE_frami.dic"));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(find_in(&[PathBuf::from("/gibt/es/nicht")]), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A tiny dictionary with one suffix rule (Hunspell format).
    const AFF: &str =
        "SET UTF-8\nTRY esianrtolcdugmphbyfvkwzESIANRTOLCDUGMPHBYFVKWZ\nSFX N Y 1\nSFX N 0 en .\n";
    const DIC: &str = "6\nHaus/N\nMaus\nliefern\nmontieren\nund\nStraße\n";

    fn speller() -> Speller {
        Speller::from_text(AFF, DIC, &["Fühlerkabel".to_string()]).unwrap()
    }

    #[test]
    fn words_skip_numbers_abbreviations_and_addresses() {
        let found: Vec<String> = words("Liefern 2 Stk. DDC-Station, Pt1000 und www.beispiel.de BACnet/IP – Lüftungs- und Heiz-Anlage o'clock")
            .into_iter()
            .map(|w| w.text)
            .collect();
        assert_eq!(
            found,
            vec![
                "Liefern",
                "Stk",
                "DDC-Station",
                "und",
                "BACnet",
                "und",
                "Heiz-Anlage",
                "o'clock"
            ]
        );
        let placeholders: Vec<String> =
            words("bis {abgabetermin} an {bieter.name}, {ausfuehrung_beginn} {Text")
                .into_iter()
                .map(|w| w.text)
                .collect();
        assert_eq!(
            placeholders,
            vec!["bis", "an", "Text"],
            "Platzhalter in {{…}} nicht prüfen"
        );
        let word = &words("ab Fühler")[1];
        assert_eq!((word.chars.clone(), word.text.as_str()), (3..9, "Fühler"));
    }

    #[test]
    fn checks_dictionary_lists_cases_and_hyphens() {
        let s = speller();
        assert!(s.check("Haus") && s.check("Hausen") && s.check("Straße"));
        assert!(s.check("Liefern"), "Satzanfang");
        assert!(!s.check("Hauss"));
        assert!(
            s.check("BACnet") && s.check("Automationsstationen"),
            "GA-Liste mit Endung"
        );
        assert!(
            s.check("Fühlerkabel") && s.check("Fühlerkabeln"),
            "persönlich"
        );
        assert!(s.check("Maus-Haus") && s.check("DDC-Haus") && !s.check("Maus-Huas"));
        for word in [
            "Automationseinrichtungen",
            "Patchkabeln",
            "bauseitigen",
            "Aufputzmontage",
            "inkl",
        ] {
            assert!(s.check(word), "GA-Liste: {word}");
        }
        s.add_personal("Huas");
        assert!(s.check("Maus-Huas"));
        assert!(!s.check("Muas"));
        s.ignore("Muas");
        assert!(s.check("Muas"));
        assert_eq!(s.personal_words(), vec!["Fühlerkabel", "Huas"]);
    }

    #[test]
    fn listed_words_come_first_in_suggestions() {
        let s = speller();
        assert_eq!(s.suggest("Hutschienenmontge", 3)[0], "Hutschienenmontage");
        assert!(
            !s.suggest("Hutschienenmontge", 5)
                .iter()
                .any(|w| w.starts_with("Hutschienenmontage") && w != "Hutschienenmontage"),
            "nur die nächste Form"
        );
        assert_eq!(
            s.suggest("Automationsstatoinen", 3)[0],
            "Automationsstationen"
        );
        assert_eq!(
            s.suggest("Fühlerkabl", 3)[0],
            "Fühlerkabel",
            "eigene Wörter"
        );
        assert_eq!(edit_distance("Statoin", "Station"), 1, "Vertauschung");
        assert_eq!(edit_distance("abc", "abcdef"), 3);
    }

    #[test]
    fn suggestions_and_collected_places() {
        let s = speller();
        assert!(s.suggest("Huas", 5).contains(&"Haus".to_string()));
        let found = collect_unknown(
            &s,
            vec![
                ("01.0010".to_string(), "Hauss und Muas"),
                ("01.0020".to_string(), "Muas Muas"),
            ],
        );
        assert_eq!(
            found["Muas"],
            (3, vec!["01.0010".to_string(), "01.0020".to_string()])
        );
        assert_eq!(found["Hauss"].0, 1);
        assert_eq!(s.unknown_words("Haus Hauss")[0].chars, 5..10);
    }

    /// With the system dictionary (skipped when it is not installed).
    #[test]
    fn german_system_dictionary_when_installed() {
        let Some(files) = find_dictionary(None) else {
            return;
        };
        let s = Speller::load(&files, &[]).unwrap();
        for word in [
            "Lüftungsanlage",
            "Raumbediengerät",
            "Automationsstation",
            "Inbetriebnahme",
            "Maßnahme",
            "Außenluft",
            "Hutschienenmontage",
        ] {
            assert!(s.check(word), "{word}");
        }
        assert!(!s.check("Wrtfehler"));
        assert!(s
            .suggest("Wrtfehler", 5)
            .contains(&"Wortfehler".to_string()));
    }
}
