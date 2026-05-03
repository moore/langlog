use super::{ByteOffset, FileId, SourceFile, Span};

//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# Source files MUST preserve configured file identifiers, line counts, line text, and byte-offset to line/column locations.
#[test]
fn requirement_llg_diag_01_source_file_tracks_lines_and_locations() {
    let source = SourceFile::with_id(FileId::new(7), "demo.llg", "fn main() {\n    1\n}\n");

    assert_eq!(source.file_id().index(), 7);
    assert_eq!(source.line_count(), 4);
    assert_eq!(source.line_text(1), Some("fn main() {"));
    assert_eq!(source.line_text(2), Some("    1"));
    assert_eq!(source.line_text(3), Some("}"));
    assert_eq!(source.line_text(4), Some(""));

    let location = source.location(ByteOffset::new(16)).unwrap();
    assert_eq!(location.line, 2);
    assert_eq!(location.column, 5);
}

//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# Source files MUST extract source text and line spans from valid same-file spans.
#[test]
fn requirement_llg_diag_01_source_file_extracts_spans() {
    let source = SourceFile::new("demo.llg", "observe count <= limit else { return; }\n");
    let span = source.span(8, 13);

    assert_eq!(source.span_text(span), Some("count"));
    assert_eq!(source.line_span(1), Some(source.span(0, 39)));
}

//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# Span and source length helpers MUST report exact byte lengths and emptiness.
#[test]
fn requirement_llg_diag_01_span_and_source_lengths_match_the_underlying_text() {
    let source = SourceFile::new("demo.llg", "abc");
    let non_empty = source.span(0, 2);
    let empty = source.span(2, 2);

    assert_eq!(non_empty.len(), 2);
    assert!(!non_empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());
    assert_eq!(source.len(), 3);
    assert!(!source.is_empty());

    let empty_source = SourceFile::new("empty.llg", "");
    assert_eq!(empty_source.len(), 0);
    assert!(empty_source.is_empty());
}

//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# Source files MUST reject foreign spans, out-of-bounds locations, and locations that do not land on UTF-8 character boundaries.
#[test]
fn requirement_llg_diag_01_source_file_rejects_foreign_spans_and_invalid_locations() {
    let source = SourceFile::with_id(FileId::new(7), "demo.llg", "hé\n");
    let foreign = Span::new(FileId::new(9), ByteOffset::new(0), ByteOffset::new(1));

    assert_eq!(source.span_text(foreign), None);
    assert!(source
        .location(ByteOffset::new(source.len() as u32))
        .is_some());
    assert_eq!(source.location(ByteOffset::new(2)), None);
    assert_eq!(source.location(ByteOffset::new(99)), None);
}

//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# Source line helpers MUST trim CRLF line endings without trimming source content before the line ending.
#[test]
fn requirement_llg_diag_01_source_file_line_helpers_trim_crlf_endings() {
    let source = SourceFile::new("demo.llg", "one\r\ntwo\r\n");

    assert_eq!(source.line_text(1), Some("one"));
    assert_eq!(source.line_text(2), Some("two"));
    assert_eq!(source.line_text(3), Some(""));
    assert_eq!(
        source.line_span(1).and_then(|span| source.span_text(span)),
        Some("one")
    );
    assert_eq!(
        source.line_span(2).and_then(|span| source.span_text(span)),
        Some("two")
    );
    assert_eq!(
        source.line_span(3).and_then(|span| source.span_text(span)),
        Some("")
    );
}

//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# Empty source files MUST still expose one empty first line.
#[test]
fn requirement_llg_diag_01_empty_source_still_has_an_empty_first_line() {
    let source = SourceFile::new("empty.llg", "");

    assert_eq!(source.line_count(), 1);
    assert_eq!(source.line_text(1), Some(""));
    assert_eq!(
        source.line_span(1).and_then(|span| source.span_text(span)),
        Some("")
    );
}
