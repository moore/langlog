use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelStyle {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub style: LabelStyle,
    pub span: Span,
    pub message: Option<String>,
}

impl Label {
    //= SPEC.md#llg-diag-01-source-spans-and-syntax-diagnostics
    //# Syntax diagnostics MUST include a primary source span.
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Self {
            style: LabelStyle::Primary,
            span,
            message: Some(message.into()),
        }
    }

    pub fn secondary(span: Span, message: impl Into<String>) -> Self {
        Self {
            style: LabelStyle::Secondary,
            span,
            message: Some(message.into()),
        }
    }

    pub fn unlabeled(style: LabelStyle, span: Span) -> Self {
        Self {
            style,
            span,
            message: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}
