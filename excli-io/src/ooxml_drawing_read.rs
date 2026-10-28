//! Embedded pictures referenced by worksheet DrawingML parts.

use std::collections::HashMap;
use std::io::{Read, Seek};

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::reference::CellReference;
use excli_core::sheet::{Chart, ChartType, Image};
use excli_core::Sheet;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::ooxml_package::{read_part, related_parts};

fn corrupted(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| corrupted(format!("Invalid drawing attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| corrupted(format!("Invalid drawing value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn relation_attribute(tag: &BytesStart<'_>, local: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| corrupted(format!("Invalid drawing relationship: {error}")))?;
        if item.key.as_ref().contains(&b':') && item.key.local_name().as_ref() == local {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| corrupted(format!("Invalid drawing relationship: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn drawing_ids(xml: &[u8]) -> ExcliResult<Vec<String>> {
    let mut reader = Reader::from_reader(xml);
    let mut ids = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid worksheet XML: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"drawing" =>
            {
                ids.push(
                    relation_attribute(tag, b"id")?
                        .ok_or_else(|| corrupted("Drawing has no relationship ID"))?,
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(ids)
}

#[derive(Default)]
struct Anchor {
    from_col: Option<u32>,
    from_row: Option<u32>,
    width: u32,
    height: u32,
}

#[derive(Default)]
struct Picture {
    id: Option<u32>,
    description: Option<String>,
    embed: Option<String>,
}

fn anchor_position(anchor: &Anchor) -> String {
    match (anchor.from_col, anchor.from_row) {
        (Some(col), Some(row)) => CellReference::new(col, row).to_a1(),
        _ => "absolute".to_owned(),
    }
}

fn parse_chart(xml: &[u8], anchor: &Anchor) -> ExcliResult<Option<Chart>> {
    let mut reader = Reader::from_reader(xml);
    let mut chart_type = None;
    let mut in_title = false;
    let mut in_title_text = false;
    let mut in_formula = false;
    let mut title = String::new();
    let mut source_range = String::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid chart XML: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag) => match tag.local_name().as_ref() {
                b"barChart" => chart_type = Some(ChartType::Bar),
                b"lineChart" => chart_type = Some(ChartType::Line),
                b"pieChart" | b"pie3DChart" | b"doughnutChart" => chart_type = Some(ChartType::Pie),
                b"scatterChart" => chart_type = Some(ChartType::Scatter),
                b"areaChart" | b"area3DChart" => chart_type = Some(ChartType::Area),
                b"radarChart" => chart_type = Some(ChartType::Radar),
                b"barDir" if chart_type == Some(ChartType::Bar) => {
                    if attribute(tag, b"val")?.as_deref() == Some("col") {
                        chart_type = Some(ChartType::Column);
                    }
                }
                b"title" => in_title = true,
                b"t" if in_title => in_title_text = true,
                b"f" if !in_title && source_range.is_empty() => in_formula = true,
                _ => {}
            },
            Event::Text(ref text) => {
                let value = text
                    .unescape()
                    .map_err(|error| corrupted(format!("Invalid chart text: {error}")))?;
                if in_title_text {
                    title.push_str(&value);
                } else if in_formula {
                    source_range.push_str(&value);
                }
            }
            Event::End(ref tag) => match tag.local_name().as_ref() {
                b"title" => in_title = false,
                b"t" => in_title_text = false,
                b"f" => in_formula = false,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(chart_type.map(|chart_type| Chart {
        title: (!title.is_empty()).then_some(title),
        chart_type,
        source_range,
        position: anchor_position(anchor),
        width: anchor.width,
        height: anchor.height,
    }))
}

fn parse_drawing<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    xml: &[u8],
    sheet: &mut Sheet,
) -> ExcliResult<()> {
    let relations = related_parts(archive, path)?;
    let mut reader = Reader::from_reader(xml);
    let mut stack = Vec::<Vec<u8>>::new();
    let mut anchor = None::<Anchor>;
    let mut picture = None::<Picture>;
    let mut chart_relation = None::<String>;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid drawing XML: {error}")))?;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                let local = tag.local_name().as_ref().to_vec();
                let parent = stack.last().map(Vec::as_slice);
                if matches!(
                    local.as_slice(),
                    b"oneCellAnchor" | b"twoCellAnchor" | b"absoluteAnchor"
                ) {
                    anchor = Some(Anchor::default());
                } else if local == b"pic" && anchor.is_some() {
                    picture = Some(Picture::default());
                } else if local == b"graphicFrame" && anchor.is_some() {
                    chart_relation = None;
                } else if local == b"chart" && chart_relation.is_none() {
                    chart_relation = relation_attribute(tag, b"id")?;
                } else if local == b"ext"
                    && matches!(parent, Some(b"oneCellAnchor" | b"absoluteAnchor"))
                {
                    if let Some(anchor) = &mut anchor {
                        anchor.width = attribute(tag, b"cx")?
                            .and_then(|value| value.parse::<u64>().ok())
                            .map(|emu| (emu / 9_525).min(u64::from(u32::MAX)) as u32)
                            .unwrap_or(0);
                        anchor.height = attribute(tag, b"cy")?
                            .and_then(|value| value.parse::<u64>().ok())
                            .map(|emu| (emu / 9_525).min(u64::from(u32::MAX)) as u32)
                            .unwrap_or(0);
                    }
                } else if local == b"cNvPr" {
                    if let Some(picture) = &mut picture {
                        picture.id = attribute(tag, b"id")?
                            .map(|value| {
                                value
                                    .parse::<u32>()
                                    .map_err(|_| corrupted("Invalid picture ID"))
                            })
                            .transpose()?;
                        picture.description = attribute(tag, b"descr")?;
                    }
                } else if local == b"blip" {
                    if let Some(picture) = &mut picture {
                        picture.embed = relation_attribute(tag, b"embed")?;
                    }
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(local);
                }
            }
            Event::Text(ref text) => {
                if stack.len() >= 2 && stack[stack.len() - 2] == b"from" {
                    if let Some(anchor) = &mut anchor {
                        let value = text
                            .unescape()
                            .map_err(|error| corrupted(format!("Invalid anchor marker: {error}")))?
                            .parse::<u32>()
                            .map_err(|_| corrupted("Invalid anchor marker"))?;
                        match stack.last().map(Vec::as_slice) {
                            Some(b"col") => anchor.from_col = Some(value),
                            Some(b"row") => anchor.from_row = Some(value),
                            _ => {}
                        }
                    }
                }
            }
            Event::End(ref tag) => {
                let local = tag.local_name().as_ref().to_vec();
                if local == b"pic" {
                    if let (Some(picture), Some(anchor)) = (picture.take(), anchor.as_ref()) {
                        let (media_path, data) = if let Some(embed) = &picture.embed {
                            let (kind, target) = relations.get(embed).ok_or_else(|| {
                                corrupted(format!("Missing picture relationship: {embed}"))
                            })?;
                            if !kind.ends_with("/image") {
                                return Err(corrupted(
                                    "Picture relationship does not target an image",
                                ));
                            }
                            (Some(target.clone()), Some(read_part(archive, target)?))
                        } else {
                            (None, None)
                        };
                        sheet.images.push(Image {
                            index: picture.id.unwrap_or(sheet.images.len() as u32 + 1),
                            position: anchor_position(anchor),
                            width: anchor.width,
                            height: anchor.height,
                            description: picture.description,
                            media_path,
                            data,
                        });
                    }
                } else if local == b"graphicFrame" {
                    if let (Some(relation), Some(anchor)) = (chart_relation.take(), anchor.as_ref())
                    {
                        let (kind, target) = relations.get(&relation).ok_or_else(|| {
                            corrupted(format!("Missing chart relationship: {relation}"))
                        })?;
                        if !kind.ends_with("/chart") {
                            return Err(corrupted(
                                "Graphic frame relationship does not target a chart",
                            ));
                        }
                        if let Some(chart) = parse_chart(&read_part(archive, target)?, anchor)? {
                            sheet.charts.push(chart);
                        }
                    }
                } else if matches!(
                    local.as_slice(),
                    b"oneCellAnchor" | b"twoCellAnchor" | b"absoluteAnchor"
                ) {
                    anchor = None;
                }
                if stack.pop().as_deref() != Some(local.as_slice()) {
                    return Err(corrupted("Invalid drawing XML nesting"));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(corrupted("Unclosed drawing XML"));
    }
    Ok(())
}

pub(crate) fn enrich_sheet<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
    worksheet_xml: &[u8],
    sheet: &mut Sheet,
) -> ExcliResult<()> {
    let ids = drawing_ids(worksheet_xml)?;
    if ids.is_empty() {
        return Ok(());
    }
    let relations: HashMap<_, _> = related_parts(archive, worksheet_path)?;
    for id in ids {
        let (kind, path) = relations
            .get(&id)
            .ok_or_else(|| corrupted(format!("Missing drawing relationship: {id}")))?;
        if !kind.ends_with("/drawing") {
            return Err(corrupted("Worksheet drawing relationship has wrong type"));
        }
        let xml = read_part(archive, path)?;
        parse_drawing(archive, path, &xml, sheet)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::Workbook;

    use crate::ooxml_edit::write_package_parts;

    #[test]
    fn reads_embedded_picture_and_media_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("picture.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::string("Value"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let worksheet =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        let worksheet = worksheet.replace("</worksheet>", "<drawing xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId1\"/></worksheet>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/xl/drawings/drawing1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/></Types>");
        let media = vec![137, 80, 78, 71, 13, 10, 26, 10];
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/worksheets/sheet1.xml".to_owned(), worksheet.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing1.xml\"/></Relationships>".to_vec()),
            ("xl/drawings/drawing1.xml".to_owned(), b"<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><xdr:oneCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:row>2</xdr:row></xdr:from><xdr:ext cx=\"95250\" cy=\"190500\"/><xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"7\" name=\"Logo\" descr=\"Brand logo\"/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"rId2\"/></xdr:blipFill></xdr:pic><xdr:clientData/></xdr:oneCellAnchor></xdr:wsDr>".to_vec()),
            ("xl/drawings/_rels/drawing1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image1.png\"/></Relationships>".to_vec()),
            ("xl/media/image1.png".to_owned(), media.clone()),
        ])).unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let image = &loaded.sheets[0].images[0];
        assert_eq!(image.index, 7);
        assert_eq!(image.position, "B3");
        assert_eq!((image.width, image.height), (10, 20));
        assert_eq!(image.description.as_deref(), Some("Brand logo"));
        assert_eq!(image.media_path.as_deref(), Some("xl/media/image1.png"));
        assert_eq!(image.data.as_deref(), Some(media.as_slice()));
    }

    #[test]
    fn reads_chart_type_title_and_source_formula() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chart.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let worksheet =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        let worksheet = worksheet.replace("</worksheet>", "<drawing xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId1\"/></worksheet>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/drawings/drawing1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/><Override PartName=\"/xl/charts/chart1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/></Types>");
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/worksheets/sheet1.xml".to_owned(), worksheet.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing1.xml\"/></Relationships>".to_vec()),
            ("xl/drawings/drawing1.xml".to_owned(), b"<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><xdr:oneCellAnchor><xdr:from><xdr:col>2</xdr:col><xdr:row>3</xdr:row></xdr:from><xdr:ext cx=\"1905000\" cy=\"952500\"/><xdr:graphicFrame><c:chart r:id=\"rId2\"/></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor></xdr:wsDr>".to_vec()),
            ("xl/drawings/_rels/drawing1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/chart1.xml\"/></Relationships>".to_vec()),
            ("xl/charts/chart1.xml".to_owned(), b"<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><c:chart><c:title><a:t>Revenue</a:t></c:title><c:plotArea><c:barChart><c:barDir val=\"col\"/><c:ser><c:val><c:numRef><c:f>Data!$B$2:$B$4</c:f></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>".to_vec()),
        ])).unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let chart = &loaded.sheets[0].charts[0];
        assert_eq!(chart.chart_type, ChartType::Column);
        assert_eq!(chart.title.as_deref(), Some("Revenue"));
        assert_eq!(chart.source_range, "Data!$B$2:$B$4");
        assert_eq!(chart.position, "C4");
        assert_eq!((chart.width, chart.height), (200, 100));
    }
}
