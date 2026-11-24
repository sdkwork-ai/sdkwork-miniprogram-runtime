# Headless Renderer

Maps the WXML parser's source-located document tree to a serializable structural Render Tree with deterministic preorder node IDs and retained unparsed binding expressions.

```rust
let document = sdkwork_miniprogram_wxml_compiler::parse("inline.wxml", "<view>ok</view>")?;
let tree = sdkwork_miniprogram_headless_renderer::build(&document);
assert_eq!(tree.node_count, 2);
# Ok::<(), sdkwork_miniprogram_wxml_compiler::Diagnostic>(())
```

This crate provides inspection and structural testing data. It does not implement expression evaluation, WXSS, computed styles, layout, pixels, event dispatch, component state, accessibility mapping or screenshot capture.
