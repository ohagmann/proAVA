//! `projekt.xml` inside the project container (schema 2) and the plain XML
//! files of schema 1.

use proava_domain::{
    Address, Align, Award, Bid, BidComplement, BidId, BidPrice, Cell, ComplementKind, Contact,
    ContractTerms, CostCatalog, CostStage, Decimal, Deduction, Discount, EditStatus, InviteStatus,
    InvitedBidder, InviteeId, Lv, LvGroup, LvPurpose, LvRow, Money, Note, OzMask,
    PerformanceDescription, Position, PositionKind, PriceComponent, Project, ProjectName, RichText,
    RowId, Run, Spacer, SubDescription, Surcharge, SurchargeKind, SurchargeReference, Table,
    TextAnchor, TextBlock, TextComplement, WorkCatalog,
};

use crate::project_billing;
use crate::project_stlb;
use crate::xml_tree::{read_document, write_document, Element};

pub const ROOT: &str = "proAVA";

/// Name of the frozen preview of a PDF clip inside the image folder of the
/// container (`bilder/vorschau/<file>.png`; image names never hold a slash).
pub fn frozen_preview_name(file: &str) -> String {
    format!("vorschau/{file}.png")
}

/// PDF clips whose PDF is the same as that of an earlier clip: file → file
/// of the earlier one. The container holds such a PDF only once (several
/// parts of one topology do not make the project several times larger).
pub fn shared_pdfs(project: &Project) -> std::collections::HashMap<String, String> {
    let mut shared = std::collections::HashMap::new();
    let clips: Vec<&proava_domain::ImageAsset> = project
        .images()
        .iter()
        .filter(|image| image.clip.is_some())
        .collect();
    for (index, image) in clips.iter().enumerate() {
        if let Some(first) = clips[..index].iter().find(|earlier| {
            earlier.bytes.len() == image.bytes.len() && earlier.bytes == image.bytes
        }) {
            // Point at the file that is really written.
            let target = shared
                .get(&first.file)
                .cloned()
                .unwrap_or_else(|| first.file.clone());
            shared.insert(image.file.clone(), target);
        }
    }
    shared
}

/// `<pdf-clip>` of an image: page, page size and rectangle in points (from
/// the top-left corner), source, preview choices; `pdf` = the clip whose
/// file holds the same PDF.
fn pdf_clip_element(file: &str, clip: &proava_domain::PdfClip, pdf: Option<&String>) -> Element {
    let frozen = clip.preview_frozen && clip.frozen_png.is_some();
    Element::new("pdf-clip")
        .attr("page", clip.page.to_string())
        .attr("page-width", clip.page_width.to_string())
        .attr("page-height", clip.page_height.to_string())
        .attr("left", clip.rect.left.to_string())
        .attr("top", clip.rect.top.to_string())
        .attr("width", clip.rect.width.to_string())
        .attr("height", clip.rect.height.to_string())
        .attr("source", clip.source.as_str())
        .flag("preview-hidden", clip.preview_hidden)
        .flag("preview-frozen", frozen)
        .attr_opt("preview", frozen.then(|| frozen_preview_name(file)))
        .attr_opt("pdf", pdf.map(String::as_str))
}

fn pdf_clip_of(node: &Element) -> Result<proava_domain::PdfClip, String> {
    let value = |key: &str| -> Result<f32, String> {
        number::<f32>(node, key)?
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("<pdf-clip> {key} fehlt."))
    };
    let page_size = (value("page-width")?, value("page-height")?);
    let rect = proava_domain::ClipRect::new(
        value("left")?,
        value("top")?,
        value("width")?,
        value("height")?,
    );
    let mut clip = proava_domain::PdfClip::new(
        number::<u32>(node, "page")?.unwrap_or(1),
        page_size,
        rect,
        node.text("source"),
    );
    clip.preview_hidden = node.is_set("preview-hidden");
    clip.preview_frozen = node.is_set("preview-frozen");
    Ok(clip)
}
/// Folder of the filed letters (PDF) in the project container.
pub const CORRESPONDENCE_FOLDER: &str = "schriftverkehr/";
/// Folder of the images in the project container.
pub const IMAGE_FOLDER: &str = "bilder/";
pub const SCHEMA: u32 = 2;

const DEFAULT_LV_TITLE: &str = "Leistungsverzeichnis";

pub fn write_project_xml(project: &Project) -> Result<String, String> {
    let mut root = Element::new(ROOT).attr("schema", SCHEMA.to_string());
    root.push(
        Element::new("project")
            .attr("name", project.name().as_str())
            .attr("created", project.created_iso())
            .attr("active-lv", project.active_lv().to_string()),
    );
    root.push(info_element(project));
    for (index, lv) in project.lvs().enumerate() {
        let mut element = lv_element(lv);
        let mut bids =
            Element::new("bids").attr("next-id", project.next_bid_id_of(index).0.to_string());
        for bid in project.bids_of(index) {
            bids.push(bid_element(bid));
        }
        element.push(bids);
        if !project.invitees_of(index).is_empty() {
            let mut invitees = Element::new("invitees");
            for invitee in project.invitees_of(index) {
                invitees.push(invitee_element(invitee));
            }
            element.push(invitees);
        }
        project_billing::write(project, index, &mut element);
        crate::project_tracking::write_lv(project, index, &mut element);
        root.push(element);
    }

    if !project.letter_templates().is_empty() {
        let mut letters = Element::new("letters");
        for template in project.letter_templates() {
            let mut letter = Element::new("letter")
                .attr("kind", template.kind.key())
                .attr("subject", template.subject.as_str());
            letter.push(rich_text_element(&template.body));
            letters.push(letter);
        }
        root.push(letters);
    }

    let mut images = Element::new("images");
    let shared = shared_pdfs(project);
    for image in project.images() {
        let mut element = Element::new("image").attr("file", image.file.as_str());
        if let Some(clip) = &image.clip {
            element.push(pdf_clip_element(&image.file, clip, shared.get(&image.file)));
        }
        images.push(element);
    }
    root.push(images);

    if !project.correspondence().is_empty() {
        root.push(correspondence_element(project));
    }
    if let Some(costs) = crate::project_costs::write(project) {
        root.push(costs);
    }
    if let Some(tracking) = crate::project_tracking::write(project) {
        root.push(tracking);
    }

    write_document(&root)
}

/// Filed letters (Schriftverkehr-Archiv); the PDFs lie in
/// `schriftverkehr/<file>` of the container.
fn correspondence_element(project: &Project) -> Element {
    Element::new("correspondence").children(project.correspondence().iter().map(|entry| {
        Element::new("letter")
            .attr("id", entry.id.0.to_string())
            .attr("kind", entry.kind.key())
            .attr("file", entry.file.as_str())
            .attr("size", entry.size.to_string())
            .attr("date", entry.date.as_str())
            .attr_opt("lv-number", non_empty(&entry.lv_number))
            .attr_opt("lv-title", non_empty(&entry.lv_title))
            .attr_opt("recipient-id", entry.recipient.map(|id| id.0.to_string()))
            .attr_opt("recipient", non_empty(&entry.recipient_name))
            .attr("subject", entry.subject.as_str())
            .attr_opt("remark", non_empty(&entry.remark))
    }))
}

