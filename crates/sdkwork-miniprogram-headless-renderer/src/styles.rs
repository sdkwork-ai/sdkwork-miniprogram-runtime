//! A small, explicit WXSS declaration parser and structural selector matcher.

use std::collections::{BTreeMap, BTreeSet};

use sdkwork_miniprogram_wxml_compiler::{AttributeValue, Element};
use serde::Serialize;

/// Source location in a WXSS stylesheet.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct StyleSourceSpan {
    /// Inclusive UTF-8 byte offset.
    pub start: usize,
    /// Exclusive UTF-8 byte offset.
    pub end: usize,
    /// 1-based source line.
    pub line: usize,
    /// 1-based Unicode scalar column.
    pub column: usize,
}

/// Classification for an unsupported or malformed WXSS construct.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleDiagnosticKind {
    /// Selector or declaration block was malformed.
    MalformedRule,
    /// Selector uses syntax outside this structural matcher.
    UnsupportedSelector,
    /// Declaration property or value is invalid or unsupported.
    UnsupportedDeclaration,
}

/// A stylesheet diagnostic with its source file and location.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StyleDiagnostic {
    /// Stylesheet path supplied to [`super::build_with_styles`].
    pub file: String,
    /// Stable diagnostic category.
    pub kind: StyleDiagnosticKind,
    /// Human-readable reason.
    pub message: String,
    /// Location at the offending token.
    pub span: StyleSourceSpan,
}

/// Structural tree with matched declarations and stylesheet diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StyledRenderTree {
    /// WXML structural tree.
    pub tree: super::RenderTree,
    /// Computed supported declarations indexed by Render Tree node id.
    pub computed_styles: BTreeMap<usize, BTreeMap<String, String>>,
    /// Malformed and unsupported stylesheet constructs.
    pub diagnostics: Vec<StyleDiagnostic>,
}

#[derive(Clone, Debug)]
pub(super) struct ElementFacts {
    tag_name: String,
    classes: BTreeSet<String>,
    id: Option<String>,
}

impl ElementFacts {
    pub(super) fn from_wxml(element: &Element) -> Self {
        let mut classes = BTreeSet::new();
        let mut id = None;
        for attribute in &element.attributes {
            let AttributeValue::Literal(value) = &attribute.value else {
                continue;
            };
            match attribute.name.as_str() {
                "class" => classes.extend(value.split_ascii_whitespace().map(str::to_owned)),
                "id" => id = Some(value.clone()),
                _ => {}
            }
        }
        Self {
            tag_name: element.tag_name.to_ascii_lowercase(),
            classes,
            id,
        }
    }
}

#[derive(Clone, Debug)]
enum SimpleSelector {
    Type(String),
    Class(String),
    Id(String),
}

#[derive(Clone, Debug)]
struct CompoundSelector(Vec<SimpleSelector>);

#[derive(Clone, Debug)]
pub(super) struct StyleRule {
    selector: Vec<CompoundSelector>,
    declarations: Vec<(String, String)>,
    specificity: (usize, usize, usize),
    order: usize,
}

type Specificity = (usize, usize, usize);
type ParsedSelector = (Vec<CompoundSelector>, Specificity);
type CascadeEntry = (Specificity, usize, String);

pub(super) fn parse_stylesheet(file: &str, source: &str) -> (Vec<StyleRule>, Vec<StyleDiagnostic>) {
    let source = mask_comments(source);
    let mut rules = Vec::new();
    let mut diagnostics = Vec::new();
    let mut cursor = 0;
    let mut order = 0;
    while cursor < source.len() {
        cursor = skip_whitespace(&source, cursor);
        if cursor == source.len() {
            break;
        }
        if source[cursor..].starts_with('@') {
            let end = source[cursor..]
                .find([';', '{'])
                .map_or(source.len(), |offset| cursor + offset);
            diagnostics.push(diagnostic(
                file,
                &source,
                cursor,
                end.max(cursor + 1),
                StyleDiagnosticKind::UnsupportedSelector,
                "at-rules are outside the supported WXSS subset",
            ));
            cursor = if source.as_bytes().get(end) == Some(&b';') {
                end + 1
            } else if source.as_bytes().get(end) == Some(&b'{') {
                skip_braced_block(&source, end + 1)
            } else {
                source.len()
            };
            continue;
        }
        let Some(open_relative) = source[cursor..].find('{') else {
            diagnostics.push(diagnostic(
                file,
                source.as_str(),
                cursor,
                cursor + 1,
                StyleDiagnosticKind::MalformedRule,
                "expected `{` after selector",
            ));
            break;
        };
        let open = cursor + open_relative;
        let selector_text = source[cursor..open].trim();
        let selector_start =
            cursor + source[cursor..open].len() - source[cursor..open].trim_start().len();
        let Some(close_relative) = source[open + 1..].find('}') else {
            diagnostics.push(diagnostic(
                file,
                &source,
                open,
                open + 1,
                StyleDiagnosticKind::MalformedRule,
                "unterminated declaration block",
            ));
            break;
        };
        let close = open + 1 + close_relative;
        let parsed_selector = parse_selector(selector_text);
        match parsed_selector {
            Ok(selectors) => {
                let declarations =
                    parse_declarations(file, &source, open + 1, close, &mut diagnostics);
                for (selector, specificity) in selectors {
                    rules.push(StyleRule {
                        selector,
                        declarations: declarations.clone(),
                        specificity,
                        order,
                    });
                    order += 1;
                }
            }
            Err(message) => diagnostics.push(diagnostic(
                file,
                &source,
                selector_start,
                open.max(selector_start + 1),
                StyleDiagnosticKind::UnsupportedSelector,
                &message,
            )),
        }
        cursor = close + 1;
    }
    (rules, diagnostics)
}

