pub mod cancel;
pub mod logs;
pub mod reserve;
pub mod run;
pub mod status;
pub mod submit;

use ci_core::proto::orchestrator::orchestrator_client::OrchestratorClient;
use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;

/// Concrete interceptor that injects an optional Bearer token.
#[derive(Clone)]
pub struct AuthInterceptor {
    token: Option<String>,
}

impl tonic::service::Interceptor for AuthInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(ref t) = self.token {
            if let Ok(val) = format!("Bearer {}", t).parse::<MetadataValue<_>>() {
                req.metadata_mut().insert("authorization", val);
            }
        }
        Ok(req)
    }
}

pub type Client = OrchestratorClient<InterceptedService<Channel, AuthInterceptor>>;

pub async fn connect(controller: &str, auth_token: Option<&str>) -> anyhow::Result<Client> {
    let channel = Channel::from_shared(controller.to_string())?
        .connect()
        .await?;
    let interceptor = AuthInterceptor {
        token: auth_token.map(|t| t.to_string()),
    };
    Ok(OrchestratorClient::with_interceptor(channel, interceptor)
        .max_decoding_message_size(64 * 1024 * 1024)
        .max_encoding_message_size(64 * 1024 * 1024))
}
