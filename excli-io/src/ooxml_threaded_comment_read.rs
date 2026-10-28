//! Modern OOXML threaded comments and workbook person identities.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek};

use excli_core::cell::Comment as CellComment;
use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::reference::CellReference;
use excli_core::sheet::{ThreadedComment, ThreadedMention};
use excli_core::Sheet;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::ooxml_package::{read_part, related_parts};

pub(crate) type People = HashMap<String, Option<String>>;

fn corrupted(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item = item
            .map_err(|error| corrupted(format!("Invalid threaded comment attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| corrupted(format!("Invalid threaded comment value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

pub(crate) fn people<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    workbook_path: &str,
) -> ExcliResult<People> {
    let mut people = HashMap::new();
    for (_, (kind, path)) in related_parts(archive, workbook_path)? {
        if !kind.ends_with("/person") {
            continue;
        }
        let xml = read_part(archive, &path)?;
        let mut reader = Reader::from_reader(xml.as_slice());
        loop {
            match reader
                .read_event()
                .map_err(|error| corrupted(format!("Invalid persons XML: {error}")))?
            {
                Event::Start(ref tag) | Event::Empty(ref tag)
                    if tag.local_name().as_ref() == b"person" =>
                {
                    let id = attribute(tag, b"id")?.ok_or_else(|| corrupted("Person has no ID"))?;
                    let name = attribute(tag, b"displayName")?;
                    if people.insert(id.to_lowercase(), name).is_some() {
                        return Err(corrupted("Duplicate person ID"));
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    Ok(people)
}

struct Pending {
    id: String,
    parent_id: Option<String>,
    cell: Option<String>,
    text: String,
    person_id: String,
    created_at: Option<String>,
    done: bool,
    mentions: Vec<PendingMention>,
}

struct PendingMention {
    id: String,
    person_id: String,
    start_index: u32,
    length: u32,
}

impl PendingMention {
    fn from_tag(tag: &BytesStart<'_>) -> ExcliResult<Self> {
        let number = |key: &[u8]| -> ExcliResult<u32> {
            attribute(tag, key)?
                .ok_or_else(|| corrupted("Mention is missing a text position"))?
                .parse::<u32>()
                .map_err(|_| corrupted("Invalid mention text position"))
        };
        Ok(Self {
            id: attribute(tag, b"mentionId")?.ok_or_else(|| corrupted("Mention has no ID"))?,
            person_id: attribute(tag, b"mentionpersonId")?
                .ok_or_else(|| corrupted("Mention has no person ID"))?,
            start_index: number(b"startIndex")?,
            length: number(b"length")?,
        })
    }
}

impl Pending {
    fn from_tag(tag: &BytesStart<'_>) -> ExcliResult<Self> {
        let done = match attribute(tag, b"done")?.as_deref() {
            Some("1" | "true") => true,
            None | Some("0" | "false") => false,
            Some(_) => return Err(corrupted("Invalid threaded comment done flag")),
        };
        let cell = attribute(tag, b"ref")?;
        if cell
            .as_deref()
            .is_some_and(|value| CellReference::from_a1(value).is_none())
        {
            return Err(corrupted("Invalid threaded comment cell reference"));
        }
        Ok(Self {
            id: attribute(tag, b"id")?.ok_or_else(|| corrupted("Threaded comment has no ID"))?,
            parent_id: attribute(tag, b"parentId")?,
            cell,
            text: String::new(),
            person_id: attribute(tag, b"personId")?
                .ok_or_else(|| corrupted("Threaded comment has no person ID"))?,
            created_at: attribute(tag, b"dT")?,
            done,
            mentions: Vec::new(),
        })
    }
}

fn parse_threaded_comments(xml: &[u8], people: &People) -> ExcliResult<Vec<ThreadedComment>> {
    let mut reader = Reader::from_reader(xml);
    let mut pending = Vec::<Pending>::new();
    let mut current = None::<Pending>;
    let mut in_text = false;
    let mut in_mentions = false;
    loop {
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid threaded comments XML: {error}")))?
        {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"threadedComment" => {
                if current.is_some() {
                    return Err(corrupted("Nested threaded comments"));
                }
                current = Some(Pending::from_tag(tag)?);
            }
            Event::Empty(ref tag) if tag.local_name().as_ref() == b"threadedComment" => {
                pending.push(Pending::from_tag(tag)?);
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"text" && current.is_some() => {
                in_text = true;
            }
            Event::Start(ref tag)
                if tag.local_name().as_ref() == b"mentions" && current.is_some() =>
            {
                in_mentions = true;
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"mention" && in_mentions =>
            {
                if let Some(comment) = &mut current {
                    comment.mentions.push(PendingMention::from_tag(tag)?);
                }
            }
            Event::Text(ref text) if in_text => {
                if let Some(comment) = &mut current {
                    comment.text.push_str(&text.unescape().map_err(|error| {
                        corrupted(format!("Invalid threaded comment text: {error}"))
                    })?);
                }
            }
            Event::CData(ref text) if in_text => {
                if let Some(comment) = &mut current {
                    comment
                        .text
                        .push_str(std::str::from_utf8(text.as_ref()).map_err(|error| {
                            corrupted(format!("Invalid threaded comment text: {error}"))
                        })?);
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"text" => in_text = false,
            Event::End(ref tag) if tag.local_name().as_ref() == b"mentions" => in_mentions = false,
            Event::End(ref tag) if tag.local_name().as_ref() == b"threadedComment" => {
                pending.push(
                    current
                        .take()
                        .ok_or_else(|| corrupted("Unmatched threaded comment end"))?,
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if current.is_some() {
        return Err(corrupted("Unclosed threaded comment"));
    }
    let mut by_id = HashMap::new();
    for (index, comment) in pending.iter().enumerate() {
        if by_id.insert(comment.id.to_lowercase(), index).is_some() {
            return Err(corrupted("Duplicate threaded comment ID"));
        }
    }
    let mut result = Vec::with_capacity(pending.len());
    for comment in &pending {
        let mut cursor = comment;
        let mut visited = HashSet::new();
        let cell = loop {
            if !visited.insert(cursor.id.to_lowercase()) {
                return Err(corrupted("Cyclic threaded comment parent IDs"));
            }
            if let Some(cell) = &cursor.cell {
                break cell.clone();
            }
            let parent_id = cursor
                .parent_id
                .as_deref()
                .ok_or_else(|| corrupted("Threaded comment has no cell or parent"))?;
            let parent = by_id
                .get(&parent_id.to_lowercase())
                .ok_or_else(|| corrupted("Missing parent threaded comment"))?;
            cursor = &pending[*parent];
        };
        let author = people
            .get(&comment.person_id.to_lowercase())
            .cloned()
            .ok_or_else(|| corrupted("Missing threaded comment person"))?;
        let mut mention_ids = HashSet::new();
        let mentions = comment
            .mentions
            .iter()
            .map(|mention| {
                if !mention_ids.insert(mention.id.to_lowercase()) {
                    return Err(corrupted("Duplicate mention ID"));
                }
                let person_name = people
                    .get(&mention.person_id.to_lowercase())
                    .cloned()
                    .ok_or_else(|| corrupted("Missing mentioned person"))?;
                Ok(ThreadedMention {
                    id: mention.id.clone(),
                    person_id: mention.person_id.clone(),
                    person_name,
                    start_index: mention.start_index,
                    length: mention.length,
                })
            })
            .collect::<ExcliResult<Vec<_>>>()?;
        result.push(ThreadedComment {
            id: comment.id.clone(),
            parent_id: comment.parent_id.clone(),
            cell,
            text: comment.text.clone(),
            person_id: comment.person_id.clone(),
            author,
            created_at: comment.created_at.clone(),
            done: comment.done,
            mentions,
        });
    }
    Ok(result)
}

pub(crate) fn enrich_sheet<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
    people: &People,
    sheet: &mut Sheet,
) -> ExcliResult<()> {
    for (_, (kind, path)) in related_parts(archive, worksheet_path)? {
        if !kind.ends_with("/threadedComment") {
            continue;
        }
        for comment in parse_threaded_comments(&read_part(archive, &path)?, people)? {
            if comment.parent_id.is_none() {
                if let Some(reference) = CellReference::from_a1(&comment.cell) {
                    if let Some(cell) = sheet
                        .rows
                        .get_mut(reference.row as usize)
                        .and_then(|row| row.cells.get_mut(reference.column as usize))
                    {
                        if cell.comment.is_none() {
                            cell.comment = Some(CellComment::new(
                                comment.text.clone(),
                                comment.author.clone(),
                            ));
                        }
                    }
                }
            }
            sheet.threaded_comments.push(comment);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::Workbook;

    use crate::ooxml_edit::write_package_parts;
    use crate::ooxml_package::read_part;

    #[test]
    fn resolves_reply_cell_and_people() {
        let people = HashMap::from([
            ("{alice}".to_owned(), Some("Alice".to_owned())),
            ("{bob}".to_owned(), Some("Bob".to_owned())),
        ]);
        let xml = br#"<ThreadedComments><threadedComment ref="B2" personId="{ALICE}" id="{root}" dT="2024-01-02T03:04:05Z"><text>Check &amp; fix</text><mentions><mention mentionpersonId="{bob}" mentionId="{mention}" startIndex="6" length="3"/></mentions></threadedComment><threadedComment personId="{bob}" id="{reply}" parentId="{ROOT}" done="1"><text>Done</text></threadedComment></ThreadedComments>"#;
        let comments = parse_threaded_comments(xml, &people).unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].text, "Check & fix");
        assert_eq!(comments[0].author.as_deref(), Some("Alice"));
        assert_eq!(comments[0].mentions.len(), 1);
        assert_eq!(comments[0].mentions[0].person_name.as_deref(), Some("Bob"));
        assert_eq!(comments[0].mentions[0].start_index, 6);
        assert_eq!(comments[0].mentions[0].length, 3);
        assert_eq!(
            comments[0].created_at.as_deref(),
            Some("2024-01-02T03:04:05Z")
        );
        assert_eq!(comments[1].cell, "B2");
        assert_eq!(comments[1].parent_id.as_deref(), Some("{ROOT}"));
        assert_eq!(comments[1].author.as_deref(), Some("Bob"));
        assert!(comments[1].done);
    }

    #[test]
    fn rejects_missing_parent_and_person() {
        let xml = br#"<ThreadedComments><threadedComment personId="{missing}" id="{reply}" parentId="{unknown}"><text>x</text></threadedComment></ThreadedComments>"#;
        assert!(parse_threaded_comments(xml, &HashMap::new()).is_err());
    }

    #[test]
    fn rejects_missing_mentioned_person() {
        let people = HashMap::from([("{alice}".to_owned(), Some("Alice".to_owned()))]);
        let xml = br#"<ThreadedComments><threadedComment ref="A1" personId="{alice}" id="{root}"><text>Hello</text><mentions><mention mentionpersonId="{missing}" mentionId="{m}" startIndex="0" length="5"/></mentions></threadedComment></ThreadedComments>"#;
        assert!(parse_threaded_comments(xml, &people).is_err());
    }

    #[test]
    fn reads_threaded_comments_from_workbook_package() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("threads.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::string("Value"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let rels =
            String::from_utf8(read_part(&mut archive, "xl/_rels/workbook.xml.rels").unwrap())
                .unwrap();
        let rels = rels.replace("</Relationships>", "<Relationship Id=\"rId99\" Type=\"http://schemas.microsoft.com/office/2017/10/relationships/person\" Target=\"persons/person.xml\"/></Relationships>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/persons/person.xml\" ContentType=\"application/vnd.ms-excel.person+xml\"/><Override PartName=\"/xl/threadedComments/threadedComment1.xml\" ContentType=\"application/vnd.ms-excel.threadedcomments+xml\"/></Types>");
        write_package_parts(
            &path,
            &mut archive,
            &HashMap::from([
                ("xl/_rels/workbook.xml.rels".to_owned(), rels.into_bytes()),
                ("[Content_Types].xml".to_owned(), types.into_bytes()),
            ]),
            &HashMap::from([
                ("xl/persons/person.xml".to_owned(), b"<personList><person id=\"{a}\" displayName=\"Alice\"/><person id=\"{b}\" displayName=\"Bob\"/></personList>".to_vec()),
                ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId9\" Type=\"http://schemas.microsoft.com/office/2017/10/relationships/threadedComment\" Target=\"../threadedComments/threadedComment1.xml\"/></Relationships>".to_vec()),
                ("xl/threadedComments/threadedComment1.xml".to_owned(), b"<ThreadedComments><threadedComment ref=\"A1\" personId=\"{a}\" id=\"{root}\"><text>Review</text><mentions><mention mentionpersonId=\"{b}\" mentionId=\"{m}\" startIndex=\"0\" length=\"3\"/></mentions></threadedComment><threadedComment personId=\"{b}\" id=\"{reply}\" parentId=\"{root}\"><text>Done</text></threadedComment></ThreadedComments>".to_vec()),
            ]),
        )
        .unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[0].threaded_comments.len(), 2);
        assert_eq!(
            loaded.sheets[0].threaded_comments[0].mentions[0]
                .person_name
                .as_deref(),
            Some("Bob")
        );
        assert_eq!(loaded.sheets[0].threaded_comments[1].cell, "A1");
        assert_eq!(
            loaded.sheets[0].threaded_comments[1].author.as_deref(),
            Some("Bob")
        );
        assert_eq!(
            loaded.sheets[0]
                .get_cell(0, 0)
                .unwrap()
                .comment
                .as_ref()
                .unwrap()
                .text,
            "Review"
        );
    }
}
