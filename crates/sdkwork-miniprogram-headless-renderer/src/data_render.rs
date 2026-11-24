use sdkwork_miniprogram_wxml_compiler::{
    AttributeValue, ConditionalDirectiveKind, Document, Element, ElementDirective, Node, Span,
    ValuePart,
};
use serde::Serialize;
use serde_json::Value;

use crate::{RenderAttribute, RenderAttributePart, RenderNode, RenderTree};

/// Hard ceiling for data-driven loop expansion, even when a caller requests a larger limit.
pub const MAX_LOOP_ITERATIONS: usize = 1_000;

/// A source-located issue encountered while evaluating the supported WXML subset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DataRenderDiagnostic {
    /// Template file supplied to [`build_with_data`].
    pub file: String,
    /// Stable diagnostic category.
    pub kind: DataRenderDiagnosticKind,
    /// Human-readable description.
    pub message: String,
    /// Original WXML source location.
    pub span: Span,
}

/// Classification for data-rendering limitations or invalid values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DataRenderDiagnosticKind {
    /// An expression is outside the supported property-path and literal subset.
    UnsupportedExpression,
    /// A loop expression did not resolve to an array.
    InvalidLoopSource,
    /// A loop exceeded the caller or implementation iteration bound.
    LoopLimitExceeded,
    /// An `wx:elif` or `wx:else` did not follow a conditional branch.
    OrphanConditional,
}

/// Render tree evaluated against one page data object.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DataRenderTree {
    /// Evaluated structural tree with stable preorder identifiers.
    pub tree: RenderTree,
    /// Unsupported expressions and bounded-loop failures.
    pub diagnostics: Vec<DataRenderDiagnostic>,
}

#[derive(Clone)]
struct Scope<'a> {
    data: &'a Value,
    locals: Vec<(String, Value)>,
}

impl<'a> Scope<'a> {
    fn new(data: &'a Value) -> Self {
        Self {
            data,
            locals: Vec::new(),
        }
    }

    fn with_local(&self, name: &str, value: Value) -> Self {
        let mut nested = self.clone();
        nested.locals.push((name.to_owned(), value));
        nested
    }

    fn resolve(&self, expression: &str) -> Option<Value> {
        let mut parts = expression.trim().split('.');
        let root = parts.next()?;
        if !is_identifier(root) {
            return None;
        }
        let local = self
            .locals
            .iter()
            .rev()
            .find_map(|(name, value)| (name == root).then_some(value));
        let mut value = local.unwrap_or(self.data);
        if local.is_none() {
            value = self.data.get(root)?;
        }
        for part in parts {
            if !is_identifier(part) {
                return None;
            }
            value = value.get(part)?;
        }
        Some(value.clone())
    }
}

/// Evaluates property-path bindings and structural control directives using page data.
///
/// Supported expressions are JSON literals, simple quoted strings, property paths,
/// unary `!`, and equality comparisons. Loops iterate over JSON arrays. The effective
/// loop limit is the smaller of `max_loop_iterations` and [`MAX_LOOP_ITERATIONS`].
#[must_use]
pub fn build_with_data(
    document: &Document,
    template_file: impl Into<String>,
    data: &Value,
    max_loop_iterations: usize,
) -> DataRenderTree {
    let file = template_file.into();
    let scope = Scope::new(data);
    let mut next_id = 0;
    let mut diagnostics = Vec::new();
    let children = render_nodes(
        &document.children,
        &scope,
        &file,
        max_loop_iterations.min(MAX_LOOP_ITERATIONS),
        &mut next_id,
        &mut diagnostics,
    );
    DataRenderTree {
        tree: RenderTree {
            children,
            node_count: next_id,
        },
        diagnostics,
    }
}

