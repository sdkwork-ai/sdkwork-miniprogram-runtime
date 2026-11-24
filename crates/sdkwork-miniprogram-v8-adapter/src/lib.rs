//! One-shot V8 execution behind a small, typed Rust interface.
//!
//! Each call creates a new V8 isolate and context. Values and JavaScript state
//! do not persist between calls.

use std::sync::OnceLock;

/// Maximum JavaScript source size accepted by one execution request.
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// Errors returned by the V8 adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V8Error {
    /// Source was larger than the configured limit.
    SourceTooLarge { size: usize, maximum: usize },
    /// V8 rejected source-string construction.
    InvalidSource,
    /// JavaScript compilation failed.
    Compile(String),
    /// JavaScript execution threw an exception.
    Execution(String),
    /// V8 returned a value that could not be converted to a string.
    ResultConversion,
}

impl std::fmt::Display for V8Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceTooLarge { size, maximum } => {
                write!(formatter, "JavaScript source is {size} bytes, limit is {maximum}")
            }
            Self::InvalidSource => formatter.write_str("JavaScript source is not representable by V8"),
            Self::Compile(message) => write!(formatter, "JavaScript compilation failed: {message}"),
            Self::Execution(message) => write!(formatter, "JavaScript execution failed: {message}"),
            Self::ResultConversion => formatter.write_str("JavaScript result could not be converted to a string"),
        }
    }
}

impl std::error::Error for V8Error {}

/// Result type returned by this adapter.
pub type Result<T> = std::result::Result<T, V8Error>;

/// Evaluates JavaScript in a newly created V8 isolate and returns its string form.
///
/// # Errors
///
/// Returns an error for source beyond `max_source_bytes`, compile errors,
/// JavaScript exceptions, or result conversion failures.
///
/// # Examples
///
/// ```
/// let output = sdkwork_miniprogram_v8_adapter::evaluate("1 + 2")?;
/// assert_eq!(output, "3");
/// # Ok::<(), sdkwork_miniprogram_v8_adapter::V8Error>(())
/// ```
pub fn evaluate(source: &str) -> Result<String> {
    evaluate_with_limit(source, DEFAULT_MAX_SOURCE_BYTES)
}

/// Evaluates JavaScript subject to an explicit UTF-8 source-size limit.
///
/// # Errors
///
/// Returns an error when the size limit is exceeded or V8 cannot compile,
/// execute, or convert the result.
pub fn evaluate_with_limit(source: &str, max_source_bytes: usize) -> Result<String> {
    if source.len() > max_source_bytes {
        return Err(V8Error::SourceTooLarge {
            size: source.len(),
            maximum: max_source_bytes,
        });
    }
    initialize_v8();

    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &v8::ContextScope::new(scope, context);
    let code = v8::String::new(scope, source).ok_or(V8Error::InvalidSource)?;
    let try_catch = &mut v8::TryCatch::new(scope);
    let script = v8::Script::compile(try_catch, code, None)
        .ok_or_else(|| V8Error::Compile(exception_message(try_catch)))?;
    let value = script
        .run(try_catch)
        .ok_or_else(|| V8Error::Execution(exception_message(try_catch)))?;
    let value = value.to_string(try_catch).ok_or(V8Error::ResultConversion)?;
    Ok(value.to_rust_string_lossy(try_catch))
}

fn initialize_v8() {
    static INITIALIZED: OnceLock<()> = OnceLock::new();
    INITIALIZED.get_or_init(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

fn exception_message(scope: &mut v8::TryCatch<v8::HandleScope>) -> String {
    scope
        .exception()
        .and_then(|exception| exception.to_string(scope))
        .map(|exception| exception.to_rust_string_lossy(scope))
        .unwrap_or_else(|| "unknown V8 exception".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{evaluate, evaluate_with_limit, V8Error};

    #[test]
    fn evaluates_javascript_in_v8() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(evaluate("1 + 2")?, "3");
        assert_eq!(evaluate("'mini' + 'program'")?, "miniprogram");
        Ok(())
    }

    #[test]
    fn maps_compile_and_execution_errors() {
        assert!(matches!(evaluate("let = ;"), Err(V8Error::Compile(_))));
        assert!(matches!(evaluate("throw new Error('expected')"), Err(V8Error::Execution(_))));
    }

    #[test]
    fn rejects_source_over_configured_limit() {
        assert_eq!(
            evaluate_with_limit("123", 2),
            Err(V8Error::SourceTooLarge { size: 3, maximum: 2 })
        );
    }

    #[test]
    fn isolates_state_between_calls() -> Result<(), Box<dyn std::error::Error>> {
        let _ = evaluate("globalThis.privateValue = 42")?;
        assert_eq!(evaluate("typeof globalThis.privateValue")?, "undefined");
        Ok(())
    }
}