pub(super) fn matching_declarations(
    element: &ElementFacts,
    ancestors: &[ElementFacts],
    rules: &[StyleRule],
) -> BTreeMap<String, String> {
    let mut winners: BTreeMap<String, CascadeEntry> = BTreeMap::new();
    for rule in rules {
        if !matches_selector(&rule.selector, element, ancestors) {
            continue;
        }
        for (property, value) in &rule.declarations {
            let priority = (rule.specificity, rule.order);
            if winners
                .get(property)
                .is_none_or(|(specificity, order, _)| (*specificity, *order) <= priority)
            {
                winners.insert(
                    property.clone(),
                    (rule.specificity, rule.order, value.clone()),
                );
            }
        }
    }
    winners
        .into_iter()
        .map(|(property, (_, _, value))| (property, value))
        .collect()
}

fn matches_selector(
    selector: &[CompoundSelector],
    element: &ElementFacts,
    ancestors: &[ElementFacts],
) -> bool {
    let Some((last, preceding)) = selector.split_last() else {
        return false;
    };
    if !matches_compound(last, element) {
        return false;
    }
    let mut ancestor_cursor = ancestors.len();
    for compound in preceding.iter().rev() {
        let Some(found) = ancestors[..ancestor_cursor]
            .iter()
            .rposition(|candidate| matches_compound(compound, candidate))
        else {
            return false;
        };
        ancestor_cursor = found;
    }
    true
}

fn matches_compound(selector: &CompoundSelector, element: &ElementFacts) -> bool {
    selector.0.iter().all(|part| match part {
        SimpleSelector::Type(name) => name == &element.tag_name,
        SimpleSelector::Class(name) => element.classes.contains(name),
        SimpleSelector::Id(name) => element.id.as_ref() == Some(name),
    })
}

fn parse_selector(input: &str) -> Result<Vec<ParsedSelector>, String> {
    if input.is_empty() {
        return Err("selector must not be empty".to_owned());
    }
    let mut alternatives = Vec::new();
    for alternative in input.split(',') {
        let mut compounds = Vec::new();
        let mut specificity = (0, 0, 0);
        for token in alternative.split_ascii_whitespace() {
            let compound = parse_compound(token)?;
            for part in &compound.0 {
                match part {
                    SimpleSelector::Type(_) => specificity.2 += 1,
                    SimpleSelector::Class(_) => specificity.1 += 1,
                    SimpleSelector::Id(_) => specificity.0 += 1,
                }
            }
            compounds.push(compound);
        }
        if compounds.is_empty() {
            return Err("selector list contains an empty selector".to_owned());
        }
        alternatives.push((compounds, specificity));
    }
    Ok(alternatives)
}

fn parse_compound(input: &str) -> Result<CompoundSelector, String> {
    let mut parts = Vec::new();
    let mut cursor = 0;
    let bytes = input.as_bytes();
    if bytes.first().is_some_and(u8::is_ascii_alphabetic)
        || bytes.first() == Some(&b'_')
        || bytes.first() == Some(&b'-')
    {
        let end = identifier_end(input, cursor);
        parts.push(SimpleSelector::Type(
            input[cursor..end].to_ascii_lowercase(),
        ));
        cursor = end;
    }
    while cursor < input.len() {
        let marker = bytes[cursor];
        if marker != b'.' && marker != b'#' {
            return Err("only type, class, and ID selectors are supported".to_owned());
        }
        cursor += 1;
        let end = identifier_end(input, cursor);
        if end == cursor {
            return Err("selector marker must be followed by a name".to_owned());
        }
        let name = input[cursor..end].to_owned();
        parts.push(if marker == b'.' {
            SimpleSelector::Class(name)
        } else {
            SimpleSelector::Id(name)
        });
        cursor = end;
    }
    if parts.is_empty() {
        return Err("unsupported or empty selector".to_owned());
    }
    Ok(CompoundSelector(parts))
}

