use crate::Span;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

/// Result type returned by the WXML parser.
pub type ParseResult<T> = Result<T, Diagnostic>;

/// Classification for a structural WXML parse failure.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DiagnosticKind {
    /// Input ended before a required closing tag.
    UnexpectedEnd,
    /// Element or attribute name was missing.
    InvalidName,
    /// Attribute assignment did not contain a value.
    InvalidAttribute,
    /// Element delimiter was incomplete.
    UnterminatedTag,
    /// Comment did not contain its closing delimiter.
    UnterminatedComment,
    /// Quoted attribute value did not close.
    UnterminatedString,
    /// Interpolation did not contain balanced closing delimiters.
    UnterminatedInterpolation,
    /// Closing tag appeared outside any element.
    UnexpectedClosingTag,
    /// Closing tag did not match the open element.
    MismatchedClosingTag,
}

/// Parse diagnostic with a file and 1-based source location.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    /// Source file path supplied to [`crate::parse`].
    pub file: String,
    /// Stable parser diagnostic category.
    pub kind: DiagnosticKind,
    /// Human-readable reason for the failure.
    pub message: String,
    /// Zero-width source location where parsing failed.
    pub span: Span,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: {}",
            self.file, self.span.line, self.span.column, self.message
        )
    }
}

impl Error for Diagnostic {}