fn render_nodes(
    nodes: &[Node],
    scope: &Scope<'_>,
    file: &str,
    max_loop_iterations: usize,
    next_id: &mut usize,
    diagnostics: &mut Vec<DataRenderDiagnostic>,
) -> Vec<RenderNode> {
    let mut rendered = Vec::new();
    let mut index = 0;
    while index < nodes.len() {
        if conditional_kind(&nodes[index]) == Some(ConditionalDirectiveKind::If) {
            let (selected, after_group) =
                select_conditional_branch(nodes, index, scope, file, diagnostics);
            if let Some(element) = selected {
                render_element(
                    element,
                    scope,
                    file,
                    max_loop_iterations,
                    false,
                    next_id,
                    diagnostics,
                    &mut rendered,
                );
            }
            index = after_group;
            continue;
        }

        if let Some(kind @ (ConditionalDirectiveKind::ElseIf | ConditionalDirectiveKind::Else)) =
            conditional_kind(&nodes[index])
        {
            if let Node::Element(element) = &nodes[index] {
                diagnostics.push(diagnostic(
                    file,
                    match element.directives.first() {
                        Some(ElementDirective::Conditional { span, .. }) => *span,
                        _ => element.span,
                    },
                    DataRenderDiagnosticKind::OrphanConditional,
                    format!("{kind:?} branch has no preceding wx:if"),
                ));
            }
            index += 1;
            continue;
        }

        match &nodes[index] {
            Node::Element(element) => render_element(
                element,
                scope,
                file,
                max_loop_iterations,
                false,
                next_id,
                diagnostics,
                &mut rendered,
            ),
            Node::Text { value, span } => {
                let id = allocate_id(next_id);
                rendered.push(RenderNode::Text {
                    id,
                    value: value.clone(),
                    source: *span,
                });
            }
            Node::Interpolation { expression, span } => {
                if let Some(value) = evaluate(expression, scope) {
                    let id = allocate_id(next_id);
                    rendered.push(RenderNode::Text {
                        id,
                        value: value_to_string(&value),
                        source: *span,
                    });
                } else {
                    diagnostics.push(diagnostic(
                        file,
                        *span,
                        DataRenderDiagnosticKind::UnsupportedExpression,
                        format!("unsupported data binding expression `{expression}`"),
                    ));
                    let id = allocate_id(next_id);
                    rendered.push(RenderNode::Binding {
                        id,
                        expression: expression.clone(),
                        source: *span,
                    });
                }
            }
        }
        index += 1;
    }
    rendered
}

fn select_conditional_branch<'a>(
    nodes: &'a [Node],
    start: usize,
    scope: &Scope<'_>,
    file: &str,
    diagnostics: &mut Vec<DataRenderDiagnostic>,
) -> (Option<&'a Element>, usize) {
    let mut cursor = start;
    let mut selected = None;
    loop {
        if cursor >= nodes.len() {
            break;
        }
        let Some(element) = element(&nodes[cursor]) else {
            if selected.is_some() && is_whitespace(&nodes[cursor]) {
                cursor += 1;
                continue;
            }
            break;
        };
        let Some(ElementDirective::Conditional {
            kind,
            expression,
            span,
        }) = element
            .directives
            .iter()
            .find(|directive| matches!(directive, ElementDirective::Conditional { .. }))
        else {
            break;
        };
        let matches = selected.is_none()
            && match kind {
                ConditionalDirectiveKind::If | ConditionalDirectiveKind::ElseIf => expression
                    .as_deref()
                    .and_then(|expression| evaluate(expression, scope))
                    .map_or_else(
                        || {
                            if let Some(expression) = expression {
                                diagnostics.push(diagnostic(
                                    file,
                                    *span,
                                    DataRenderDiagnosticKind::UnsupportedExpression,
                                    format!("unsupported conditional expression `{expression}`"),
                                ));
                            }
                            false
                        },
                        |value| truthy(&value),
                    ),
                ConditionalDirectiveKind::Else => true,
            };
        if selected.is_none() && matches {
            selected = Some(element);
        }
        cursor += 1;
        if *kind == ConditionalDirectiveKind::Else {
            break;
        }
        let after_branch = cursor;
        while cursor < nodes.len() && is_whitespace(&nodes[cursor]) {
            cursor += 1;
        }
        if !matches!(
            nodes.get(cursor).and_then(conditional_kind),
            Some(ConditionalDirectiveKind::ElseIf | ConditionalDirectiveKind::Else)
        ) {
            cursor = after_branch;
            break;
        }
    }
    (selected, cursor)
}

