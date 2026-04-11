use langlog_syntax::ParsedModule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    pub parsed: ParsedModule,
}

//= SPEC.md#llg-sema-01-name-resolution-and-scopes
//= type=todo
//# The semantic phase MUST resolve item, parameter, and local bindings according to lexical scope.
//= SPEC.md#llg-sema-01-name-resolution-and-scopes
//= type=todo
//# The semantic phase MUST reject references to undefined bindings.
//= SPEC.md#llg-sema-02-totality-constraints
//= type=todo
//# The semantic phase MUST reject direct recursion.
//= SPEC.md#llg-sema-02-totality-constraints
//= type=todo
//# The semantic phase MUST reject indirect recursion.
//= SPEC.md#llg-sema-02-totality-constraints
//= type=todo
//# The semantic phase MUST reject unbounded iteration forms that are outside the bounded phase 1 loop model.
pub fn analyze(parsed: ParsedModule) -> CheckedProgram {
    CheckedProgram { parsed }
}
