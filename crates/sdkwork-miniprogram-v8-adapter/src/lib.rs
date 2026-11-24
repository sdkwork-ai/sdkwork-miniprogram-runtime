//! One-shot V8 execution behind a small, typed Rust interface.
//!
//! Each call creates a new V8 isolate and context. Values and JavaScript state
//! do not persist between calls.

use std::sync::{OnceLock, mpsc};
use std::time::Duration;

/// Maximum JavaScript source size accepted by one execution request.
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// Maximum V8 heap size used by one-shot evaluation.
pub const DEFAULT_MAX_HEAP_BYTES: usize = 64 * 1024 * 1024;

/// Default deadline applied by [`evaluate`].
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Errors returned by the V8 adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V8Error {
    /// Source was larger than the configured limit.
    SourceTooLarge {
        /// Number of UTF-8 source bytes provided.
        size: usize,
        /// Maximum permitted source bytes.
        maximum: usize,
    },
    /// V8 rejected source-string construction.
    InvalidSource,
    /// JavaScript compilation failed.
    Compile(String),
    /// JavaScript execution threw an exception.
    Execution(String),
    /// V8 returned a value that could not be converted to a string.
    ResultConversion,
    /// JavaScript execution exceeded its configured deadline.
    ExecutionTimeout {
        /// Execution time limit in milliseconds.
        timeout_ms: u64,
    },
    /// A background evaluation thread could not be started.
    Initialization(String),
    /// The evaluation thread exited without returning a result.
    WorkerStopped,
    /// A configured resource ceiling is zero or invalid.
    InvalidResourceLimit {
        /// Resource setting that was rejected.
        resource: &'static str,
    },
}

impl std::fmt::Display for V8Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceTooLarge { size, maximum } => {
                write!(
                    formatter,
                    "JavaScript source is {size} bytes, limit is {maximum}"
                )
            }
            Self::InvalidSource => {
                formatter.write_str("JavaScript source is not representable by V8")
            }
            Self::Compile(message) => write!(formatter, "JavaScript compilation failed: {message}"),
            Self::Execution(message) => write!(formatter, "JavaScript execution failed: {message}"),
            Self::ResultConversion => {
                formatter.write_str("JavaScript result could not be converted to a string")
            }
            Self::ExecutionTimeout { timeout_ms } => {
                write!(formatter, "JavaScript execution exceeded {timeout_ms} ms")
            }
            Self::Initialization(message) => {
                write!(formatter, "could not start V8 execution thread: {message}")
            }
            Self::WorkerStopped => formatter.write_str("V8 execution thread stopped unexpectedly"),
            Self::InvalidResourceLimit { resource } => {
                write!(formatter, "invalid V8 resource limit `{resource}`")
            }
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
    evaluate_with_options(
        source,
        DEFAULT_MAX_SOURCE_BYTES,
        DEFAULT_MAX_HEAP_BYTES,
        DEFAULT_TIMEOUT,
    )
}

/// Evaluates JavaScript subject to an explicit UTF-8 source-size limit.
///
/// # Errors
///
/// Returns an error when the size limit is exceeded or V8 cannot compile,
/// execute, or convert the result.
pub fn evaluate_with_limit(source: &str, max_source_bytes: usize) -> Result<String> {
    evaluate_with_options(
        source,
        max_source_bytes,
        DEFAULT_MAX_HEAP_BYTES,
        DEFAULT_TIMEOUT,
    )
}

/// Evaluates JavaScript with explicit source-size and execution-time limits.
///
/// V8 runs on a dedicated thread so the caller can enforce the deadline by
/// terminating the isolate from a timer thread.
///
/// # Errors
///
/// Returns an error if source validation, compilation, execution, result
/// conversion, or the timeout deadline fails.
pub fn evaluate_with_options(
    source: &str,
    max_source_bytes: usize,
    max_heap_bytes: usize,
    timeout: Duration,
) -> Result<String> {
    if source.len() > max_source_bytes {
        return Err(V8Error::SourceTooLarge {
            size: source.len(),
            maximum: max_source_bytes,
        });
    }
    let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    if timeout.is_zero() {
        return Err(V8Error::ExecutionTimeout { timeout_ms });
    }
    if max_heap_bytes == 0 {
        return Err(V8Error::InvalidResourceLimit { resource: "max_heap_bytes" });
    }
    initialize_v8();
    let source = source.to_owned();
    let (handle_sender, handle_receiver) = mpsc::sync_channel(1);
    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    let started_at = std::time::Instant::now();
    std::thread::Builder::new()
        .name("sdkwork-v8-eval".to_owned())
        .spawn(move || {
            let result = evaluate_in_current_thread(&source, max_heap_bytes, handle_sender);
            let _ = result_sender.send(result);
        })
        .map_err(|error| V8Error::Initialization(error.to_string()))?;

    let handle = match handle_receiver.recv_timeout(timeout) {
        Ok(handle) => handle,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            return Err(V8Error::ExecutionTimeout { timeout_ms });
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => return Err(V8Error::WorkerStopped),
    };
    let remaining = timeout.saturating_sub(started_at.elapsed());
    match result_receiver.recv_timeout(remaining) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            tracing::warn!(timeout_ms, "V8 evaluation deadline expired");
            let _ = handle.terminate_execution();
            let _ = result_receiver.recv();
            Err(V8Error::ExecutionTimeout { timeout_ms })
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(V8Error::WorkerStopped),
    }
}

