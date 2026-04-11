use langlog_syntax::SourceFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    pub source: SourceFile,
}

pub fn analyze(source: SourceFile) -> CheckedProgram {
    CheckedProgram { source }
}
