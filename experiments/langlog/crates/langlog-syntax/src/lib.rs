pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
#[cfg(test)]
mod requirement_tests;
pub mod span;
pub mod token;

pub use ast::{
    AssignmentTransfer, Block, Expr, ExprKind, Function, Item, Module, ObserveOp, ParamTransfer,
    PlaceMode, Stmt, Task, TrustedOperation, Type, TypeKind,
};
pub use diagnostic::{Diagnostic, Label, LabelStyle, Severity};
pub use lexer::{lex, lex_source, LexedSource};
pub use parser::{parse_lexed, ParsedModule};
pub use span::{ByteOffset, FileId, SourceFile, SourceLocation, Span, Spanned};
pub use token::{Token, TokenKind, TokenTag};

pub fn parse(path: impl Into<std::path::PathBuf>, contents: impl Into<String>) -> ParsedModule {
    parse_lexed(lex(path, contents))
}
