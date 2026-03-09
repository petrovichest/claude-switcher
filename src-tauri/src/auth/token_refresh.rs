//! Claude OAuth token refresh helpers

use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use tokio::time::{sleep, Duration};

use super::{load_accounts, switch_to_account, update_account_claude_tokens};
use crate::types::{
    map_claude_organization_type_to_plan_type, AuthData, ClaudeOAuthProfileResponse,
    StoredAccount,
};

pub const CLAUDE_AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
pub const CLAUDE_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
pub const CLAUDE_OAUTH_PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
pub const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const CLAUDE_OAUTH_SCOPES: [&str; 3] = [
    "user:profile",
    "user:inference",
    "user:sessions:claude_code",
];

const EXPIRY_SKEW_MS: i64 = 300_000;

#[derive(Debug, serde::Deserialize)]
pub struct RefreshTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClaudeProfileMetadata {
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub account_uuid: Option<String>,
    pub organization_uuid: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub display_name: Option<String>,
    pub has_extra_usage_enabled: Option<bool>,
}

pub async fn fetch_claude_oauth_profile(access_token: &str) -> Result<ClaudeOAuthProfileResponse> {
    let client = reqwest::Client::new();
    let response = client
        .get(CLAUDE_OAUTH_PROFILE_URL)
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header(CONTENT_TYPE, "application/json")
        .send()
        .await
        .context("Failed to send Claude profile request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Claude profile request failed: {status} - {body}");
    }

    response
        .json::<ClaudeOAuthProfileResponse>()
        .await
        .context("Failed to parse Claude profile response")
}

pub fn extract_profile_metadata(profile: &ClaudeOAuthProfileResponse) -> ClaudeProfileMetadata {
    ClaudeProfileMetadata {
        email: profile.account.email.clone(),
        plan_type: map_claude_organization_type_to_plan_type(
            profile.organization.organization_type.as_deref(),
        ),
        account_uuid: profile.account.uuid.clone(),
        organization_uuid: profile.organization.uuid.clone(),
        rate_limit_tier: profile.organization.rate_limit_tier.clone(),
        display_name: profile.account.display_name.clone(),
        has_extra_usage_enabled: profile.organization.has_extra_usage_enabled,
    }
}

pub async fn ensure_claude_tokens_fresh(account: &StoredAccount) -> Result<StoredAccount> {
    match &account.auth_data {
        AuthData::ClaudeOAuth { expires_at_ms, .. } => {
            if token_expired_or_near_expiry(*expires_at_ms) {
                refresh_claude_tokens(account).await
            } else {
                Ok(account.clone())
            }
        }
    }
}

pub async fn refresh_claude_tokens(account: &StoredAccount) -> Result<StoredAccount> {
    let (current_refresh_token, current_scopes) = match &account.auth_data {
        AuthData::ClaudeOAuth {
            refresh_token,
            scopes,
            ..
        } => (refresh_token.clone(), scopes.clone()),
    };

    if current_refresh_token.trim().is_empty() {
        anyhow::bail!("Missing refresh token for account {}", account.name);
    }

    let refreshed = refresh_tokens_with_refresh_token(&current_refresh_token).await?;
    let next_refresh_token = refreshed
        .refresh_token
        .unwrap_or_else(|| current_refresh_token.clone());
    let next_scopes = parse_scopes(refreshed.scope.as_deref(), &current_scopes);
    let next_expires_at_ms = Utc::now().timestamp_millis() + refreshed.expires_in * 1000;

    let profile = fetch_claude_oauth_profile(&refreshed.access_token).await?;
    let metadata = extract_profile_metadata(&profile);

    let is_active = load_accounts()?.active_account_id.as_deref() == Some(account.id.as_str());

    let updated = update_account_claude_tokens(
        &account.id,
        refreshed.access_token,
        next_refresh_token,
        next_expires_at_ms,
        next_scopes,
        metadata.account_uuid,
        metadata.organization_uuid,
        metadata.rate_limit_tier,
        metadata.display_name,
        metadata.has_extra_usage_enabled,
        metadata.email,
        metadata.plan_type,
    )?;

    if is_active {
        if let Err(err) = switch_to_account(&updated) {
            println!("[Auth] Failed to sync active Claude credentials after token refresh: {err}");
        }
    }

    Ok(updated)
}

pub async fn create_claude_account_from_refresh_token(
    account_name: String,
    refresh_token: String,
) -> Result<StoredAccount> {
    if refresh_token.trim().is_empty() {
        anyhow::bail!("Missing refresh token for account {account_name}");
    }

    let refreshed = refresh_tokens_with_refresh_token(&refresh_token).await?;
    let profile = fetch_claude_oauth_profile(&refreshed.access_token).await?;
    let metadata = extract_profile_metadata(&profile);
    let scopes = parse_scopes(refreshed.scope.as_deref(), &[]);
    let expires_at_ms = Utc::now().timestamp_millis() + refreshed.expires_in * 1000;

    Ok(StoredAccount::new_claude(
        account_name,
        metadata.email,
        metadata.plan_type,
        refreshed.access_token,
        refreshed.refresh_token.unwrap_or(refresh_token),
        expires_at_ms,
        scopes,
        metadata.account_uuid,
        metadata.organization_uuid,
        metadata.rate_limit_tier,
        metadata.display_name,
        metadata.has_extra_usage_enabled,
    ))
}

fn token_expired_or_near_expiry(expires_at_ms: i64) -> bool {
    Utc::now().timestamp_millis() + EXPIRY_SKEW_MS >= expires_at_ms
}

fn parse_scopes(scope: Option<&str>, fallback: &[String]) -> Vec<String> {
    let parsed: Vec<String> = scope
        .unwrap_or_default()
        .split(' ')
        .filter(|item| !item.trim().is_empty())
        .map(String::from)
        .collect();

    if !parsed.is_empty() {
        parsed
    } else if !fallback.is_empty() {
        fallback.to_vec()
    } else {
        CLAUDE_OAUTH_SCOPES.iter().map(|value| String::from(*value)).collect()
    }
}

pub async fn refresh_tokens_with_refresh_token(refresh_token: &str) -> Result<RefreshTokenResponse> {
    let client = reqwest::Client::new();
    let body = json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLAUDE_CLIENT_ID,
        "scope": CLAUDE_OAUTH_SCOPES.join(" "),
    });

    let mut last_send_error = None;
    let mut response = None;

    for attempt in 1..=3u8 {
        match client
            .post(CLAUDE_TOKEN_URL)
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                response = Some(resp);
                break;
            }
            Err(err) => {
                last_send_error = Some(err);
                if attempt < 3 {
                    sleep(Duration::from_millis(250 * u64::from(attempt))).await;
                }
            }
        }
    }

    let response = match response {
        Some(resp) => resp,
        None => {
            let err = last_send_error.context("Failed to send Claude token refresh request")?;
            return Err(err.into());
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Claude token refresh failed: {status} - {body}");
    }

    response
        .json::<RefreshTokenResponse>()
        .await
        .context("Failed to parse Claude token refresh response")
}
