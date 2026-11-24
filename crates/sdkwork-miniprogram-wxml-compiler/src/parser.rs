use crate::{
    ast::{Attribute, AttributeValue, BindingKind, Document, Element, Node, Span, ValuePart},
    diagnostic::{Diagnostic, DiagnosticKind, ParseResult},
};

/// Parse one WXML source file, reporting the first structural error.
///
/// # Errors
///
/// Returns a source-located diagnostic when the document contains malformed
/// tags, attributes, comments, or interpolation delimiters.
pub fn parse(file: impl Into<String>, source: &str) -> ParseResult<Document> {
    Parser::new(file.into(), source).parse_document()
}

struct Parser<'a> {
    file: String,
    source: &'a str,
    offset: usize,
}

impl<'a> Parser<'a> {
    fn new(file: String, source: &'a str) -> Self {
        Self {
            file,
            source,
            offset: 0,
        }
    }

    fn parse_document(mut self) -> ParseResult<Document> {
        let children = self.parse_children(None)?;
        Ok(Document { children })
    }

    fn parse_children(&mut self, parent: Option<&str>) -> ParseResult<Vec<Node>> {
        let mut children = Vec::new();
        loop {
            if self.is_eof() {
                if let Some(tag) = parent {
                    return self.error(
                        DiagnosticKind::UnexpectedEnd,
                        format!("missing closing tag for <{tag}>"),
                    );
                }
                return Ok(children);
            }

            if self.starts_with("<!--") {
                self.skip_comment()?;
            } else if self.starts_with("</") {
                let start = self.offset;
                self.advance_ascii(2);
                let closing_name = self.parse_name()?;
                self.skip_whitespace();
                if !self.consume_char('>') {
                    return self.error_at(
                        start,
                        DiagnosticKind::UnterminatedTag,
                        "expected `>` after closing tag".into(),
                    );
                }
                match parent {
                    Some(expected) if expected == closing_name => return Ok(children),
                    Some(expected) => {
                        return self.error_at(
                            start,
                            DiagnosticKind::MismatchedClosingTag,
                            format!("expected </{expected}> but found </{closing_name}>"),
                        );
                    }
                    None => {
                        return self.error_at(
                            start,
                            DiagnosticKind::UnexpectedClosingTag,
                            format!("unexpected closing tag </{closing_name}>"),
                        );
                    }
                }
            } else if self.peek_char() == Some('<') {
                children.push(Node::Element(self.parse_element()?));
            } else if self.starts_with("{{") {
                children.push(self.parse_interpolation()?);
            } else {
                children.push(self.parse_text());
            }
        }
    }

    fn parse_element(&mut self) -> ParseResult<Element> {
        let start = self.offset;
        self.advance_ascii(1);
        let tag_name = self.parse_name()?;
        let mut attributes = Vec::new();

        loop {
            self.skip_whitespace();
            if self.consume_str("/>") {
                return Ok(Element {
                    tag_name,
                    attributes,
                    children: Vec::new(),
                    span: self.span(start, self.offset),
                    self_closing: true,
                });
            }
            if self.consume_char('>') {
                let children = self.parse_children(Some(&tag_name))?;
                return Ok(Element {
                    tag_name,
                    attributes,
                    children,
                    span: self.span(start, self.offset),
                    self_closing: false,
                });
            }
            if self.is_eof() {
                return self.error_at(
                    start,
                    DiagnosticKind::UnterminatedTag,
                    format!("unterminated opening tag <{tag_name}>"),
                );
            }
            attributes.push(self.parse_attribute()?);
        }
    }

    fn parse_attribute(&mut self) -> ParseResult<Attribute> {
        let start = self.offset;
        let name = self.parse_name()?;
        let binding = classify_binding(&name);
        self.skip_whitespace();
        let value = if self.consume_char('=') {
            self.skip_whitespace();
            self.parse_attribute_value()?
        } else {
            AttributeValue::Boolean
        };
        Ok(Attribute {
            name,
            value,
            binding,
            span: self.span(start, self.offset),
        })
    }

