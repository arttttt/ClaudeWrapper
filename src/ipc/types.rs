use tokio::sync::oneshot;

use crate::backend::BackendError;
use crate::config::DebugLoggingConfig;
use crate::metrics::MetricsSnapshot;
use crate::proxy::thinking::TransformError;

#[derive(Debug)]
pub enum IpcError {
    Disconnected,
    Timeout,
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::Disconnected => write!(f, "IPC channel disconnected"),
            IpcError::Timeout => write!(f, "IPC request timed out"),
        }
    }
}

impl std::error::Error for IpcError {}

#[derive(Debug, Clone)]
pub struct ProxyStatus {
    pub active_backend: String,
    pub uptime_seconds: u64,
    pub total_requests: u64,
    pub healthy: bool,
}

#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub id: String,
    pub display_name: String,
    pub is_active: bool,
    pub is_configured: bool,
    pub base_url: String,
}

pub enum IpcCommand {
    SwitchBackend {
        backend_id: String,
        respond_to: oneshot::Sender<Result<String, BackendError>>,
    },
    /// Summarize session and then switch backend.
    ///
    /// This is used when thinking.mode = summarize.
    /// 1. Calls transformer.on_backend_switch() to summarize
    /// 2. If successful, switches to the new backend
    /// 3. Returns Ok(summary_preview) or Err(TransformError)
    SummarizeAndSwitchBackend {
        from_backend: String,
        to_backend: String,
        respond_to: oneshot::Sender<Result<String, TransformError>>,
    },
    GetStatus {
        respond_to: oneshot::Sender<ProxyStatus>,
    },
    GetMetrics {
        backend_id: Option<String>,
        respond_to: oneshot::Sender<MetricsSnapshot>,
    },
    ListBackends {
        respond_to: oneshot::Sender<Vec<BackendInfo>>,
    },
    GetDebugLogging {
        respond_to: oneshot::Sender<DebugLoggingConfig>,
    },
    SetDebugLogging {
        config: DebugLoggingConfig,
        respond_to: oneshot::Sender<Result<(), IpcError>>,
    },
}
