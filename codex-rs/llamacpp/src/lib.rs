mod client;

pub use client::LlamaCppClient;
use codex_core::config::Config;

/// Prepare the local llama.cpp environment when `--oss` is selected.
///
/// - Ensures a local `llama-server` is reachable.
/// - Verifies the requested model alias exists in `/v1/models`.
pub async fn ensure_oss_ready(config: &Config) -> std::io::Result<()> {
    let model = config.model.as_ref().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "llama.cpp requires an explicit model. Pass `-m <model>` or set `model` in config.toml.",
        )
    })?;

    let client = LlamaCppClient::try_from_oss_provider(config).await?;
    let models = client.fetch_models().await?;
    if models.iter().any(|candidate| candidate == model) {
        return Ok(());
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "Model '{model}' is not available from llama.cpp. Check `/v1/models` and ensure your `llama-server` uses the expected `--alias`."
        ),
    ))
}
