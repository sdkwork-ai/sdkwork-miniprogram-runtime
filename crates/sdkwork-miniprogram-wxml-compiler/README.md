# WXML Compiler

Parses WXML document structure into a serializable source-located syntax tree. The parser handles nested elements, attributes, text, balanced interpolation boundaries, comments, common event-binding spellings, and syntax metadata for `wx:if`, `wx:elif`, `wx:else` and `wx:for`.

Recognized directives are attached to each element as source-located metadata. Their expressions are preserved without evaluation. `wx:for-item` and `wx:for-index` identifiers default to `item` and `index`, and an optional `wx:key` is recorded. Invalid values, duplicate directives and loop modifiers without `wx:for` produce parser diagnostics.

```rust
let document = sdkwork_miniprogram_wxml_compiler::parse(
    "pages/index/index.wxml",
    "<view>{{title}}</view>",
)?;
assert_eq!(document.children.len(), 1);
# Ok::<(), sdkwork_miniprogram_wxml_compiler::Diagnostic>(())
```

Expression evaluation, conditional-chain adjacency and rendering semantics, template imports, WXS, component semantics and complete WeChat compatibility remain outside the current parser.
