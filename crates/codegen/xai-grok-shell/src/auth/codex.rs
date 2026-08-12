//! OpenAI Codex/ChatGPT credentials for the native Responses provider.
//!
//! ZeroClaw owns the OAuth protocol and encrypted profile storage.  DX only
//! adapts that supported credential source to the shell's request builder;
//! xAI credentials never participate in this path.

use anyhow::{Context, Result};
use std::path::PathBuf;

pub const CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
pub const CODEX_PROFILE: &str = "default";

#[derive(Debug, Clone)]
pub struct CodexCredentials {
    pub access_token: String,
    pub account_id: Option<String>,
}

fn state_dir() -> PathBuf {
    // The provider-connect TUI and the Agent runtime share this profile root.
    // Keeping one path prevents a successful login in the TUI from appearing
    // missing to the session sampler (and avoids creating a second credential
    // store under ~/.zeroclaw).
    crate::util::grok_home::grok_home().join("agent")
}

fn auth_service() -> zeroclaw_providers::auth::AuthService {
    // ZeroClaw's profile store encrypts secrets at rest.  This is deliberately
    // independent from ~/.grok/auth.json and from the xAI AuthManager.
    zeroclaw_providers::auth::AuthService::new(&state_dir(), true)
}

pub fn is_codex_endpoint(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    parsed.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("chatgpt.com") || host.eq_ignore_ascii_case("chat.openai.com")
    }) && parsed
        .path()
        .trim_end_matches('/')
        .ends_with("/codex/responses")
}

/// Resolve and proactively refresh the active Codex OAuth profile.
pub async fn resolve_credentials() -> Result<Option<CodexCredentials>> {
    let auth = auth_service();
    let Some(profile) = auth
        .get_profile("openai-codex", Some(CODEX_PROFILE))
        .await
        .context("failed to read the OpenAI Codex auth profile")?
    else {
        return Ok(None);
    };

    let access_token = auth
        .get_valid_openai_access_token(Some(CODEX_PROFILE))
        .await
        .context("failed to refresh the OpenAI Codex OAuth token")?
        .or_else(|| profile.token_set.map(|tokens| tokens.access_token));

    Ok(access_token
        .filter(|token| !token.trim().is_empty())
        .map(|access_token| CodexCredentials {
            access_token,
            account_id: profile.account_id,
        }))
}

/// Run the supported ChatGPT/Codex browser or device-code login.
pub async fn run_login(device_code: bool) -> Result<()> {
    use zeroclaw_providers::auth::openai_oauth;

    let client = reqwest::Client::new();
    let auth = auth_service();

    let token_set = if device_code {
        let device = openai_oauth::start_device_code_flow(&client)
            .await
            .context("could not start the OpenAI device-code flow")?;
        println!("Open {}", device.verification_uri);
        println!("Enter code: {}", device.user_code);
        openai_oauth::poll_device_code_tokens(&client, &device)
            .await
            .context("OpenAI device-code login failed")?
    } else {
        let pkce = openai_oauth::generate_pkce_state();
        let url = openai_oauth::build_authorize_url(&pkce);
        println!("Opening ChatGPT sign-in in your browser…");
        if webbrowser::open(&url).is_err() {
            println!("Open this URL manually:\n{url}");
        }
        let code =
            openai_oauth::receive_loopback_code(&pkce.state, std::time::Duration::from_secs(180))
                .await
                .context("timed out waiting for the ChatGPT OAuth callback")?;
        openai_oauth::exchange_code_for_tokens(&client, &code, &pkce)
            .await
            .context("ChatGPT OAuth code exchange failed")?
    };

    let account_id = openai_oauth::extract_account_id_from_jwt(&token_set.access_token);
    auth.store_openai_tokens(CODEX_PROFILE, token_set, account_id, true)
        .await
        .context("failed to save the encrypted ChatGPT/Codex profile")?;
    println!("ChatGPT/Codex account connected for DX.");
    Ok(())
}

pub async fn run_logout() -> Result<()> {
    auth_service()
        .remove_profile("openai-codex", CODEX_PROFILE)
        .await
        .context("failed to remove the ChatGPT/Codex profile")?;
    println!("ChatGPT/Codex account disconnected from DX.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_codex_endpoint;

    #[test]
    fn recognizes_only_codex_responses_hosts() {
        assert!(is_codex_endpoint(
            "https://chatgpt.com/backend-api/codex/responses"
        ));
        assert!(!is_codex_endpoint("https://api.openai.com/v1/responses"));
        assert!(!is_codex_endpoint("https://opencode.ai/zen/v1/responses"));
    }
}