fn read_correspondence(
    project: &mut Project,
    node: Option<&Element>,
    entry_bytes: &mut impl FnMut(&str) -> Option<Vec<u8>>,
) -> Result<(), String> {
    let Some(node) = node else {
        return Ok(());
    };
    for item in node.all("letter") {
        let file = item.text("file");
        // Unknown kinds (a newer proAVA) are kept as „eigenes Schreiben“.
        let kind = proava_domain::CorrespondenceKind::from_key(&item.text("kind"))
            .unwrap_or(proava_domain::CorrespondenceKind::Own);
        let pdf = if file.trim().is_empty() {
            Vec::new()
        } else {
            entry_bytes(&format!("{CORRESPONDENCE_FOLDER}{file}")).unwrap_or_default()
        };
        let mut entry = proava_domain::CorrespondenceEntry::new(
            kind,
            item.text("date"),
            item.text("subject"),
            pdf,
        );
        entry.id = proava_domain::CorrespondenceId(number::<u64>(item, "id")?.unwrap_or_default());
        entry.file = file;
        if !entry.has_pdf() {
            entry.size = number::<u64>(item, "size")?.unwrap_or_default();
        }
        entry.lv_number = item.text("lv-number");
        entry.lv_title = item.text("lv-title");
        entry.recipient = number::<u64>(item, "recipient-id")?.map(InviteeId);
        entry.recipient_name = item.text("recipient");
        entry.remark = item.text("remark");
        project.restore_correspondence(entry);
    }
    Ok(())
}

/// Reads schema 1 or 2. `image_bytes` delivers the content of `bilder/<file>`
/// (called with `<file>`); images it cannot deliver are left out, filed
/// letters stay in the list without their PDF.
pub fn read_project_xml(
    text: &str,
    mut image_bytes: impl FnMut(&str) -> Option<Vec<u8>>,
) -> Result<Project, String> {
    read_project_entries(text, |name| {
        name.strip_prefix(IMAGE_FOLDER).and_then(&mut image_bytes)
    })
}

/// Like [`read_project_xml`]; `entry_bytes` delivers any entry of the
/// container by its path (`bilder/<file>`, `schriftverkehr/<file>`).
pub fn read_project_entries(
    text: &str,
    mut entry_bytes: impl FnMut(&str) -> Option<Vec<u8>>,
) -> Result<Project, String> {
    let root = read_document(text)?;
    if root.name != ROOT {
        return Err(format!(
            "Das ist keine proAVA-Projektdatei (Wurzel <{}>).",
            root.name
        ));
    }
    let schema: u32 = root
        .get("schema")
        .unwrap_or("1")
        .parse()
        .map_err(|_| format!("Unbekannte Schemaversion: {}", root.text("schema")))?;
    match schema {
        1 => read_schema_1(&root),
        2 => read_schema_2(&root, &mut entry_bytes),
        other => Err(format!(
            "Die Projektdatei hat Schema {other}; diese proAVA-Version kennt nur bis Schema {SCHEMA}. \
             Bitte proAVA aktualisieren."
        )),
    }
}

// ---------- writing ----------

fn info_element(project: &Project) -> Element {
    let info = project.info();
    Element::new("info")
        .attr("label", info.label.as_str())
        .attr_opt(
            "short-name",
            Some(info.short_name.as_str()).filter(|name| !name.is_empty()),
        )
        .attr("award-number", info.award_number.as_str())
        .attr_opt(
            "exchange-number",
            Some(info.exchange_number.clone()).filter(|number| !number.is_empty()),
        )
        .attr_opt("client-account", non_empty_text(&info.client_account))
        .attr_opt(
            "client-country-kind",
            non_empty_text(&info.client_country_kind),
        )
        .attr_opt(
            "client-country-code",
            non_empty_text(&info.client_country_code),
        )
        .attr("submission-deadline", info.submission_deadline.as_str())
        .attr("submission-place", info.submission_place.as_str())
        .attr_opt("award-procedure", non_empty(&info.award_procedure))
        .attr_opt("binding-period-end", non_empty(&info.binding_period_end))
        .attr_opt("execution-start", non_empty(&info.execution_start))
        .attr_opt("execution-end", non_empty(&info.execution_end))
        .attr_opt("execution-note", non_empty(&info.execution_note))
        .attr_opt("description", non_empty(&info.description))
        .child(address_element("owner", &info.owner))
        // Own Auftraggeber only when it is not the Bauherr (older files
        // and readers without it keep „wie Bauherr“).
        .children(
            info.own_client
                .iter()
                .map(|client| address_element("client", client)),
        )
        .child(address_element("planner", &info.planner))
        .child(
            address_element("site", &info.site)
                .attr_opt("number", non_empty_text(&info.site_number))
                .attr_opt("name", non_empty_text(&info.site_name)),
        )
        // GAEB Benachrichtigungsort, further Ausführungsorte, Bedarfsträger.
        .children(
            info.notification_site
                .iter()
                .map(|place| place_element("notification-site", place)),
        )
        .children(info.further_sites.iter().map(|site| {
            Element::new("further-site")
                .child(place_element("place", &site.site))
                .children(
                    site.notification
                        .iter()
                        .map(|place| place_element("notification", place)),
                )
        }))
        .children(
            info.requesters
                .iter()
                .map(|place| place_element("requester", place)),
        )
        .children(info.contacts.iter().map(|contact| {
            address_element("contact", &contact.address).attr("role", contact.role.as_str())
        }))
        .child(terms_element(&info.terms))
        .children(info.page_frame.as_ref().map(page_frame_element))
}

/// Own page header/footer of the project; one attribute per field line
/// (no line breaks in attributes).
fn page_frame_element(frame: &proava_domain::PageFrame) -> Element {
    let mut element = Element::new("page-frame")
        .flag("header-rule", frame.header_rule)
        .flag("footer-rule", frame.footer_rule)
        .attr("logo", frame.logo.key())
        .attr("size", frame.font_size_pt.to_string());
    for (side, fields) in [("head", &frame.header), ("foot", &frame.footer)] {
        for (place, field) in ["left", "centre", "right"].iter().zip(fields.iter()) {
            let mut lines = field.lines();
            element = element
                .attr(&format!("{side}-{place}"), lines.next().unwrap_or_default())
                .attr_opt(
                    &format!("{side}-{place}-2"),
                    lines.next().filter(|line| !line.is_empty()),
                );
        }
    }
    element
}

fn page_frame_of(node: &Element) -> proava_domain::PageFrame {
    let default = proava_domain::PageFrame::default();
    let field = |side: &str, place: &str| {
        let first = node.text(&format!("{side}-{place}"));
        match node.get(&format!("{side}-{place}-2")) {
            Some(second) if !second.is_empty() => format!("{first}\n{second}"),
            _ => first,
        }
    };
    proava_domain::PageFrame {
        header: [
            field("head", "left"),
            field("head", "centre"),
            field("head", "right"),
        ],
        footer: [
            field("foot", "left"),
            field("foot", "centre"),
            field("foot", "right"),
        ],
        header_rule: node.is_set("header-rule"),
        footer_rule: node.is_set("footer-rule"),
        logo: proava_domain::FrameLogoPlace::from_key(&node.text("logo")).unwrap_or(default.logo),
        font_size_pt: node.text("size").parse().unwrap_or(default.font_size_pt),
    }
    .normalized()
}

fn non_empty(text: &str) -> Option<&str> {
    Some(text).filter(|text| !text.is_empty())
}

