mod apns;
mod fcm;

use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("push delivery failed: {0}")]
    Delivery(String),
    #[error("invalid push token")]
    InvalidToken,
    #[error("configuration error: {0}")]
    Config(String),
}

#[async_trait]
pub trait PushProvider: Send + Sync + 'static {
    async fn send_silent_push(&self, token: &str, platform: &str) -> Result<(), PushError>;
}

pub struct NoopPushProvider;

#[async_trait]
impl PushProvider for NoopPushProvider {
    async fn send_silent_push(&self, _token: &str, _platform: &str) -> Result<(), PushError> {
        tracing::debug!("noop push provider: silent push not sent");
        Ok(())
    }
}

pub use apns::ApnsProvider;
pub use fcm::FcmProvider;

#[derive(Debug, Clone)]
pub struct ApnsConfig {
    pub key_pem: String,
    pub key_id: String,
    pub team_id: String,
    pub topic: String,
    pub sandbox: bool,
}

#[derive(Debug, Clone)]
pub struct FcmConfig {
    pub project_id: String,
    pub service_account_json: String,
}

pub fn create_provider(
    apns: Option<ApnsConfig>,
    fcm: Option<FcmConfig>,
) -> Result<Box<dyn PushProvider>, PushError> {
    if let Some(cfg) = apns {
        return Ok(Box::new(ApnsProvider::new(cfg)?));
    }
    if let Some(cfg) = fcm {
        return Ok(Box::new(FcmProvider::new(cfg)?));
    }
    Ok(Box::new(NoopPushProvider))
}
