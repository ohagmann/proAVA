//! Small element tree with namespace-free local names, built with quick-xml.

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

use super::GaebXmlError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|element| element.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.elements().filter(move |element| element.name == name)
    }

    /// Concatenated character data of this element and its descendants.
    pub fn text(&self) -> String {
        let mut out = String::new();
        collect_text(self, &mut out);
        out
    }

    /// Trimmed text of a direct child, None when missing or empty.
    pub fn child_text(&self, name: &str) -> Option<String> {
        let text = self.child(name)?.text();
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    }

    pub fn path(&self, names: &[&str]) -> Option<&Element> {
        let mut current = self;
        for name in names {
            current = current.child(name)?;
        }
        Some(current)
    }
}

fn collect_text(element: &Element, out: &mut String) {
    for node in &element.children {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Element(child) => collect_text(child, out),
        }
    }
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Turns file bytes into text: UTF-8 (with or without BOM) or, when declared
/// or when the bytes are not valid UTF-8, Windows-1252/ISO-8859-1.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_ascii_lowercase();
    let declared_single_byte = [
        "iso-8859-1",
        "iso-8859-15",
        "windows-1252",
        "latin1",
        "cp1252",
    ]
    .iter()
    .any(|name| {
        head.contains(&format!("encoding=\"{name}\""))
            || head.contains(&format!("encoding='{name}'"))
    });
    if !declared_single_byte {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return text.to_string();
        }
    }
    bytes.iter().map(|&byte| cp1252(byte)).collect()
}

fn cp1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9F => HIGH[(byte - 0x80) as usize],
        _ => byte as char,
    }
}

pub(crate) fn parse(text: &str) -> Result<Element, GaebXmlError> {
    let mut reader = Reader::from_str(text);
    let config = reader.config_mut();
    config.check_end_names = true;
    config.allow_dangling_amp = true;

    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let position = |reader: &Reader<&[u8]>| reader.buffer_position();

    loop {
        let event = reader.read_event().map_err(|error| GaebXmlError::Syntax {
            position: position(&reader),
            message: error.to_string(),
        })?;
        match event {
            Event::Start(start) => {
                stack.push(element_from(&start, &reader)?);
            }
            Event::Empty(start) => {
                let element = element_from(&start, &reader)?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Node::Element(element)),
                    None => {
                        root = Some(element);
                        break;
                    }
                }
            }
            Event::End(_) => {
                let element = stack.pop().ok_or_else(|| GaebXmlError::Syntax {
                    position: position(&reader),
                    message: "Endtag ohne Starttag".into(),
                })?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Node::Element(element)),
                    None => {
                        root = Some(element);
                        break;
                    }
                }
            }
            Event::Text(text) => {
                if let Some(parent) = stack.last_mut() {
                    push_text(parent, &text.xml10_content());
                }
            }
            Event::CData(data) => {
                if let Some(parent) = stack.last_mut() {
                    push_text(parent, &data.into_inner());
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(parent) = stack.last_mut() {
                    let resolved = match reference.resolve_char_ref() {
                        Ok(Some(ch)) => ch.to_string(),
                        _ => match &*reference {
                            "lt" => "<".into(),
                            "gt" => ">".into(),
                            "amp" => "&".into(),
                            "apos" => "'".into(),
                            "quot" => "\"".into(),
                            "nbsp" => "\u{a0}".into(),
                            other => format!("&{other};"),
                        },
                    };
                    push_text(parent, &resolved);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(GaebXmlError::Syntax {
            position: position(&reader),
            message: format!(
                "Element <{}> wurde nicht geschlossen",
                stack[stack.len() - 1].name
            ),
        });
    }
    root.ok_or(GaebXmlError::NotGaeb(
        "die Datei enthält kein XML-Element".into(),
    ))
}

/// Text without control characters XML 1.0 does not allow (a lenient
/// parse of a damaged file, `&#1;`): they would end up in the project.
fn clean(text: &str) -> std::borrow::Cow<'_, str> {
    let stray = |character: char| matches!(character, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}');
    if text.chars().any(stray) {
        std::borrow::Cow::Owned(
            text.chars()
                .filter(|character| !stray(*character))
                .collect(),
        )
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

fn push_text(parent: &mut Element, text: &str) {
    let text = &*clean(text);
    if let Some(Node::Text(last)) = parent.children.last_mut() {
        last.push_str(text);
    } else {
        parent.children.push(Node::Text(text.to_string()));
    }
}

fn element_from(
    start: &quick_xml::events::BytesStart<'_>,
    reader: &Reader<&[u8]>,
) -> Result<Element, GaebXmlError> {
    let name = local(start.name().as_ref()).to_string();
    let mut attrs = Vec::new();
    for attr in start.attributes().with_checks(false) {
        let attr = attr.map_err(|error| GaebXmlError::Syntax {
            position: reader.buffer_position(),
            message: error.to_string(),
        })?;
        let key = attr.key.as_ref();
        if key == "xmlns" || key.starts_with("xmlns:") {
            attrs.push((key.to_string(), attr.value.to_string()));
            continue;
        }
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| attr.value.to_string());
        attrs.push((local(key).to_string(), clean(&value).into_owned()));
    }
    Ok(Element {
        name,
        attrs,
        children: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::{parse, Node};

    /// Steuerzeichen, die XML 1.0 nicht erlaubt (beschädigte Datei,
    /// `&#11;`), kommen nicht ins Modell.
    #[test]
    fn control_characters_do_not_reach_the_model() {
        let root = parse("<Text a=\"x&#1;y\">A&#11;B\u{c}C\tD</Text>").unwrap();
        assert_eq!(root.attr("a"), Some("xy"));
        let text: String = root
            .children
            .iter()
            .filter_map(|node| match node {
                Node::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "ABC\tD");
    }
}
