use crate::core::Element::{List, Paragraph, Text};
use crate::core::{Document, Element, ListItem, ParserError, TransformerTrait};

use anyhow;
use bytes::Bytes;
use log::{debug, warn};
use lopdf::content::Content;
use lopdf::{Document as PdfDocument, Object, ObjectId};
use std::collections::BTreeMap;
use typst::{eval::Tracer, foundations::Smart};

/// Attempts to decode PDF text bytes using multiple fallback strategies
fn decode_pdf_text_robust(encoding: Option<&str>, bytes: &[u8]) -> String {
    // First try the standard PDF decoding
    let decoded_text = PdfDocument::decode_text(encoding, bytes);

    // Check if the decoding failed or returned an error message
    if decoded_text.contains("Unimplemented") ||
       decoded_text.contains("Identity-H") ||
       decoded_text.trim().is_empty() {

        debug!("Standard PDF decoding failed for encoding: {:?}, trying fallbacks", encoding);

        // Try UTF-8 decoding
        if let Ok(utf8_text) = String::from_utf8(bytes.to_vec()) {
            if utf8_text.chars().any(|c| c.is_alphanumeric() || c.is_whitespace()) {
                debug!("Successfully decoded as UTF-8");
                return utf8_text;
            }
        }

        // Try UTF-16 decoding (common in PDFs)
        if bytes.len() >= 2 && bytes.len() % 2 == 0 {
            let utf16_bytes: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
                .collect();

            if let Ok(utf16_text) = String::from_utf16(&utf16_bytes) {
                if utf16_text.chars().any(|c| c.is_alphanumeric() || c.is_whitespace()) {
                    debug!("Successfully decoded as UTF-16 BE");
                    return utf16_text;
                }
            }

            // Try UTF-16 LE
            let utf16_le_bytes: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();

            if let Ok(utf16_le_text) = String::from_utf16(&utf16_le_bytes) {
                if utf16_le_text.chars().any(|c| c.is_alphanumeric() || c.is_whitespace()) {
                    debug!("Successfully decoded as UTF-16 LE");
                    return utf16_le_text;
                }
            }
        }

        // Try Latin-1 (ISO-8859-1) as final fallback
        let latin1_text: String = bytes.iter().map(|&b| b as char).collect();
        if latin1_text.chars().any(|c| c.is_alphanumeric() || c.is_whitespace()) {
            debug!("Using Latin-1 fallback decoding");
            return latin1_text;
        }

        // If all else fails, return empty string instead of error message
        debug!("All text decoding attempts failed, returning empty string");
        return String::new();
    }

    decoded_text
}

