//! Read worksheet conditional-format rules without discarding unmodeled rules.

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::sheet::Sheet;
use excli_core::style::{
    CellStyle, ConditionalFormat, ConditionalFormatExtension, ConditionalFormatExtensionRule,
    ConditionalFormatRule, ConditionalOperator, ScalePoint,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::{NsReader, Reader};

const X14_NAMESPACE: &[u8] = b"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";

fn malformed(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| malformed(format!("Invalid conditional format: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| malformed(format!("Invalid conditional format: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn boolean(tag: &BytesStart<'_>, key: &[u8], default: bool) -> ExcliResult<bool> {
    match attribute(tag, key)?.as_deref() {
        None => Ok(default),
        Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        Some(value) => Err(malformed(format!(
            "Invalid conditional-format Boolean: {value}"
        ))),
    }
}

fn has_extra_attributes(tag: &BytesStart<'_>, allowed: &[&[u8]]) -> ExcliResult<bool> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| malformed(format!("Invalid conditional format: {error}")))?;
        if !allowed.contains(&item.key.as_ref()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn scale_point(tag: &BytesStart<'_>) -> ExcliResult<Option<ScalePoint>> {
    let kind = attribute(tag, b"type")?;
    let value = attribute(tag, b"val")?;
    Ok(match kind.as_deref() {
        Some("min" | "autoMin") => Some(ScalePoint::Min),
        Some("max" | "autoMax") => Some(ScalePoint::Max),
        Some("percentile") => value
            .as_deref()
            .and_then(|value| value.parse::<u8>().ok())
            .map(ScalePoint::Percentile),
        Some("percent") => value
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .map(ScalePoint::Percent),
        Some("num") => value
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .map(ScalePoint::Number),
        Some("formula") => value.map(ScalePoint::Formula),
        _ => None,
    })
}

fn cell_operator(value: &str) -> Option<ConditionalOperator> {
    match value {
        "greaterThan" => Some(ConditionalOperator::GreaterThan),
        "lessThan" => Some(ConditionalOperator::LessThan),
        "equal" => Some(ConditionalOperator::Equal),
        "notEqual" => Some(ConditionalOperator::NotEqual),
        _ => None,
    }
}

fn rule(xml: &[u8], range: &str, dxfs: &[Option<CellStyle>]) -> ExcliResult<ConditionalFormat> {
    let mut reader = Reader::from_reader(xml);
    let mut kind = None;
    let mut priority = None;
    let mut section = None;
    let mut points = Vec::new();
    let mut colors = Vec::new();
    let mut show_value = true;
    let mut icon_set = None;
    let mut reverse = false;
    let mut unmodeled = false;
    let mut dxf_id = None;
    let mut operator = None;
    let mut formulas = Vec::<String>::new();
    let mut in_formula = false;
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid conditional-format XML: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"cfRule" && kind.is_none() =>
            {
                kind = Some(attribute(tag, b"type")?.unwrap_or_default());
                priority = Some(
                    attribute(tag, b"priority")?
                        .ok_or_else(|| malformed("Conditional-format rule has no priority"))?
                        .parse::<u32>()
                        .map_err(|_| malformed("Invalid conditional-format priority"))?,
                );
                dxf_id = attribute(tag, b"dxfId")?
                    .map(|value| {
                        value
                            .parse::<usize>()
                            .map_err(|_| malformed("Invalid differential style ID"))
                    })
                    .transpose()?;
                operator = attribute(tag, b"operator")?;
                unmodeled |= if matches!(kind.as_deref(), Some("expression")) {
                    has_extra_attributes(tag, &[b"type", b"priority", b"dxfId"])?
                } else if matches!(kind.as_deref(), Some("cellIs")) {
                    has_extra_attributes(tag, &[b"type", b"priority", b"dxfId", b"operator"])?
                } else {
                    has_extra_attributes(tag, &[b"type", b"priority"])?
                };
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"formula" => {
                formulas.push(String::new());
                in_formula = true;
            }
            Event::Text(ref text) if in_formula => {
                if let Some(formula) = formulas.last_mut() {
                    formula.push_str(&text.unescape().map_err(|error| {
                        malformed(format!("Invalid conditional formula: {error}"))
                    })?);
                }
            }
            Event::CData(ref text) if in_formula => {
                if let Some(formula) = formulas.last_mut() {
                    formula.push_str(
                        std::str::from_utf8(text.as_ref())
                            .map_err(|_| malformed("Invalid conditional formula UTF-8"))?,
                    );
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"formula" => {
                in_formula = false;
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"colorScale" => {
                section = Some(b"colorScale".as_slice());
                unmodeled |= has_extra_attributes(tag, &[])?;
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"dataBar" => {
                section = Some(b"dataBar".as_slice());
                show_value = boolean(tag, b"showValue", true)?;
                unmodeled |= has_extra_attributes(tag, &[b"showValue"])?;
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"iconSet" => {
                section = Some(b"iconSet".as_slice());
                icon_set = attribute(tag, b"iconSet")?;
                reverse = boolean(tag, b"reverse", false)?;
                show_value = boolean(tag, b"showValue", true)?;
                unmodeled |= has_extra_attributes(tag, &[b"iconSet", b"reverse", b"showValue"])?;
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"cfvo" && section.is_some() =>
            {
                match scale_point(tag)? {
                    Some(point) => points.push(point),
                    None => unmodeled = true,
                }
                unmodeled |= has_extra_attributes(tag, &[b"type", b"val"])?;
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"color" && section.is_some() =>
            {
                match attribute(tag, b"rgb")? {
                    Some(color) => colors.push(color),
                    None => unmodeled = true,
                }
                unmodeled |= has_extra_attributes(tag, &[b"rgb"])?;
            }
            Event::End(ref tag)
                if section.is_some_and(|current| tag.local_name().as_ref() == current) =>
            {
                section = None;
            }
            Event::Start(_) | Event::Empty(_) => unmodeled = true,
            Event::Eof => break,
            _ => {}
        }
    }
    let kind = kind.ok_or_else(|| malformed("Missing conditional-format rule"))?;
    let raw = || ConditionalFormatRule::RawXml {
        rule_type: kind.clone(),
        xml: String::from_utf8_lossy(xml).into_owned(),
    };
    let parsed = if unmodeled {
        raw()
    } else {
        match kind.as_str() {
            "expression" if formulas.len() == 1 && !formulas[0].is_empty() => {
                match dxf_id.and_then(|id| dxfs.get(id)).and_then(Option::as_ref) {
                    Some(style) => ConditionalFormatRule::Formula {
                        formula: formulas[0].clone(),
                        style: style.clone(),
                    },
                    None => raw(),
                }
            }
            "cellIs" if formulas.len() == 1 && !formulas[0].is_empty() => {
                match (
                    operator.as_deref().and_then(cell_operator),
                    dxf_id.and_then(|id| dxfs.get(id)).and_then(Option::as_ref),
                ) {
                    (Some(operator), Some(style)) => ConditionalFormatRule::CellValue {
                        operator,
                        value: formulas[0].clone(),
                        style: style.clone(),
                    },
                    _ => raw(),
                }
            }
            "colorScale" if (2..=3).contains(&points.len()) && colors.len() == points.len() => {
                ConditionalFormatRule::ColorScale {
                    min_type: points[0].clone(),
                    mid_type: (points.len() == 3).then(|| points[1].clone()),
                    max_type: points[points.len() - 1].clone(),
                    colors: (
                        colors[0].clone(),
                        (colors.len() == 3).then(|| colors[1].clone()),
                        colors[colors.len() - 1].clone(),
                    ),
                }
            }
            "dataBar"
                if points.len() == 2
                    && matches!(points[0], ScalePoint::Min)
                    && matches!(points[1], ScalePoint::Max)
                    && colors.len() == 1 =>
            {
                ConditionalFormatRule::DataBar {
                    color: colors[0].clone(),
                    show_value,
                }
            }
            "iconSet" if points.is_empty() && icon_set.is_some() => {
                ConditionalFormatRule::IconSet {
                    icon_set: icon_set.unwrap_or_default(),
                    reverse,
                    show_value,
                }
            }
            _ => raw(),
        }
    };
    Ok(ConditionalFormat {
        range: range.to_owned(),
        priority: priority.unwrap_or_default(),
        rule: parsed,
    })
}

fn extension_group(xml: &[u8]) -> ExcliResult<ConditionalFormatExtension> {
    let mut reader = Reader::from_reader(xml);
    let mut range = String::new();
    let mut in_range = false;
    let mut rules = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid extended conditional format: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"cfRule" =>
            {
                rules.push(ConditionalFormatExtensionRule {
                    rule_type: attribute(tag, b"type")?.unwrap_or_default(),
                    priority: attribute(tag, b"priority")?
                        .map(|value| {
                            value.parse::<u32>().map_err(|_| {
                                malformed("Invalid extended conditional-format priority")
                            })
                        })
                        .transpose()?,
                    id: attribute(tag, b"id")?,
                });
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"sqref" => in_range = true,
            Event::Text(ref text) if in_range => {
                range.push_str(&text.unescape().map_err(|error| {
                    malformed(format!(
                        "Invalid extended conditional-format range: {error}"
                    ))
                })?);
            }
            Event::CData(ref text) if in_range => {
                range.push_str(
                    std::str::from_utf8(text.as_ref()).map_err(|_| {
                        malformed("Invalid extended conditional-format range UTF-8")
                    })?,
                );
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"sqref" => in_range = false,
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(ConditionalFormatExtension {
        range: range.trim().to_owned(),
        rules,
        raw_xml: std::str::from_utf8(xml)
            .map_err(|_| malformed("Invalid extended conditional-format UTF-8"))?
            .to_owned(),
    })
}

fn enrich_extensions(xml: &[u8], sheet: &mut Sheet) -> ExcliResult<()> {
    let mut reader = NsReader::from_reader(xml);
    let mut start = None;
    let mut depth = 0usize;
    loop {
        let from = reader.buffer_position() as usize;
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|error| malformed(format!("Invalid worksheet XML: {error}")))?;
        let is_x14 =
            matches!(namespace, ResolveResult::Bound(uri) if uri.as_ref() == X14_NAMESPACE);
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag)
                if is_x14 && tag.local_name().as_ref() == b"conditionalFormatting" =>
            {
                start = Some(from);
                depth = 1;
            }
            Event::Start(_) if start.is_some() => depth += 1,
            Event::End(_) if start.is_some() && depth > 1 => depth -= 1,
            Event::End(ref tag)
                if is_x14
                    && tag.local_name().as_ref() == b"conditionalFormatting"
                    && start.is_some() =>
            {
                let from = start
                    .take()
                    .ok_or_else(|| malformed("Missing extended conditional-format start"))?;
                sheet
                    .conditional_format_extensions
                    .push(extension_group(&xml[from..to])?);
                depth = 0;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn enrich_sheet(
    xml: &[u8],
    sheet: &mut Sheet,
    dxfs: &[Option<CellStyle>],
) -> ExcliResult<()> {
    let mut reader = Reader::from_reader(xml);
    let mut range = None;
    let mut rule_start = None;
    let mut rule_depth = 0usize;
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid worksheet XML: {error}")))?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"conditionalFormatting" => {
                range = attribute(tag, b"sqref")?;
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"cfRule" && range.is_some() => {
                rule_start = Some(from);
                rule_depth = 1;
            }
            Event::Start(_) if rule_start.is_some() => rule_depth += 1,
            Event::End(_) if rule_start.is_some() && rule_depth > 1 => rule_depth -= 1,
            Event::End(ref tag)
                if tag.local_name().as_ref() == b"cfRule" && rule_start.is_some() =>
            {
                let start = rule_start
                    .take()
                    .ok_or_else(|| malformed("Missing rule start"))?;
                sheet.conditional_formats.push(rule(
                    &xml[start..to],
                    range.as_deref().unwrap_or_default(),
                    dxfs,
                )?);
                rule_depth = 0;
            }
            Event::Empty(ref tag) if tag.local_name().as_ref() == b"cfRule" && range.is_some() => {
                sheet.conditional_formats.push(rule(
                    &xml[from..to],
                    range.as_deref().unwrap_or_default(),
                    dxfs,
                )?);
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"conditionalFormatting" => {
                range = None;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    enrich_extensions(xml, sheet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::Workbook;
    use std::collections::HashMap;
    use zip::ZipArchive;

    use crate::ooxml_edit::write_modified_parts;
    use crate::ooxml_package::read_part;

    #[test]
    fn reads_conditional_rules_and_retains_unmodeled_details() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("formats.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let original =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        let styles = String::from_utf8(read_part(&mut archive, "xl/styles.xml").unwrap()).unwrap();
        let styles = styles.replace("</styleSheet>", "<dxfs count=\"2\"><dxf><font><b/><color rgb=\"FFFF0000\"/></font><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFFFFF00\"/></patternFill></fill><border diagonalUp=\"1\"><left style=\"thin\"><color rgb=\"FF0000FF\"/></left></border></dxf><dxf><border><vertical style=\"thin\"/></border></dxf></dxfs></styleSheet>");
        let formatting = r#"<conditionalFormatting sqref="A1:A10"><cfRule type="colorScale" priority="1"><colorScale><cfvo type="min"/><cfvo type="percentile" val="50"/><cfvo type="max"/><color rgb="FFFF0000"/><color rgb="FFFFFF00"/><color rgb="FF00FF00"/></colorScale></cfRule><cfRule type="expression" dxfId="0" priority="2"><formula>A1&gt;5</formula></cfRule><cfRule type="cellIs" dxfId="0" priority="5" operator="greaterThan"><formula>10</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="B1:B10"><cfRule type="dataBar" priority="3"><dataBar showValue="0"><cfvo type="min"/><cfvo type="max"/><color rgb="FF4472C4"/></dataBar></cfRule><cfRule type="iconSet" priority="4"><iconSet iconSet="3TrafficLights1"><cfvo type="percent" val="0"/><cfvo type="percent" val="33"/><cfvo type="percent" val="67"/></iconSet></cfRule><cfRule type="expression" dxfId="0" priority="6" stopIfTrue="1"><formula>B1&lt;0</formula></cfRule><cfRule type="expression" dxfId="1" priority="7"><formula>B1&gt;20</formula></cfRule></conditionalFormatting>"#;
        let extended = r#"<extLst><ext uri="{78C0D931-6437-407D-A8EE-F0AAD7539E65}"><a14:conditionalFormattings xmlns:a14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" xmlns:m="http://schemas.microsoft.com/office/excel/2006/main"><a14:conditionalFormatting><a14:cfRule type="dataBar" priority="3" id="{A1111111-1111-1111-1111-111111111111}"><a14:dataBar gradient="0"><a14:cfvo type="autoMin"/><a14:cfvo type="autoMax"/><a14:fillColor rgb="FF4472C4"/></a14:dataBar></a14:cfRule><m:sqref>B1:B10</m:sqref></a14:conditionalFormatting></a14:conditionalFormattings></ext></extLst>"#;
        let changed = original.replace(
            "</worksheet>",
            &format!("{formatting}{extended}</worksheet>"),
        );
        write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([
                ("xl/worksheets/sheet1.xml".to_owned(), changed.into_bytes()),
                ("xl/styles.xml".to_owned(), styles.into_bytes()),
            ]),
        )
        .unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let rules = &loaded.sheets[0].conditional_formats;
        assert_eq!(rules.len(), 7);
        assert_eq!(rules[0].range, "A1:A10");
        assert_eq!(rules[0].priority, 1);
        assert!(matches!(
            rules[0].rule,
            ConditionalFormatRule::ColorScale { .. }
        ));
        match &rules[1].rule {
            ConditionalFormatRule::Formula { formula, style } => {
                assert_eq!(formula, "A1>5");
                assert_eq!(style.font.as_ref().and_then(|font| font.bold), Some(true));
                assert_eq!(
                    style
                        .fill
                        .as_ref()
                        .and_then(|fill| fill.fg_color.as_deref()),
                    Some("FFFFFF00")
                );
                assert_eq!(
                    style
                        .border
                        .as_ref()
                        .and_then(|border| border.left.as_ref())
                        .and_then(|side| side.color.as_deref()),
                    Some("FF0000FF")
                );
                assert!(style
                    .border
                    .as_ref()
                    .is_some_and(|border| border.diagonal_up));
            }
            _ => panic!("formula rule should resolve its differential style"),
        }
        assert!(matches!(
            &rules[2].rule,
            ConditionalFormatRule::CellValue { operator: ConditionalOperator::GreaterThan, value, .. }
                if value == "10"
        ));
        assert!(matches!(
            rules[3].rule,
            ConditionalFormatRule::DataBar {
                show_value: false,
                ..
            }
        ));
        assert!(matches!(
            &rules[4].rule,
            ConditionalFormatRule::RawXml { rule_type, xml }
                if rule_type == "iconSet" && xml.contains("val=\"67\"")
        ));
        assert!(matches!(
            &rules[5].rule,
            ConditionalFormatRule::RawXml { rule_type, xml }
                if rule_type == "expression" && xml.contains("stopIfTrue")
        ));
        assert!(matches!(
            &rules[6].rule,
            ConditionalFormatRule::RawXml { rule_type, xml }
                if rule_type == "expression" && xml.contains("dxfId=\"1\"")
        ));
        let extensions = &loaded.sheets[0].conditional_format_extensions;
        assert_eq!(extensions.len(), 1);
        assert_eq!(extensions[0].range, "B1:B10");
        assert_eq!(extensions[0].rules[0].rule_type, "dataBar");
        assert_eq!(extensions[0].rules[0].priority, Some(3));
        assert_eq!(
            extensions[0].rules[0].id.as_deref(),
            Some("{A1111111-1111-1111-1111-111111111111}")
        );
        assert!(extensions[0].raw_xml.contains("gradient=\"0\""));
    }
}
