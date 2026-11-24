//! Converts parsed WXML into a stable, source-mapped structural render tree.
//!
//! The tree supports headless inspection and tests. A limited WXSS subset can
//! add computed declarations, without calculating layout, geometry, pixels,
//! component state, or event behavior.

use std::collections::BTreeMap;

use sdkwork_miniprogram_wxml_compiler::{AttributeValue, Document, Node, Span, ValuePart};
use serde::Serialize;

mod data_render;
mod styles;

pub use data_render::{
    DataRenderDiagnostic, DataRenderDiagnosticKind, DataRenderTree, build_with_data,
};
pub use styles::{StyleDiagnostic, StyleDiagnosticKind, StyleSourceSpan, StyledRenderTree};

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
    build_with_styles(document, "", "").tree
}

/// Converts WXML to a structural tree and applies a supported WXSS subset.
///
/// Type, class, and ID selectors with descendant combinators are supported.
/// Computed declarations are indexed by stable Render Tree node id. Unsupported
/// or malformed syntax is omitted and reported with its source location.
#[must_use]
pub fn build_with_styles(
    document: &Document,
    stylesheet_file: impl Into<String>,
    stylesheet_source: &str,
) -> StyledRenderTree {
    let stylesheet_file = stylesheet_file.into();
    let (rules, diagnostics) = styles::parse_stylesheet(&stylesheet_file, stylesheet_source);
    let mut next_id = 0;
    let mut ancestors = Vec::new();
    let mut computed_styles = BTreeMap::new();
    let children = document
        .children
        .iter()
        .map(|node| {
            build_node(
                node,
                &mut next_id,
                &mut ancestors,
                &rules,
                &mut computed_styles,
            )
        })
        .collect();
    StyledRenderTree {
        tree: RenderTree {
            children,
            node_count: next_id,
        },
        computed_styles,
        diagnostics,
    }
}

fn build_node(
    node: &Node,
    next_id: &mut usize,
    ancestors: &mut Vec<styles::ElementFacts>,
    rules: &[styles::StyleRule],
    computed_styles: &mut BTreeMap<usize, BTreeMap<String, String>>,
) -> RenderNode {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);

    match node {
        Node::Element(element) => {
            let facts = styles::ElementFacts::from_wxml(element);
            let declarations = styles::matching_declarations(&facts, ancestors, rules);
            if !declarations.is_empty() {
                computed_styles.insert(id, declarations);
            }
            ancestors.push(facts);
            let children = element
                .children
                .iter()
                .map(|child| build_node(child, next_id, ancestors, rules, computed_styles))
                .collect();
            ancestors.pop();
            RenderNode::Element {
                id,
                tag_name: element.tag_name.clone(),
                attributes: element
                    .attributes
                    .iter()
                    .map(|attribute| (attribute.name.clone(), convert_attribute(&attribute.value)))
                    .collect(),
                children,
                source: element.span,
            }
        }
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
    use super::{RenderAttribute, RenderAttributePart, RenderNode, build, build_with_styles};

    #[test]
    fn preserves_structure_bindings_attributes_and_source_locations()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = sdkwork_miniprogram_wxml_compiler::parse(
            "pages/index/index.wxml",
            "<view id='root' class='{{tone}}-box'>Hello {{name}}</view>",
        )?;
        let tree = build(&document);

        assert_eq!(tree.node_count, 3);
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
        assert_eq!(children.len(), 2);
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

    #[test]
    fn applies_type_class_id_and_descendant_styles_in_cascade_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = sdkwork_miniprogram_wxml_compiler::parse(
            "inline.wxml",
            "<view class='panel'><text id='heading' class='title'>Hi</text></view>",
        )?;
        let styled = build_with_styles(
            &document,
            "inline.wxss",
            "view { display: flex; color: red } .panel text { color: blue } #heading { font-size: 18rpx; color: green }",
        );

        assert_eq!(styled.diagnostics.len(), 0);
        assert_eq!(styled.computed_styles[&0]["display"], "flex");
        assert_eq!(styled.computed_styles[&1]["color"], "green");
        assert_eq!(styled.computed_styles[&1]["font-size"], "18rpx");
        Ok(())
    }

    #[test]
    fn reports_unsupported_styles_with_source_locations() -> Result<(), Box<dyn std::error::Error>>
    {
        let document = sdkwork_miniprogram_wxml_compiler::parse("inline.wxml", "<view />")?;
        let styled = build_with_styles(
            &document,
            "pages/index/index.wxss",
            "@import 'x';\n@media screen { view { color: red } }\nview:hover { color: blue }",
        );

        assert!(styled.computed_styles.is_empty());
        assert_eq!(styled.diagnostics.len(), 3);
        assert_eq!(styled.diagnostics[0].span.line, 1);
        assert_eq!(styled.diagnostics[1].span.line, 2);
        assert_eq!(styled.diagnostics[2].span.line, 3);
        Ok(())
    }
}
