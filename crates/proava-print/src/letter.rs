//! Letters to bidders (Schriftverkehr): one or many letters in one PDF,
//! each starting on a new page. Layout after DIN 5008 (Form B, simplified).

use serde_json::{json, Value};

use crate::model::{PrintImage, TextBlock};
use crate::view::{self, Files};
use crate::PrintError;

const LETTER_TEMPLATE: &str = include_str!("../templates/letter.typ");

/// One letter.
#[derive(Debug, Clone, Default)]
pub struct PrintLetter {
    /// Absender (planner) – lines of the letterhead.
    pub sender: Vec<String>,
    /// Rücksendeangabe über dem Anschriftfeld, one line.
    pub return_line: String,
    pub recipient: Vec<String>,
    /// (Bezeichnung, Wert) right of the address, e.g. („Datum“, „17.11.2026“).
    pub references: Vec<(String, String)>,
    pub subject: String,
    pub body: Vec<TextBlock>,
    /// Lines under the greeting (Name of the planner).
    pub signature: Vec<String>,
}

/// Letters in one PDF (Serienbrief) with the planner's logo.
#[derive(Debug, Clone, Default)]
pub struct PrintLetters {
    pub title: String,
    pub logo: Option<PrintImage>,
    pub letters: Vec<PrintLetter>,
    /// Only the footer is used: letters have their own letterhead (DIN 5008).
    /// „Seite {seite} von {seiten}“ counts the pages of each letter.
    pub frame: crate::frame::PrintFrame,
    /// Druckstil and accent colour (letters never get a Blattrahmen).
    pub look: crate::PrintLook,
}

/// The PDF and its page count.
pub fn render_letters_pdf(letters: &PrintLetters) -> Result<(Vec<u8>, usize), PrintError> {
    let world = letters_world(letters)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

pub(crate) fn letters_world(
    letters: &PrintLetters,
) -> Result<crate::world::PrintWorld, PrintError> {
    if letters.letters.is_empty() {
        return Err(PrintError::Data(
            "Kein Empfänger für das Schreiben.".to_string(),
        ));
    }
    let mut files = Files::default();
    let data = data(letters, &mut files);
    crate::build_world(LETTER_TEMPLATE, files, &data)
}

fn data(letters: &PrintLetters, files: &mut Files) -> Value {
    let logo = letters.logo.as_ref().map(|logo| files.add_image(logo));
    let items: Vec<Value> = letters
        .letters
        .iter()
        .map(|letter| {
            json!({
                // PDF-Lesezeichen: one per letter, the recipient's first line.
                "bm": view::bookmark(1, &[letter.recipient.first().map(String::as_str).unwrap_or(&letter.subject)]),
                "sender": letter.sender,
                "return_line": letter.return_line,
                "recipient": letter.recipient,
                "references": letter.references.iter().map(|(label, value)| json!({"label": label, "value": value})).collect::<Vec<_>>(),
                "subject": letter.subject,
                "body": view::blocks(&letter.body, files),
                "signature": letter.signature,
            })
        })
        .collect();
    json!({
        "title": letters.title,
        "logo": logo.clone(),
        "letters": items,
        "header": json!({
            "frame": crate::frame::frame_json(&letters.frame, &[("dokumentart", letters.title.clone())]),
            "logo": logo,
            "look": view::look_json(&crate::PrintLook { border: crate::PageBorder::None, ..letters.look.clone() }),
        }),
    })
}
