//! Local OAuth server for handling Claude login flow

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use rand::RngCore;
use reqwest::header::CONTENT_TYPE;
use serde_json::json;
use sha2::{Digest, Sha256};
use tiny_http::{Header, Request, Response, Server};
use tokio::sync::oneshot;

use crate::types::{OAuthLoginInfo, StoredAccount};

use super::{
    extract_profile_metadata, fetch_claude_oauth_profile, CLAUDE_AUTHORIZE_URL, CLAUDE_CLIENT_ID,
    CLAUDE_OAUTH_SCOPES, CLAUDE_TOKEN_URL,
};

const DEFAULT_PORT: u16 = 1455;

#[derive(Debug, Clone)]
pub struct PkceCodes {
    pub code_verifier: String,
    pub code_challenge: String,
}

pub fn generate_pkce() -> PkceCodes {
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);

    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

    PkceCodes {
        code_verifier,
        code_challenge,
    }
}

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn build_authorize_url(redirect_uri: &str, pkce: &PkceCodes, state: &str) -> String {
    let scopes = CLAUDE_OAUTH_SCOPES.join(" ");
    let params = [
        ("code", "true"),
        ("client_id", CLAUDE_CLIENT_ID),
        ("response_type", "code"),
        ("redirect_uri", redirect_uri),
        ("scope", scopes.as_str()),
        ("code_challenge", &pkce.code_challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
    ];

    let query_string = params
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
        .collect::<Vec<_>>()
        .join("&");

    format!("{CLAUDE_AUTHORIZE_URL}?{query_string}")
}

#[derive(Debug, Clone, serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    scope: Option<String>,
}

async fn exchange_code_for_tokens(
    redirect_uri: &str,
    pkce: &PkceCodes,
    code: &str,
    state: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    let body = json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": redirect_uri,
        "client_id": CLAUDE_CLIENT_ID,
        "code_verifier": pkce.code_verifier,
        "state": state,
    });

    let response = client
        .post(CLAUDE_TOKEN_URL)
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .context("Failed to send Claude token exchange request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Claude token exchange failed: {status} - {body}");
    }

    response
        .json::<TokenResponse>()
        .await
        .context("Failed to parse Claude token exchange response")
}

pub struct OAuthLoginResult {
    pub account: StoredAccount,
}

pub async fn start_oauth_login(
    account_name: String,
) -> Result<(
    OAuthLoginInfo,
    oneshot::Receiver<Result<OAuthLoginResult>>,
    Arc<AtomicBool>,
)> {
    let pkce = generate_pkce();
    let state = generate_state();

    let server = match Server::http(format!("127.0.0.1:{DEFAULT_PORT}")) {
        Ok(server) => server,
        Err(default_err) => {
            println!(
                "[OAuth] Default callback port {DEFAULT_PORT} unavailable ({default_err}), using a random local port"
            );
            Server::http("127.0.0.1:0").map_err(|fallback_err| {
                anyhow::anyhow!(
                    "Failed to start OAuth server: default port {DEFAULT_PORT} error: {default_err}; fallback error: {fallback_err}"
                )
            })?
        }
    };

    let actual_port = match server.server_addr().to_ip() {
        Some(addr) => addr.port(),
        None => anyhow::bail!("Failed to determine callback server port"),
    };

    let redirect_uri = format!("http://localhost:{actual_port}/callback");
    let auth_url = build_authorize_url(&redirect_uri, &pkce, &state);

    let login_info = OAuthLoginInfo {
        auth_url,
        callback_port: actual_port,
    };

    let (tx, rx) = oneshot::channel();
    let cancelled = Arc::new(AtomicBool::new(false));

    let server = Arc::new(server);
    let cancelled_clone = cancelled.clone();

    thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime.block_on(run_oauth_server(
            server,
            pkce,
            state,
            redirect_uri,
            account_name,
            cancelled_clone,
        ));
        let _ = tx.send(result);
    });

    Ok((login_info, rx, cancelled))
}

async fn run_oauth_server(
    server: Arc<Server>,
    pkce: PkceCodes,
    expected_state: String,
    redirect_uri: String,
    account_name: String,
    cancelled: Arc<AtomicBool>,
) -> Result<OAuthLoginResult> {
    let timeout = Duration::from_secs(300);
    let start = std::time::Instant::now();

    loop {
        if cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("OAuth login cancelled");
        }

        if start.elapsed() > timeout {
            anyhow::bail!("OAuth login timed out");
        }

        let request = match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(req)) => req,
            Ok(None) => continue,
            Err(_) => continue,
        };

        let result = handle_oauth_request(
            request,
            &pkce,
            &expected_state,
            &redirect_uri,
            &account_name,
        )
        .await;

        match result {
            HandleResult::Continue => continue,
            HandleResult::Success(account) => {
                server.unblock();
                return Ok(OAuthLoginResult { account });
            }
            HandleResult::Error(err) => {
                server.unblock();
                return Err(err);
            }
        }
    }
}

