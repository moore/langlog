pub mod diagnostic;
pub mod span;

pub use diagnostic::{Diagnostic, Label, LabelStyle, Severity};
pub use span::{ByteOffset, FileId, SourceFile, SourceLocation, Span, Spanned};

pub fn parse(
    path: impl Into<std::path::PathBuf>,
    contents: impl Into<String>,
) -> SourceFile {
    SourceFile::new(path, contents)
}
