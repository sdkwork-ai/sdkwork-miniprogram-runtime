# Mini Program Package Loader

Loads source-directory projects and extracts ZIP packages with bounded resource usage and path validation. The loader reports unsupported or malformed project inputs as structured errors and does not attempt to parse proprietary package formats.

## Status

Directory project discovery and ZIP safety checks are implemented in this crate. Compatibility with any specific WeChat base-library release remains unverified. The crate validates project entry files and pages, it does not compile JavaScript, WXML, or WXSS.