enum HandleResult {
    Continue,
    Success(StoredAccount),
    Error(anyhow::Error),
}

async fn handle_oauth_request(
    request: Request,
    pkce: &PkceCodes,
    expected_state: &str,
    redirect_uri: &str,
    account_name: &str,
) -> HandleResult {
    let url_str = request.url().to_string();
    let parsed = match url::Url::parse(&format!("http://localhost{url_str}")) {
        Ok(value) => value,
        Err(_) => {
            let _ = request.respond(Response::from_string("Bad Request").with_status_code(400));
            return HandleResult::Continue;
        }
    };

    if parsed.path() != "/callback" {
        let _ = request.respond(Response::from_string("Not Found").with_status_code(404));
        return HandleResult::Continue;
    }

    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();

    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or("Unknown error");
        let _ = request.respond(
            Response::from_string(format!("OAuth Error: {error} - {description}"))
                .with_status_code(400),
        );
        return HandleResult::Error(anyhow::anyhow!(
            "OAuth error: {error} - {description}"
        ));
    }

    if params.get("state").map(String::as_str) != Some(expected_state) {
        let _ = request.respond(Response::from_string("State mismatch").with_status_code(400));
        return HandleResult::Error(anyhow::anyhow!("OAuth state mismatch"));
    }

    let code = match params.get("code") {
        Some(value) if !value.is_empty() => value.clone(),
        _ => {
            let _ = request.respond(
                Response::from_string("Missing authorization code").with_status_code(400),
            );
            return HandleResult::Error(anyhow::anyhow!("Missing authorization code"));
        }
    };

    match exchange_code_for_tokens(redirect_uri, pkce, &code, expected_state).await {
        Ok(tokens) => {
            let profile = match fetch_claude_oauth_profile(&tokens.access_token).await {
                Ok(profile) => profile,
                Err(err) => {
                    let _ = request.respond(
                        Response::from_string(format!("Profile fetch failed: {err}"))
                            .with_status_code(500),
                    );
                    return HandleResult::Error(err);
                }
            };
            let metadata = extract_profile_metadata(&profile);
            let scopes = tokens
                .scope
                .as_deref()
                .unwrap_or_default()
                .split(' ')
                .filter(|value| !value.trim().is_empty())
                .map(String::from)
                .collect::<Vec<_>>();

            let account = StoredAccount::new_claude(
                account_name.to_string(),
                metadata.email,
                metadata.plan_type,
                tokens.access_token,
                tokens.refresh_token,
                chrono::Utc::now().timestamp_millis() + tokens.expires_in * 1000,
                if scopes.is_empty() {
                    CLAUDE_OAUTH_SCOPES
                        .iter()
                        .map(|value| String::from(*value))
                        .collect()
                } else {
                    scopes
                },
                metadata.account_uuid,
                metadata.organization_uuid,
                metadata.rate_limit_tier,
                metadata.display_name,
                metadata.has_extra_usage_enabled,
            );

            let success_html = r#"<!DOCTYPE html>
<html>
<head>
  <title>Claude Login Successful</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: linear-gradient(135deg, #f0ede4 0%, #d8c7a1 100%); }
    .container { text-align: center; background: white; padding: 40px 60px; border-radius: 18px; box-shadow: 0 18px 50px rgba(42, 34, 23, 0.18); }
    h1 { color: #2e261c; margin-bottom: 10px; }
    p { color: #665847; }
    .checkmark { font-size: 48px; margin-bottom: 20px; color: #8f6b3b; }
  </style>
</head>
<body>
  <div class="container">
    <div class="checkmark">✓</div>
    <h1>Login Successful</h1>
    <p>You can close this window and return to Claude Switcher.</p>
  </div>
</body>
</html>"#;

            let response = Response::from_string(success_html).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                    .unwrap(),
            );
            let _ = request.respond(response);

            HandleResult::Success(account)
        }
        Err(err) => {
            let _ = request.respond(
                Response::from_string(format!("Token exchange failed: {err}"))
                    .with_status_code(500),
            );
            HandleResult::Error(err)
        }
    }
}

pub async fn wait_for_oauth_login(
    rx: oneshot::Receiver<Result<OAuthLoginResult>>,
) -> Result<StoredAccount> {
    let result = rx.await.context("OAuth login was cancelled")??;
    Ok(result.account)
}
