//! Decode commonly used OOXML cell styles into the workbook data model.

use std::collections::HashMap;

use excli_core::cell::Cell;
use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::reference::CellReference;
use excli_core::sheet::{Row, Sheet};
use excli_core::style::{
    Alignment, Border, BorderSide, BorderStyle, CellProtection, CellStyle, Fill, FillPattern,
    FillType, Font, HorizontalAlign, Underline, VerticalAlign,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

fn malformed(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(event: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in event.attributes() {
        let item = item.map_err(|error| malformed(format!("Invalid style attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| malformed(format!("Invalid style value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn index_attribute(event: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<usize>> {
    attribute(event, key)?
        .map(|value| value.parse().map_err(|_| malformed("Invalid style index")))
        .transpose()
}

fn bool_attribute(event: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<bool>> {
    Ok(attribute(event, key)?.map(|value| matches!(value.as_str(), "1" | "true")))
}

fn has_extra_attributes(event: &BytesStart<'_>, allowed: &[&[u8]]) -> ExcliResult<bool> {
    for item in event.attributes() {
        let item = item.map_err(|error| malformed(format!("Invalid style attribute: {error}")))?;
        if !allowed.contains(&item.key.as_ref()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn font_property(event: &BytesStart<'_>, font: &mut Font) -> ExcliResult<()> {
    match event.local_name().as_ref() {
        b"name" => font.name = attribute(event, b"val")?,
        b"sz" => {
            font.size = attribute(event, b"val")?
                .map(|value| value.parse().map_err(|_| malformed("Invalid font size")))
                .transpose()?
        }
        b"b" => font.bold = Some(bool_attribute(event, b"val")?.unwrap_or(true)),
        b"i" => font.italic = Some(bool_attribute(event, b"val")?.unwrap_or(true)),
        b"strike" => font.strikethrough = Some(bool_attribute(event, b"val")?.unwrap_or(true)),
        b"u" => {
            font.underline = match attribute(event, b"val")?.as_deref() {
                None | Some("single") => Some(Underline::Single),
                Some("double") => Some(Underline::Double),
                Some("singleAccounting") => Some(Underline::SingleAccounting),
                Some("doubleAccounting") => Some(Underline::DoubleAccounting),
                _ => None,
            }
        }
        b"color" => font.color = attribute(event, b"rgb")?,
        _ => {}
    }
    Ok(())
}

fn fill_pattern(value: &str) -> Option<FillPattern> {
    match value {
        "solid" => Some(FillPattern::Solid),
        "darkGray" => Some(FillPattern::DarkGray),
        "mediumGray" => Some(FillPattern::MediumGray),
        "lightGray" => Some(FillPattern::LightGray),
        "gray125" => Some(FillPattern::Gray125),
        "gray0625" => Some(FillPattern::Gray0625),
        "darkHorizontal" => Some(FillPattern::DarkHorizontal),
        "darkVertical" => Some(FillPattern::DarkVertical),
        "darkDown" => Some(FillPattern::DarkDown),
        "darkUp" => Some(FillPattern::DarkUp),
        "darkGrid" => Some(FillPattern::DarkGrid),
        "darkTrellis" => Some(FillPattern::DarkTrellis),
        "lightHorizontal" => Some(FillPattern::LightHorizontal),
        "lightVertical" => Some(FillPattern::LightVertical),
        "lightDown" => Some(FillPattern::LightDown),
        "lightUp" => Some(FillPattern::LightUp),
        "lightGrid" => Some(FillPattern::LightGrid),
        "lightTrellis" => Some(FillPattern::LightTrellis),
        _ => None,
    }
}

fn border_style(value: &str) -> Option<BorderStyle> {
    match value {
        "none" => Some(BorderStyle::None),
        "thin" => Some(BorderStyle::Thin),
        "medium" => Some(BorderStyle::Medium),
        "thick" => Some(BorderStyle::Thick),
        "double" => Some(BorderStyle::Double),
        "dotted" => Some(BorderStyle::Dotted),
        "dashed" => Some(BorderStyle::Dashed),
        "hair" => Some(BorderStyle::Hair),
        "dashDot" => Some(BorderStyle::DashDot),
        "dashDotDot" => Some(BorderStyle::DashDotDot),
        "slantDashDot" => Some(BorderStyle::SlantDashDot),
        _ => None,
    }
}

fn alignment(event: &BytesStart<'_>) -> ExcliResult<Alignment> {
    let horizontal = match attribute(event, b"horizontal")?.as_deref() {
        Some("left") => Some(HorizontalAlign::Left),
        Some("center") => Some(HorizontalAlign::Center),
        Some("right") => Some(HorizontalAlign::Right),
        Some("fill") => Some(HorizontalAlign::Fill),
        Some("justify") => Some(HorizontalAlign::Justify),
        Some("centerContinuous") => Some(HorizontalAlign::CenterContinuous),
        Some("distributed") => Some(HorizontalAlign::Distributed),
        _ => None,
    };
    let vertical = match attribute(event, b"vertical")?.as_deref() {
        Some("top") => Some(VerticalAlign::Top),
        Some("center") => Some(VerticalAlign::Center),
        Some("bottom") => Some(VerticalAlign::Bottom),
        Some("justify") => Some(VerticalAlign::Justify),
        Some("distributed") => Some(VerticalAlign::Distributed),
        _ => None,
    };
    let text_rotation = attribute(event, b"textRotation")?
        .map(|value| {
            value
                .parse()
                .map_err(|_| malformed("Invalid text rotation"))
        })
        .transpose()?;
    let indent = attribute(event, b"indent")?
        .map(|value| value.parse().map_err(|_| malformed("Invalid text indent")))
        .transpose()?;
    Ok(Alignment {
        horizontal,
        vertical,
        wrap_text: bool_attribute(event, b"wrapText")?.unwrap_or(false),
        shrink_to_fit: bool_attribute(event, b"shrinkToFit")?.unwrap_or(false),
        text_rotation,
        indent,
    })
}

#[derive(Default)]
struct Xf {
    font: usize,
    fill: usize,
    border: usize,
    number_format: u32,
    alignment: Option<Alignment>,
    protection: Option<CellProtection>,
}

impl Xf {
    fn from_event(event: &BytesStart<'_>) -> ExcliResult<Self> {
        Ok(Self {
            font: index_attribute(event, b"fontId")?.unwrap_or(0),
            fill: index_attribute(event, b"fillId")?.unwrap_or(0),
            border: index_attribute(event, b"borderId")?.unwrap_or(0),
            number_format: attribute(event, b"numFmtId")?
                .map(|value| {
                    value
                        .parse()
                        .map_err(|_| malformed("Invalid number format ID"))
                })
                .transpose()?
                .unwrap_or(0),
            alignment: None,
            protection: None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Fonts,
    Fills,
    Borders,
    NumFormats,
    CellXfs,
}

fn referenced<T: Clone>(items: &[T], index: usize, kind: &str) -> ExcliResult<Option<T>> {
    if items.is_empty() && index == 0 {
        Ok(None)
    } else {
        items
            .get(index)
            .cloned()
            .map(Some)
            .ok_or_else(|| malformed(format!("{kind} style index out of range")))
    }
}

/// Resolved cell styles indexed by the `s` attribute in worksheet XML.
pub(crate) fn parse_styles(xml: &[u8]) -> ExcliResult<Vec<CellStyle>> {
    let mut reader = Reader::from_reader(xml);
    let mut section = None;
    let mut fonts = Vec::new();
    let mut fills = Vec::new();
    let mut borders = Vec::new();
    let mut number_formats = HashMap::new();
    let mut xfs = Vec::new();
    let mut font: Option<Font> = None;
    let mut fill: Option<Fill> = None;
    let mut border: Option<Border> = None;
    let mut side: Option<(String, BorderSide)> = None;
    let mut xf: Option<Xf> = None;

    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid styles XML: {error}")))?
        {
            Event::Start(event) => match event.local_name().as_ref() {
                b"fonts" => section = Some(Section::Fonts),
                b"fills" => section = Some(Section::Fills),
                b"borders" => section = Some(Section::Borders),
                b"numFmts" => section = Some(Section::NumFormats),
                b"cellXfs" => section = Some(Section::CellXfs),
                b"font" if section == Some(Section::Fonts) => font = Some(Font::default()),
                b"fill" if section == Some(Section::Fills) => fill = Some(Fill::default()),
                b"border" if section == Some(Section::Borders) => {
                    border = Some(Border {
                        diagonal_up: bool_attribute(&event, b"diagonalUp")?.unwrap_or(false),
                        diagonal_down: bool_attribute(&event, b"diagonalDown")?.unwrap_or(false),
                        ..Border::default()
                    });
                }
                b"xf" if section == Some(Section::CellXfs) => xf = Some(Xf::from_event(&event)?),
                b"left" | b"right" | b"top" | b"bottom" | b"diagonal" if border.is_some() => {
                    if let Some(style) = attribute(&event, b"style")?
                        .as_deref()
                        .and_then(border_style)
                    {
                        side = Some((
                            String::from_utf8_lossy(event.local_name().as_ref()).into_owned(),
                            BorderSide { style, color: None },
                        ));
                    }
                }
                _ => apply_property(
                    &event,
                    section,
                    font.as_mut(),
                    fill.as_mut(),
                    side.as_mut(),
                    xf.as_mut(),
                    &mut number_formats,
                )?,
            },
            Event::Empty(event) => match event.local_name().as_ref() {
                b"xf" if section == Some(Section::CellXfs) => xfs.push(Xf::from_event(&event)?),
                b"left" | b"right" | b"top" | b"bottom" | b"diagonal" if border.is_some() => {
                    if let Some(style) = attribute(&event, b"style")?
                        .as_deref()
                        .and_then(border_style)
                    {
                        assign_side(
                            border.as_mut(),
                            String::from_utf8_lossy(event.local_name().as_ref()).as_ref(),
                            BorderSide { style, color: None },
                        );
                    }
                }
                _ => apply_property(
                    &event,
                    section,
                    font.as_mut(),
                    fill.as_mut(),
                    side.as_mut(),
                    xf.as_mut(),
                    &mut number_formats,
                )?,
            },
            Event::End(event) => match event.local_name().as_ref() {
                b"font" if section == Some(Section::Fonts) => {
                    if let Some(value) = font.take() {
                        fonts.push(value);
                    }
                }
                b"fill" if section == Some(Section::Fills) => {
                    if let Some(value) = fill.take() {
                        fills.push(value);
                    }
                }
                b"border" if section == Some(Section::Borders) => {
                    if let Some(value) = border.take() {
                        borders.push(value);
                    }
                }
                b"xf" if section == Some(Section::CellXfs) => {
                    if let Some(value) = xf.take() {
                        xfs.push(value);
                    }
                }
                b"left" | b"right" | b"top" | b"bottom" | b"diagonal" => {
                    if let Some((name, value)) = side.take() {
                        assign_side(border.as_mut(), &name, value);
                    }
                }
                b"fonts" | b"fills" | b"borders" | b"numFmts" | b"cellXfs" => section = None,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }

    xfs.into_iter()
        .map(|record| {
            Ok(CellStyle {
                font: referenced(&fonts, record.font, "Font")?,
                fill: referenced(&fills, record.fill, "Fill")?,
                border: referenced(&borders, record.border, "Border")?,
                number_format: number_formats.get(&record.number_format).cloned(),
                number_format_id: Some(record.number_format),
                alignment: record.alignment,
                protection: record.protection,
            })
        })
        .collect()
}

fn parse_dxf(xml: &[u8]) -> ExcliResult<Option<CellStyle>> {
    let mut reader = Reader::from_reader(xml);
    let mut style = CellStyle::default();
    let mut font = None;
    let mut fill = None;
    let mut border = None;
    let mut side: Option<(String, BorderSide)> = None;
    let mut supported = true;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid differential style: {error}")))?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                match tag.local_name().as_ref() {
                    b"dxf" => supported &= !has_extra_attributes(tag, &[])?,
                    b"font" => {
                        supported &= !has_extra_attributes(tag, &[])?;
                        font = Some(Font::default());
                    }
                    b"fill" => {
                        supported &= !has_extra_attributes(tag, &[])?;
                        fill = Some(Fill::default());
                    }
                    b"border" => {
                        supported &= !has_extra_attributes(tag, &[b"diagonalUp", b"diagonalDown"])?;
                        border = Some(Border {
                            diagonal_up: bool_attribute(tag, b"diagonalUp")?.unwrap_or(false),
                            diagonal_down: bool_attribute(tag, b"diagonalDown")?.unwrap_or(false),
                            ..Border::default()
                        });
                    }
                    b"left" | b"right" | b"top" | b"bottom" | b"diagonal" if border.is_some() => {
                        supported &= !has_extra_attributes(tag, &[b"style"])?;
                        if let Some(value) = attribute(tag, b"style")? {
                            match border_style(&value) {
                                Some(style) => {
                                    side = Some((
                                        String::from_utf8_lossy(tag.local_name().as_ref())
                                            .into_owned(),
                                        BorderSide { style, color: None },
                                    ));
                                }
                                None => supported = false,
                            }
                        }
                    }
                    b"name" | b"sz" | b"b" | b"i" | b"strike" | b"u" | b"color"
                        if font.is_some() =>
                    {
                        if tag.local_name().as_ref() == b"color" {
                            supported &= !has_extra_attributes(tag, &[b"rgb"])?;
                        } else {
                            supported &= !has_extra_attributes(tag, &[b"val"])?;
                        }
                        if tag.local_name().as_ref() == b"color"
                            && attribute(tag, b"rgb")?.is_none()
                        {
                            supported = false;
                        }
                        if let Some(ref mut value) = font {
                            font_property(tag, value)?;
                        }
                    }
                    b"patternFill" if fill.is_some() => {
                        supported &= !has_extra_attributes(tag, &[b"patternType"])?;
                        if attribute(tag, b"patternType")?.as_deref() == Some("solid") {
                            if let Some(ref mut value) = fill {
                                value.fill_type = FillType::Solid;
                                value.pattern = Some(FillPattern::Solid);
                            }
                        } else {
                            supported = false;
                        }
                    }
                    b"fgColor" | b"bgColor" if fill.is_some() => {
                        supported &= !has_extra_attributes(tag, &[b"rgb"])?;
                        let color = attribute(tag, b"rgb")?;
                        if color.is_none() {
                            supported = false;
                        }
                        if let Some(ref mut value) = fill {
                            if tag.local_name().as_ref() == b"fgColor" {
                                value.fg_color = color;
                            } else {
                                value.bg_color = color;
                            }
                        }
                    }
                    b"color" if border.is_some() => {
                        supported &= !has_extra_attributes(tag, &[b"rgb"])?;
                        if let (Some((_, value)), Some(color)) =
                            (side.as_mut(), attribute(tag, b"rgb")?)
                        {
                            value.color = Some(color);
                        } else {
                            supported = false;
                        }
                    }
                    b"numFmt" => {
                        supported &= !has_extra_attributes(tag, &[b"numFmtId", b"formatCode"])?;
                        style.number_format_id = attribute(tag, b"numFmtId")?
                            .map(|value| {
                                value
                                    .parse()
                                    .map_err(|_| malformed("Invalid dxf number format ID"))
                            })
                            .transpose()?;
                        style.number_format = attribute(tag, b"formatCode")?;
                    }
                    b"alignment" => {
                        supported &= !has_extra_attributes(
                            tag,
                            &[
                                b"horizontal",
                                b"vertical",
                                b"wrapText",
                                b"shrinkToFit",
                                b"textRotation",
                                b"indent",
                            ],
                        )?;
                        style.alignment = Some(alignment(tag)?);
                    }
                    b"protection" => {
                        supported &= !has_extra_attributes(tag, &[b"locked", b"hidden"])?;
                        style.protection = Some(CellProtection {
                            locked: bool_attribute(tag, b"locked")?.unwrap_or(true),
                            hidden: bool_attribute(tag, b"hidden")?.unwrap_or(false),
                        });
                    }
                    _ => supported = false,
                }
                if empty {
                    match tag.local_name().as_ref() {
                        b"font" => style.font = font.take(),
                        b"fill" => style.fill = fill.take(),
                        b"border" => style.border = border.take(),
                        b"left" | b"right" | b"top" | b"bottom" | b"diagonal" => {
                            if let Some((name, value)) = side.take() {
                                assign_side(border.as_mut(), &name, value);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"font" => {
                style.font = font.take();
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"fill" => {
                style.fill = fill.take();
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"border" => {
                style.border = border.take();
            }
            Event::End(ref tag)
                if matches!(
                    tag.local_name().as_ref(),
                    b"left" | b"right" | b"top" | b"bottom" | b"diagonal"
                ) =>
            {
                if let Some((name, value)) = side.take() {
                    assign_side(border.as_mut(), &name, value);
                }
            }
            Event::Text(ref value) if !value.as_ref().iter().all(u8::is_ascii_whitespace) => {
                supported = false;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(supported.then_some(style))
}

pub(crate) fn parse_differential_styles(xml: &[u8]) -> ExcliResult<Vec<Option<CellStyle>>> {
    let mut reader = Reader::from_reader(xml);
    let mut in_dxfs = false;
    let mut start = None;
    let mut depth = 0usize;
    let mut styles = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid styles XML: {error}")))?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"dxfs" => in_dxfs = true,
            Event::Start(ref tag) if in_dxfs && tag.local_name().as_ref() == b"dxf" => {
                start = Some(from);
                depth = 1;
            }
            Event::Start(_) if start.is_some() => depth += 1,
            Event::End(_) if start.is_some() && depth > 1 => depth -= 1,
            Event::End(ref tag) if tag.local_name().as_ref() == b"dxf" && start.is_some() => {
                let from = start.take().ok_or_else(|| malformed("Missing dxf start"))?;
                styles.push(parse_dxf(&xml[from..to])?);
                depth = 0;
            }
            Event::Empty(ref tag) if in_dxfs && tag.local_name().as_ref() == b"dxf" => {
                styles.push(Some(CellStyle::default()));
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"dxfs" => in_dxfs = false,
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(styles)
}

fn assign_side(border: Option<&mut Border>, name: &str, side: BorderSide) {
    if let Some(border) = border {
        match name {
            "left" => border.left = Some(side),
            "right" => border.right = Some(side),
            "top" => border.top = Some(side),
            "bottom" => border.bottom = Some(side),
            "diagonal" => border.diagonal = Some(side),
            _ => {}
        }
    }
}

fn apply_property(
    event: &BytesStart<'_>,
    section: Option<Section>,
    font: Option<&mut Font>,
    fill: Option<&mut Fill>,
    side: Option<&mut (String, BorderSide)>,
    xf: Option<&mut Xf>,
    number_formats: &mut HashMap<u32, String>,
) -> ExcliResult<()> {
    if let Some(font) = font {
        font_property(event, font)?;
    }
    if let Some(fill) = fill {
        match event.local_name().as_ref() {
            b"patternFill" => {
                if let Some(pattern) = attribute(event, b"patternType")? {
                    fill.pattern = fill_pattern(&pattern);
                    fill.fill_type = match pattern.as_str() {
                        "none" => FillType::None,
                        "solid" => FillType::Solid,
                        _ => FillType::Pattern,
                    };
                }
            }
            b"gradientFill" => fill.fill_type = FillType::Gradient,
            b"fgColor" => fill.fg_color = attribute(event, b"rgb")?,
            b"bgColor" => fill.bg_color = attribute(event, b"rgb")?,
            _ => {}
        }
    }
    if let Some((_, side)) = side {
        if event.local_name().as_ref() == b"color" {
            side.color = attribute(event, b"rgb")?;
        }
    }
    if let Some(xf) = xf {
        match event.local_name().as_ref() {
            b"alignment" => xf.alignment = Some(alignment(event)?),
            b"protection" => {
                xf.protection = Some(CellProtection {
                    locked: bool_attribute(event, b"locked")?.unwrap_or(true),
                    hidden: bool_attribute(event, b"hidden")?.unwrap_or(false),
                })
            }
            _ => {}
        }
    }
    if section == Some(Section::NumFormats) && event.local_name().as_ref() == b"numFmt" {
        if let (Some(id), Some(code)) = (
            attribute(event, b"numFmtId")?,
            attribute(event, b"formatCode")?,
        ) {
            let id = id
                .parse::<u32>()
                .map_err(|_| malformed("Invalid number format ID"))?;
            number_formats.insert(id, code);
        }
    }
    Ok(())
}

pub(crate) fn enrich_sheet(xml: &[u8], sheet: &mut Sheet, styles: &[CellStyle]) -> ExcliResult<()> {
    let mut reader = Reader::from_reader(xml);
    let mut last_row = 0u32;
    let mut current_row = None;
    let mut next_column = 0u32;
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid worksheet XML: {error}")))?
        {
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"row" => {
                let number = attribute(&event, b"r")?
                    .map(|value| {
                        value
                            .parse::<u32>()
                            .map_err(|_| malformed("Invalid row number"))
                    })
                    .transpose()?
                    .unwrap_or(last_row + 1);
                if number == 0 || number > 1_048_576 {
                    return Err(malformed("Row number outside Excel limits"));
                }
                last_row = number;
                current_row = Some(number - 1);
                next_column = 0;
                if let Some(index) = index_attribute(&event, b"s")? {
                    let style = styles
                        .get(index)
                        .ok_or_else(|| malformed("Row style index out of range"))?
                        .clone();
                    if sheet.rows.len() < number as usize {
                        sheet.rows.resize_with(number as usize, Row::default);
                    }
                    sheet.rows[number as usize - 1].style = Some(style);
                }
            }
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"col" => {
                if let Some(index) = index_attribute(&event, b"style")? {
                    let first = index_attribute(&event, b"min")?
                        .ok_or_else(|| malformed("Styled column has no minimum"))?;
                    let last = index_attribute(&event, b"max")?
                        .ok_or_else(|| malformed("Styled column has no maximum"))?;
                    let style = styles
                        .get(index)
                        .ok_or_else(|| malformed("Column style index out of range"))?
                        .clone();
                    for column in &mut sheet.columns {
                        let column_number = column.index as usize + 1;
                        if (first..=last).contains(&column_number) {
                            column.style = Some(style.clone());
                        }
                    }
                }
            }
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"c" => {
                let reference = if let Some(reference) = attribute(&event, b"r")? {
                    CellReference::from_a1(&reference)
                        .ok_or_else(|| malformed("Invalid styled cell reference"))?
                } else {
                    CellReference::new(
                        next_column,
                        current_row.ok_or_else(|| malformed("Cell has no row context"))?,
                    )
                };
                if reference.column >= 16_384 || reference.row >= 1_048_576 {
                    return Err(malformed("Styled cell outside Excel limits"));
                }
                next_column = reference.column + 1;
                if let Some(index) = index_attribute(&event, b"s")? {
                    let style = styles
                        .get(index)
                        .ok_or_else(|| malformed("Cell style index out of range"))?
                        .clone();
                    let mut cell = sheet
                        .get_cell(reference.column, reference.row)
                        .cloned()
                        .unwrap_or_else(Cell::empty);
                    cell.style = Some(style);
                    sheet.set_cell(reference.column, reference.row, cell);
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"row" => current_row = None,
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::sheet::Column;
    use excli_core::Workbook;
    use std::collections::HashMap;
    use zip::ZipArchive;

    use crate::ooxml_edit::write_modified_parts;
    use crate::ooxml_package::read_part;

    #[test]
    fn reads_font_fill_border_alignment_and_custom_number_format() {
        let xml = br#"<styleSheet><numFmts><numFmt numFmtId="164" formatCode="0.00%"/></numFmts><fonts><font><name val="Aptos"/><sz val="12"/><b/><color rgb="FF112233"/></font></fonts><fills><fill><patternFill patternType="solid"><fgColor rgb="FFFF0000"/></patternFill></fill></fills><borders><border><left style="thin"><color rgb="FF000000"/></left></border></borders><cellXfs><xf fontId="0" fillId="0" borderId="0" numFmtId="164"><alignment horizontal="center" wrapText="1"/></xf></cellXfs></styleSheet>"#;
        let styles = parse_styles(xml).unwrap();
        let style = &styles[0];
        assert_eq!(style.font.as_ref().unwrap().name.as_deref(), Some("Aptos"));
        assert_eq!(style.font.as_ref().unwrap().bold, Some(true));
        assert_eq!(
            style.fill.as_ref().unwrap().fg_color.as_deref(),
            Some("FFFF0000")
        );
        assert_eq!(
            style.border.as_ref().unwrap().left.as_ref().unwrap().style,
            BorderStyle::Thin
        );
        assert_eq!(style.number_format.as_deref(), Some("0.00%"));
        assert_eq!(style.number_format_id, Some(164));
        assert_eq!(
            style.alignment.as_ref().unwrap().horizontal,
            Some(HorizontalAlign::Center)
        );
        assert!(style.alignment.as_ref().unwrap().wrap_text);

        let mut sheet = Sheet::new("Data");
        sheet.columns.push(Column::new(1, 18.0));
        enrich_sheet(b"<worksheet><cols><col min=\"2\" max=\"2\" style=\"0\"/></cols><sheetData><row r=\"1\" s=\"0\"><c r=\"B1\" s=\"0\"><v>0.5</v></c></row></sheetData></worksheet>", &mut sheet, &styles).unwrap();
        assert!(sheet.rows[0].style.is_some());
        assert!(sheet.columns[0].style.is_some());
        assert_eq!(
            sheet
                .get_cell(1, 0)
                .unwrap()
                .style
                .as_ref()
                .unwrap()
                .number_format
                .as_deref(),
            Some("0.00%")
        );
    }

    #[test]
    fn workbook_reader_exposes_custom_cell_style() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("styled.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data").set_cell(0, 0, Cell::number(0.5));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let styles = br#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="164" formatCode="0.00%"/></numFmts><fonts count="1"><font><b/></font></fonts><fills count="1"><fill><patternFill patternType="solid"><fgColor rgb="FFFF0000"/></patternFill></fill></fills><borders count="1"><border/></borders><cellXfs count="1"><xf fontId="0" fillId="0" borderId="0" numFmtId="164"/></cellXfs></styleSheet>"#;
        {
            let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
            let original =
                String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                    .unwrap();
            let sheet = original.replace("<c r=\"A1\">", "<c r=\"A1\" s=\"0\">");
            assert_ne!(sheet, original);
            write_modified_parts(
                &path,
                &mut archive,
                &HashMap::from([
                    ("xl/worksheets/sheet1.xml".to_owned(), sheet.into_bytes()),
                    ("xl/styles.xml".to_owned(), styles.to_vec()),
                ]),
            )
            .unwrap();
        }
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let cell = loaded
            .sheet_by_name("Data")
            .unwrap()
            .get_cell(0, 0)
            .unwrap();
        assert_eq!(cell.value.to_string(), "0.5");
        let style = cell.style.as_ref().unwrap();
        assert_eq!(style.font.as_ref().unwrap().bold, Some(true));
        assert_eq!(style.number_format.as_deref(), Some("0.00%"));
        assert_eq!(
            style.fill.as_ref().unwrap().fg_color.as_deref(),
            Some("FFFF0000")
        );
    }

    #[test]
    fn rejects_missing_style_components() {
        let styles = b"<styleSheet><cellXfs><xf fontId=\"4\"/></cellXfs></styleSheet>";
        assert!(parse_styles(styles).is_err());
    }

    #[test]
    fn keeps_builtin_number_format_id_without_guessing_locale() {
        let styles =
            parse_styles(b"<styleSheet><cellXfs><xf numFmtId=\"14\"/></cellXfs></styleSheet>")
                .unwrap();
        assert_eq!(styles[0].number_format_id, Some(14));
        assert!(styles[0].number_format.is_none());
    }
}