    fn parse_attribute_value(&mut self) -> ParseResult<AttributeValue> {
        let quote = match self.peek_char() {
            Some('\'' | '"') => self.advance_char().unwrap_or('\0'),
            Some('>') | None => {
                return self.error(
                    DiagnosticKind::InvalidAttribute,
                    "expected attribute value".into(),
                );
            }
            _ => '\0',
        };
        let start = self.offset;
        while let Some(character) = self.peek_char() {
            if quote != '\0' && character == quote {
                let value = &self.source[start..self.offset];
                self.advance_char();
                return self.parse_value_parts(value, start);
            }
            if quote == '\0' && (character.is_whitespace() || character == '>') {
                let value = &self.source[start..self.offset];
                if value.is_empty() {
                    return self.error(
                        DiagnosticKind::InvalidAttribute,
                        "expected attribute value".into(),
                    );
                }
                return self.parse_value_parts(value, start);
            }
            self.advance_char();
        }
        if quote != '\0' {
            return self.error_at(
                start,
                DiagnosticKind::UnterminatedString,
                "unterminated quoted attribute value".into(),
            );
        }
        self.parse_value_parts(&self.source[start..self.offset], start)
    }

    fn parse_value_parts(&self, value: &str, source_start: usize) -> ParseResult<AttributeValue> {
        let mut parts = Vec::new();
        let mut cursor = 0;
        while let Some(relative_open) = value[cursor..].find("{{") {
            let open = cursor + relative_open;
            if open > cursor {
                parts.push(ValuePart::Literal(value[cursor..open].to_owned()));
            }
            let expression_start = open + 2;
            let Some(close) = interpolation_end(value, expression_start) else {
                return self.error_at(
                    source_start + open,
                    DiagnosticKind::UnterminatedInterpolation,
                    "unterminated `{{ ... }}` attribute interpolation".into(),
                );
            };
            parts.push(ValuePart::Expression(
                value[expression_start..close].trim().to_owned(),
            ));
            cursor = close + 2;
        }
        if cursor < value.len() {
            parts.push(ValuePart::Literal(value[cursor..].to_owned()));
        }
        match parts.len() {
            0 => Ok(AttributeValue::Literal(value.to_owned())),
            1 => match parts.remove(0) {
                ValuePart::Expression(expression) => Ok(AttributeValue::Expression(expression)),
                ValuePart::Literal(literal) => Ok(AttributeValue::Literal(literal)),
            },
            _ => Ok(AttributeValue::Interpolated(parts)),
        }
    }

    fn parse_text(&mut self) -> Node {
        let start = self.offset;
        while !self.is_eof() && self.peek_char() != Some('<') && !self.starts_with("{{") {
            self.advance_char();
        }
        Node::Text {
            value: self.source[start..self.offset].to_owned(),
            span: self.span(start, self.offset),
        }
    }

    fn parse_interpolation(&mut self) -> ParseResult<Node> {
        let start = self.offset;
        self.advance_ascii(2);
        let expression_start = self.offset;
        let mut nested_braces = 0usize;
        let mut quote = None;
        let mut escaped = false;

        while let Some(character) = self.peek_char() {
            if let Some(active_quote) = quote {
                self.advance_char();
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == active_quote {
                    quote = None;
                }
                continue;
            }

            match character {
                '\'' | '"' | '`' => quote = Some(character),
                '{' => nested_braces += 1,
                '}' if nested_braces > 0 => nested_braces -= 1,
                '}' if self.source[self.offset..].starts_with("}}") => {
                    let expression = self.source[expression_start..self.offset].trim().to_owned();
                    self.advance_ascii(2);
                    return Ok(Node::Interpolation {
                        expression,
                        span: self.span(start, self.offset),
                    });
                }
                _ => {}
            }
            self.advance_char();
        }

        self.error_at(
            start,
            DiagnosticKind::UnterminatedInterpolation,
            "unterminated `{{ ... }}` interpolation".into(),
        )
    }

