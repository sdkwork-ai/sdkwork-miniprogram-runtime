use serde::{Deserialize, Serialize};

/// Half-open byte range in the original UTF-8 source.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Span {
    /// Inclusive starting byte offset in the UTF-8 source.
    pub start: usize,
    /// Exclusive ending byte offset in the UTF-8 source.
    pub end: usize,
    /// 1-based source line.
    pub line: usize,
    /// 1-based Unicode scalar column.
    pub column: usize,
}

/// Parsed template document. Whitespace-only text nodes are preserved.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Document {
    /// Top-level nodes in source order.
    pub children: Vec<Node>,
}

/// A WXML node represented by the structural parser.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Node {
    /// Nested element and its contents.
    Element(Element),
    /// Text outside interpolation delimiters.
    Text {
        /// Text content as written in the source file.
        value: String,
        /// Source location of this text segment.
        span: Span,
    },
    /// Expression body retained without evaluating it.
    Interpolation {
        /// Unparsed expression body inside `{{ ... }}`.
        expression: String,
        /// Source location including interpolation delimiters.
        span: Span,
    },
}

/// Element node with source-located attributes and children.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Element {
    /// Exact source element name.
    pub tag_name: String,
    /// Attributes in source order.
    pub attributes: Vec<Attribute>,
    /// Recognized control-flow directives derived from the original attributes.
    /// The source attributes remain available in `attributes` as written.
    pub directives: Vec<ElementDirective>,
    /// Child nodes in source order.
    pub children: Vec<Node>,
    /// Source range from the opening `<` through the closing delimiter.
    pub span: Span,
    /// Whether the source used a self-closing delimiter.
    pub self_closing: bool,
}

/// Recognized structural control-flow metadata attached to an element.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ElementDirective {
    /// Conditional branch directive. Conditions remain unevaluated source expressions.
    Conditional {
        /// Branch kind selected by the source attribute.
        kind: ConditionalDirectiveKind,
        /// Expression body, or `None` for `wx:else`.
        expression: Option<String>,
        /// Source range of the directive attribute.
        span: Span,
    },
    /// List-rendering directive. The collection expression remains unevaluated.
    ForEach {
        /// Expression yielding the collection.
        expression: String,
        /// Bound item variable, defaulting to `item`.
        item: String,
        /// Bound index variable, defaulting to `index`.
        index: String,
        /// Key property, or `*` when the source explicitly requests index keys.
        key: Option<String>,
        /// Source range of the `wx:for` attribute.
        span: Span,
    },
}

/// Conditional branch selected by a WXML control-flow directive.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ConditionalDirectiveKind {
    /// `wx:if` branch.
    If,
    /// `wx:elif` branch.
    ElseIf,
    /// `wx:else` branch.
    Else,
}

/// Attribute attached to an element.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Attribute {
    /// Exact source attribute name.
    pub name: String,
    /// Parsed literal or expression segments.
    pub value: AttributeValue,
    /// Event binding mode inferred from the name, if applicable.
    pub binding: Option<BindingKind>,
    /// Source range covering the attribute name and value.
    pub span: Span,
}

/// Attribute contents, retained as source text for a later compiler stage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AttributeValue {
    /// Boolean attribute without an explicit `=` value.
    Boolean,
    /// Quoted or unquoted literal value.
    Literal(String),
    /// Value consisting of exactly one `{{ ... }}` expression.
    Expression(String),
    /// Mixed literal and expression segments, in source order.
    Interpolated(Vec<ValuePart>),
}

/// Literal or expression fragment within a mixed attribute value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ValuePart {
    /// Literal segment of a mixed attribute value.
    Literal(String),
    /// Unparsed expression body of a mixed attribute value.
    Expression(String),
}

/// Binding semantics inferred from the public WXML attribute spelling.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BindingKind {
    /// Bubbling event listener.
    Bind,
    /// Bubbling event listener that stops propagation.
    Catch,
    /// Capturing event listener.
    CaptureBind,
    /// Capturing event listener that stops propagation.
    CaptureCatch,
    /// Two-way component property binding.
    MutBind,
}