fn evaluate_in_current_thread(
    source: &str,
    max_heap_bytes: usize,
    handle_sender: mpsc::SyncSender<v8::IsolateHandle>,
) -> Result<String> {
    let create_params = v8::CreateParams::default().heap_limits(0, max_heap_bytes);
    let isolate = &mut v8::Isolate::new(create_params);
    let isolate_handle = isolate.thread_safe_handle();
    handle_sender
        .send(isolate_handle)
        .map_err(|_| V8Error::WorkerStopped)?;
    v8::scope!(let handle_scope, isolate);
    let context = v8::Context::new(handle_scope, v8::ContextOptions::default());
    v8::scope_with_context!(let context_scope, handle_scope, context);
    v8::tc_scope!(let try_catch, context_scope);

    let code = v8::String::new(try_catch, source).ok_or(V8Error::InvalidSource)?;
    let Some(script) = v8::Script::compile(try_catch, code, None) else {
        let message = try_catch
            .stack_trace()
            .or_else(|| try_catch.exception())
            .map_or_else(
                || "unknown V8 exception".to_owned(),
                |value| value.to_rust_string_lossy(try_catch),
            );
        return Err(V8Error::Compile(message));
    };
    let Some(value) = script.run(try_catch) else {
        let message = try_catch
            .stack_trace()
            .or_else(|| try_catch.exception())
            .map_or_else(
                || "unknown V8 exception".to_owned(),
                |value| value.to_rust_string_lossy(try_catch),
            );
        return Err(V8Error::Execution(message));
    };
    let value = value
        .to_string(try_catch)
        .ok_or(V8Error::ResultConversion)?;
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

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_HEAP_BYTES, DEFAULT_MAX_SOURCE_BYTES, DEFAULT_TIMEOUT, V8Error, evaluate,
        evaluate_with_limit, evaluate_with_options,
    };

    #[test]
    fn evaluates_javascript_in_v8() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(evaluate("1 + 2")?, "3");
        assert_eq!(evaluate("'mini' + 'program'")?, "miniprogram");
        Ok(())
    }

    #[test]
    fn maps_compile_and_execution_errors() {
        assert!(matches!(evaluate("let = ;"), Err(V8Error::Compile(_))));
        assert!(matches!(
            evaluate("throw new Error('expected')"),
            Err(V8Error::Execution(_))
        ));
    }

    #[test]
    fn rejects_source_over_configured_limit() {
        assert_eq!(
            evaluate_with_limit("123", 2),
            Err(V8Error::SourceTooLarge {
                size: 3,
                maximum: 2
            })
        );
    }

    #[test]
    fn rejects_execution_over_configured_deadline() {
        assert!(matches!(
            evaluate_with_options(
                "while (true) {}",
                DEFAULT_MAX_SOURCE_BYTES,
                DEFAULT_MAX_HEAP_BYTES,
                Duration::from_millis(250),
            ),
            Err(V8Error::ExecutionTimeout { timeout_ms: 250 })
        ));
    }

    #[test]
    fn rejects_zero_heap_limit() {
        assert_eq!(
            evaluate_with_options("1 + 1", DEFAULT_MAX_SOURCE_BYTES, 0, DEFAULT_TIMEOUT),
            Err(V8Error::InvalidResourceLimit {
                resource: "max_heap_bytes"
            })
        );
    }

    #[test]
    fn isolates_state_between_calls() -> Result<(), Box<dyn std::error::Error>> {
        let _ = evaluate("globalThis.privateValue = 42")?;
        assert_eq!(evaluate("typeof globalThis.privateValue")?, "undefined");
        Ok(())
    }
}