fn identifier_end(input: &str, mut cursor: usize) -> usize {
    let bytes = input.as_bytes();
    while cursor < bytes.len() {
        if bytes[cursor].is_ascii_alphanumeric() || matches!(bytes[cursor], b'_' | b'-') {
            cursor += 1;
        } else if input[cursor..]
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_ascii())
        {
            cursor += input[cursor..].chars().next().map_or(1, char::len_utf8);
        } else {
            break;
        }
    }
    cursor
}

fn parse_declarations(
    file: &str,
    source: &str,
    start: usize,
    end: usize,
    diagnostics: &mut Vec<StyleDiagnostic>,
) -> Vec<(String, String)> {
    let mut declarations = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let next = source[cursor..end]
            .find(';')
            .map_or(end, |offset| cursor + offset);
        let raw = &source[cursor..next];
        let trimmed = raw.trim();
        let trim_offset = raw.len() - raw.trim_start().len();
        if !trimmed.is_empty() {
            if let Some(colon) = trimmed.find(':') {
                let property = trimmed[..colon].trim().to_ascii_lowercase();
                let value = trimmed[colon + 1..].trim();
                if supported_property(&property) && supported_value(value) {
                    declarations.push((property, value.to_owned()));
                } else {
                    diagnostics.push(diagnostic(
                        file,
                        source,
                        cursor + trim_offset,
                        next.max(cursor + trim_offset + 1),
                        StyleDiagnosticKind::UnsupportedDeclaration,
                        "unsupported property or declaration value",
                    ));
                }
            } else {
                diagnostics.push(diagnostic(
                    file,
                    source,
                    cursor + trim_offset,
                    next.max(cursor + trim_offset + 1),
                    StyleDiagnosticKind::UnsupportedDeclaration,
                    "declaration must contain a property and value separated by `:`",
                ));
            }
        }
        cursor = next.saturating_add(1);
    }
    declarations
}

fn supported_property(property: &str) -> bool {
    matches!(
        property,
        "color"
            | "background"
            | "background-color"
            | "display"
            | "width"
            | "height"
            | "min-width"
            | "min-height"
            | "max-width"
            | "max-height"
            | "margin"
            | "margin-top"
            | "margin-right"
            | "margin-bottom"
            | "margin-left"
            | "padding"
            | "padding-top"
            | "padding-right"
            | "padding-bottom"
            | "padding-left"
            | "font-size"
            | "font-weight"
            | "font-family"
            | "line-height"
            | "text-align"
            | "text-decoration"
            | "white-space"
            | "overflow"
            | "position"
            | "top"
            | "right"
            | "bottom"
            | "left"
            | "border"
            | "border-width"
            | "border-style"
            | "border-color"
            | "border-radius"
            | "opacity"
            | "flex"
            | "flex-direction"
            | "flex-wrap"
            | "flex-grow"
            | "flex-shrink"
            | "flex-basis"
            | "justify-content"
            | "align-items"
            | "align-self"
            | "align-content"
            | "gap"
            | "box-sizing"
    )
}

fn supported_value(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['{', '}', ';'])
        && !value.to_ascii_lowercase().contains("!important")
}

fn mask_comments(source: &str) -> String {
    let mut masked = source.as_bytes().to_vec();
    let mut cursor = 0;
    while cursor + 1 < masked.len() {
        if masked[cursor] == b'/' && masked[cursor + 1] == b'*' {
            let comment_start = cursor;
            cursor += 2;
            while cursor + 1 < masked.len()
                && !(masked[cursor] == b'*' && masked[cursor + 1] == b'/')
            {
                if masked[cursor] != b'\n' && masked[cursor] != b'\r' {
                    masked[cursor] = b' ';
                }
                cursor += 1;
            }
            if cursor + 1 >= masked.len() {
                for byte in &mut masked[comment_start..] {
                    if *byte != b'\n' && *byte != b'\r' {
                        *byte = b' ';
                    }
                }
                break;
            }
            masked[cursor] = b' ';
            masked[cursor + 1] = b' ';
            cursor += 2;
        } else {
            cursor += 1;
        }
    }
    String::from_utf8(masked).unwrap_or_else(|_| source.to_owned())
}

fn skip_whitespace(source: &str, mut cursor: usize) -> usize {
    while cursor < source.len() && source.as_bytes()[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    cursor
}

fn skip_braced_block(source: &str, opening_brace: usize) -> usize {
    let mut depth = 1_usize;
    let mut cursor = opening_brace;
    while cursor < source.len() {
        match source.as_bytes()[cursor] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return cursor + 1;
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    source.len()
}

fn diagnostic(
    file: &str,
    source: &str,
    start: usize,
    end: usize,
    kind: StyleDiagnosticKind,
    message: &str,
) -> StyleDiagnostic {
    let start = start.min(source.len());
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit('\n')
        .next()
        .map_or(1, |line| line.chars().count() + 1);
    StyleDiagnostic {
        file: file.to_owned(),
        kind,
        message: message.to_owned(),
        span: StyleSourceSpan {
            start,
            end: end.min(source.len()).max(start),
            line,
            column,
        },
    }
}