pub(crate) fn terms_element(terms: &ContractTerms) -> Element {
    Element::new("terms")
        .attr_opt(
            "performance-security",
            terms
                .performance_security_percent
                .map(|value| value.to_string()),
        )
        .attr_opt(
            "warranty-security",
            terms
                .warranty_security_percent
                .map(|value| value.to_string()),
        )
        .attr_opt(
            "cash-discount",
            terms.cash_discount_percent.map(|value| value.to_string()),
        )
        .attr_opt(
            "cash-discount-days",
            terms.cash_discount_days.map(|value| value.to_string()),
        )
        .attr_opt("payment-terms", non_empty(&terms.payment_terms))
        .attr_opt("notes", non_empty(&terms.notes))
        .children(terms.deductions.iter().map(|deduction| {
            Element::new("deduction")
                .attr("label", deduction.label.as_str())
                .attr("percent", deduction.percent.to_string())
        }))
}

/// Text attribute only when filled.
fn non_empty_text(text: &str) -> Option<String> {
    Some(text.to_string()).filter(|text| !text.trim().is_empty())
}

/// A place of the award (Requester, CnstSite, NotifSite): address with
/// Kennnummer and Bezeichnung.
fn place_element(name: &str, place: &proava_domain::Place) -> Element {
    address_element(name, &place.address)
        .attr_opt("number", non_empty_text(&place.number))
        .attr_opt("name", non_empty_text(&place.name))
}

fn place_of(node: &Element) -> proava_domain::Place {
    proava_domain::Place {
        address: address_of(Some(node)),
        number: node.text("number"),
        name: node.text("name"),
    }
}

fn address_element(name: &str, address: &Address) -> Element {
    // Fields beyond the first ones only when filled (older files have none).
    let optional = |text: &str| Some(text.to_string()).filter(|text| !text.is_empty());
    Element::new(name)
        .attr("name1", address.name1.as_str())
        .attr("name2", address.name2.as_str())
        .attr_opt("name3", optional(&address.name3))
        .attr_opt("name4", optional(&address.name4))
        .attr("street", address.street.as_str())
        .attr("postal-code", address.postal_code.as_str())
        .attr("city", address.city.as_str())
        .attr_opt("country", optional(&address.country))
        .attr_opt("iln", optional(&address.iln))
        .attr_opt("contact", optional(&address.contact))
        .attr("phone", address.phone.as_str())
        .attr_opt("fax", optional(&address.fax))
        .attr("email", address.email.as_str())
        .attr_opt("vat-id", optional(&address.vat_id))
}

/// `purpose="cost"` with `cost-stage` = Kostenermittlung; anything else
/// (older files: no attribute) = Ausschreibung. An unknown Stufe (a newer
/// proAVA) reads as Kostenberechnung, like the Datenpunktrechnungen – it
/// must not become a tender LV that could go to bidders.
fn lv_purpose(lv_node: &Element) -> LvPurpose {
    match lv_node.get("purpose").map(str::trim) {
        Some("cost") => LvPurpose::Cost(
            CostStage::from_key(&lv_node.text("cost-stage")).unwrap_or(CostStage::Calculation),
        ),
        _ => LvPurpose::Tender,
    }
}

fn lv_element(lv: &Lv) -> Element {
    // A tender LV (the default) writes no purpose, so its XML stays as before.
    let cost_stage = lv.purpose().cost_stage();
    let element = Element::new("lv")
        .attr("title", lv.title())
        .attr_opt("purpose", cost_stage.map(|_| "cost".to_string()))
        .attr_opt(
            "cost-stage",
            cost_stage.map(|stage| stage.key().to_string()),
        )
        .attr_opt(
            "number",
            Some(lv.number()).filter(|number| !number.is_empty()),
        )
        .attr_opt(
            "time-label",
            Some(lv.time_label()).filter(|label| !label.is_empty()),
        )
        .flag("alphanumeric-positions", lv.has_alphanumeric_positions())
        .attr("oz-mask", lv.oz_mask().pattern())
        .attr_opt(
            "request-exported",
            lv.request_exported().map(str::to_string),
        )
        .attr_opt(
            "sent-fingerprint",
            lv.sent_fingerprint().map(str::to_string),
        )
        .attr_opt("sent-file", lv.sent_file().map(str::to_string))
        .attr("position-step", lv.position_step().to_string())
        .attr("vat-percent", lv.vat_percent().to_string())
        .attr_opt(
            "lump-sum-total",
            lv.lump_sum_total()
                .map(|total| total.to_decimal().to_string()),
        )
        .attr("currency", lv.currency())
        .attr("next-id", lv.next_row_id().0.to_string());
    let catalog = lv.cost_catalog();
    let mut element = if *catalog == CostCatalog::default() {
        element
    } else {
        element
            .attr("cost-catalog-id", catalog.id.as_str())
            .attr("cost-catalog-type", catalog.kind.as_str())
            .attr("cost-catalog-name", catalog.name.as_str())
    };
    let work_catalog = lv.work_catalog();
    if *work_catalog != WorkCatalog::default() {
        element = element
            .attr("work-catalog-id", work_catalog.id.as_str())
            .attr("work-catalog-type", work_catalog.kind.as_str())
            .attr("work-catalog-name", work_catalog.name.as_str());
    }
    for (level, label) in lv.level_labels().iter().enumerate() {
        element.push(
            Element::new("level-label")
                .attr("level", level.to_string())
                .attr("label", label.as_str()),
        );
    }
    for component in lv.price_components() {
        element.push(
            Element::new("price-component")
                .attr("label", component.label.as_str())
                .attr("kind", component.kind.as_str()),
        );
    }
    for row in lv.rows() {
        element.push(row_element(row));
    }
    element
}

