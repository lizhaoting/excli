//! Legacy OOXML cell comments (notes).

use std::io::{Read, Seek};

use excli_core::cell::Comment as CellComment;
use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::reference::CellReference;
use excli_core::sheet::Comment;
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
            item.map_err(|error| corrupted(format!("Invalid comment attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| {
                        corrupted(format!("Invalid comment attribute value: {error}"))
                    })?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

struct PendingComment {
    cell: String,
    author_id: Option<usize>,
    text: String,
}

fn parse_comments(xml: &[u8]) -> ExcliResult<Vec<Comment>> {
    let mut reader = Reader::from_reader(xml);
    let mut authors = Vec::new();
    let mut author = None::<String>;
    let mut comment = None::<PendingComment>;
    let mut in_comment_text = false;
    let mut in_text_run = false;
    let mut comments = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid comments XML: {error}")))?
        {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"author" => {
                author = Some(String::new());
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"comment" => {
                if comment.is_some() {
                    return Err(corrupted("Nested comments"));
                }
                let cell = attribute(tag, b"ref")?
                    .ok_or_else(|| corrupted("Comment has no cell reference"))?;
                if CellReference::from_a1(&cell).is_none() {
                    return Err(corrupted(format!("Invalid comment cell reference: {cell}")));
                }
                let author_id = attribute(tag, b"authorId")?
                    .map(|value| {
                        value
                            .parse::<usize>()
                            .map_err(|_| corrupted("Invalid comment author ID"))
                    })
                    .transpose()?;
                comment = Some(PendingComment {
                    cell,
                    author_id,
                    text: String::new(),
                });
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"text" && comment.is_some() => {
                in_comment_text = true;
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"t" && in_comment_text => {
                in_text_run = true;
            }
            Event::Text(ref text) => {
                let value = text
                    .unescape()
                    .map_err(|error| corrupted(format!("Invalid comment text: {error}")))?;
                if let Some(author) = &mut author {
                    author.push_str(&value);
                } else if in_text_run {
                    if let Some(comment) = &mut comment {
                        comment.text.push_str(&value);
                    }
                }
            }
            Event::CData(ref text) => {
                let value = std::str::from_utf8(text.as_ref())
                    .map_err(|error| corrupted(format!("Invalid comment text: {error}")))?;
                if let Some(author) = &mut author {
                    author.push_str(value);
                } else if in_text_run {
                    if let Some(comment) = &mut comment {
                        comment.text.push_str(value);
                    }
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"t" => in_text_run = false,
            Event::End(ref tag) if tag.local_name().as_ref() == b"text" => in_comment_text = false,
            Event::End(ref tag) if tag.local_name().as_ref() == b"author" => {
                authors.push(
                    author
                        .take()
                        .ok_or_else(|| corrupted("Unmatched author end"))?,
                );
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"comment" => {
                let pending = comment
                    .take()
                    .ok_or_else(|| corrupted("Unmatched comment end"))?;
                let author = pending
                    .author_id
                    .map(|id| {
                        authors
                            .get(id)
                            .cloned()
                            .ok_or_else(|| corrupted("Comment author ID is out of range"))
                    })
                    .transpose()?;
                comments.push(Comment {
                    cell: pending.cell,
                    text: pending.text,
                    author,
                });
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if author.is_some() || comment.is_some() {
        return Err(corrupted("Unclosed comment element"));
    }
    Ok(comments)
}

pub(crate) fn enrich_sheet<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
    sheet: &mut Sheet,
) -> ExcliResult<()> {
    let relations = related_parts(archive, worksheet_path)?;
    for (_, (kind, path)) in relations {
        if !kind.ends_with("/comments") {
            continue;
        }
        for comment in parse_comments(&read_part(archive, &path)?)? {
            if let Some(reference) = CellReference::from_a1(&comment.cell) {
                if let Some(cell) = sheet
                    .rows
                    .get_mut(reference.row as usize)
                    .and_then(|row| row.cells.get_mut(reference.column as usize))
                {
                    cell.comment = Some(CellComment::new(
                        comment.text.clone(),
                        comment.author.clone(),
                    ));
                }
            }
            sheet.comments.push(comment);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::Workbook;
    use std::collections::HashMap;

    use crate::ooxml_edit::write_package_parts;
    use crate::ooxml_package::read_part;

    #[test]
    fn parses_authors_and_rich_text_runs() {
        let xml = r#"<comments><authors><author>Alice &amp; Bob</author><author>陈</author></authors><commentList><comment ref="B2" authorId="0"><text><r><t>First</t></r><r><t> line</t></r></text></comment><comment ref="C3" authorId="1"><text><t>你好</t></text></comment></commentList></comments>"#;
        let comments = parse_comments(xml.as_bytes()).unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].cell, "B2");
        assert_eq!(comments[0].text, "First line");
        assert_eq!(comments[0].author.as_deref(), Some("Alice & Bob"));
        assert_eq!(comments[1].text, "你好");
        assert_eq!(comments[1].author.as_deref(), Some("陈"));
    }

    #[test]
    fn rejects_invalid_author_reference() {
        let xml = br#"<comments><authors><author>Alice</author></authors><commentList><comment ref="A1" authorId="4"><text><t>x</t></text></comment></commentList></comments>"#;
        assert!(parse_comments(xml).is_err());
    }

    #[test]
    fn reads_comment_through_worksheet_relationship() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comments.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::string("Value"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/comments/comment1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml\"/></Types>");
        write_package_parts(
            &path,
            &mut archive,
            &HashMap::from([("[Content_Types].xml".to_owned(), types.into_bytes())]),
            &HashMap::from([
                ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments\" Target=\"../comments/comment1.xml\"/></Relationships>".to_vec()),
                ("xl/comments/comment1.xml".to_owned(), b"<comments><authors><author>Alice</author></authors><commentList><comment ref=\"A1\" authorId=\"0\"><text><t>Review this</t></text></comment></commentList></comments>".to_vec()),
            ]),
        )
        .unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[0].comments[0].text, "Review this");
        assert_eq!(
            loaded.sheets[0].comments[0].author.as_deref(),
            Some("Alice")
        );
        assert_eq!(
            loaded.sheets[0]
                .get_cell(0, 0)
                .unwrap()
                .comment
                .as_ref()
                .unwrap()
                .text,
            "Review this"
        );
    }
}
