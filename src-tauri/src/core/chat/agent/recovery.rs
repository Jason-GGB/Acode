use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::core::ai::provider::ProviderError;
use crate::core::runtime::{ChatRequest, StreamEvent};

use super::super::agent_loop::stream_turn::{self, StreamTurnResult};

impl super::AgentRunner {
    pub(super) async fn stream_with_recovery(
        &self,
        request: &ChatRequest,
        tx: &mpsc::Sender<StreamEvent>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<StreamTurnResult, ProviderError> {
        for recovery in 0..=3u32 {
            if cancelled.load(Ordering::Relaxed) || tx.is_closed() {
                return Err(ProviderError::cancelled());
            }
            let (turn_tx, turn_rx) = mpsc::channel(64);
            let provider = Arc::clone(&self.provider);
            let turn_request = request.clone();
            let provider_task =
                tauri::async_runtime::spawn(
                    async move { provider.stream(turn_request, turn_tx).await },
                );
            let collected = stream_turn::collect_stream_turn(turn_rx, tx, cancelled).await;
            let result = match collected {
                Ok(turn) => match provider_task.await {
                    Ok(Ok(())) => Ok(turn),
                    Ok(Err(error)) => Err(error),
                    Err(error) => Err(ProviderError::message(format!(
                        "provider task failed: {error}"
                    ))),
                },
                Err(error) => {
                    provider_task.abort();
                    Err(error)
                }
            };
            match result {
                Err(error)
                    if recovery < 3
                        && crate::core::ai::deepseek::is_retryable_stream_error(&error) =>
                {
                    #[cfg(not(test))]
                    let delay = std::time::Duration::from_secs(10 * (1 << recovery));
                    #[cfg(test)]
                    let delay = std::time::Duration::from_millis(20 * (1 << recovery));
                    tracing::warn!(session_id = %request.session_id, recovery = recovery + 1, ?delay, %error, "recovering pending agent round");
                    let _ = tx
                        .send(StreamEvent::Status {
                            kind: format!("stream_retry:{}:3", recovery + 1),
                        })
                        .await;
                    let deadline = tokio::time::Instant::now() + delay;
                    while tokio::time::Instant::now() < deadline {
                        if cancelled.load(Ordering::Relaxed) || tx.is_closed() {
                            return Err(ProviderError::cancelled());
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
                result => return result,
            }
        }
        unreachable!("bounded recovery returns on its final attempt")
    }
}
