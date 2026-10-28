//! Data-validation rules in OOXML worksheets.

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::style::{DataValidation, ValidationType};
use excli_core::Sheet;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

fn corrupted(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| corrupted(format!("Invalid data validation attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| corrupted(format!("Invalid data validation value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn flag(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<bool> {
    Ok(matches!(
        attribute(tag, key)?.as_deref(),
        Some("1" | "true")
    ))
}

struct Pending {
    range: String,
    kind: String,
    operator: Option<String>,
    allow_blank: bool,
    show_error: bool,
    error_title: Option<String>,
    error_message: Option<String>,
    prompt_title: Option<String>,
    prompt_message: Option<String>,
    formula1: Option<String>,
    formula2: Option<String>,
}

impl Pending {
    fn from_tag(tag: &BytesStart<'_>, extension: bool) -> ExcliResult<Self> {
        Ok(Self {
            range: match attribute(tag, b"sqref")? {
                Some(range) => range,
                None if extension => String::new(),
                None => return Err(corrupted("Data validation has no sqref")),
            },
            kind: attribute(tag, b"type")?.unwrap_or_else(|| "none".to_owned()),
            operator: attribute(tag, b"operator")?,
            allow_blank: flag(tag, b"allowBlank")?,
            show_error: flag(tag, b"showErrorMessage")?,
            error_title: attribute(tag, b"errorTitle")?,
            error_message: attribute(tag, b"error")?,
            prompt_title: attribute(tag, b"promptTitle")?,
            prompt_message: attribute(tag, b"prompt")?,
            formula1: None,
            formula2: None,
        })
    }

    fn finish(self) -> ExcliResult<DataValidation> {
        if self.range.trim().is_empty() {
            return Err(corrupted("Data validation has no sqref"));
        }
        let validation_type = match (self.kind.as_str(), self.operator.as_deref()) {
            ("list", _) if self.formula2.is_none() => {
                self.formula1.as_ref().map(|source| ValidationType::List {
                    source: source.clone(),
                })
            }
            ("custom", _) if self.formula2.is_none() => {
                self.formula1
                    .as_ref()
                    .map(|formula| ValidationType::Custom {
                        formula: formula.clone(),
                    })
            }
            ("whole", Some("between") | None) => self
                .formula1
                .as_ref()
                .and_then(|first| first.parse::<i64>().ok())
                .zip(
                    self.formula2
                        .as_ref()
                        .and_then(|second| second.parse::<i64>().ok()),
                )
                .map(|between| ValidationType::Whole {
                    between: Some(between),
                    min: None,
                    max: None,
                }),
            ("decimal", Some("between") | None) => self
                .formula1
                .as_ref()
                .and_then(|first| first.parse::<f64>().ok())
                .zip(
                    self.formula2
                        .as_ref()
                        .and_then(|second| second.parse::<f64>().ok()),
                )
                .filter(|(first, second)| first.is_finite() && second.is_finite())
                .map(|between| ValidationType::Decimal {
                    between: Some(between),
                    min: None,
                    max: None,
                }),
            ("textLength", Some("between") | None) => self
                .formula1
                .as_ref()
                .and_then(|first| first.parse::<u32>().ok())
                .zip(
                    self.formula2
                        .as_ref()
                        .and_then(|second| second.parse::<u32>().ok()),
                )
                .map(|(min, max)| ValidationType::TextLength {
                    min: Some(min),
                    max: Some(max),
                }),
            ("date", Some("between") | None)
                if self.formula1.is_some() && self.formula2.is_some() =>
            {
                Some(ValidationType::Date {
                    min: self.formula1.clone(),
                    max: self.formula2.clone(),
                })
            }
            ("time", Some("between") | None)
                if self.formula1.is_some() && self.formula2.is_some() =>
            {
                Some(ValidationType::Time {
                    min: self.formula1.clone(),
                    max: self.formula2.clone(),
                })
            }
            _ => None,
        }
        .unwrap_or(ValidationType::Other {
            kind: self.kind,
            operator: self.operator,
            formula1: self.formula1,
            formula2: self.formula2,
        });
        Ok(DataValidation {
            range: self.range,
            validation_type,
            allow_blank: self.allow_blank,
            show_error: self.show_error,
            error_title: self.error_title,
            error_message: self.error_message,
            prompt_title: self.prompt_title,
            prompt_message: self.prompt_message,
        })
    }
}

pub(crate) fn enrich_sheet(xml: &[u8], sheet: &mut Sheet) -> ExcliResult<()> {
    let mut reader = Reader::from_reader(xml);
    let mut pending: Option<Pending> = None;
    let mut formula: Option<u8> = None;
    let mut extension_depth = 0usize;
    let mut in_sqref = false;
    loop {
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid worksheet XML: {error}")))?
        {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"extLst" => extension_depth += 1,
            Event::End(ref tag) if tag.local_name().as_ref() == b"extLst" => {
                extension_depth = extension_depth.saturating_sub(1)
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"dataValidation" => {
                if pending.is_some() {
                    return Err(corrupted("Nested data validation"));
                }
                pending = Some(Pending::from_tag(tag, extension_depth > 0)?);
            }
            Event::Empty(ref tag) if tag.local_name().as_ref() == b"dataValidation" => {
                sheet
                    .data_validations
                    .push(Pending::from_tag(tag, extension_depth > 0)?.finish()?);
            }
            Event::Start(ref tag) if pending.is_some() && tag.local_name().as_ref() == b"sqref" => {
                in_sqref = true
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"sqref" => in_sqref = false,
            Event::Start(ref tag)
                if pending.is_some() && tag.local_name().as_ref() == b"formula1" =>
            {
                formula = Some(1);
                if let Some(rule) = &mut pending {
                    rule.formula1 = Some(String::new());
                }
            }
            Event::Start(ref tag)
                if pending.is_some() && tag.local_name().as_ref() == b"formula2" =>
            {
                formula = Some(2);
                if let Some(rule) = &mut pending {
                    rule.formula2 = Some(String::new());
                }
            }
            Event::Text(ref event) if formula.is_some() => {
                let value = event
                    .unescape()
                    .map_err(|error| corrupted(format!("Invalid validation formula: {error}")))?;
                let pending = pending
                    .as_mut()
                    .ok_or_else(|| corrupted("Formula outside data validation"))?;
                if let Some(target) = if formula == Some(1) {
                    &mut pending.formula1
                } else {
                    &mut pending.formula2
                } {
                    target.push_str(&value);
                }
            }
            Event::Text(ref event) if in_sqref => {
                let value = event
                    .unescape()
                    .map_err(|error| corrupted(format!("Invalid validation sqref: {error}")))?;
                if let Some(rule) = &mut pending {
                    rule.range.push_str(&value);
                }
            }
            Event::CData(ref event) if formula.is_some() => {
                let value = std::str::from_utf8(event.as_ref())
                    .map_err(|error| corrupted(format!("Invalid validation formula: {error}")))?;
                let pending = pending
                    .as_mut()
                    .ok_or_else(|| corrupted("Formula outside data validation"))?;
                if let Some(target) = if formula == Some(1) {
                    &mut pending.formula1
                } else {
                    &mut pending.formula2
                } {
                    target.push_str(value);
                }
            }
            Event::End(ref tag)
                if matches!(tag.local_name().as_ref(), b"formula1" | b"formula2") =>
            {
                formula = None
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"dataValidation" => {
                sheet.data_validations.push(
                    pending
                        .take()
                        .ok_or_else(|| corrupted("Unexpected data validation end"))?
                        .finish()?,
                );
            }
            Event::Eof => {
                if pending.is_some() {
                    return Err(corrupted("Unclosed data validation"));
                }
                break;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use zip::ZipArchive;

    #[test]
    fn reads_list_bounds_and_unrepresentable_operator() {
        let xml = br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dataValidations count="3"><dataValidation type="list" sqref="A1:A10" allowBlank="1" showErrorMessage="1" errorTitle="Invalid"><formula1>&quot;Red,Blue&quot;</formula1></dataValidation><dataValidation type="whole" operator="between" sqref="B1:B10"><formula1>1</formula1><formula2>9</formula2></dataValidation><dataValidation type="decimal" operator="greaterThan" sqref="C1:C10"><formula1>0.5</formula1></dataValidation></dataValidations></worksheet>"#;
        let mut sheet = Sheet::new("Data");
        enrich_sheet(xml, &mut sheet).unwrap();
        assert_eq!(sheet.data_validations.len(), 3);
        assert!(
            matches!(&sheet.data_validations[0].validation_type, ValidationType::List { source } if source == "\"Red,Blue\"")
        );
        assert_eq!(sheet.data_validations[0].range, "A1:A10");
        assert!(sheet.data_validations[0].allow_blank);
        assert!(sheet.data_validations[0].show_error);
        assert!(matches!(
            sheet.data_validations[1].validation_type,
            ValidationType::Whole {
                between: Some((1, 9)),
                ..
            }
        ));
        assert!(
            matches!(&sheet.data_validations[2].validation_type, ValidationType::Other { kind, operator, formula1, .. } if kind == "decimal" && operator.as_deref() == Some("greaterThan") && formula1.as_deref() == Some("0.5"))
        );
    }

    #[test]
    fn workbook_reader_exposes_data_validations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("validation.xlsx");
        let mut workbook = excli_core::Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let part = "xl/worksheets/sheet1.xml";
        let xml = String::from_utf8(crate::ooxml_package::read_part(&mut archive, part).unwrap())
            .unwrap();
        let updated = xml.replace("</worksheet>", "<dataValidations count=\"1\"><dataValidation type=\"list\" sqref=\"A1:A10\"><formula1>\"A,B\"</formula1></dataValidation></dataValidations></worksheet>");
        assert_ne!(xml, updated);
        crate::ooxml_edit::write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([(part.to_owned(), updated.into_bytes())]),
        )
        .unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert!(
            matches!(&loaded.sheets[0].data_validations[0].validation_type, ValidationType::List { source } if source == "\"A,B\"")
        );
    }

    #[test]
    fn reads_extension_validation_with_nested_formula_and_sqref() {
        let xml = br#"<worksheet><dataValidations><dataValidation type="list" sqref="A1"><formula1>&quot;A,B&quot;</formula1></dataValidation></dataValidations><extLst><ext><x14:dataValidations xmlns:x14="urn:extension"><x14:dataValidation type="list"><x14:formula1><xm:f xmlns:xm="urn:formula">NamedRange</xm:f></x14:formula1><xm:sqref xmlns:xm="urn:formula">B1</xm:sqref></x14:dataValidation></x14:dataValidations></ext></extLst></worksheet>"#;
        let mut sheet = Sheet::new("Data");
        enrich_sheet(xml, &mut sheet).unwrap();
        assert_eq!(sheet.data_validations.len(), 2);
        assert_eq!(sheet.data_validations[1].range, "B1");
        assert!(
            matches!(&sheet.data_validations[1].validation_type, ValidationType::List { source } if source == "NamedRange")
        );
    }
}
