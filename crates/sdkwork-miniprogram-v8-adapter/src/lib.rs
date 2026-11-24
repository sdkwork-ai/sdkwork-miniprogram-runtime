//! V8 execution behind a small, typed Rust interface.
//!
//! [`evaluate`] creates an isolated context for one-shot evaluation. [`JavaScriptContext`]
//! owns a persistent context for one application and serializes evaluation on its V8 thread.

use std::sync::{OnceLock, mpsc};
use std::time::Duration;

/// Maximum JavaScript source size accepted by one execution request.
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// Maximum V8 heap size for one application context.
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
    /// A configured resource ceiling is invalid.
    InvalidResourceLimit {
        /// Name of the rejected resource limit.
        resource: &'static str,
    },
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
            Self::InvalidResourceLimit { resource } => {
                write!(
                    formatter,
                    "JavaScript resource limit `{resource}` must be positive"
                )
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
        }
    }
}

impl std::error::Error for V8Error {}

/// Result type returned by this adapter.
pub type Result<T> = std::result::Result<T, V8Error>;

enum ContextCommand {
    Evaluate {
        source: String,
        result: mpsc::SyncSender<Result<String>>,
    },
}

/// A persistent, isolated JavaScript context intended to be owned by one application.
///
/// Evaluation is serialized on a dedicated V8 thread. Global JavaScript state remains
/// available between calls on this context, while another context receives a distinct isolate.
/// A timed-out context is terminated and cannot be used again.
pub struct JavaScriptContext {
    commands: mpsc::SyncSender<ContextCommand>,
    isolate: v8::IsolateHandle,
    max_source_bytes: usize,
    max_heap_bytes: usize,
    timeout: Duration,
    terminated: bool,
}

impl std::fmt::Debug for JavaScriptContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JavaScriptContext")
            .field("max_source_bytes", &self.max_source_bytes)
            .field("max_heap_bytes", &self.max_heap_bytes)
            .field("timeout", &self.timeout)
            .field("terminated", &self.terminated)
            .finish_non_exhaustive()
    }
}

impl JavaScriptContext {
    /// Create an empty isolated context with default source and execution limits.
    ///
    /// # Errors
    /// Returns an error if the V8 worker thread cannot be initialized.
    pub fn new() -> Result<Self> {
        Self::with_options(
            DEFAULT_MAX_SOURCE_BYTES,
            DEFAULT_MAX_HEAP_BYTES,
            DEFAULT_TIMEOUT,
        )
    }

    /// Create an empty isolated context with explicit source and execution limits.
    ///
    /// # Errors
    /// Returns an error if the limits are invalid or the V8 worker thread cannot be initialized.
    pub fn with_options(
        max_source_bytes: usize,
        max_heap_bytes: usize,
        timeout: Duration,
    ) -> Result<Self> {
        if max_heap_bytes == 0 {
            return Err(V8Error::InvalidResourceLimit {
                resource: "max_heap_bytes",
            });
        }
        if timeout.is_zero() {
            return Err(V8Error::ExecutionTimeout { timeout_ms: 0 });
        }
        initialize_v8();
        let (command_sender, command_receiver) = mpsc::sync_channel(1);
        let (handle_sender, handle_receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("sdkwork-v8-context".to_owned())
            .spawn(move || context_worker(command_receiver, handle_sender, max_heap_bytes))
            .map_err(|error| V8Error::Initialization(error.to_string()))?;
        let isolate = handle_receiver.recv().map_err(|_| V8Error::WorkerStopped)?;
        Ok(Self {
            commands: command_sender,
            isolate,
            max_source_bytes,
            max_heap_bytes,
            timeout,
            terminated: false,
        })
    }

    /// Evaluate source in this context and return the JavaScript string form of its result.
    ///
    /// # Errors
    /// Returns an error for an oversized source, a JavaScript failure, a timeout, or a stopped worker.
    pub fn evaluate(&mut self, source: &str) -> Result<String> {
        if source.len() > self.max_source_bytes {
            return Err(V8Error::SourceTooLarge {
                size: source.len(),
                maximum: self.max_source_bytes,
            });
        }
        if self.terminated {
            return Err(V8Error::WorkerStopped);
        }
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        self.commands
            .send(ContextCommand::Evaluate {
                source: source.to_owned(),
                result: result_sender,
            })
            .map_err(|_| V8Error::WorkerStopped)?;
        match result_receiver.recv_timeout(self.timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.terminated = true;
                let timeout_ms = u64::try_from(self.timeout.as_millis()).unwrap_or(u64::MAX);
                tracing::warn!(timeout_ms, "persistent V8 evaluation deadline expired");
                let _ = self.isolate.terminate_execution();
                Err(V8Error::ExecutionTimeout { timeout_ms })
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.terminated = true;
                Err(V8Error::WorkerStopped)
            }
        }
    }
}