    fn skip_comment(&mut self) -> ParseResult<()> {
        let start = self.offset;
        self.advance_ascii(4);
        if let Some(relative_end) = self.source[self.offset..].find("-->") {
            self.offset += relative_end + 3;
            Ok(())
        } else {
            self.error_at(
                start,
                DiagnosticKind::UnterminatedComment,
                "unterminated XML comment".into(),
            )
        }
    }

    fn parse_name(&mut self) -> ParseResult<String> {
        let start = self.offset;
        while let Some(character) = self.peek_char() {
            if is_name_character(character) {
                self.advance_char();
            } else {
                break;
            }
        }
        if self.offset == start {
            self.error(
                DiagnosticKind::InvalidName,
                "expected element or attribute name".into(),
            )
        } else {
            Ok(self.source[start..self.offset].to_owned())
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek_char(), Some(character) if character.is_whitespace()) {
            self.advance_char();
        }
    }

    fn starts_with(&self, value: &str) -> bool {
        self.source[self.offset..].starts_with(value)
    }

    fn consume_str(&mut self, value: &str) -> bool {
        if self.starts_with(value) {
            self.advance_ascii(value.len());
            true
        } else {
            false
        }
    }

    fn consume_char(&mut self, expected: char) -> bool {
        if self.peek_char() == Some(expected) {
            self.advance_char();
            true
        } else {
            false
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let character = self.peek_char()?;
        self.offset += character.len_utf8();
        Some(character)
    }

    fn advance_ascii(&mut self, byte_count: usize) {
        self.offset += byte_count;
    }

    fn is_eof(&self) -> bool {
        self.offset >= self.source.len()
    }

    fn span(&self, start: usize, end: usize) -> Span {
        let before = &self.source[..start];
        let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = before
            .rsplit('\n')
            .next()
            .map_or(1, |last_line| last_line.chars().count() + 1);
        Span {
            start,
            end,
            line,
            column,
        }
    }

    fn error<T>(&self, kind: DiagnosticKind, message: String) -> ParseResult<T> {
        self.error_at(self.offset, kind, message)
    }

    fn error_at<T>(&self, offset: usize, kind: DiagnosticKind, message: String) -> ParseResult<T> {
        Err(Diagnostic {
            file: self.file.clone(),
            kind,
            message,
            span: self.span(offset, offset),
        })
    }
}

fn is_name_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | ':' | '.')
}

fn classify_binding(name: &str) -> Option<BindingKind> {
    if name.starts_with("capture-bind:") || name.starts_with("captureBind:") {
        Some(BindingKind::CaptureBind)
    } else if name.starts_with("capture-catch:") || name.starts_with("captureCatch:") {
        Some(BindingKind::CaptureCatch)
    } else if name.starts_with("mut-bind:") || name.starts_with("mutBind:") {
        Some(BindingKind::MutBind)
    } else if name.starts_with("catch:") || name.starts_with("catch") {
        Some(BindingKind::Catch)
    } else if name.starts_with("bind:") || name.starts_with("bind") {
        Some(BindingKind::Bind)
    } else {
        None
    }
}