#[allow(clippy::too_many_arguments)]
fn render_element(
    element: &Element,
    scope: &Scope<'_>,
    file: &str,
    max_loop_iterations: usize,
    skip_loop: bool,
    next_id: &mut usize,
    diagnostics: &mut Vec<DataRenderDiagnostic>,
    rendered: &mut Vec<RenderNode>,
) {
    if !skip_loop
        && let Some(ElementDirective::ForEach {
            expression,
            item,
            index,
            span,
            ..
        }) = element
            .directives
            .iter()
            .find(|directive| matches!(directive, ElementDirective::ForEach { .. }))
    {
        let Some(Value::Array(values)) = evaluate(expression, scope) else {
            diagnostics.push(diagnostic(
                file,
                *span,
                DataRenderDiagnosticKind::InvalidLoopSource,
                format!("wx:for expression `{expression}` must resolve to an array"),
            ));
            return;
        };
        if values.len() > max_loop_iterations {
            diagnostics.push(diagnostic(
                file,
                *span,
                DataRenderDiagnosticKind::LoopLimitExceeded,
                format!(
                    "wx:for has {} items, limit is {max_loop_iterations}",
                    values.len()
                ),
            ));
            return;
        }
        for (position, value) in values.into_iter().enumerate() {
            let nested = scope
                .with_local(item, value)
                .with_local(index, Value::from(position));
            render_element(
                element,
                &nested,
                file,
                max_loop_iterations,
                true,
                next_id,
                diagnostics,
                rendered,
            );
        }
        return;
    }

    let id = allocate_id(next_id);
    let children = render_nodes(
        &element.children,
        scope,
        file,
        max_loop_iterations,
        next_id,
        diagnostics,
    );
    rendered.push(RenderNode::Element {
        id,
        tag_name: element.tag_name.clone(),
        attributes: element
            .attributes
            .iter()
            .filter(|attribute| !attribute.name.starts_with("wx:"))
            .map(|attribute| {
                (
                    attribute.name.clone(),
                    evaluate_attribute(&attribute.value, scope, file, attribute.span, diagnostics),
                )
            })
            .collect(),
        children,
        source: element.span,
    });
}

fn evaluate_attribute(
    attribute: &AttributeValue,
    scope: &Scope<'_>,
    file: &str,
    span: Span,
    diagnostics: &mut Vec<DataRenderDiagnostic>,
) -> RenderAttribute {
    match attribute {
        AttributeValue::Boolean => RenderAttribute::Boolean,
        AttributeValue::Literal(value) => RenderAttribute::Literal {
            value: value.clone(),
        },
        AttributeValue::Expression(expression) => {
            if let Some(value) = evaluate(expression, scope) {
                RenderAttribute::Literal {
                    value: value_to_string(&value),
                }
            } else {
                diagnostics.push(diagnostic(
                    file,
                    span,
                    DataRenderDiagnosticKind::UnsupportedExpression,
                    format!("unsupported attribute expression `{expression}`"),
                ));
                RenderAttribute::Binding {
                    expression: expression.clone(),
                }
            }
        }
        AttributeValue::Interpolated(parts) => {
            let mut value = String::new();
            let mut supported = true;
            for part in parts {
                match part {
                    ValuePart::Literal(literal) => value.push_str(literal),
                    ValuePart::Expression(expression) => {
                        if let Some(result) = evaluate(expression, scope) {
                            value.push_str(&value_to_string(&result));
                        } else {
                            supported = false;
                            diagnostics.push(diagnostic(
                                file,
                                span,
                                DataRenderDiagnosticKind::UnsupportedExpression,
                                format!("unsupported attribute expression `{expression}`"),
                            ));
                        }
                    }
                }
            }
            if supported {
                RenderAttribute::Literal { value }
            } else {
                RenderAttribute::Interpolated {
                    parts: parts.iter().map(convert_part).collect(),
                }
            }
        }
    }
}

fn evaluate(expression: &str, scope: &Scope<'_>) -> Option<Value> {
    let expression = expression.trim();
    if let Some(rest) = expression.strip_prefix('!') {
        return Some(Value::Bool(!truthy(&evaluate(rest, scope)?)));
    }
    for operator in ["===", "!==", "==", "!="] {
        if let Some((left, right)) = expression.split_once(operator) {
            let equal = evaluate(left, scope)? == evaluate(right, scope)?;
            return Some(Value::Bool(if operator.starts_with('!') {
                !equal
            } else {
                equal
            }));
        }
    }
    if let Ok(value) = serde_json::from_str::<Value>(expression) {
        return Some(value);
    }
    if expression.len() >= 2
        && ((expression.starts_with('\'') && expression.ends_with('\''))
            || (expression.starts_with('"') && expression.ends_with('"')))
    {
        return Some(Value::String(
            expression[1..expression.len() - 1].to_owned(),
        ));
    }
    scope.resolve(expression)
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| String::new())
        }
    }
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphanumeric() || byte == b'_' || (index > 0 && byte == b'$')
        })
        && !value.as_bytes()[0].is_ascii_digit()
}

fn conditional_kind(node: &Node) -> Option<ConditionalDirectiveKind> {
    let element = element(node)?;
    element
        .directives
        .iter()
        .find_map(|directive| match directive {
            ElementDirective::Conditional { kind, .. } => Some(*kind),
            ElementDirective::ForEach { .. } => None,
        })
}

fn element(node: &Node) -> Option<&Element> {
    match node {
        Node::Element(element) => Some(element),
        Node::Text { .. } | Node::Interpolation { .. } => None,
    }
}

