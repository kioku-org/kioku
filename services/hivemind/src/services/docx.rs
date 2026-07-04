use docx_rs::{
    DocumentChild, InsertChild, ParagraphChild, RunChild, TableCellContent, TableChild,
    TableRowChild,
};

/// Extract plain text content from a .docx file's raw bytes (paragraphs + table cells).
pub fn extract_text_from_bytes(data: &[u8]) -> anyhow::Result<String> {
    let docx = docx_rs::read_docx(data).map_err(|e| anyhow::anyhow!("Failed to parse DOCX: {}", e))?;

    let mut out = String::new();
    for child in &docx.document.children {
        push_document_child(child, &mut out);
    }
    Ok(out)
}

fn push_document_child(child: &DocumentChild, out: &mut String) {
    match child {
        DocumentChild::Paragraph(p) => {
            for pc in &p.children {
                push_paragraph_child(pc, out);
            }
            out.push('\n');
        }
        DocumentChild::Table(t) => push_table(t, out),
        _ => {}
    }
}

fn push_paragraph_child(child: &ParagraphChild, out: &mut String) {
    match child {
        ParagraphChild::Run(r) => {
            for rc in &r.children {
                push_run_child(rc, out);
            }
        }
        ParagraphChild::Hyperlink(h) => {
            for pc in &h.children {
                push_paragraph_child(pc, out);
            }
        }
        // Tracked-change insertions are live, visible content in Word (pending review, not
        // rejected) — include them. Deletions (ParagraphChild::Delete) stay excluded via the
        // catch-all below, since that text is struck through / removed.
        ParagraphChild::Insert(i) => {
            for ic in &i.children {
                if let InsertChild::Run(r) = ic {
                    for rc in &r.children {
                        push_run_child(rc, out);
                    }
                }
            }
        }
        _ => {}
    }
}

fn push_run_child(child: &RunChild, out: &mut String) {
    match child {
        RunChild::Text(t) => out.push_str(&t.text),
        RunChild::Tab(_) => out.push('\t'),
        RunChild::Break(_) => out.push('\n'),
        _ => {}
    }
}

fn push_table(table: &docx_rs::Table, out: &mut String) {
    for row in &table.rows {
        let TableChild::TableRow(row) = row;
        for cell in &row.cells {
            let TableRowChild::TableCell(cell) = cell;
            for content in &cell.children {
                match content {
                    TableCellContent::Paragraph(p) => {
                        for pc in &p.children {
                            push_paragraph_child(pc, out);
                        }
                        out.push('\t');
                    }
                    TableCellContent::Table(t) => push_table(t, out),
                    _ => {}
                }
            }
        }
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use docx_rs::Docx;

    fn build_test_docx() -> Vec<u8> {
        let docx = Docx::new()
            .add_paragraph(docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text("Hello from a test document.")))
            .add_paragraph(docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text("Second paragraph.")));
        let mut buf = Vec::new();
        docx.build().pack(&mut std::io::Cursor::new(&mut buf)).unwrap();
        buf
    }

    #[test]
    fn extracts_paragraph_text() {
        let bytes = build_test_docx();
        let text = extract_text_from_bytes(&bytes).unwrap();
        assert!(text.contains("Hello from a test document."));
        assert!(text.contains("Second paragraph."));
    }
}
