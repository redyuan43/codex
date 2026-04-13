use codex_core::config::Config;
use codex_model_provider_info::LLAMACPP_OSS_PROVIDER_ID;
use codex_model_provider_info::ModelProviderInfo;
use serde_json::Value as JsonValue;
use std::io;

const LLAMACPP_CONNECTION_ERROR: &str = "No running llama.cpp server detected. Start `llama-server` and expose an OpenAI-compatible `/v1` endpoint.";

#[derive(Clone)]
pub struct LlamaCppClient {
    client: reqwest::Client,
    base_url: String,
}

impl LlamaCppClient {
    pub async fn try_from_oss_provider(config: &Config) -> io::Result<Self> {
        let provider = config
            .model_providers
            .get(LLAMACPP_OSS_PROVIDER_ID)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Built-in provider {LLAMACPP_OSS_PROVIDER_ID} not found"),
                )
            })?;
        Self::try_from_provider(provider).await
    }

    pub(crate) async fn try_from_provider(provider: &ModelProviderInfo) -> io::Result<Self> {
        #![expect(clippy::expect_used)]
        let base_url = provider
            .base_url
            .as_ref()
            .expect("oss provider must have a base_url");
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let client = Self {
            client,
            base_url: base_url.to_string(),
        };
        client.check_server().await?;
        Ok(client)
    }

    async fn check_server(&self) -> io::Result<()> {
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let response = self.client.get(&url).send().await.map_err(|err| {
            tracing::warn!("Failed to connect to llama.cpp server: {err:?}");
            io::Error::other(LLAMACPP_CONNECTION_ERROR)
        })?;

        if response.status().is_success() {
            return Ok(());
        }

        tracing::warn!(
            "Failed to probe llama.cpp server at {}: HTTP {}",
            self.base_url,
            response.status()
        );
        Err(io::Error::other(LLAMACPP_CONNECTION_ERROR))
    }

    pub async fn fetch_models(&self) -> io::Result<Vec<String>> {
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(io::Error::other)?;
        if !response.status().is_success() {
            return Err(io::Error::other(format!(
                "Failed to fetch models: {}",
                response.status()
            )));
        }

        let value = response
            .json::<JsonValue>()
            .await
            .map_err(io::Error::other)?;
        let models = value
            .get("data")
            .and_then(|data| data.as_array())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "No 'data' array in response")
            })?
            .iter()
            .filter_map(|model| model.get("id").and_then(|id| id.as_str()))
            .map(std::string::ToString::to_string)
            .collect();
        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use wiremock::MockServer;

    #[tokio::test]
    async fn fetch_models_reads_model_ids() {
        let server = MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_raw(
                    serde_json::json!({
                        "data": [
                            {"id": "minimax-m2.7@q3_k_s"},
                            {"id": "other-model"},
                        ]
                    })
                    .to_string(),
                    "application/json",
                ),
            )
            .mount(&server)
            .await;

        let provider = ModelProviderInfo {
            name: "llama.cpp".to_string(),
            base_url: Some(format!("{}/v1", server.uri())),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: codex_model_provider_info::WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            websocket_connect_timeout_ms: None,
            requires_openai_auth: false,
            supports_websockets: false,
        };

        let client = LlamaCppClient::try_from_provider(&provider)
            .await
            .expect("llama.cpp should be reachable");
        let models = client.fetch_models().await.expect("fetch models");
        assert_eq!(
            models,
            vec!["minimax-m2.7@q3_k_s".to_string(), "other-model".to_string()]
        );
    }
}
