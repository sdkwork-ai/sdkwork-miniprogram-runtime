# Headless Renderer

Maps the WXML parser's source-located document tree to a serializable structural Render Tree with deterministic preorder node IDs and retained unparsed binding expressions. `build_with_styles` applies a documented WXSS subset and returns supported declarations indexed by node id, along with source-located diagnostics for unsupported constructs.

```rust
let document = sdkwork_miniprogram_wxml_compiler::parse("inline.wxml", "<view>ok</view>")?;
let tree = sdkwork_miniprogram_headless_renderer::build(&document);
assert_eq!(tree.node_count, 2);
# Ok::<(), sdkwork_miniprogram_wxml_compiler::Diagnostic>(())
```

This crate provides inspection and structural testing data. It does not implement expression evaluation, full WXSS semantics, layout, pixels, event dispatch, component state, accessibility mapping or screenshot capture.

## WXSS subset

`build_with_styles` supports type, class, and ID selectors, descendant combinators, selector lists, and cascade ordering by specificity followed by stylesheet order. Matching uses literal WXML `class` and `id` attributes. Attribute bindings are not evaluated. Supported declaration names are a structural set of common color, box, typography, positioning, border, and flex properties. Values are retained as strings for inspection.

Pseudo-classes, pseudo-elements, attribute selectors, sibling and child combinators, at-rules, imports, variables, `!important`, and layout calculation are unsupported. Each unsupported rule or declaration produces a stylesheet source diagnostic. The output is not a browser-equivalent computed style or pixel render.
