//! Read workbook data connections from the OOXML Connections part.

use std::io::{Read, Seek};

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::workbook::{ConnectionType, DataConnection};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::ooxml_package::{read_part, relationships, rels_path, resolve_part};

fn malformed(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| malformed(format!("Invalid connection attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| malformed(format!("Invalid connection value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn connection_type(tag: &BytesStart<'_>) -> ExcliResult<ConnectionType> {
    let kind = attribute(tag, b"type")?
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|_| malformed("Invalid connection type"))
        })
        .transpose()?;
    Ok(match kind {
        Some(1) => ConnectionType::Odbc,
        Some(4) => ConnectionType::Web,
        Some(5) => ConnectionType::OleDb,
        Some(6) => ConnectionType::Text,
        _ => ConnectionType::Other,
    })
}

fn deleted(tag: &BytesStart<'_>) -> ExcliResult<bool> {
    match attribute(tag, b"deleted")?.as_deref() {
        None | Some("0" | "false") => Ok(false),
        Some("1" | "true") => Ok(true),
        Some(value) => Err(malformed(format!(
            "Invalid deleted connection flag: {value}"
        ))),
    }
}

fn new_connection(tag: &BytesStart<'_>) -> ExcliResult<Option<DataConnection>> {
    if deleted(tag)? {
        return Ok(None);
    }
    Ok(Some(DataConnection {
        name: attribute(tag, b"name")?.ok_or_else(|| malformed("Connection has no name"))?,
        connection_type: connection_type(tag)?,
        connection_string: attribute(tag, b"sourceFile")?.or(attribute(tag, b"odcFile")?),
        command_text: None,
    }))
}

fn apply_source(tag: &BytesStart<'_>, connection: &mut DataConnection) -> ExcliResult<()> {
    match tag.local_name().as_ref() {
        b"dbPr" => {
            connection.connection_string =
                attribute(tag, b"connection")?.or(connection.connection_string.take());
            connection.command_text = attribute(tag, b"command")?;
        }
        b"webPr" => {
            connection.connection_string =
                attribute(tag, b"url")?.or(connection.connection_string.take());
        }
        b"textPr" => {
            connection.connection_string =
                attribute(tag, b"sourceFile")?.or(connection.connection_string.take());
        }
        _ => {}
    }
    Ok(())
}

fn parse_connections(xml: &[u8]) -> ExcliResult<Vec<DataConnection>> {
    let mut reader = Reader::from_reader(xml);
    let mut connections = Vec::new();
    let mut active = None;
    let mut depth = 0usize;
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid connections XML: {error}")))?
        {
            Event::Start(ref tag) => {
                if depth == 1 && tag.local_name().as_ref() == b"connection" {
                    active = new_connection(tag)?;
                } else if depth == 2 {
                    if let Some(connection) = active.as_mut() {
                        apply_source(tag, connection)?;
                    }
                }
                depth += 1;
            }
            Event::Empty(ref tag) => {
                if depth == 1 && tag.local_name().as_ref() == b"connection" {
                    if let Some(connection) = new_connection(tag)? {
                        connections.push(connection);
                    }
                } else if depth == 2 {
                    if let Some(connection) = active.as_mut() {
                        apply_source(tag, connection)?;
                    }
                }
            }
            Event::End(ref tag) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| malformed("Unbalanced connections XML"))?;
                if depth == 1 && tag.local_name().as_ref() == b"connection" {
                    if let Some(connection) = active.take() {
                        connections.push(connection);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(connections)
}

pub(crate) fn read_connections<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    workbook_path: &str,
) -> ExcliResult<Vec<DataConnection>> {
    let relations = relationships(&read_part(archive, &rels_path(workbook_path)?)?)?;
    let mut targets = relations
        .values()
        .filter(|(kind, _)| kind.ends_with("/connections"));
    let Some((_, target)) = targets.next() else {
        return Ok(Vec::new());
    };
    if targets.next().is_some() {
        return Err(malformed("Workbook has multiple Connections parts"));
    }
    let path = resolve_part(workbook_path, target)?;
    parse_connections(&read_part(archive, &path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::Workbook;
    use std::collections::HashMap;

    use crate::ooxml_edit::write_package_parts;

    #[test]
    fn ignores_deleted_and_nested_extension_connections() {
        let xml = b"<connections><connection name=\"Removed\" deleted=\"1\"/><connection name=\"Live\" type=\"5\"><extLst><connection name=\"Nested\" type=\"1\"/></extLst></connection></connections>";
        let connections = parse_connections(xml).unwrap();
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "Live");
        assert!(matches!(
            connections[0].connection_type,
            ConnectionType::OleDb
        ));
    }

    #[test]
    fn reads_connection_types_and_sources_from_workbook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("connections.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let rels =
            String::from_utf8(read_part(&mut archive, "xl/_rels/workbook.xml.rels").unwrap())
                .unwrap();
        let rels = rels.replace("</Relationships>", "<Relationship Id=\"rId20\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections\" Target=\"connections.xml\"/></Relationships>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/connections.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml\"/></Types>");
        let connection_xml = b"<connections><connection id=\"1\" name=\"Database\" type=\"1\"><dbPr connection=\"DSN=Warehouse\" command=\"SELECT * FROM Sales\"/></connection><connection id=\"2\" name=\"Web\" type=\"4\"><webPr url=\"https://example.com/data\"/></connection><connection id=\"3\" name=\"Text\" type=\"6\"><textPr sourceFile=\"../data.csv\"/></connection><connection id=\"4\" name=\"Other\" type=\"103\"/></connections>".to_vec();
        write_package_parts(
            &path,
            &mut archive,
            &HashMap::from([
                ("xl/_rels/workbook.xml.rels".to_owned(), rels.into_bytes()),
                ("[Content_Types].xml".to_owned(), types.into_bytes()),
            ]),
            &HashMap::from([("xl/connections.xml".to_owned(), connection_xml)]),
        )
        .unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.connections.len(), 4);
        assert!(matches!(
            loaded.connections[0].connection_type,
            ConnectionType::Odbc
        ));
        assert_eq!(
            loaded.connections[0].connection_string.as_deref(),
            Some("DSN=Warehouse")
        );
        assert_eq!(
            loaded.connections[0].command_text.as_deref(),
            Some("SELECT * FROM Sales")
        );
        assert!(matches!(
            loaded.connections[1].connection_type,
            ConnectionType::Web
        ));
        assert_eq!(
            loaded.connections[1].connection_string.as_deref(),
            Some("https://example.com/data")
        );
        assert!(matches!(
            loaded.connections[2].connection_type,
            ConnectionType::Text
        ));
        assert_eq!(
            loaded.connections[2].connection_string.as_deref(),
            Some("../data.csv")
        );
        assert!(matches!(
            loaded.connections[3].connection_type,
            ConnectionType::Other
        ));
    }
}