fn is_whitespace(node: &Node) -> bool {
    matches!(node, Node::Text { value, .. } if value.trim().is_empty())
}

fn allocate_id(next_id: &mut usize) -> usize {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);
    id
}

fn convert_part(part: &ValuePart) -> RenderAttributePart {
    match part {
        ValuePart::Literal(value) => RenderAttributePart::Literal {
            value: value.clone(),
        },
        ValuePart::Expression(expression) => RenderAttributePart::Binding {
            expression: expression.clone(),
        },
    }
}

fn diagnostic(
    file: &str,
    span: Span,
    kind: DataRenderDiagnosticKind,
    message: String,
) -> DataRenderDiagnostic {
    DataRenderDiagnostic {
        file: file.to_owned(),
        kind,
        message,
        span,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DataRenderDiagnosticKind, build_with_data};
    use crate::RenderNode;

    #[test]
    fn evaluates_bindings_conditionals_loops_and_loop_locals()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = sdkwork_miniprogram_wxml_compiler::parse(
            "pages/list.wxml",
            "<view wx:if=\"{{ready}}\">{{title}}</view><view wx:elif=\"{{count == 2}}\">two</view><view wx:else>waiting</view><view wx:for=\"{{items}}\" wx:for-item=\"row\" wx:for-index=\"position\" wx:key=\"id\"><text>{{position}}:{{row.name}}</text></view>",
        )?;
        let rendered = build_with_data(
            &document,
            "pages/list.wxml",
            &json!({
                "ready": false,
                "count": 2,
                "title": "Welcome",
                "items": [{ "id": "a", "name": "Alpha" }, { "id": "b", "name": "Beta" }]
            }),
            10,
        );

        assert!(
            rendered.diagnostics.is_empty(),
            "{:?}",
            rendered.diagnostics
        );
        assert_eq!(rendered.tree.children.len(), 3);
        assert!(matches!(
            &rendered.tree.children[0],
            RenderNode::Element { children, .. }
                if matches!(&children[0], RenderNode::Text { value, .. } if value == "two")
        ));
        assert!(matches!(
            &rendered.tree.children[1],
            RenderNode::Element { children, .. }
                if matches!(&children[0], RenderNode::Element { children, .. }
                    if children.len() == 3
                        && matches!(&children[0], RenderNode::Text { value, .. } if value == "0")
                        && matches!(&children[1], RenderNode::Text { value, .. } if value == ":")
                        && matches!(&children[2], RenderNode::Text { value, .. } if value == "Alpha"))
        ));
        assert!(matches!(
            &rendered.tree.children[2],
            RenderNode::Element { children, .. }
                if matches!(&children[0], RenderNode::Element { children, .. }
                    if children.len() == 3
                        && matches!(&children[0], RenderNode::Text { value, .. } if value == "1")
                        && matches!(&children[2], RenderNode::Text { value, .. } if value == "Beta"))
        ));
        Ok(())
    }

    #[test]
    fn reports_unsupported_expressions_with_template_locations()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = sdkwork_miniprogram_wxml_compiler::parse(
            "pages/unsupported.wxml",
            "<view>{{title.toUpperCase()}}</view>",
        )?;
        let rendered = build_with_data(
            &document,
            "pages/unsupported.wxml",
            &json!({ "title": "hello" }),
            10,
        );

        assert_eq!(rendered.diagnostics.len(), 1);
        assert_eq!(rendered.diagnostics[0].file, "pages/unsupported.wxml");
        assert_eq!(
            rendered.diagnostics[0].kind,
            DataRenderDiagnosticKind::UnsupportedExpression
        );
        assert_eq!(rendered.diagnostics[0].span.line, 1);
        assert!(matches!(
            &rendered.tree.children[0],
            RenderNode::Element { children, .. }
                if matches!(&children[0], RenderNode::Binding { expression, .. } if expression == "title.toUpperCase()")
        ));
        Ok(())
    }

    #[test]
    fn rejects_loops_that_exceed_the_iteration_bound() -> Result<(), Box<dyn std::error::Error>> {
        let document = sdkwork_miniprogram_wxml_compiler::parse(
            "pages/list.wxml",
            "<view wx:for=\"{{items}}\">{{item}}</view>",
        )?;
        let rendered = build_with_data(
            &document,
            "pages/list.wxml",
            &json!({ "items": [1, 2, 3] }),
            2,
        );

        assert_eq!(rendered.tree.children.len(), 0);
        assert_eq!(rendered.diagnostics.len(), 1);
        assert_eq!(
            rendered.diagnostics[0].kind,
            DataRenderDiagnosticKind::LoopLimitExceeded
        );
        Ok(())
    }
}
