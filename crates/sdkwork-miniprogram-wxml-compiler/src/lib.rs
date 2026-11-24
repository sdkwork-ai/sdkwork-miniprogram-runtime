//! Parsing primitives for `WeChat` Mini Program WXML templates.
//!
//! This crate currently parses document structure, attributes, text, and
//! balanced `{{ ... }}` interpolations. It does not evaluate expressions or
//! implement template imports, WXS, or component semantics.

mod ast;
mod diagnostic;
mod parser;

pub use ast::{
    Attribute, AttributeValue, BindingKind, ConditionalDirectiveKind, Document, Element,
    ElementDirective, Node, Span, ValuePart,
};
pub use diagnostic::{Diagnostic, DiagnosticKind, ParseResult};

/// Parse a WXML document into source-located structural nodes.
///
/// Expressions remain unevaluated source text. The first malformed structure
/// is returned as a [`Diagnostic`] with the input file and source location.
pub use parser::parse;
