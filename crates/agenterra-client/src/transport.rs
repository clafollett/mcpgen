//! Transport abstraction layer for MCP communication

use crate::error::Result;
use async_trait::async_trait;

/// Abstraction over different MCP transport mechanisms
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a message and receive a response
    async fn send(&mut self, message: serde_json::Value) -> Result<serde_json::Value>;

    /// Close the transport gracefully
    async fn close(&mut self) -> Result<()>;
}

/// Mock transport for testing
#[cfg(test)]
pub struct MockTransport {
    responses: Vec<serde_json::Value>,
    call_count: usize,
}

#[cfg(test)]
impl MockTransport {
    pub fn new(responses: Vec<serde_json::Value>) -> Self {
        Self {
            responses,
            call_count: 0,
        }
    }
}

#[cfg(test)]
#[async_trait]
impl Transport for MockTransport {
    async fn send(&mut self, _message: serde_json::Value) -> Result<serde_json::Value> {
        use crate::error::ClientError;

        if self.call_count >= self.responses.len() {
            return Err(ClientError::Transport("No more mock responses".to_string()));
        }

        let response = self.responses[self.call_count].clone();
        self.call_count += 1;
        Ok(response)
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}
