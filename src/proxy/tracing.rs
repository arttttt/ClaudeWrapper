use tracing_subscriber::EnvFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize tracing with optional file output.
///
/// Logging is disabled by default for TUI mode.
/// Set `CLAUDE_WRAPPER_LOG` env var to a file path to enable logging.
///
/// Log files are created with unique names to prevent conflicts when
/// multiple instances run simultaneously: `{path}.{timestamp}.{pid}`
pub fn init_tracing() {
    let Some(log_path) = std::env::var("CLAUDE_WRAPPER_LOG").ok() else {
        // No logging configured - skip initialization entirely
        // This is the default for TUI mode to avoid corrupting the display
        return;
    };

    // Generate unique filename with timestamp and PID to prevent conflicts
    // when multiple instances run simultaneously
    let pid = std::process::id();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let unique_path = format!("{}.{}.{}", log_path, timestamp, pid);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let Ok(file) = std::fs::File::create(&unique_path) else {
        eprintln!("Warning: Failed to create log file: {}", unique_path);
        return;
    };

    let file_layer = fmt::layer()
        .with_writer(file)
        .with_ansi(false)
        .with_target(true)
        .with_level(true);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .init();
}