/// "1.5;;2" – one value per EP column, empty for none.
fn decimals_text(values: &[Option<Decimal>]) -> Option<String> {
    (!values.is_empty()).then(|| {
        values
            .iter()
            .map(|value| value.map(|value| value.to_string()).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(";")
    })
}

fn decimals_of(node: &Element, key: &str) -> Result<Vec<Option<Decimal>>, String> {
    let Some(text) = node.get(key) else {
        return Ok(Vec::new());
    };
    text.split(';')
        .map(|part| {
            let part = part.trim();
            (!part.is_empty())
                .then(|| decimal(part, "EP-Anteil"))
                .transpose()
        })
        .collect()
}

fn row_element(row: &LvRow) -> Element {
    let id = row.id().0.to_string();
    match row {
        LvRow::Group(group) => {
            let element = Element::new("group")
                .attr("id", id)
                .attr("level", group.level().to_string())
                .attr("oz", group.ordinal())
                .attr("short", group.short_text())
                .attr_opt("cost-group", group.cost_group())
                .attr_opt("work-category", group.work_category())
                .attr_opt(
                    "lump-sum-total",
                    group
                        .lump_sum_total()
                        .map(|total| total.to_decimal().to_string()),
                );
            // Datenpunkt-Abschnitt of a Kosten-LV: the Datenpunktrechnung as
            // child `points` (older files have none).
            match group.points() {
                Some(points) => {
                    let mut node = crate::project_costs::estimate_element(points);
                    node.name = "points".to_string();
                    element.child(node)
                }
                None => element,
            }
        }
        LvRow::Note(note) => {
            let mut element = Element::new("note")
                .attr("id", id)
                .attr_opt(
                    "heading",
                    Some(note.heading()).filter(|text| !text.is_empty()),
                )
                .flag("preliminary", note.is_preliminary())
                .flag("closing", note.is_closing())
                .child(rich_text_element(note.text()));
            for complement in note.complements() {
                element.push(complement_element(complement));
            }
            let element = project_stlb::with_keys_of(element, note.catalog());
            project_stlb::with_text_origin(element, note.text_origin())
        }
        LvRow::Performance(performance) => {
            let mut element = Element::new("performance")
                .attr("id", id)
                .attr("number", performance.number())
                .attr("label", performance.label())
                .attr("short", performance.short_text())
                .child(rich_text_element(performance.long_text()));
            for complement in performance.complements() {
                element.push(complement_element(complement));
            }
            let element = project_stlb::with_keys_of(element, performance.catalog());
            project_stlb::with_text_origin(element, performance.text_origin())
        }
        // Abstandshalter: only its place and id (schema 2, additive).
        LvRow::Spacer(_) => Element::new("spacer").attr("id", id),
        LvRow::Position(position) => {
            let element = Element::new("position")
                .attr("id", id)
                .attr("oz", position.ordinal())
                .attr("short", position.short_text())
                .attr("quantity", position.quantity().to_string())
                .attr_opt(
                    "quantity-formula",
                    position.quantity_formula().map(str::to_string),
                )
                .attr("unit", position.unit())
                .attr_opt(
                    "unit-price",
                    position.unit_price().map(|price| price.to_string()),
                )
                .attr_opt(
                    "performance",
                    position.performance().map(|id| id.0.to_string()),
                )
                .attr_opt("repeats", position.repeats().map(|id| id.0.to_string()))
                .attr_opt(
                    "price-components",
                    decimals_text(position.price_components()),
                )
                .attr_opt(
                    "time-quantity",
                    position.time_quantity().map(|hours| hours.to_string()),
                );
            let mut element = kind_attributes(element, position.kind())
                .flag("lump-sum", position.is_lump_sum())
                .flag("hourly", position.is_hourly())
                .flag("stlb", position.is_stlb_placeholder())
                .flag("quantity-open", position.is_quantity_open())
                .flag("price-breakdown", position.has_price_breakdown())
                .flag("not-applicable", position.is_not_applicable())
                .flag("key-item", position.is_key_item())
                .flag("marked-for-surcharge", position.is_marked_for_surcharge())
                .attr_opt("status", position.status().key())
                .attr_opt("cost-group", position.cost_group())
                .attr_opt("work-category", position.work_category())
                .child(rich_text_element(position.long_text()));
            if let Some(surcharge) = position.surcharge() {
                // Zuschlagsposition: kind and listed positions; the
                // percentage is the unit price, the base is computed.
                element.push(
                    Element::new("surcharge")
                        .attr("kind", surcharge.kind.key())
                        // Aufteilungszeile of a Kosten-LV (older files: none).
                        .flag("split", surcharge.split)
                        .children(surcharge.references.iter().map(|reference| {
                            Element::new("ref")
                                .attr("position", reference.position.0.to_string())
                                .attr_opt(
                                    "quantity",
                                    reference.quantity.map(|quantity| quantity.to_string()),
                                )
                        })),
                );
            }
            for sub in position.sub_descriptions() {
                element.push(
                    Element::new("sub")
                        .attr("number", sub.number.as_str())
                        .attr("short", sub.short_text.as_str())
                        .attr_opt("quantity", sub.quantity.map(|qty| qty.to_string()))
                        .attr("unit", sub.unit.as_str())
                        .child(rich_text_element(&sub.long_text)),
                );
                if let Some(last) = element.children.pop() {
                    element.push(project_stlb::with_keys_of(last, &sub.catalog));
                }
            }
            if !position.takeoff().is_empty() {
                // Mengenermittlung: the same lines as the Aufmaß in <billing>.
                element.push(
                    Element::new("takeoff").children(
                        position
                            .takeoff()
                            .iter()
                            .map(project_billing::takeoff_line_element),
                    ),
                );
            }
            for complement in position.complements() {
                element.push(
                    Element::new("complement")
                        .attr("marker", complement.marker.as_str())
                        .attr(
                            "kind",
                            match complement.kind {
                                ComplementKind::Owner => "owner",
                                ComplementKind::Bidder => "bidder",
                            },
                        )
                        .attr("caption", complement.caption.as_str())
                        .attr("body", complement.body.as_str())
                        .attr("tail", complement.tail.as_str())
                        .attr_opt("anchor", complement.anchor.map(anchor_text)),
                );
            }
            crate::project_price_check::with_price_check(
                crate::project_library::with_library_data(
                    project_stlb::with_catalog_keys(element, position),
                    position,
                ),
                position,
            )
        }
    }
}

/// `<complement>` of a hint text (same form as at positions).
fn complement_element(complement: &TextComplement) -> Element {
    Element::new("complement")
        .attr("marker", complement.marker.as_str())
        .attr(
            "kind",
            match complement.kind {
                ComplementKind::Owner => "owner",
                ComplementKind::Bidder => "bidder",
            },
        )
        .attr("caption", complement.caption.as_str())
        .attr("body", complement.body.as_str())
        .attr("tail", complement.tail.as_str())
        .attr_opt("anchor", complement.anchor.map(anchor_text))
}

fn complement_of(complement: &Element) -> TextComplement {
    TextComplement {
        marker: complement.text("marker"),
        kind: match complement.get("kind") {
            Some("owner") => ComplementKind::Owner,
            _ => ComplementKind::Bidder,
        },
        caption: complement.text("caption"),
        body: complement.text("body"),
        tail: complement.text("tail"),
        anchor: complement.get("anchor").and_then(anchor_of),
    }
}

/// "3:12" = block 3, character 12; "3:line" = own line before block 3.
fn anchor_text(anchor: TextAnchor) -> String {
    if anchor.own_line {
        format!("{}:line", anchor.block)
    } else {
        format!("{}:{}", anchor.block, anchor.offset)
    }
}

/// Unreadable anchors are dropped (the complement then shows after the text).
fn anchor_of(text: &str) -> Option<TextAnchor> {
    let (block, offset) = text.trim().split_once(':')?;
    let block = block.parse().ok()?;
    Some(if offset == "line" {
        TextAnchor {
            block,
            offset: 0,
            own_line: true,
        }
    } else {
        TextAnchor {
            block,
            offset: offset.parse().ok()?,
            own_line: false,
        }
    })
}

pub(crate) fn kind_attributes(element: Element, kind: PositionKind) -> Element {
    match kind {
        PositionKind::Normal => element.attr("kind", "normal"),
        PositionKind::Provisional { with_total } => element
            .attr("kind", "provisional")
            .attr("with-total", if with_total { "1" } else { "0" }),
        PositionKind::Base { group } => element
            .attr("kind", "base")
            .attr("choice-group", group.to_string()),
        PositionKind::Alternative { group, serial } => element
            .attr("kind", "alternative")
            .attr("choice-group", group.to_string())
            .attr("serial", serial.to_string()),
        PositionKind::Supplement => element.attr("kind", "supplement"),
    }
}

fn rich_text_element(text: &RichText) -> Element {
    let mut element = Element::new("text");
    for block in text.blocks() {
        element.push(match block {
            TextBlock::Paragraph { runs, align } => runs_element("p", runs, *align),
            // `level` only for Unterpunkte: files without it are level 0.
            TextBlock::ListItem {
                ordered,
                level,
                runs,
                align,
            } => runs_element("li", runs, *align)
                .attr("ordered", if *ordered { "1" } else { "0" })
                .attr_opt("level", (*level > 0).then(|| level.to_string())),
            TextBlock::Table(table) => table_element(table),
            TextBlock::Image { file, width_mm } => Element::new("img")
                .attr("file", file.as_str())
                .attr_opt("width-mm", width_mm.map(|width| width.to_string())),
        });
    }
    element
}

/// `<table header-rows="1" widths="30;;45">` with `<tr><td align=…><r …/></td></tr>`.
fn table_element(table: &Table) -> Element {
    // Also only automatic widths (`widths=";"`): the list comes back as it was.
    let widths = (!table.column_widths_mm.is_empty()).then(|| {
        table
            .column_widths_mm
            .iter()
            .map(|width| width.map(|mm| mm.to_string()).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(";")
    });
    let mut element = Element::new("table")
        .attr_opt(
            "header-rows",
            (table.header_rows > 0).then(|| table.header_rows.to_string()),
        )
        .attr_opt("widths", widths);
    for cells in &table.rows {
        let mut row = Element::new("tr");
        for cell in cells {
            row.push(runs_element("td", &cell.runs, cell.align));
        }
        element.push(row);
    }
    element
}

fn runs_element(name: &str, runs: &[Run], align: Align) -> Element {
    let mut element =
        Element::new(name).attr_opt("align", (align != Align::Left).then(|| align.css()));
    for run in runs {
        element.push(
            Element::new("r")
                .flag("b", run.bold)
                .flag("i", run.italic)
                .flag("u", run.underline)
                .attr_opt("font", run.font.as_deref())
                .attr_opt("size", run.size_pt.map(|size| size.to_string()))
                .attr("t", run.text.as_str()),
        );
    }
    element
}

/// One bidder of the Bieterverzeichnis.
fn invitee_element(invitee: &InvitedBidder) -> Element {
    let mut element = Element::new("invitee")
        .attr("id", invitee.id.0.to_string())
        .attr("status", invitee.status.key())
        .attr_opt("contact", non_empty(&invitee.contact_person))
        .attr_opt("requested", non_empty(&invitee.requested))
        .attr_opt("received", non_empty(&invitee.received))
        .attr_opt("bid", invitee.bid.map(|bid| bid.0.to_string()))
        .flag("side-offers", invitee.side_offers)
        .attr_opt("remark", non_empty(&invitee.remark))
        .attr_opt("rejection-reason", non_empty(&invitee.rejection_reason));
    element.push(address_element("address", &invitee.address));
    element
}

fn read_invitees(
    project: &mut Project,
    index: usize,
    node: Option<&Element>,
) -> Result<(), String> {
    let Some(node) = node else {
        return Ok(());
    };
    for item in node.all("invitee") {
        let invitee = InvitedBidder {
            id: InviteeId(number::<u64>(item, "id")?.unwrap_or_default()),
            address: address_of(item.first("address")),
            contact_person: item.text("contact"),
            requested: item.text("requested"),
            status: InviteStatus::from_key(&item.text("status")).unwrap_or_default(),
            bid: number::<u64>(item, "bid")?.map(BidId),
            received: item.text("received"),
            side_offers: item.is_set("side-offers"),
            remark: item.text("remark"),
            rejection_reason: item.text("rejection-reason"),
        };
        project.add_invitee_to(index, invitee);
    }
    Ok(())
}

fn bid_element(bid: &Bid) -> Element {
    let mut element = Element::new("bid")
        .attr("id", bid.id().0.to_string())
        .attr("source", bid.source())
        .attr_opt("date", bid.bid_date())
        .attr_opt("exchange-number", non_empty_text(bid.exchange_number()))
        .flag("excluded", bid.is_excluded());
    element = match bid.discount() {
        Discount::None => element,
        Discount::Percent(percent) => element.attr("discount-percent", percent.to_string()),
        Discount::Amount(amount) => {
            element.attr("discount-amount", amount.to_decimal().to_string())
        }
    };
    element.push(address_element("bidder", bid.bidder()));

    let mut prices: Vec<(&RowId, &BidPrice)> = bid.prices().iter().collect();
    prices.sort_by_key(|(row, _)| **row);
    for (row, price) in prices {
        element.push(
            Element::new("price")
                .attr("row", row.0.to_string())
                .attr_opt(
                    "unit-price",
                    price.unit_price.map(|value| value.to_string()),
                )
                .attr_opt(
                    "stated-total",
                    price.stated_total.map(|value| value.to_string()),
                )
                // LV quantity when the bid came in (Angebotsmenge).
                .attr_opt(
                    "offered-quantity",
                    bid.offered_quantities()
                        .get(row)
                        .map(|value| value.to_string()),
                )
                .flag("not-offered", price.not_offered),
        );
    }
    for ordinal in bid.unknown_ordinals() {
        element.push(Element::new("unknown").attr("oz", ordinal.as_str()));
    }
    let mut complements: Vec<_> = bid.complements().iter().collect();
    complements.sort_by_key(|(row, _)| **row);
    for (row, list) in complements {
        for complement in list {
            element.push(
                Element::new("bidder-text")
                    .attr("row", row.0.to_string())
                    .attr("marker", complement.marker.as_str())
                    .attr("t", complement.text.as_str()),
            );
        }
    }
    if let Some(structure) = crate::project_structure::write(bid.structure()) {
        element.push(structure);
    }
    if let Some(award) = bid.award() {
        element.push(
            Element::new("award")
                .attr("date", award.date.as_str())
                .attr("total", award.total.to_decimal().to_string()),
        );
    }
    element
}

// ---------- reading schema 2 ----------

fn read_schema_2(
    root: &Element,
    entry_bytes: &mut impl FnMut(&str) -> Option<Vec<u8>>,
) -> Result<Project, String> {
    let (name, created) = project_head(root.first("project"))?;
    let lv_nodes: Vec<&Element> = root.all("lv").collect();
    let Some((first_node, other_nodes)) = lv_nodes.split_first() else {
        return Err("Die Projektdatei enthält kein Leistungsverzeichnis.".to_string());
    };

    let mut project = Project::new(name, created, lv_of(first_node)?);
    read_bids(&mut project, 0, first_node.first("bids"))?;
    read_invitees(&mut project, 0, first_node.first("invitees"))?;
    project_billing::read(&mut project, 0, first_node)?;
    for node in other_nodes {
        let index = project.add_lv(lv_of(node)?);
        read_bids(&mut project, index, node.first("bids"))?;
        read_invitees(&mut project, index, node.first("invitees"))?;
        project_billing::read(&mut project, index, node)?;
    }
    // Files from before several LVs kept the bids beside the LV.
    read_bids(&mut project, 0, root.first("bids"))?;
    if let Some(active) = root
        .first("project")
        .and_then(|head| head.get("active-lv"))
        .and_then(|value| value.parse().ok())
    {
        project.set_active_lv(active);
    }

    if let Some(letters) = root.first("letters") {
        for letter in letters.all("letter") {
            if let Some(kind) = proava_domain::LetterKind::from_key(&letter.text("kind")) {
                project.set_letter_template(proava_domain::LetterTemplate {
                    kind,
                    subject: letter.text("subject"),
                    body: rich_text_of(letter),
                });
            }
        }
    }

    if let Some(info) = root.first("info") {
        let target = project.info_mut();
        target.label = info.text("label");
        target.short_name = info.text("short-name");
        target.award_number = info.text("award-number");
        target.exchange_number = info.text("exchange-number");
        target.submission_deadline = info.text("submission-deadline");
        target.submission_place = info.text("submission-place");
        target.owner = address_of(info.first("owner"));
        target.own_client = info.first("client").map(|client| address_of(Some(client)));
        target.planner = address_of(info.first("planner"));
        target.site = address_of(info.first("site"));
        if let Some(site) = info.first("site") {
            target.site_number = site.text("number");
            target.site_name = site.text("name");
        }
        target.notification_site = info.first("notification-site").map(place_of);
        target.further_sites = info
            .all("further-site")
            .map(|node| proava_domain::ExecutionSite {
                site: node.first("place").map(place_of).unwrap_or_default(),
                notification: node.first("notification").map(place_of),
            })
            .collect();
        target.requesters = info.all("requester").map(place_of).collect();
        target.client_account = info.text("client-account");
        target.client_country_kind = info.text("client-country-kind");
        target.client_country_code = info.text("client-country-code");
        target.award_procedure = info.text("award-procedure");
        target.binding_period_end = info.text("binding-period-end");
        target.execution_start = info.text("execution-start");
        target.execution_end = info.text("execution-end");
        target.execution_note = info.text("execution-note");
        target.description = info.text("description");
        target.contacts = info
            .all("contact")
            .map(|node| Contact {
                role: node.text("role"),
                address: address_of(Some(node)),
            })
            .collect();
        if let Some(terms) = info.first("terms") {
            target.terms = terms_of(terms)?;
        }
        target.page_frame = info.first("page-frame").map(page_frame_of);
    }

    if let Some(images) = root.first("images") {
        for image in images.all("image") {
            let file = image.text("file");
            // A PDF clip may use the PDF stored with another clip.
            let stored = image
                .first("pdf-clip")
                .and_then(|clip| clip.get("pdf"))
                .filter(|other| !other.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| file.clone());
            if let Some(bytes) = entry_bytes(&format!("{IMAGE_FOLDER}{stored}")) {
                match image.first("pdf-clip") {
                    Some(node) => {
                        let mut clip = pdf_clip_of(node)?;
                        if clip.preview_frozen {
                            clip.frozen_png = entry_bytes(&format!(
                                "{IMAGE_FOLDER}{}",
                                frozen_preview_name(&file)
                            ))
                            .map(std::sync::Arc::new);
                            clip.preview_frozen = clip.frozen_png.is_some();
                        }
                        project.add_pdf_clip(&file, bytes, clip);
                    }
                    None => {
                        project.add_image(&file, bytes);
                    }
                }
            }
        }
    }
    read_correspondence(&mut project, root.first("correspondence"), entry_bytes)?;
    crate::project_costs::read(&mut project, root)?;
    crate::project_tracking::read(&mut project, root)?;

    Ok(project)
}

fn lv_of(lv_node: &Element) -> Result<Lv, String> {
    let mut lv = Lv::new(lv_title(lv_node), oz_mask(lv_node)?);
    lv.set_purpose(lv_purpose(lv_node));
    lv.set_number(lv_node.text("number"));
    lv.set_time_label(lv_node.text("time-label"));
    if let Some(id) = lv_node
        .get("cost-catalog-id")
        .filter(|id| !id.trim().is_empty())
    {
        lv.set_cost_catalog(CostCatalog {
            id: id.to_string(),
            kind: lv_node.text("cost-catalog-type"),
            name: lv_node.text("cost-catalog-name"),
        });
    }
    if let Some(id) = lv_node
        .get("work-catalog-id")
        .filter(|id| !id.trim().is_empty())
    {
        lv.set_work_catalog(WorkCatalog {
            id: id.to_string(),
            kind: lv_node.text("work-catalog-type"),
            name: lv_node.text("work-catalog-name"),
        });
    }
    lv.set_alphanumeric_positions(lv_node.is_set("alphanumeric-positions"));
    lv.set_price_components(
        lv_node
            .all("price-component")
            .map(|component| PriceComponent {
                label: component.text("label"),
                kind: component.text("kind"),
            })
            .collect(),
    );
    if let Some(step) = lv_node.get("position-step").and_then(|v| v.parse().ok()) {
        lv.set_position_step(step);
    }
    let mut labels: Vec<String> = Vec::new();
    for node in lv_node.all("level-label") {
        let level = number::<usize>(node, "level")?.unwrap_or(labels.len());
        if level < 16 {
            if labels.len() <= level {
                labels.resize(level + 1, String::new());
            }
            labels[level] = node.text("label");
        }
    }
    lv.set_level_labels(labels);
    lv.set_request_exported(lv_node.get("request-exported").map(str::to_string));
    lv.set_sent_fingerprint(lv_node.get("sent-fingerprint").map(str::to_string));
    lv.set_sent_file(lv_node.get("sent-file").map(str::to_string));
    if let Some(vat) = lv_node.get("vat-percent") {
        lv.set_vat_percent(decimal(vat, "MwSt")?);
    }
    if let Some(total) = lv_node.get("lump-sum-total") {
        lv.set_lump_sum_total(Some(Money::from_decimal(decimal(total, "Pauschalsumme")?)));
    }
    if let Some(currency) = lv_node.get("currency") {
        lv.set_currency(currency);
    }
    for node in &lv_node.children {
        let row = match node.name.as_str() {
            "group" => {
                let mut group = LvGroup::new(
                    number(node, "level")?.unwrap_or(0),
                    node.text("oz"),
                    node.text("short"),
                );
                group.set_cost_group(node.get("cost-group").map(str::to_string));
                group.set_work_category(node.get("work-category").map(str::to_string));
                if let Some(total) = node.get("lump-sum-total") {
                    group.set_lump_sum_total(Some(Money::from_decimal(decimal(
                        total,
                        "Pauschalsumme",
                    )?)));
                }
                if let Some(points) = node.first("points") {
                    group.set_points(Some(crate::project_costs::estimate_of(points)?));
                }
                LvRow::Group(group)
            }
            "note" => {
                let mut note = Note::rich(rich_text_of(node));
                note.set_heading(node.text("heading"));
                note.set_preliminary(node.is_set("preliminary"));
                note.set_closing(node.is_set("closing"));
                note.set_complements(node.all("complement").map(complement_of).collect());
                note.set_catalog(project_stlb::read_keys(node)?);
                note.set_text_origin(project_stlb::read_text_origin(node));
                LvRow::Note(note)
            }
            "performance" => {
                let mut performance = PerformanceDescription::new(
                    node.text("number"),
                    node.text("label"),
                    rich_text_of(node),
                );
                performance.set_short_text(node.text("short"));
                performance.set_catalog(project_stlb::read_keys(node)?);
                performance.set_complements(node.all("complement").map(complement_of).collect());
                performance.set_text_origin(project_stlb::read_text_origin(node));
                LvRow::Performance(performance)
            }
            "position" => LvRow::Position(position_of(node)?),
            "spacer" => LvRow::Spacer(Spacer::new()),
            _ => continue,
        };
        let row = match number::<u64>(node, "id")? {
            Some(id) => row.with_id(RowId(id)),
            None => row,
        };
        lv.push_row(row);
    }
    if let Some(next) = number::<u64>(lv_node, "next-id")? {
        lv.reserve_row_ids(RowId(next));
    }
    lv.unlink_missing_references();
    // A Datenpunkt-Abschnitt only in a Kosten-LV, in its stage (a file from
    // elsewhere may say otherwise).
    lv.fit_point_sections();
    Ok(lv)
}

fn read_bids(project: &mut Project, index: usize, bids: Option<&Element>) -> Result<(), String> {
    let Some(bids) = bids else {
        return Ok(());
    };
    for node in bids.all("bid") {
        project.add_bid_to(index, bid_of(node)?);
    }
    if let Some(next) = number::<u64>(bids, "next-id")? {
        project.reserve_bid_ids_of(index, BidId(next));
    }
    Ok(())
}

pub(crate) fn terms_of(node: &Element) -> Result<ContractTerms, String> {
    Ok(ContractTerms {
        performance_security_percent: optional_decimal(
            node,
            "performance-security",
            "Sicherheit Vertragserfüllung",
        )?,
        warranty_security_percent: optional_decimal(
            node,
            "warranty-security",
            "Sicherheit Mängelansprüche",
        )?,
        cash_discount_percent: optional_decimal(node, "cash-discount", "Skonto")?,
        cash_discount_days: number(node, "cash-discount-days")?,
        deductions: node
            .all("deduction")
            .map(|deduction| {
                Ok(Deduction {
                    label: deduction.text("label"),
                    percent: decimal(deduction.get("percent").unwrap_or("0"), "Abzug")?,
                })
            })
            .collect::<Result<_, String>>()?,
        payment_terms: node.text("payment-terms"),
        notes: node.text("notes"),
    })
}

fn project_head(head: Option<&Element>) -> Result<(ProjectName, String), String> {
    let head = head.ok_or("Die Projektdatei enthält keinen Projektnamen.")?;
    let name = head
        .get("name")
        .ok_or("Die Projektdatei enthält keinen Projektnamen.")?;
    let name = ProjectName::parse(name).map_err(|error| error.to_string())?;
    Ok((name, head.text("created")))
}

fn lv_title(node: &Element) -> String {
    node.get("title").unwrap_or(DEFAULT_LV_TITLE).to_string()
}

fn oz_mask(node: &Element) -> Result<OzMask, String> {
    OzMask::parse(node.get("oz-mask").unwrap_or(OzMask::DEFAULT_PATTERN))
}

fn position_of(node: &Element) -> Result<Position, String> {
    let quantity = match node.get("quantity") {
        Some(text) => decimal(text, "Menge")?,
        None => Decimal::ZERO,
    };
    let unit_price = optional_decimal(node, "unit-price", "Einheitspreis")?;
    let mut position = Position::new(
        node.text("oz"),
        node.text("short"),
        quantity,
        node.text("unit"),
        unit_price,
    );
    position.set_quantity_formula(node.get("quantity-formula").map(str::to_string));
    position.set_kind(kind_of(node)?);
    position.set_lump_sum(node.is_set("lump-sum"));
    position.set_hourly(node.is_set("hourly"));
    position.set_stlb_placeholder(node.is_set("stlb"));
    position.set_performance(number::<u64>(node, "performance")?.map(RowId));
    position.set_repeats(number::<u64>(node, "repeats")?.map(RowId));
    position.set_quantity_open(node.is_set("quantity-open"));
    position.set_price_breakdown(node.is_set("price-breakdown"));
    position.set_not_applicable(node.is_set("not-applicable"));
    position.set_key_item(node.is_set("key-item"));
    position.set_marked_for_surcharge(node.is_set("marked-for-surcharge"));
    if let Some(surcharge) = node.all("surcharge").next() {
        let kind = SurchargeKind::from_key(surcharge.get("kind").unwrap_or(""))
            .ok_or_else(|| format!("Unbekannte Zuschlagsart „{}“", surcharge.text("kind")))?;
        let mut references = Vec::new();
        for reference in surcharge.all("ref") {
            let Some(id) = number::<u64>(reference, "position")? else {
                continue;
            };
            references.push(SurchargeReference {
                position: RowId(id),
                quantity: optional_decimal(reference, "quantity", "Teilmenge")?,
            });
        }
        position.set_surcharge(Some(Surcharge {
            kind,
            references,
            split: surcharge.is_set("split"),
        }));
    }
    position.set_cost_group(node.get("cost-group").map(str::to_string));
    position.set_work_category(node.get("work-category").map(str::to_string));
    position.set_status(EditStatus::parse(node.get("status").unwrap_or(""))?);
    position.set_price_components(decimals_of(node, "price-components")?);
    position.set_time_quantity(optional_decimal(node, "time-quantity", "Zeitansatz")?);
    let mut subs = Vec::new();
    for sub in node.all("sub") {
        subs.push(SubDescription {
            number: sub.text("number"),
            short_text: sub.text("short"),
            long_text: rich_text_of(sub),
            quantity: optional_decimal(sub, "quantity", "Menge")?,
            unit: sub.text("unit"),
            catalog: project_stlb::read_keys(sub)?,
        });
    }
    position.set_sub_descriptions(subs);
    let mut takeoff = Vec::new();
    for lines in node.all("takeoff") {
        for line in lines.all("line") {
            takeoff.push(project_billing::takeoff_line_of(line)?);
        }
    }
    position.set_takeoff(takeoff);
    position.set_complements(
        node.all("complement")
            .map(|complement| TextComplement {
                marker: complement.text("marker"),
                kind: match complement.get("kind") {
                    Some("owner") => ComplementKind::Owner,
                    _ => ComplementKind::Bidder,
                },
                caption: complement.text("caption"),
                body: complement.text("body"),
                tail: complement.text("tail"),
                anchor: complement.get("anchor").and_then(anchor_of),
            })
            .collect(),
    );
    position.set_long_text(rich_text_of(node));
    project_stlb::read_catalog_keys(node, &mut position)?;
    crate::project_library::read_library_data(node, &mut position)?;
    crate::project_price_check::read_price_check(node, &mut position)?;
    Ok(position)
}

pub(crate) fn kind_of(node: &Element) -> Result<PositionKind, String> {
    Ok(match node.get("kind").unwrap_or("normal") {
        "provisional" => PositionKind::Provisional {
            with_total: node.is_set("with-total"),
        },
        "base" => PositionKind::Base {
            group: number(node, "choice-group")?.unwrap_or(0),
        },
        "alternative" => PositionKind::Alternative {
            group: number(node, "choice-group")?.unwrap_or(0),
            serial: number(node, "serial")?.unwrap_or(0),
        },
        "supplement" => PositionKind::Supplement,
        _ => PositionKind::Normal,
    })
}

fn rich_text_of(owner: &Element) -> RichText {
    let Some(text) = owner.first("text") else {
        return RichText::default();
    };
    let blocks = text
        .children
        .iter()
        .filter_map(|block| match block.name.as_str() {
            "p" => Some(TextBlock::Paragraph {
                runs: runs_of(block),
                align: align_of(block),
            }),
            "li" => Some(TextBlock::ListItem {
                ordered: block.is_set("ordered"),
                level: block
                    .get("level")
                    .and_then(|level| level.trim().parse::<u8>().ok())
                    .unwrap_or(0)
                    .min(proava_domain::MAX_LIST_LEVEL),
                runs: runs_of(block),
                align: align_of(block),
            }),
            "table" => Some(TextBlock::Table(table_of(block))),
            "img" => Some(TextBlock::Image {
                file: block.text("file"),
                width_mm: block.get("width-mm").and_then(|width| width.parse().ok()),
            }),
            _ => None,
        })
        .collect();
    RichText::new(blocks)
}

fn align_of(block: &Element) -> Align {
    block
        .get("align")
        .and_then(Align::from_css)
        .unwrap_or_default()
}

fn table_of(block: &Element) -> Table {
    Table {
        rows: block
            .all("tr")
            .map(|row| {
                row.all("td")
                    .map(|cell| match cell.get("t") {
                        // Before formatted cells (up to 17.11.2026): text in `t`.
                        Some(text) if cell.first("r").is_none() => Cell::plain(text),
                        _ => Cell {
                            runs: runs_of(cell),
                            align: align_of(cell),
                        },
                    })
                    .collect()
            })
            .collect(),
        header_rows: block
            .get("header-rows")
            .and_then(|rows| rows.parse().ok())
            .unwrap_or(0),
        column_widths_mm: block
            .get("widths")
            .map(|widths| widths.split(';').map(|mm| mm.parse().ok()).collect())
            .unwrap_or_default(),
    }
}

fn runs_of(block: &Element) -> Vec<Run> {
    block
        .all("r")
        .map(|run| Run {
            text: run.text("t"),
            bold: run.is_set("b"),
            italic: run.is_set("i"),
            underline: run.is_set("u"),
            font: run.get("font").map(str::to_string),
            size_pt: run.get("size").and_then(|size| size.parse().ok()),
        })
        .collect()
}

fn address_of(node: Option<&Element>) -> Address {
    let Some(node) = node else {
        return Address::default();
    };
    Address {
        name1: node.text("name1"),
        name2: node.text("name2"),
        name3: node.text("name3"),
        name4: node.text("name4"),
        street: node.text("street"),
        postal_code: node.text("postal-code"),
        city: node.text("city"),
        country: node.text("country"),
        iln: node.text("iln"),
        contact: node.text("contact"),
        phone: node.text("phone"),
        fax: node.text("fax"),
        email: node.text("email"),
        vat_id: node.text("vat-id"),
    }
}

fn bid_of(node: &Element) -> Result<Bid, String> {
    let mut bid = Bid::new(address_of(node.first("bidder")), node.text("source"));
    if let Some(id) = number::<u64>(node, "id")? {
        bid = bid.with_id(BidId(id));
    }
    bid.set_bid_date(node.get("date").map(str::to_string));
    bid.set_exchange_number(node.text("exchange-number"));
    bid.set_excluded(node.is_set("excluded"));
    if let Some(percent) = optional_decimal(node, "discount-percent", "Nachlass")? {
        bid.set_discount(Discount::Percent(percent));
    } else if let Some(amount) = optional_decimal(node, "discount-amount", "Nachlass")? {
        bid.set_discount(Discount::Amount(Money::from_decimal(amount)));
    }
    for price in node.all("price") {
        let Some(row) = number::<u64>(price, "row")? else {
            continue;
        };
        bid.set_price(
            RowId(row),
            BidPrice {
                unit_price: optional_decimal(price, "unit-price", "Einheitspreis")?,
                stated_total: optional_decimal(price, "stated-total", "Gesamtpreis")?,
                not_offered: price.is_set("not-offered"),
            },
        );
        bid.set_offered_quantity(
            RowId(row),
            optional_decimal(price, "offered-quantity", "Angebotsmenge")?,
        );
    }
    bid.set_unknown_ordinals(
        node.all("unknown")
            .map(|unknown| unknown.text("oz"))
            .collect(),
    );
    let mut texts: std::collections::BTreeMap<u64, Vec<BidComplement>> = Default::default();
    for text in node.all("bidder-text") {
        let Some(row) = number::<u64>(text, "row")? else {
            continue;
        };
        texts.entry(row).or_default().push(BidComplement {
            marker: text.text("marker"),
            text: text.text("t"),
        });
    }
    for (row, list) in texts {
        bid.set_complements(RowId(row), list);
    }
    bid.set_structure(crate::project_structure::read(node.first("structure"))?);
    if let Some(award) = node.first("award") {
        bid.set_award(Some(Award {
            date: award.text("date"),
            total: Money::from_decimal(
                optional_decimal(award, "total", "Auftragssumme")?.unwrap_or_default(),
            ),
        }));
    }
    Ok(bid)
}

pub(crate) fn decimal(text: &str, what: &str) -> Result<Decimal, String> {
    text.trim()
        .parse::<Decimal>()
        .map_err(|_| format!("{what}: „{text}“ ist keine Zahl."))
}

pub(crate) fn optional_decimal(
    node: &Element,
    key: &str,
    what: &str,
) -> Result<Option<Decimal>, String> {
    node.get(key).map(|text| decimal(text, what)).transpose()
}

pub(crate) fn number<T: std::str::FromStr>(node: &Element, key: &str) -> Result<Option<T>, String> {
    node.get(key)
        .map(|text| {
            text.trim()
                .parse::<T>()
                .map_err(|_| format!("<{}> {key}: „{text}“ ist keine Zahl.", node.name))
        })
        .transpose()
}

// ---------- reading schema 1 ----------

/// Schema 1 (proAVA 0.2–0.4): flat attributes, prices in cents, no ids.
/// Those versions escaped every value twice on each save, so values are
/// unescaped until nothing changes; line breaks were stored raw and survive.
fn read_schema_1(root: &Element) -> Result<Project, String> {
    let (name, created) = project_head(root.first("project").map(repaired).as_ref())?;
    let lv_node = repaired(root.first("lv").unwrap_or(&Element::new("lv")));

    let mut lv = Lv::new(lv_title(&lv_node), oz_mask(&lv_node)?);
    if let Some(step) = lv_node.get("position-step").and_then(|v| v.parse().ok()) {
        lv.set_position_step(step);
    }
    for node in root
        .first("lv")
        .map(|lv| lv.children.as_slice())
        .unwrap_or_default()
    {
        let node = repaired(node);
        match node.name.as_str() {
            "group" => {
                let ordinal = node.text("oz");
                let level = ordinal
                    .split('.')
                    .filter(|piece| !piece.trim().is_empty())
                    .count()
                    .saturating_sub(1);
                lv.push_group(LvGroup::new(level, ordinal, node.text("short")));
            }
            "note" => {
                lv.push_note(Note::new(node.text("text")));
            }
            "position" => {
                let quantity = match node.get("quantity") {
                    Some(text) => decimal(text, "Menge")?,
                    None => Decimal::ZERO,
                };
                let cents = number::<i64>(&node, "unit-price-cents")?.unwrap_or(0);
                let price = (cents != 0).then(|| Decimal::new(cents, 2));
                let mut position = Position::new(
                    node.text("oz"),
                    node.text("short"),
                    quantity,
                    node.text("unit"),
                    price,
                );
                position.set_long_text(RichText::plain(&node.text("lang")));
                position.set_stlb_placeholder(node.is_set("stlb"));
                lv.push_position(position);
            }
            _ => {}
        }
    }
    Ok(Project::new(name, created, lv))
}

fn repaired(node: &Element) -> Element {
    let mut copy = Element::new(&node.name);
    copy.children = node.children.clone();
    for (key, value) in &node.attributes {
        let mut value = value.clone();
        for _ in 0..16 {
            if !value.contains('&') {
                break;
            }
            match quick_xml::escape::unescape(&value) {
                Ok(once) if once != value => value = once.into_owned(),
                _ => break,
            }
        }
        copy.attributes.push((key.clone(), value));
    }
    copy
}
