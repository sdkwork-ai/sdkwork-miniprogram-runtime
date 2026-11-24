# WXML Compiler

Parses WXML document structure into a serializable source-located syntax tree. The parser currently handles nested elements, attributes, text, balanced interpolation boundaries, comments and common event-binding spellings.

```rust
let document = sdkwork_miniprogram_wxml_compiler::parse(
    "pages/index/index.wxml",
    "<view>{{title}}</view>",
)?;
assert_eq!(document.children.len(), 1);
# Ok::<(), sdkwork_miniprogram_wxml_compiler::Diagnostic>(())
```

Expression evaluation, directives such as `wx:if` and `wx:for`, template imports, WXS, component semantics and complete WeChat compatibility remain outside the current parser.
