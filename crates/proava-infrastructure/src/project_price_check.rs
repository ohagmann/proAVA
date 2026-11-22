//! GP-Prüfung of an imported position in `projekt.xml` (schema 2, optional –
//! older files simply have none): GP laut Datei and errechnet in cents.
//!
//! ```xml
//! <price-check stated="1200" computed="1000"/>
//! ```

use proava_domain::{Money, Position, PriceCheck};

use crate::xml_tree::Element;

/// Adds the mark of `position` to its element.
pub(crate) fn with_price_check(mut element: Element, position: &Position) -> Element {
    if let Some(check) = position.price_check() {
        element.push(
            Element::new("price-check")
                .attr("stated", check.stated.cents().to_string())
                .attr("computed", check.computed.cents().to_string()),
        );
    }
    element
}

/// Reads the mark of a `<position>` element.
pub(crate) fn read_price_check(node: &Element, position: &mut Position) -> Result<(), String> {
    let Some(check) = node.first("price-check") else {
        return Ok(());
    };
    let cents = |name: &str| {
        let text = check.text(name);
        text.trim()
            .parse::<i64>()
            .map(Money::from_cents)
            .map_err(|_| format!("GP-Prüfung: „{text}“ ist kein Betrag in Cent."))
    };
    position.set_price_check(Some(PriceCheck {
        stated: cents("stated")?,
        computed: cents("computed")?,
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use proava_domain::{Decimal, Lv, OzMask, Project, ProjectName};

    use super::*;

    #[test]
    fn the_mark_survives_saving_and_older_files_have_none() {
        let mut lv = Lv::new("CSV", OzMask::parse("11.PPPP").unwrap());
        let mut marked = Position::new(
            "01.0010",
            "Regler",
            Decimal::from(2),
            "St",
            Some(Decimal::from(10)),
        );
        marked.set_price_check(Some(PriceCheck {
            stated: Money::from_cents(2400),
            computed: Money::from_cents(2000),
        }));
        lv.push_position(marked);
        lv.push_position(Position::new("01.0020", "Fühler", Decimal::ONE, "St", None));
        let project = Project::new(ProjectName::parse("P").unwrap(), "2026-11-18", lv);
        let xml = crate::project_xml::write_project_xml(&project).unwrap();
        assert!(
            xml.contains(r#"<price-check stated="2400" computed="2000"/>"#),
            "{xml}"
        );
        let back = crate::project_xml::read_project_xml(&xml, |_| None).unwrap();
        let checks: Vec<Option<PriceCheck>> = back
            .lv()
            .positions()
            .map(|position| position.price_check().copied())
            .collect();
        assert_eq!(
            checks,
            [
                Some(PriceCheck {
                    stated: Money::from_cents(2400),
                    computed: Money::from_cents(2000)
                }),
                None
            ]
        );
    }
}