fn interpolation_end(source: &str, start: usize) -> Option<usize> {
    let mut offset = start;
    let mut nested_braces = 0usize;
    let mut quote = None;
    let mut escaped = false;

    while let Some(character) = source[offset..].chars().next() {
        if let Some(active_quote) = quote {
            offset += character.len_utf8();
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' | '`' => quote = Some(character),
            '{' => nested_braces += 1,
            '}' if nested_braces > 0 => nested_braces -= 1,
            '}' if source[offset..].starts_with("}}") => return Some(offset),
            _ => {}
        }
        offset += character.len_utf8();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_elements_bindings_and_interpolations() -> Result<(), Box<dyn std::error::Error>>
    {
        let document = parse(
            "pages/home/index.wxml",
            "<view class=\"card {{active ? 'on' : 'off'}}\"><text>{{ user.name }}</text><button bind:tap=\"save\"/></view>",
        )?;

        assert_eq!(document.children.len(), 1);
        let Node::Element(root) = &document.children[0] else {
            return Err("expected element".into());
        };
        assert_eq!(root.tag_name, "view");
        assert_eq!(root.children.len(), 2);
        assert!(matches!(
            &root.attributes[0].value,
            AttributeValue::Interpolated(_)
        ));
        let Node::Element(button) = &root.children[1] else {
            return Err("expected button".into());
        };
        assert_eq!(button.attributes[0].binding, Some(BindingKind::Bind));
        Ok(())
    }

    #[test]
    fn preserves_unicode_and_reports_source_location() -> Result<(), Box<dyn std::error::Error>> {
        let document = parse("pages/home/index.wxml", "你好\n<view>世界</view>")?;
        assert!(matches!(&document.children[0], Node::Text { value, .. } if value == "你好\n"));

        let Err(diagnostic) = parse("pages/broken.wxml", "<view>\n <text></view>") else {
            return Err("mismatched tag was accepted".into());
        };
        assert_eq!(diagnostic.file, "pages/broken.wxml");
        assert_eq!(diagnostic.kind, DiagnosticKind::MismatchedClosingTag);
        assert_eq!(diagnostic.span.line, 2);
        assert_eq!(diagnostic.span.column, 8);
        Ok(())
    }

    #[test]
    fn handles_nested_braces_and_strings_in_interpolation() -> Result<(), Box<dyn std::error::Error>>
    {
        let document = parse(
            "inline.wxml",
            "{{ ({ text: '}}', nested: { value: 3 } }) }}",
        )?;
        assert!(
            matches!(&document.children[0], Node::Interpolation { expression, .. } if expression.contains("nested"))
        );
        Ok(())
    }

    #[test]
    fn skips_comments_and_requires_closed_elements() -> Result<(), Box<dyn std::error::Error>> {
        let document = parse("inline.wxml", "<!-- ignored --><view/>text")?;
        assert_eq!(document.children.len(), 2);
        assert!(parse("inline.wxml", "<view>").is_err());
        assert!(parse("inline.wxml", "</view>").is_err());
        Ok(())
    }

    #[test]
    fn exposes_legacy_event_binding_spellings() -> Result<(), Box<dyn std::error::Error>> {
        let document = parse(
            "inline.wxml",
            "<view bindtap='a' catch:tap='b' capture-bind:tap='c'/>",
        )?;
        let Node::Element(element) = &document.children[0] else {
            return Err("expected element".into());
        };
        assert_eq!(element.attributes[0].binding, Some(BindingKind::Bind));
        assert_eq!(element.attributes[1].binding, Some(BindingKind::Catch));
        assert_eq!(
            element.attributes[2].binding,
            Some(BindingKind::CaptureBind)
        );
        Ok(())
    }

    #[test]
    fn parses_attribute_interpolation_with_nested_braces_and_rejects_unclosed_interpolation()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = parse(
            "inline.wxml",
            "<view value=\"{{ ({ text: '}}', item: {} }) }}\"/>",
        )?;
        let Node::Element(element) = &document.children[0] else {
            return Err("expected element".into());
        };
        assert!(matches!(
            &element.attributes[0].value,
            AttributeValue::Expression(_)
        ));

        let Err(error) = parse("inline.wxml", "<view value=\"{{ unresolved\"/>") else {
            return Err("unterminated attribute interpolation was accepted".into());
        };
        assert_eq!(error.kind, DiagnosticKind::UnterminatedInterpolation);
        Ok(())
    }

    #[test]
    fn serializes_ast_for_inspector_consumers() -> Result<(), Box<dyn std::error::Error>> {
        let document = parse("inline.wxml", "<view>{{ title }}</view>")?;
        let serialized = serde_json::to_value(document)?;
        assert_eq!(serialized["children"][0]["Element"]["tag_name"], "view");
        Ok(())
    }
}
