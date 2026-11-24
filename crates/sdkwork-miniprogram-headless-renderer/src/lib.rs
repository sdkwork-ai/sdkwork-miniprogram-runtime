//! Converts parsed WXML into a stable, source-mapped structural render tree.
//!
//! The tree supports headless inspection and tests. It does not calculate
//! styles, geometry, pixels, layout, component state, or event behavior.

use std::collections::BTreeMap;

use sdkwork_miniprogram_wxml_compiler::{AttributeValue, Document, Node, Span, ValuePart};
use serde::Serialize;

/// A stable, serializable structural tree generated from one WXML document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RenderTree {
    /// Top-level nodes in document order.
    pub children: Vec<RenderNode>,
    /// Number of element and text nodes in this tree.
    pub node_count: usize,
}

/// A structural element, text segment, or unevaluated data binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RenderNode {
    /// WXML element with its parsed properties and ordered children.
    Element {
        /// Stable preorder node identifier within this render tree.
        id: usize,
        /// Source element name.
        tag_name: String,
        /// Parsed properties in deterministic name order.
        attributes: BTreeMap<String, RenderAttribute>,
        /// Ordered child nodes.
        children: Vec<RenderNode>,
        /// Original template source location.
        source: Span,
    },
    /// Literal text from the WXML document.
    Text {
        /// Stable preorder node identifier within this render tree.
        id: usize,
        /// Text as written in the template.
        value: String,
        /// Original template source location.
        source: Span,
    },
    /// An expression preserved for later data-binding evaluation.
    Binding {
        /// Stable preorder node identifier within this render tree.
        id: usize,
        /// Unparsed expression source.
        expression: String,
        /// Original template source location.
        source: Span,
    },
}

/// A parsed element attribute, retained without evaluating its bindings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RenderAttribute {
    /// Attribute with no explicit assignment value.
    Boolean,
    /// Attribute containing literal text only.
    Literal {
        /// Literal text value.
        value: String,
    },
    /// Attribute containing one unevaluated data-binding expression.
    Binding {
        /// Source expression inside interpolation braces.
        expression: String,
    },
    /// Attribute containing alternating literal and expression pieces.
    Interpolated {
        /// Attribute value pieces in their original order.
        parts: Vec<RenderAttributePart>,
    },
}

/// One literal or unevaluated-expression segment in an attribute value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RenderAttributePart {
    /// Literal attribute text.
    Literal {
        /// Literal text.
        value: String,
    },
    /// Unevaluated template expression.
    Binding {
        /// Expression source.
        expression: String,
    },
}

/// Converts a WXML syntax tree to a stable structural render tree.
///
/// IDs start at zero and follow preorder traversal. Text and interpolation
/// nodes each receive their own ID.
///
/// # Examples
///
/// ```
/// let document = sdkwork_miniprogram_wxml_compiler::parse(
///     "inline.wxml",
///     "<view>{{title}}</view>",
/// )?;
/// let tree = sdkwork_miniprogram_headless_renderer::build(&document);
/// assert_eq!(tree.node_count, 2);
/// # Ok::<(), sdkwork_miniprogram_wxml_compiler::Diagnostic>(())
/// ```
#[must_use]
pub fn build(document: &Document) -> RenderTree {
    let mut next_id = 0;
    let children = document
        .children
        .iter()
        .map(|node| build_node(node, &mut next_id))
        .collect();
    RenderTree {
        children,
        node_count: next_id,
    }
}

fn build_node(node: &Node, next_id: &mut usize) -> RenderNode {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);

    match node {
        Node::Element(element) => RenderNode::Element {
            id,
            tag_name: element.tag_name.clone(),
            attributes: element
                .attributes
                .iter()
                .map(|attribute| {
                    (
                        attribute.name.clone(),
                        convert_attribute(&attribute.value),
                    )
                })
                .collect(),
            children: element
                .children
                .iter()
                .map(|child| build_node(child, next_id))
                .collect(),
            source: element.span,
        },
        Node::Text { value, span } => RenderNode::Text {
            id,
            value: value.clone(),
            source: *span,
        },
        Node::Interpolation { expression, span } => RenderNode::Binding {
            id,
            expression: expression.clone(),
            source: *span,
        },
    }
}

fn convert_attribute(value: &AttributeValue) -> RenderAttribute {
    match value {
        AttributeValue::Boolean => RenderAttribute::Boolean,
        AttributeValue::Literal(value) => RenderAttribute::Literal {
            value: value.clone(),
        },
        AttributeValue::Expression(expression) => RenderAttribute::Binding {
            expression: expression.clone(),
        },
        AttributeValue::Interpolated(parts) => RenderAttribute::Interpolated {
            parts: parts.iter().map(convert_attribute_part).collect(),
        },
    }
}

fn convert_attribute_part(part: &ValuePart) -> RenderAttributePart {
    match part {
        ValuePart::Literal(value) => RenderAttributePart::Literal {
            value: value.clone(),
        },
        ValuePart::Expression(expression) => RenderAttributePart::Binding {
            expression: expression.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{RenderAttribute, RenderAttributePart, RenderNode, build};

    #[test]
    fn preserves_structure_bindings_attributes_and_source_locations()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = sdkwork_miniprogram_wxml_compiler::parse(
            "pages/index/index.wxml",
            "<view id='root' class='{{tone}}-box'>Hello {{name}}</view>",
        )?;
        let tree = build(&document);

        assert_eq!(tree.node_count, 4);
        let RenderNode::Element {
            id: 0,
            attributes,
            children,
            source,
            ..
        } = &tree.children[0]
        else {
            return Err("expected a source-mapped root element".into());
        };
        assert_eq!(source.line, 1);
        assert!(matches!(
            attributes.get("class"),
            Some(RenderAttribute::Interpolated { parts })
                if parts == &vec![
                    RenderAttributePart::Binding { expression: "tone".to_owned() },
                    RenderAttributePart::Literal { value: "-box".to_owned() },
                ]
        ));
        assert!(matches!(children[0], RenderNode::Text { id: 1, .. }));
        assert!(matches!(
            children[1],
            RenderNode::Binding { id: 2, ref expression, .. } if expression == "name"
        ));
        assert!(matches!(children[2], RenderNode::Text { id: 3, .. }));
        Ok(())
    }

    #[test]
    fn serializes_stable_binding_tags_for_inspection() -> Result<(), Box<dyn std::error::Error>> {
        let document = sdkwork_miniprogram_wxml_compiler::parse("inline.wxml", "{{title}}")?;
        let json = serde_json::to_value(build(&document))?;

        assert_eq!(json["children"][0]["kind"], "binding");
        assert_eq!(json["children"][0]["expression"], "title");
        Ok(())
    }
}
