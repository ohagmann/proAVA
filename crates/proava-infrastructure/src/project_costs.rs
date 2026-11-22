//! Kostenschätzungen und -berechnungen in `projekt.xml` (schema 2, additive:
//! files without the element read as before, older proAVA ignore it).
//!
//! ```xml
//! <cost-estimates>
//!   <estimate id="1" stage="kostenberechnung" title="Kostenberechnung LPH 3" date="12.11.2026"
//!             author="…" remark="…" rate="450" rate-low="400" rate-high="500" rate-per-kind="1">
//!     <kind key="AI" label="Analoge Eingabe" price="400"/>
//!     <kind key="AV" label="Analogwert" price="200"/>   (virtuell folgt aus dem Kürzel;
//!                                                       ein altes virtual="1" wird überlesen)
//!     <share code="481" percent="40"/>
//!     <section name="Bestand">
//!       <share code="481" percent="45"/>   (own split; none = the estimate's)
//!       <row location="ASP01" trade="RLT" plant="TKA-2001" remark="…" counts="12;30;6;6;0;0"/>
//!     </section>
//!   </estimate>
//! </cost-estimates>
//! ```
//!
//! A Datenpunkt-Abschnitt of a Kosten-LV is the same element named `points`
//! inside its `<group>` (id 0, no title; the stage follows the LV).

use proava_domain::{
    CostEstimate, CostEstimateId, CostSection, CostShare, CostStage, Decimal, PointKind, PointRow,
    Project,
};

use crate::project_xml::{decimal, number, optional_decimal};
use crate::xml_tree::Element;

/// The element for the project, None without estimates.
pub(crate) fn write(project: &Project) -> Option<Element> {
    let estimates = project.cost_estimates();
    (!estimates.is_empty())
        .then(|| Element::new("cost-estimates").children(estimates.iter().map(estimate_element)))
}

/// The attribute only when there is text – also a text of blanks, which
/// must come back as it was (Rundlauf-Eigenschaftstest).
fn non_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

pub(crate) fn estimate_element(estimate: &CostEstimate) -> Element {
    let mut element = Element::new("estimate")
        .attr("id", estimate.id.0.to_string())
        .attr("stage", estimate.stage.key())
        .attr("title", estimate.title.as_str())
        .attr_opt("date", non_empty(&estimate.date))
        .attr_opt("author", non_empty(&estimate.author))
        .attr_opt("remark", non_empty(&estimate.remark))
        .attr("rate", estimate.rate.normalize().to_string())
        .attr_opt(
            "rate-low",
            estimate.rate_low.map(|value| value.normalize().to_string()),
        )
        .attr_opt(
            "rate-high",
            estimate
                .rate_high
                .map(|value| value.normalize().to_string()),
        )
        .flag("rate-per-kind", estimate.rate_per_kind);
    for kind in &estimate.kinds {
        element.push(
            Element::new("kind")
                .attr("key", kind.key.as_str())
                .attr_opt("label", non_empty(&kind.label))
                .attr("price", kind.unit_price.normalize().to_string()),
        );
    }
    element = element.children(estimate.shares.iter().map(share_element));
    for section in &estimate.sections {
        let mut node = Element::new("section").attr("name", section.name.as_str());
        if let Some(shares) = &section.shares {
            // An own split without lines is still an own split.
            node = node
                .flag("own-shares", true)
                .children(shares.iter().map(share_element));
        }
        for row in &section.rows {
            let mut counts = row.counts.clone();
            counts.truncate(estimate.kinds.len());
            while counts.last() == Some(&0) {
                counts.pop();
            }
            node.push(
                Element::new("row")
                    .attr_opt("location", non_empty(&row.location))
                    .attr_opt("trade", non_empty(&row.trade))
                    .attr_opt("plant", non_empty(&row.plant))
                    .attr_opt("remark", non_empty(&row.remark))
                    .attr_opt(
                        "counts",
                        (!counts.is_empty()).then(|| {
                            counts
                                .iter()
                                .map(u32::to_string)
                                .collect::<Vec<_>>()
                                .join(";")
                        }),
                    ),
            );
        }
        element.push(node);
    }
    element
}

fn share_element(share: &CostShare) -> Element {
    Element::new("share")
        .attr("code", share.code.as_str())
        .attr("percent", share.percent.normalize().to_string())
}

/// Reads the estimates of the project file.
pub(crate) fn read(project: &mut Project, root: &Element) -> Result<(), String> {
    let Some(list) = root.first("cost-estimates") else {
        return Ok(());
    };
    for node in list.all("estimate") {
        let estimate = estimate_of(node)?;
        project.add_cost_estimate(estimate);
    }
    Ok(())
}

pub(crate) fn estimate_of(node: &Element) -> Result<CostEstimate, String> {
    // An unknown stage (a newer proAVA) reads as Kostenberechnung.
    let stage = CostStage::from_key(&node.text("stage")).unwrap_or(CostStage::Calculation);
    let kinds: Vec<PointKind> = node
        .all("kind")
        .map(|kind| {
            Ok(PointKind {
                key: kind.text("key"),
                label: kind.text("label"),
                unit_price: optional_decimal(kind, "price", "Preis der DP-Art")?
                    .unwrap_or_default(),
            })
        })
        .collect::<Result<_, String>>()?;
    let mut sections = Vec::new();
    for section in node.all("section") {
        let shares = shares_of(section)?;
        let rows = section
            .all("row")
            .map(|row| {
                let mut counts: Vec<u32> = row
                    .text("counts")
                    .split(';')
                    .filter(|part| !part.trim().is_empty())
                    .map(|part| {
                        part.trim().parse::<u32>().map_err(|_| {
                            format!("Anzahl Datenpunkte: „{part}“ ist keine ganze Zahl.")
                        })
                    })
                    .collect::<Result<_, String>>()?;
                counts.resize(kinds.len(), 0);
                Ok(PointRow {
                    location: row.text("location"),
                    trade: row.text("trade"),
                    plant: row.text("plant"),
                    remark: row.text("remark"),
                    counts,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        sections.push(CostSection {
            name: section.text("name"),
            rows,
            shares: (section.is_set("own-shares") || !shares.is_empty()).then_some(shares),
        });
    }
    Ok(CostEstimate {
        id: CostEstimateId(number::<u64>(node, "id")?.unwrap_or_default()),
        stage,
        title: node.text("title"),
        date: node.text("date"),
        author: node.text("author"),
        remark: node.text("remark"),
        kinds,
        sections,
        rate: node
            .get("rate")
            .map(|text| decimal(text, "Kostenkennwert"))
            .transpose()?
            .unwrap_or_default(),
        rate_low: optional_decimal(node, "rate-low", "Kennwert von")?,
        rate_high: optional_decimal(node, "rate-high", "Kennwert bis")?,
        rate_per_kind: node.is_set("rate-per-kind"),
        shares: shares_of(node)?,
    })
}

fn shares_of(node: &Element) -> Result<Vec<CostShare>, String> {
    node.all("share")
        .map(|share| {
            Ok(CostShare {
                code: share.text("code"),
                percent: optional_decimal(share, "percent", "Anteil der Kostengruppe")?
                    .unwrap_or(Decimal::ZERO),
            })
        })
        .collect()
}