// The spawned thread must own its channel endpoints for its entire lifetime.
#[allow(clippy::needless_pass_by_value)]
fn context_worker(
    commands: mpsc::Receiver<ContextCommand>,
    handle_sender: mpsc::SyncSender<v8::IsolateHandle>,
    max_heap_bytes: usize,
) {
    let create_params = v8::CreateParams::default().heap_limits(0, max_heap_bytes);
    let isolate = &mut v8::Isolate::new(create_params);
    if handle_sender.send(isolate.thread_safe_handle()).is_err() {
        return;
    }
    let context = create_context(isolate);

    while let Ok(ContextCommand::Evaluate { source, result }) = commands.recv() {
        let evaluation = evaluate_in_context(isolate, &context, &source);
        let terminated = isolate.is_execution_terminating();
        if terminated {
            isolate.cancel_terminate_execution();
        }
        let _ = result.send(evaluation);
        if terminated {
            break;
        }
    }
}

fn create_context(isolate: &mut v8::OwnedIsolate) -> v8::Global<v8::Context> {
    v8::scope!(let handle_scope, isolate);
    let context = v8::Context::new(handle_scope, v8::ContextOptions::default());
    v8::Global::new(handle_scope, context)
}

fn evaluate_in_context(
    isolate: &mut v8::OwnedIsolate,
    context: &v8::Global<v8::Context>,
    source: &str,
) -> Result<String> {
    v8::scope!(let handle_scope, isolate);
    let context = v8::Local::new(handle_scope, context);
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
/// V8 runs on a dedicated thread so the caller can enforce the execution
/// deadline by terminating the isolate from the calling thread.
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
        return Err(V8Error::InvalidResourceLimit {
            resource: "max_heap_bytes",
        });
    }
    initialize_v8();
    let source = source.to_owned();
    let (handle_sender, handle_receiver) = mpsc::sync_channel(1);
    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("sdkwork-v8-eval".to_owned())
        .spawn(move || {
            let result = evaluate_in_current_thread(&source, max_heap_bytes, &handle_sender);
            let _ = result_sender.send(result);
        })
        .map_err(|error| V8Error::Initialization(error.to_string()))?;

    let handle = handle_receiver.recv().map_err(|_| V8Error::WorkerStopped)?;
    match result_receiver.recv_timeout(timeout) {
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
    handle_sender: &mpsc::SyncSender<v8::IsolateHandle>,
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
    use std::time::Duration;

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
    fn isolates_state_between_calls() -> Result<(), Box<dyn std::error::Error>> {
        let _ = evaluate("globalThis.privateValue = 42")?;
        assert_eq!(evaluate("typeof globalThis.privateValue")?, "undefined");
        Ok(())
    }

    #[test]
    fn preserves_state_inside_a_context_and_isolates_other_contexts()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut first = super::JavaScriptContext::new()?;
        let mut second = super::JavaScriptContext::new()?;
        assert_eq!(first.evaluate("globalThis.count = 1")?, "1");
        assert_eq!(first.evaluate("globalThis.count += 1")?, "2");
        assert_eq!(second.evaluate("typeof globalThis.count")?, "undefined");
        Ok(())
    }

    #[test]
    fn timed_out_context_is_terminated_and_unusable() -> Result<(), Box<dyn std::error::Error>> {
        let mut context = super::JavaScriptContext::with_options(
            DEFAULT_MAX_SOURCE_BYTES,
            DEFAULT_MAX_HEAP_BYTES,
            Duration::from_millis(100),
        )?;
        assert!(matches!(
            context.evaluate("while (true) {}"),
            Err(V8Error::ExecutionTimeout { .. })
        ));
        assert_eq!(context.evaluate("1"), Err(V8Error::WorkerStopped));
        Ok(())
    }

    #[test]
    fn rejects_zero_heap_limit() {
        assert_eq!(
            evaluate_with_options("1", DEFAULT_MAX_SOURCE_BYTES, 0, DEFAULT_TIMEOUT),
            Err(V8Error::InvalidResourceLimit {
                resource: "max_heap_bytes"
            })
        );
    }
}
