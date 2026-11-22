//! A tiny element tree on top of quick-xml: all data lives in attributes,
//! text between elements is ignored. Values are escaped exactly once when
//! written and unescaped exactly once when read (no attribute-value
//! normalization, so line breaks survive).

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Self::default()
        }
    }

    pub fn attr(mut self, key: &str, value: impl Into<String>) -> Self {
        self.attributes.push((key.to_string(), value.into()));
        self
    }

    /// Adds the attribute only for `Some`.
    pub fn attr_opt(self, key: &str, value: Option<impl Into<String>>) -> Self {
        match value {
            Some(value) => self.attr(key, value),
            None => self,
        }
    }

    /// `key="1"` when set, nothing otherwise.
    pub fn flag(self, key: &str, set: bool) -> Self {
        if set {
            self.attr(key, "1")
        } else {
            self
        }
    }

    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }

    pub fn push(&mut self, child: Element) {
        self.children.push(child);
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    pub fn text(&self, key: &str) -> String {
        self.get(key).unwrap_or_default().to_string()
    }

    pub fn is_set(&self, key: &str) -> bool {
        matches!(self.get(key), Some("1") | Some("true"))
    }

    pub fn first(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.name == name)
    }

    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |child| child.name == name)
    }
}

pub fn write_document(root: &Element) -> Result<String, String> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))
        .map_err(xml_error)?;
    write_element(&mut writer, root)?;
    String::from_utf8(writer.into_inner()).map_err(|error| format!("XML: {error}"))
}

/// Control characters XML 1.0 cannot hold (not even as `&#1;`) are written
/// as their „Control Pictures“ (U+2400 + code, „␋“ for vertical tab) and
/// read back as what they were. Tab, line break and carriage return stay
/// (as character references). Older files with raw control characters are
/// still read as they are.
fn encode_controls(value: &str) -> std::borrow::Cow<'_, str> {
    let forbidden = |character: char| matches!(character, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}');
    if !value
        .chars()
        .any(|character| forbidden(character) || matches!(character, '\u{fffe}' | '\u{ffff}'))
    {
        return std::borrow::Cow::Borrowed(value);
    }
    std::borrow::Cow::Owned(
        value
            .chars()
            .map(|character| match character {
                character if forbidden(character) => {
                    char::from_u32(0x2400 + character as u32).unwrap_or('\u{fffd}')
                }
                // Non-characters: no XML at all.
                '\u{fffe}' | '\u{ffff}' => '\u{fffd}',
                character => character,
            })
            .collect(),
    )
}

/// The other way round: „Control Pictures“ of control characters XML
/// cannot hold become the control characters again.
fn decode_controls(value: String) -> String {
    let picture = |character: char| {
        let code = (character as u32).wrapping_sub(0x2400);
        code < 0x20 && !matches!(code, 0x9 | 0xA | 0xD)
    };
    if !value.chars().any(picture) {
        return value;
    }
    value
        .chars()
        .map(|character| {
            if picture(character) {
                char::from_u32(character as u32 - 0x2400).unwrap_or(character)
            } else {
                character
            }
        })
        .collect()
}

fn write_element(writer: &mut Writer<Vec<u8>>, element: &Element) -> Result<(), String> {
    let mut start = BytesStart::new(element.name.as_str());
    for (key, value) in &element.attributes {
        // The (&str, &str) form escapes the value itself – pass it raw.
        start.push_attribute((key.as_str(), &*encode_controls(value)));
    }
    if element.children.is_empty() {
        return writer.write_event(Event::Empty(start)).map_err(xml_error);
    }
    writer.write_event(Event::Start(start)).map_err(xml_error)?;
    for child in &element.children {
        write_element(writer, child)?;
    }
    writer
        .write_event(Event::End(BytesEnd::new(element.name.as_str())))
        .map_err(xml_error)
}

pub fn read_document(text: &str) -> Result<Element, String> {
    let mut reader = quick_xml::Reader::from_str(text);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => stack.push(element_of(&start)?),
            Ok(Event::Empty(start)) => {
                let element = element_of(&start)?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = root.or(Some(element)),
                }
            }
            Ok(Event::End(_)) => {
                let Some(element) = stack.pop() else {
                    return Err(format!(
                        "Die Projektdatei ist beschädigt (projekt.xml, Byte {}): Ende ohne Anfang.",
                        reader.buffer_position()
                    ));
                };
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = root.or(Some(element)),
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(format!(
                    "Die Projektdatei ist beschädigt (projekt.xml, Byte {}): {error}",
                    reader.error_position()
                ))
            }
        }
    }

    if !stack.is_empty() {
        return Err(
            "Die Projektdatei ist unvollständig: projekt.xml endet mitten im Inhalt.".to_string(),
        );
    }
    root.ok_or_else(|| "Die Projektdatei ist leer (projekt.xml ohne Inhalt).".to_string())
}

fn element_of(start: &BytesStart<'_>) -> Result<Element, String> {
    let mut element = Element::new(start.name().into_inner());
    for attribute in start.attributes().with_checks(false) {
        let attribute = attribute
            .map_err(|error| format!("Die Projektdatei ist beschädigt (projekt.xml): {error}"))?;
        let value = quick_xml::escape::unescape(&attribute.value)
            .map_err(|error| format!("Die Projektdatei ist beschädigt (projekt.xml): {error}"))?;
        element.attributes.push((
            attribute.key.into_inner().to_string(),
            decode_controls(value.into_owned()),
        ));
    }
    Ok(element)
}

fn xml_error(error: impl std::fmt::Display) -> String {
    format!("XML: {error}")
}

#[cfg(test)]
mod tests {
    use super::{read_document, write_document, Element};

    #[test]
    fn special_characters_are_escaped_exactly_once() {
        let tricky = "A & B < C > D \" E ' F ä ß € \n zweite Zeile\r\n\tTab  ";
        let root = Element::new("root").child(Element::new("x").attr("v", tricky));
        let text = write_document(&root).unwrap();
        assert!(text.contains("A &amp; B &lt; C"), "{text}");
        assert!(!text.contains("&amp;amp;"), "{text}");
        let back = read_document(&text).unwrap();
        assert_eq!(back, root);
        assert_eq!(back.first("x").unwrap().text("v"), tricky);
    }

    #[test]
    fn escaped_looking_text_stays_as_typed() {
        let typed = "&amp; &#10; &lt;";
        let root = Element::new("root").attr("v", typed);
        let back = read_document(&write_document(&root).unwrap()).unwrap();
        assert_eq!(back.text("v"), typed);
    }
}
