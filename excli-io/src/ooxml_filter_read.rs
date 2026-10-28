//! Read worksheet AutoFilter criteria without collapsing compound rules.

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::sheet::{FilterColumn, FilterType, Sheet};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

fn malformed(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item = item.map_err(|error| malformed(format!("Invalid filter attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| malformed(format!("Invalid filter value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn only_attributes(tag: &BytesStart<'_>, allowed: &[&[u8]]) -> ExcliResult<bool> {
    for item in tag.attributes() {
        let item = item.map_err(|error| malformed(format!("Invalid filter attribute: {error}")))?;
        if !allowed.contains(&item.key.as_ref()) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn raw_filter(xml: &[u8], column: u32) -> ExcliResult<Vec<FilterColumn>> {
    Ok(vec![FilterColumn {
        column,
        filter_type: FilterType::Custom(
            std::str::from_utf8(xml)
                .map_err(|error| malformed(format!("Invalid filter XML: {error}")))?
                .to_owned(),
        ),
    }])
}

fn parse_filter_column(xml: &[u8], column: u32) -> ExcliResult<Vec<FilterColumn>> {
    let mut reader = Reader::from_reader(xml);
    let mut values = Vec::new();
    let mut top = None;
    let mut dynamic = None;
    let mut unrepresented = false;
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid filter XML: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag) => match tag.local_name().as_ref() {
                b"filterColumn" => {
                    if !only_attributes(tag, &[b"colId"])? {
                        unrepresented = true;
                    }
                }
                b"filters" => {
                    if attribute(tag, b"blank")?
                        .as_deref()
                        .is_some_and(|value| !matches!(value, "0" | "false"))
                        || !only_attributes(tag, &[b"blank"])?
                    {
                        unrepresented = true;
                    }
                }
                b"filter" => {
                    if !only_attributes(tag, &[b"val"])? {
                        unrepresented = true;
                    } else if let Some(value) = attribute(tag, b"val")? {
                        values.push(value);
                    } else {
                        unrepresented = true;
                    }
                }
                b"top10" => {
                    let is_top = matches!(
                        attribute(tag, b"top")?.as_deref(),
                        None | Some("1" | "true")
                    );
                    let is_count = matches!(
                        attribute(tag, b"percent")?.as_deref(),
                        None | Some("0" | "false")
                    );
                    if is_top && is_count && only_attributes(tag, &[b"top", b"percent", b"val"])? {
                        top = attribute(tag, b"val")?.and_then(|value| value.parse::<u32>().ok());
                        if top.is_none() {
                            unrepresented = true;
                        }
                    } else {
                        unrepresented = true;
                    }
                }
                b"dynamicFilter" => {
                    if only_attributes(tag, &[b"type"])? {
                        dynamic = attribute(tag, b"type")?;
                        if dynamic.is_none() {
                            unrepresented = true;
                        }
                    } else {
                        unrepresented = true;
                    }
                }
                _ => unrepresented = true,
            },
            Event::Text(ref content) if !content.as_ref().iter().all(u8::is_ascii_whitespace) => {
                unrepresented = true;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let families = usize::from(!values.is_empty())
        + usize::from(top.is_some())
        + usize::from(dynamic.is_some());
    if unrepresented || families != 1 {
        return raw_filter(xml, column);
    }
    if let Some(count) = top {
        return Ok(vec![FilterColumn {
            column,
            filter_type: FilterType::TopN(count),
        }]);
    }
    if let Some(kind) = dynamic {
        return Ok(vec![FilterColumn {
            column,
            filter_type: FilterType::Dynamic(kind),
        }]);
    }
    Ok(values
        .into_iter()
        .map(|value| FilterColumn {
            column,
            filter_type: FilterType::Value(value),
        })
        .collect())
}

pub(crate) fn enrich_sheet(xml: &[u8], sheet: &mut Sheet) -> ExcliResult<()> {
    let Some(auto_filter) = sheet.auto_filter.as_mut() else {
        return Ok(());
    };
    let mut reader = Reader::from_reader(xml);
    let mut in_auto_filter = false;
    let mut column_start = None;
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid worksheet filter XML: {error}")))?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"autoFilter" => {
                in_auto_filter = true;
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"autoFilter" => {
                in_auto_filter = false;
            }
            Event::Start(ref tag)
                if in_auto_filter && tag.local_name().as_ref() == b"filterColumn" =>
            {
                let column = attribute(tag, b"colId")?
                    .ok_or_else(|| malformed("Filter column has no colId"))?
                    .parse::<u32>()
                    .map_err(|_| malformed("Invalid filter column index"))?;
                column_start = Some((from, column));
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"filterColumn" => {
                if let Some((start, column)) = column_start.take() {
                    auto_filter
                        .filters
                        .extend(parse_filter_column(&xml[start..to], column)?);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::sheet::AutoFilter;

    #[test]
    fn reads_values_top_n_and_dynamic_filters() {
        let xml = br#"<worksheet><autoFilter ref="A1:C20"><filterColumn colId="0"><filters><filter val="East"/><filter val="West"/></filters></filterColumn><filterColumn colId="1"><top10 val="5"/></filterColumn><filterColumn colId="2"><dynamicFilter type="thisMonth"/></filterColumn></autoFilter></worksheet>"#;
        let mut sheet = Sheet::new("Data");
        sheet.auto_filter = Some(AutoFilter {
            range: "A1:C20".to_owned(),
            filters: Vec::new(),
        });
        enrich_sheet(xml, &mut sheet).unwrap();
        let filters = &sheet.auto_filter.unwrap().filters;
        assert_eq!(filters.len(), 4);
        assert!(matches!(&filters[0].filter_type, FilterType::Value(value) if value == "East"));
        assert!(matches!(&filters[1].filter_type, FilterType::Value(value) if value == "West"));
        assert!(matches!(filters[2].filter_type, FilterType::TopN(5)));
        assert!(
            matches!(&filters[3].filter_type, FilterType::Dynamic(value) if value == "thisMonth")
        );
    }

    #[test]
    fn retains_compound_filter_xml() {
        let xml = br#"<worksheet><autoFilter ref="A1:A20"><filterColumn colId="0"><customFilters and="1"><customFilter operator="greaterThan" val="10"/><customFilter operator="lessThan" val="50"/></customFilters></filterColumn></autoFilter></worksheet>"#;
        let mut sheet = Sheet::new("Data");
        sheet.auto_filter = Some(AutoFilter {
            range: "A1:A20".to_owned(),
            filters: Vec::new(),
        });
        enrich_sheet(xml, &mut sheet).unwrap();
        let filters = &sheet.auto_filter.unwrap().filters;
        assert_eq!(filters.len(), 1);
        assert!(
            matches!(&filters[0].filter_type, FilterType::Custom(raw) if raw.contains("and=\"1\"") && raw.contains("lessThan"))
        );
        let percent = parse_filter_column(
            br#"<filterColumn colId="1"><top10 percent="true" val="5"/></filterColumn>"#,
            1,
        )
        .unwrap();
        assert!(matches!(percent[0].filter_type, FilterType::Custom(_)));
    }
}
