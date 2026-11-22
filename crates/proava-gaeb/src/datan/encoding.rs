//! Character sets found in GAEB 90 files.
//!
//! The standard asks for IBM code page 850 (PC-DOS); older mainframe files use
//! the German 7-bit variant DIN 66003, some Windows programs write Windows-1252
//! or UTF-8. Detection looks at the umlaut bytes, which differ between them.

/// Character set of a DATAN file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// IBM PC code page 850 (default of GAEB 90).
    Cp850,
    /// Windows-1252 / ISO 8859-1.
    Windows1252,
    Utf8,
    /// DIN 66003 (German ISO 646): `[\]{|}~@` stand for `ÄÖÜäöüß§`.
    Din66003,
}

const CP850_HIGH: [char; 128] = [
    '\u{c7}', '\u{fc}', '\u{e9}', '\u{e2}', '\u{e4}', '\u{e0}', '\u{e5}', '\u{e7}', '\u{ea}',
    '\u{eb}', '\u{e8}', '\u{ef}', '\u{ee}', '\u{ec}', '\u{c4}', '\u{c5}', '\u{c9}', '\u{e6}',
    '\u{c6}', '\u{f4}', '\u{f6}', '\u{f2}', '\u{fb}', '\u{f9}', '\u{ff}', '\u{d6}', '\u{dc}',
    '\u{f8}', '\u{a3}', '\u{d8}', '\u{d7}', '\u{192}', '\u{e1}', '\u{ed}', '\u{f3}', '\u{fa}',
    '\u{f1}', '\u{d1}', '\u{aa}', '\u{ba}', '\u{bf}', '\u{ae}', '\u{ac}', '\u{bd}', '\u{bc}',
    '\u{a1}', '\u{ab}', '\u{bb}', '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}',
    '\u{c1}', '\u{c2}', '\u{c0}', '\u{a9}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255d}',
    '\u{a2}', '\u{a5}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252c}', '\u{251c}', '\u{2500}',
    '\u{253c}', '\u{e3}', '\u{c3}', '\u{255a}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}',
    '\u{2550}', '\u{256c}', '\u{a4}', '\u{f0}', '\u{d0}', '\u{ca}', '\u{cb}', '\u{c8}', '\u{131}',
    '\u{cd}', '\u{ce}', '\u{cf}', '\u{2518}', '\u{250c}', '\u{2588}', '\u{2584}', '\u{a6}',
    '\u{cc}', '\u{2580}', '\u{d3}', '\u{df}', '\u{d4}', '\u{d2}', '\u{f5}', '\u{d5}', '\u{b5}',
    '\u{fe}', '\u{de}', '\u{da}', '\u{db}', '\u{d9}', '\u{fd}', '\u{dd}', '\u{af}', '\u{b4}',
    '\u{ad}', '\u{b1}', '\u{2017}', '\u{be}', '\u{b6}', '\u{a7}', '\u{f7}', '\u{b8}', '\u{b0}',
    '\u{a8}', '\u{b7}', '\u{b9}', '\u{b3}', '\u{b2}', '\u{25a0}', '\u{a0}',
];

const CP1252_80_9F: [char; 32] = [
    '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2c6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
];

const DIN66003: [(u8, char); 8] = [
    (b'@', '§'),
    (b'[', 'Ä'),
    (b'\\', 'Ö'),
    (b']', 'Ü'),
    (b'{', 'ä'),
    (b'|', 'ö'),
    (b'}', 'ü'),
    (b'~', 'ß'),
];

/// Guesses the character set: UTF-8 when valid and at least as plausible,
/// otherwise CP850 unless the text reads more plausibly as Windows-1252 (see [`plausibility`]; a tie
/// stays with CP850, the standard). DIN 66003 is never guessed (it is
/// plain ASCII and only recognisable by context); pass it explicitly.
pub fn detect(bytes: &[u8]) -> Encoding {
    if bytes.iter().all(u8::is_ascii) {
        return Encoding::Cp850;
    }
    let cp850 = plausibility(&decode(bytes, Encoding::Cp850));
    let latin = plausibility(&decode(bytes, Encoding::Windows1252));
    // Valid UTF-8 alone is no proof: a short CP850 text like „±äÜä“
    // (F1 84 9A 84) is valid UTF-8 too – one odd character.
    if let Ok(text) = std::str::from_utf8(bytes) {
        if text.starts_with('\u{feff}') || plausibility(text) >= cp850.max(latin) {
            return Encoding::Utf8;
        }
    }
    if latin > cp850 {
        Encoding::Windows1252
    } else {
        Encoding::Cp850
    }
}

