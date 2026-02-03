use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::backend::BackendError;
use crate::config::DebugLoggingConfig;
use crate::metrics::MetricsSnapshot;
use crate::proxy::thinking::TransformError;

use super::types::{BackendInfo, IpcCommand, IpcError, ProxyStatus};

const IPC_TIMEOUT: Duration = Duration::from_secs(1);
/// Longer timeout for summarization (can take time with LLM call)
const SUMMARIZE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct IpcClient {
    sender: mpsc::Sender<IpcCommand>,
}

impl IpcClient {
    pub fn new(sender: mpsc::Sender<IpcCommand>) -> Self {
        Self { sender }
    }

    pub async fn switch_backend(
        &self,
        backend_id: String,
    ) -> Result<Result<String, BackendError>, IpcError> {
        let (respond_to, receiver) = oneshot::channel();
        self.sender
            .send(IpcCommand::SwitchBackend {
                backend_id,
                respond_to,
            })
            .await
            .map_err(|_| IpcError::Disconnected)?;

        recv_with_timeout(receiver).await
    }

    pub async fn get_status(&self) -> Result<ProxyStatus, IpcError> {
        let (respond_to, receiver) = oneshot::channel();
        self.sender
            .send(IpcCommand::GetStatus { respond_to })
            .await
            .map_err(|_| IpcError::Disconnected)?;

        recv_with_timeout(receiver).await
    }

    pub async fn get_metrics(
        &self,
        backend_id: Option<String>,
    ) -> Result<MetricsSnapshot, IpcError> {
        let (respond_to, receiver) = oneshot::channel();
        self.sender
            .send(IpcCommand::GetMetrics {
                backend_id,
                respond_to,
            })
            .await
            .map_err(|_| IpcError::Disconnected)?;

        recv_with_timeout(receiver).await
    }

    pub async fn list_backends(&self) -> Result<Vec<BackendInfo>, IpcError> {
        let (respond_to, receiver) = oneshot::channel();
        self.sender
            .send(IpcCommand::ListBackends { respond_to })
            .await
            .map_err(|_| IpcError::Disconnected)?;

        recv_with_timeout(receiver).await
    }

    pub async fn get_debug_logging(&self) -> Result<DebugLoggingConfig, IpcError> {
        let (respond_to, receiver) = oneshot::channel();
        self.sender
            .send(IpcCommand::GetDebugLogging { respond_to })
            .await
            .map_err(|_| IpcError::Disconnected)?;

        recv_with_timeout(receiver).await
    }

    pub async fn set_debug_logging(
        &self,
        config: DebugLoggingConfig,
    ) -> Result<(), IpcError> {
        let (respond_to, receiver) = oneshot::channel();
        self.sender
            .send(IpcCommand::SetDebugLogging { config, respond_to })
            .await
            .map_err(|_| IpcError::Disconnected)?;

        let result = recv_with_timeout(receiver).await?;
        result
    }

    /// Summarize session and switch backend.
    ///
    /// This is used when thinking.mode = summarize.
    /// Returns Ok(summary_preview) on success, or the TransformError on failure.
    pub async fn summarize_and_switch_backend(
        &self,
        from_backend: String,
        to_backend: String,
    ) -> Result<Result<String, TransformError>, IpcError> {
        let (respond_to, receiver) = oneshot::channel();
        self.sender
            .send(IpcCommand::SummarizeAndSwitchBackend {
                from_backend,
                to_backend,
                respond_to,
            })
            .await
            .map_err(|_| IpcError::Disconnected)?;

        // Use longer timeout for summarization
        match tokio::time::timeout(SUMMARIZE_TIMEOUT, receiver).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err(IpcError::Disconnected),
            Err(_) => Err(IpcError::Timeout),
        }
    }
}

async fn recv_with_timeout<T>(receiver: oneshot::Receiver<T>) -> Result<T, IpcError> {
    match tokio::time::timeout(IPC_TIMEOUT, receiver).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err(IpcError::Disconnected),
        Err(_) => Err(IpcError::Timeout),
    }
}