pub struct Transformer;
impl TransformerTrait for Transformer {
    fn parse(document: &Bytes) -> anyhow::Result<Document> {
        let mut elements: Vec<Element> = Vec::new();
        let pdf_document = PdfDocument::load_mem(document)?;
        use crate::core::{ImageData, ImageDimension};
        for (_id, page_id) in pdf_document.get_pages() {
            // Extract images from page resources
            let (resources_opt, _) = pdf_document.get_page_resources(page_id);
            if let Some(resources) = resources_opt {
                if let Ok(xobjects) = resources.get(b"XObject") {
                    if let Ok(xobj_dict) = xobjects.as_dict() {
                        for (name, xobj_ref) in xobj_dict.iter() {
                            if let Ok(xobj_id) = xobj_ref.as_reference() {
                                if let Ok(xobj) = pdf_document.get_object(xobj_id) {
                                    if let Ok(dict) = xobj.as_dict() {
                                        if let Ok(subtype) = dict.get(b"Subtype") {
                                            if subtype.as_name_str()? == "Image" {
                                                if let Ok(stream) = xobj.as_stream() {
                                                    let image_bytes = Bytes::from(stream.content.clone());
                                                    let image_data = ImageData::new(
                                                        image_bytes,
                                                        format!("PDF Image {}", String::from_utf8_lossy(name)),
                                                        "PDF Image".to_string(),
                                                        "png".to_string(), // Assume PNG for now
                                                        "center".to_string(),
                                                        ImageDimension::default(),
                                                    );
                                                    elements.push(Element::Image(image_data));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let objects = pdf_document.get_page_contents(page_id);
            for object_id in objects {
                let object = pdf_document.get_object(object_id)?;
                parse_object(page_id, &pdf_document, object, &mut elements)?;
            }
        }
        Ok(Document::new(elements))
    }
    fn generate(document: &Document) -> anyhow::Result<Bytes> {
        let (text, img_map) = crate::typst::generate_document(document)?;

        let world = crate::typst::ShivaWorld::new(text, img_map);
        let mut tracer = Tracer::default();

        let document = typst::compile(&world, &mut tracer).unwrap();
        let warnings = tracer.warnings();

        if !warnings.is_empty() {
            // Trowing any warnings if necessary
            for warn in warnings {
                warn!("Warning - {}", warn.message);
            }
        }

        // Converting to pdf then to bytes
        let pdf = typst_pdf::pdf(&document, Smart::Auto, None);

        let bytes = Bytes::from(pdf);
        Ok(bytes)
    }
}

fn parse_object(
    page_id: ObjectId,
    pdf_document: &PdfDocument,
    _object: &Object,
    elements: &mut Vec<Element>,
) -> anyhow::Result<()> {
    fn collect_text(
        text: &mut String,
        encoding: Option<&str>,
        operands: &[Object],
        elements: &mut Vec<Element>,
    ) -> anyhow::Result<()> {
        for operand in operands.iter() {
            debug!("2 {:?}", operand);
            match *operand {
                Object::String(ref bytes, _) => {
                    let decoded_text = decode_pdf_text_robust(encoding, bytes);
                    text.push_str(&decoded_text);
                    if bytes.len() == 1 && bytes[0] == 1 {
                        match elements.last() {
                            None => {
                                let list_element = List {
                                    elements: vec![],
                                    numbered: false,
                                };
                                elements.push(list_element);
                            }
                            Some(el) => {
                                match el {
                                    List { .. } => {
                                        let old_list = elements.pop().unwrap();
                                        // let list = old_list.list_as_ref()?;
                                        if let List {
                                            elements: list_elements,
                                            numbered,
                                        } = old_list
                                        {
                                            let mut list_item_elements = list_elements.clone();
                                            let text_element = Text {
                                                text: text.clone(),
                                                size: 8,
                                            };
                                            let new_list_item_element = ListItem {
                                                element: text_element,
                                            };
                                            list_item_elements.push(new_list_item_element);
                                            let new_list = List {
                                                elements: list_item_elements,
                                                numbered,
                                            };
                                            elements.push(new_list);
                                            text.clear();
                                        }
                                    }
                                    Paragraph { .. } => {
                                        let old_paragraph = elements.pop().unwrap();
                                        // let paragraph = old_paragraph.paragraph_as_ref()?;
                                        if let Paragraph {
                                            elements: paragraph_elements,
                                        } = old_paragraph
                                        {
                                            let mut paragraph_elements = paragraph_elements.clone();
                                            let text_element = Text {
                                                text: text.clone(),
                                                size: 8,
                                            };
                                            paragraph_elements.push(text_element);
                                            let new_paragraph = Paragraph {
                                                elements: paragraph_elements,
                                            };
                                            elements.push(new_paragraph);
                                            text.clear();

                                            let list_element = List {
                                                elements: vec![],
                                                numbered: false,
                                            };
                                            elements.push(list_element);
                                        }
                                    }
                                    _ => {
                                        let list_element = List {
                                            elements: vec![],
                                            numbered: false,
                                        };
                                        elements.push(*Box::new(list_element));
                                    }
                                }
                            }
                        }
                    }
                }
                Object::Array(ref arr) => {
                    let _ = collect_text(text, encoding, arr, elements);
                    text.push(' ');
                }
                Object::Integer(i) => {
                    if i < -100 {
                        text.push(' ');
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut text = String::new();

    let fonts = pdf_document.get_page_fonts(page_id);
    let encodings = fonts
        .into_iter()
        .map(|(name, font)| {
            let encoding = font.get_font_encoding();
            debug!("Font: {:?}, Encoding: {}", String::from_utf8_lossy(&name), encoding);
            (name, encoding)
        })
        .collect::<BTreeMap<Vec<u8>, &str>>();

    let vec = pdf_document.get_page_content(page_id)?;
    let content = Content::decode(&vec)?;
    let mut current_encoding = None;
    for operation in &content.operations {
        debug!("1 {:?}", operation.operator);
        match operation.operator.as_ref() {
            "Tm" => {
                let text_element = Text {
                    text: text.clone(),
                    size: 8,
                };
                match elements.last() {
                    None => {
                        let paragraph_element = Paragraph {
                            elements: vec![text_element],
                        };
                        elements.push(paragraph_element);
                    }
                    Some(el) => match el {
                        Paragraph { .. } => {
                            let old_paragraph = elements.pop().unwrap();
                            if let Paragraph {
                                elements: paragraph_elements,
                            } = old_paragraph
                            {
                                let mut paragraph_elements = paragraph_elements.clone();
                                paragraph_elements.push(text_element);
                                let new_paragraph = Paragraph {
                                    elements: paragraph_elements,
                                };
                                elements.push(new_paragraph);
                            }
                        }
                        _ => {
                            elements.push(text_element);
                        }
                    },
                }
                text.clear();
            }
            "Tf" => {
                let current_font = operation
                    .operands
                    .first()
                    .ok_or(ParserError::Common)?
                    .as_name()?;
                current_encoding = encodings.get(current_font).cloned();
            }
            "Tj" | "TJ" => {
                _ = collect_text(&mut text, current_encoding, &operation.operands, elements);
            }
            "ET" => {
                if !text.ends_with('\n') {
                    text.push('\n')
                }
            }
            _ => {}
        }
    }

    if !text.is_empty() {
        let text_element = Text {
            text: text.clone(),
            size: 8,
        };
        match elements.last() {
            None => {
                let paragraph_element = Paragraph {
                    elements: vec![text_element],
                };
                elements.push(*Box::new(paragraph_element));
            }
            Some(el) => {
                match el {
                    Paragraph { .. } => {
                        let old_paragraph = elements.pop().unwrap();
                        if let Paragraph {
                            elements: paragraph_elements,
                        } = old_paragraph
                        {
                            let mut paragraph_elements = paragraph_elements.clone();
                            paragraph_elements.push(text_element);
                            let new_paragraph = Paragraph {
                                elements: paragraph_elements,
                            };
                            elements.push(*Box::new(new_paragraph));
                        }
                    }
                    List { .. } => {
                        let old_list = elements.pop().unwrap();
                        // let list = old_list.list_as_ref()?;
                        if let List {
                            elements: list_elements,
                            numbered,
                        } = old_list
                        {
                            let mut list_item_elements = list_elements.clone();
                            let new_list_item_element = ListItem {
                                element: text_element,
                            };
                            list_item_elements.push(new_list_item_element);
                            let new_list = List {
                                elements: list_item_elements,
                                numbered,
                            };
                            elements.push(*Box::new(new_list));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::core::*;
    use crate::pdf::Transformer;
    use crate::{markdown, pdf};
    use bytes::Bytes;
    use log::{debug, info};
    use std::collections::HashMap;

    #[test]
    fn test() -> anyhow::Result<()> {
        let pdf = std::fs::read("test/data/document.pdf")?;
        let pdf_bytes = Bytes::from(pdf);
        let parsed = Transformer::parse(&pdf_bytes);
        assert!(parsed.is_ok());
        let parsed_document = parsed.unwrap();
        info!("==========================");
        info!("{:?}", parsed_document);
        info!("==========================");
        let generated_result = Transformer::generate(&parsed_document)?;
        std::fs::write("test/data/generated.pdf", generated_result)?;
        Ok(())
    }

    #[test]
    fn test_md() -> anyhow::Result<()> {
        let document = std::fs::read("test/data/document.md")?;
        let documents_bytes = Bytes::from(document);
        let parsed_document = markdown::Transformer::parse_with_loader(
            &documents_bytes,
            disk_image_loader("test/data"),
        )?;
        debug!("==========================");
        debug!("{:?}", parsed_document);
        debug!("==========================");
        let generated_result = Transformer::generate(&parsed_document)?;
        std::fs::write("test/data/generated.pdf", generated_result)?;
        Ok(())
    }

    #[test]
    fn test_list() -> anyhow::Result<()> {
        let document = std::fs::read("test/data/document.md")?;
        let documents_bytes = Bytes::from(document);
        let mut images = HashMap::new();
        let image_bytes = std::fs::read("test/data/picture.png")?;
        let image_bytes = Bytes::from(image_bytes);
        images.insert("image0.png".to_string(), image_bytes);
        let parsed = markdown::Transformer::parse_with_loader(
            &documents_bytes,
            disk_image_loader("test/data"),
        );
        assert!(parsed.is_ok());
        let mut parsed_document = parsed.unwrap();
        debug!("==========================");
        debug!("{:?}", parsed_document);
        debug!("==========================");
        parsed_document.set_page_header(vec![Element::Text {
            text: "header".to_string(),
            size: 10,
        }]);

        parsed_document.set_page_footer(vec![Element::Text {
            text: "footer".to_string(),
            size: 10,
        }]);
        let generated_result = Transformer::generate(&parsed_document);
        assert!(generated_result.is_ok());
        std::fs::write("test/data/typst.pdf", generated_result.unwrap())?;

        Ok(())
    }

    #[test]
    fn test_hyperlink_generation() -> anyhow::Result<()> {
        use Element::*;
        let elements = vec![
            Paragraph {
                elements: vec![
                    Text {
                        text: "Line 1".to_owned(),
                        size: 8,
                    },
                    Text {
                        text: "Line 2".to_owned(),
                        size: 8,
                    },
                    Text {
                        text: "Line 3".to_owned(),
                        size: 8,
                    },
                ],
            },
            Hyperlink {
                title: "Example".to_owned(),
                url: "https://www.example.com".to_owned(),
                alt: "Example Site".to_owned(),
                size: 8,
            },
            Hyperlink {
                title: "GitHub".to_owned(),
                url: "https://www.github.com".to_owned(),
                alt: "GitHub".to_owned(),
                size: 8,
            },
        ];
        let document = Document::new(elements);

        debug!("==========================");
        debug!("{:?}", document);
        debug!("==========================");

        let generated_result = Transformer::generate(&document);

        assert!(generated_result.is_ok());

        std::fs::write(
            "test/data/generated_hyperlink.pdf",
            generated_result.unwrap(),
        )?;

        Ok(())
    }

    #[test]
    fn simple_test() {
        let content = std::fs::read("test/data/test.txt").unwrap();
        let md_content = String::from_utf8(content).unwrap();
        let input_bytes = Bytes::from(md_content);
        let document = markdown::Transformer::parse(&input_bytes).unwrap();
        let output_bytes = pdf::Transformer::generate(&document).unwrap().to_vec();

        std::fs::write("test/data/test.pdf", output_bytes).unwrap();
    }
}