/// How plausible the non-ASCII characters of a German LV text are: umlauts
/// and ß score high, „²“/„³“ right after „m“ too (m², m³ – in CP850 they are
/// the bytes of „ý“/„ü“ in Windows-1252, so counting umlaut bytes alone read
/// a CP850 file with „m³“ as „mü“), typographic characters a little; box
/// drawing characters and C1 control codes count against.
fn plausibility(text: &str) -> i64 {
    let chars: Vec<char> = text.chars().collect();
    let mut score = 0;
    for (index, &character) in chars.iter().enumerate() {
        if character.is_ascii() {
            continue;
        }
        let before = index.checked_sub(1).map(|before| chars[before]);
        let after = chars.get(index + 1).copied();
        // Where a unit exponent stands: after „m“ (m², cm³), no letter after.
        let unit = before.is_some_and(|before| before == 'm' || before == 'M')
            && !after.is_some_and(char::is_alphabetic);
        score += match character {
            '²' | '³' if unit => 4,
            'ä' | 'ö' | 'ü' | 'Ä' | 'Ö' | 'Ü' | 'ß' if unit => 1,
            'ä' | 'ö' | 'ü' | 'Ä' | 'Ö' | 'Ü' | 'ß' => 3,
            '§' | '°' | 'µ' | '€' | '½' | '¼' | '¾' | '±' | '×' | 'Ø' | 'ø' | '–' | '—' | '„'
            | '“' | '”' | '‚' | '‘' | '’' | '…' | '•' | '·' | '\u{a0}' | 'é' | 'è' | 'ê' | 'á'
            | 'à' | 'â' | 'ç' | 'ñ' | 'ó' | 'ô' | 'í' | 'ú' | 'É' => 1,
            '\u{80}'..='\u{9f}' | '\u{2500}'..='\u{25ff}' => -4,
            _ => -1,
        };
    }
    score
}

pub fn decode(bytes: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Utf8 => {
            let text = String::from_utf8_lossy(bytes);
            text.strip_prefix('\u{feff}').unwrap_or(&text).to_string()
        }
        Encoding::Cp850 => bytes
            .iter()
            .map(|&b| {
                if b < 0x80 {
                    b as char
                } else {
                    CP850_HIGH[(b - 0x80) as usize]
                }
            })
            .collect(),
        Encoding::Windows1252 => bytes
            .iter()
            .map(|&b| match b {
                0x80..=0x9f => CP1252_80_9F[(b - 0x80) as usize],
                _ => b as char,
            })
            .collect(),
        Encoding::Din66003 => bytes
            .iter()
            .map(|&b| {
                DIN66003
                    .iter()
                    .find(|(code, _)| *code == b)
                    .map(|(_, c)| *c)
                    .unwrap_or(if b < 0x80 { b as char } else { '\u{fffd}' })
            })
            .collect(),
    }
}

/// Encodes text; characters the set cannot hold become `?`.
pub fn encode(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Cp850 => text
            .chars()
            .map(|c| {
                if c.is_ascii() {
                    c as u8
                } else {
                    CP850_HIGH
                        .iter()
                        .position(|&h| h == c)
                        .map(|i| 0x80 + i as u8)
                        .unwrap_or(b'?')
                }
            })
            .collect(),
        Encoding::Windows1252 => text
            .chars()
            .map(|c| match c as u32 {
                0..=0x7f | 0xa0..=0xff => c as u32 as u8,
                _ => CP1252_80_9F
                    .iter()
                    .position(|&h| h == c)
                    .map(|i| 0x80 + i as u8)
                    .unwrap_or(b'?'),
            })
            .collect(),
        Encoding::Din66003 => text
            .chars()
            .map(|c| {
                if let Some((code, _)) = DIN66003.iter().find(|(_, d)| *d == c) {
                    *code
                } else if c.is_ascii() && !DIN66003.iter().any(|(code, _)| *code == c as u8) {
                    c as u8
                } else {
                    b'?'
                }
            })
            .collect(),
    }
}
