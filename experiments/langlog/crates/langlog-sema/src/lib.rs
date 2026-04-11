use langlog_syntax::ParsedModule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    pub parsed: ParsedModule,
}

pub fn analyze(parsed: ParsedModule) -> CheckedProgram {
    CheckedProgram { parsed }
}
