use std::ops::Range;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u32);

impl FileId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn index(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(u32);

impl ByteOffset {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn index(self) -> u32 {
        self.0
    }

    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl From<u32> for ByteOffset {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    file_id: FileId,
    start: ByteOffset,
    end: ByteOffset,
}

impl Span {
    pub fn new(file_id: FileId, start: ByteOffset, end: ByteOffset) -> Self {
        debug_assert!(start <= end, "span start must not exceed span end");
        Self {
            file_id,
            start,
            end,
        }
    }

    pub const fn file_id(self) -> FileId {
        self.file_id
    }

    pub const fn start(self) -> ByteOffset {
        self.start
    }

    pub const fn end(self) -> ByteOffset {
        self.end
    }

    pub const fn len(self) -> u32 {
        self.end.index() - self.start.index()
    }

    pub const fn is_empty(self) -> bool {
        self.start.index() == self.end.index()
    }

    pub fn to_range(self) -> Range<usize> {
        self.start.as_usize()..self.end.as_usize()
    }

    pub fn cover(self, other: Self) -> Option<Self> {
        if self.file_id != other.file_id {
            return None;
        }

        Some(Self::new(
            self.file_id,
            ByteOffset::new(self.start.index().min(other.start.index())),
            ByteOffset::new(self.end.index().max(other.end.index())),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub offset: ByteOffset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub span: Span,
    pub value: T,
}

impl<T> Spanned<T> {
    pub fn new(span: Span, value: T) -> Self {
        Self { span, value }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            span: self.span,
            value: f(self.value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    file_id: FileId,
    path: PathBuf,
    contents: String,
    line_starts: Vec<ByteOffset>,
}

impl SourceFile {
    pub fn new(path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        Self::with_id(FileId::new(0), path, contents)
    }

    pub fn with_id(file_id: FileId, path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        let contents = contents.into();
        assert!(
            u32::try_from(contents.len()).is_ok(),
            "source files larger than 4 GiB are not supported"
        );

        let mut line_starts = vec![ByteOffset::new(0)];
        for (idx, byte) in contents.bytes().enumerate() {
            if byte == b'\n' {
                let start = idx + 1;
                let offset = ByteOffset::new(start as u32);
                if line_starts.last().copied() != Some(offset) {
                    line_starts.push(offset);
                }
            }
        }

        Self {
            file_id,
            path: path.into(),
            contents,
            line_starts,
        }
    }

    pub const fn file_id(&self) -> FileId {
        self.file_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn contents(&self) -> &str {
        &self.contents
    }

    pub fn len(&self) -> usize {
        self.contents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn span(&self, start: usize, end: usize) -> Span {
        assert!(start <= end, "span start must not exceed span end");
        assert!(end <= self.contents.len(), "span end is out of bounds");
        assert!(
            self.contents.is_char_boundary(start) && self.contents.is_char_boundary(end),
            "spans must land on UTF-8 boundaries"
        );

        Span::new(
            self.file_id,
            ByteOffset::new(start as u32),
            ByteOffset::new(end as u32),
        )
    }

    pub fn eof_span(&self) -> Span {
        let end = self.contents.len();
        self.span(end, end)
    }

    pub fn span_text(&self, span: Span) -> Option<&str> {
        if span.file_id() != self.file_id || span.end().as_usize() > self.contents.len() {
            return None;
        }

        self.contents.get(span.to_range())
    }

    pub fn line_text(&self, line: usize) -> Option<&str> {
        let range = self.line_range(line)?;
        self.contents.get(range)
    }

    pub fn line_span(&self, line: usize) -> Option<Span> {
        let range = self.line_range(line)?;
        Some(self.span(range.start, range.end))
    }

    pub fn location(&self, offset: ByteOffset) -> Option<SourceLocation> {
        let offset = offset.as_usize();
        if offset > self.contents.len() || !self.contents.is_char_boundary(offset) {
            return None;
        }

        let line_index = match self
            .line_starts
            .binary_search_by_key(&(offset as u32), |line_start| line_start.index())
        {
            Ok(index) => index,
            Err(0) => return None,
            Err(index) => index - 1,
        };

        let line_start = self.line_starts[line_index].as_usize();
        let column = self.contents[line_start..offset].chars().count() + 1;

        Some(SourceLocation {
            line: line_index + 1,
            column,
            offset: ByteOffset::new(offset as u32),
        })
    }

    fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let line_index = line.checked_sub(1)?;
        let start = self.line_starts.get(line_index)?.as_usize();
        let mut end = self
            .line_starts
            .get(line_index + 1)
            .map(|offset| offset.as_usize())
            .unwrap_or_else(|| self.contents.len());

        if end > start && self.contents.as_bytes().get(end - 1) == Some(&b'\n') {
            end -= 1;
        }

        if end > start && self.contents.as_bytes().get(end - 1) == Some(&b'\r') {
            end -= 1;
        }

        Some(start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::{ByteOffset, FileId, SourceFile};

    #[test]
    fn source_file_tracks_lines_and_locations() {
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

    #[test]
    fn source_file_extracts_spans() {
        let source = SourceFile::new("demo.llg", "observe count <= limit;\n");
        let span = source.span(8, 13);

        assert_eq!(source.span_text(span), Some("count"));
        assert_eq!(source.line_span(1), Some(source.span(0, 23)));
    }
}
