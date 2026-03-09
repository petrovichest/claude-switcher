//! Usage API client for Claude account metadata and limit hints.

use anyhow::{Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT},
    StatusCode,
};
use serde_json::Value;

use crate::auth::{
    ensure_claude_tokens_fresh, extract_profile_metadata, refresh_claude_tokens,
    CLAUDE_OAUTH_PROFILE_URL,
};
use crate::types::{AuthData, StoredAccount, UsageInfo};

const CLAUDE_USER_AGENT: &str = "claude-switcher-gpt/0.1.0";
const CLAUDE_CODE_USAGE_USER_AGENT: &str = "claude-code/2.1.71";
const CLAUDE_CLI_PROFILE_URL: &str = "https://api.anthropic.com/api/claude_cli_profile";
const CLAUDE_OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_OAUTH_BETA: &str = "oauth-2025-04-20";

pub async fn get_account_usage(account: &StoredAccount) -> Result<UsageInfo> {
    let fresh_account = ensure_claude_tokens_fresh(account).await?;
    let access_token = extract_access_token(&fresh_account)?;

    let response = send_profile_request(access_token).await?;
    if response.status() == StatusCode::UNAUTHORIZED {
        let refreshed_account = refresh_claude_tokens(&fresh_account).await?;
        let retry_token = extract_access_token(&refreshed_account)?;
        let retry_response = send_profile_request(retry_token).await?;
        return parse_usage_response(&refreshed_account, retry_response).await;
    }

    parse_usage_response(&fresh_account, response).await
}

pub async fn warmup_account(account: &StoredAccount) -> Result<()> {
    let fresh_account = ensure_claude_tokens_fresh(account).await?;
    let access_token = extract_access_token(&fresh_account)?;

    let mut response = send_profile_request(access_token).await?;
    if response.status() == StatusCode::UNAUTHORIZED {
        let refreshed_account = refresh_claude_tokens(&fresh_account).await?;
        let retry_token = extract_access_token(&refreshed_account)?;
        response = send_profile_request(retry_token).await?;
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Claude warm-up failed: {status} - {body}");
    }

    Ok(())
}

fn extract_access_token(account: &StoredAccount) -> Result<&str> {
    match &account.auth_data {
        AuthData::ClaudeOAuth { access_token, .. } => Ok(access_token.as_str()),
    }
}

fn extract_account_uuid(account: &StoredAccount) -> Option<&str> {
    match &account.auth_data {
        AuthData::ClaudeOAuth { account_uuid, .. } => account_uuid.as_deref(),
    }
}

fn extract_organization_uuid(account: &StoredAccount) -> Option<&str> {
    match &account.auth_data {
        AuthData::ClaudeOAuth {
            organization_uuid, ..
        } => organization_uuid.as_deref(),
    }
}

fn extract_has_extra_usage_enabled(account: &StoredAccount) -> Option<bool> {
    match &account.auth_data {
        AuthData::ClaudeOAuth {
            has_extra_usage_enabled,
            ..
        } => *has_extra_usage_enabled,
    }
}

fn build_headers(access_token: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(CLAUDE_USER_AGENT));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}"))
            .context("Invalid Claude access token")?,
    );
    Ok(headers)
}

fn build_cli_profile_headers(access_token: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(CLAUDE_USER_AGENT));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(access_token).context("Invalid Claude access token")?,
    );
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static(CLAUDE_OAUTH_BETA),
    );
    Ok(headers)
}

async fn send_profile_request(access_token: &str) -> Result<reqwest::Response> {
    let client = reqwest::Client::new();
    client
        .get(CLAUDE_OAUTH_PROFILE_URL)
        .headers(build_headers(access_token)?)
        .send()
        .await
        .context("Failed to send Claude profile request")
}

async fn send_cli_profile_request(
    access_token: &str,
    account_uuid: Option<&str>,
) -> Result<Option<reqwest::Response>> {
    let Some(account_uuid) = account_uuid else {
        return Ok(None);
    };

    let client = reqwest::Client::new();
    let response = client
        .get(CLAUDE_CLI_PROFILE_URL)
        .query(&[("account_uuid", account_uuid)])
        .headers(build_cli_profile_headers(access_token)?)
        .send()
        .await
        .context("Failed to send Claude CLI profile request")?;

    Ok(Some(response))
}

async fn send_oauth_usage_request(access_token: &str) -> Result<reqwest::Response> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(CLAUDE_CODE_USAGE_USER_AGENT),
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}"))
            .context("Invalid Claude access token")?,
    );
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static(CLAUDE_OAUTH_BETA),
    );

    let client = reqwest::Client::new();
    client
        .get(CLAUDE_OAUTH_USAGE_URL)
        .headers(headers)
        .send()
        .await
        .context("Failed to send Claude OAuth usage request")
}

async fn parse_usage_response(
    account: &StoredAccount,
    response: reqwest::Response,
) -> Result<UsageInfo> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Ok(UsageInfo::error(
            account.id.clone(),
            format!("API error: {status} - {body}"),
        ));
    }

    let profile = response
        .json::<crate::types::ClaudeOAuthProfileResponse>()
        .await
        .context("Failed to parse Claude profile response")?;
    let metadata = extract_profile_metadata(&profile);

    let access_token = extract_access_token(account)?;
    let oauth_usage = fetch_oauth_usage_metadata(access_token).await;
    let cli_profile = fetch_cli_profile_metadata(access_token, extract_account_uuid(account)).await;
    let has_live_quota = oauth_usage
        .as_ref()
        .is_some_and(OAuthUsageMetadata::has_live_quota);

    let usage_note = if has_live_quota {
        None
    } else if oauth_usage.is_some() {
        Some(String::from(
            "Claude OAuth is active, but the usage endpoint did not expose live remaining quota.",
        ))
    } else {
        Some(String::from(
            "Live remaining quota was not available from the Claude usage endpoint.",
        ))
    };

    Ok(UsageInfo {
        account_id: account.id.clone(),
        plan_type: metadata.plan_type.or_else(|| account.plan_type.clone()),
        rate_limit_tier: cli_profile
            .as_ref()
            .and_then(|value| value.rate_limit_tier.clone())
            .or(metadata.rate_limit_tier),
        email: metadata.email.or_else(|| account.email.clone()),
        display_name: cli_profile
            .as_ref()
            .and_then(|value| value.display_name.clone())
            .or(metadata.display_name),
        account_uuid: cli_profile
            .as_ref()
            .and_then(|value| value.account_uuid.clone())
            .or_else(|| extract_account_uuid(account).map(String::from)),
        organization_uuid: cli_profile
            .as_ref()
            .and_then(|value| value.organization_uuid.clone())
            .or_else(|| extract_organization_uuid(account).map(String::from)),
        organization_name: cli_profile
            .as_ref()
            .and_then(|value| value.organization_name.clone()),
        organization_role: cli_profile
            .as_ref()
            .and_then(|value| value.organization_role.clone()),
        workspace_role: cli_profile
            .as_ref()
            .and_then(|value| value.workspace_role.clone()),
        has_extra_usage_enabled: cli_profile
            .as_ref()
            .and_then(|value| value.has_extra_usage_enabled)
            .or_else(|| oauth_usage.as_ref().and_then(|value| value.has_extra_usage_enabled))
            .or_else(|| extract_has_extra_usage_enabled(account)),
        messages_remaining: oauth_usage
            .as_ref()
            .and_then(|value| value.messages_remaining)
            .or_else(|| cli_profile.as_ref().and_then(|value| value.messages_remaining)),
        messages_limit: oauth_usage
            .as_ref()
            .and_then(|value| value.messages_limit)
            .or_else(|| cli_profile.as_ref().and_then(|value| value.messages_limit)),
        messages_reset_at: oauth_usage
            .as_ref()
            .and_then(|value| value.messages_reset_at.clone())
            .or_else(|| cli_profile.as_ref().and_then(|value| value.messages_reset_at.clone())),
        tokens_remaining: oauth_usage
            .as_ref()
            .and_then(|value| value.tokens_remaining)
            .or_else(|| cli_profile.as_ref().and_then(|value| value.tokens_remaining)),
        tokens_limit: oauth_usage
            .as_ref()
            .and_then(|value| value.tokens_limit)
            .or_else(|| cli_profile.as_ref().and_then(|value| value.tokens_limit)),
        session_percent_used: oauth_usage.as_ref().and_then(|value| value.session_percent_used),
        session_percent_remaining: oauth_usage
            .as_ref()
            .and_then(|value| value.session_percent_remaining),
        session_reset_at_label: oauth_usage
            .as_ref()
            .and_then(|value| value.session_reset_at_label.clone()),
        week_percent_used: oauth_usage.as_ref().and_then(|value| value.week_percent_used),
        week_percent_remaining: oauth_usage
            .as_ref()
            .and_then(|value| value.week_percent_remaining),
        week_reset_at_label: oauth_usage
            .as_ref()
            .and_then(|value| value.week_reset_at_label.clone()),
        usage_source: if oauth_usage.is_some() {
            Some(String::from("claude_oauth_usage"))
        } else if cli_profile.is_some() {
            Some(String::from("claude_cli_profile"))
        } else {
            Some(String::from("oauth_profile"))
        },
        usage_note,
        error: None,
    })
}

#[derive(Debug, Clone, Default)]
struct CliProfileMetadata {
    account_uuid: Option<String>,
    organization_uuid: Option<String>,
    organization_name: Option<String>,
    organization_role: Option<String>,
    workspace_role: Option<String>,
    rate_limit_tier: Option<String>,
    display_name: Option<String>,
    has_extra_usage_enabled: Option<bool>,
    messages_remaining: Option<i64>,
    messages_limit: Option<i64>,
    messages_reset_at: Option<String>,
    tokens_remaining: Option<i64>,
    tokens_limit: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct OAuthUsageMetadata {
    has_extra_usage_enabled: Option<bool>,
    messages_remaining: Option<i64>,
    messages_limit: Option<i64>,
    messages_reset_at: Option<String>,
    tokens_remaining: Option<i64>,
    tokens_limit: Option<i64>,
    session_percent_used: Option<i64>,
    session_percent_remaining: Option<i64>,
    session_reset_at_label: Option<String>,
    week_percent_used: Option<i64>,
    week_percent_remaining: Option<i64>,
    week_reset_at_label: Option<String>,
}

impl OAuthUsageMetadata {
    fn has_live_quota(&self) -> bool {
        self.messages_remaining.is_some()
            || self.tokens_remaining.is_some()
            || self.session_percent_used.is_some()
            || self.week_percent_used.is_some()
    }
}

async fn fetch_cli_profile_metadata(
    access_token: &str,
    account_uuid: Option<&str>,
) -> Option<CliProfileMetadata> {
    let response = match send_cli_profile_request(access_token, account_uuid).await {
        Ok(Some(response)) => response,
        Ok(None) | Err(_) => return None,
    };

    if !response.status().is_success() {
        return None;
    }

    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(_) => return None,
    };

    Some(CliProfileMetadata {
        account_uuid: find_string_by_keys(&value, &["account_uuid", "accountUuid", "uuid"]),
        organization_uuid: find_string_by_keys(
            &value,
            &["organization_uuid", "organizationUuid", "org_uuid"],
        ),
        organization_name: find_string_by_keys(
            &value,
            &["organization_name", "organizationName", "org_name"],
        ),
        organization_role: find_string_by_keys(
            &value,
            &["organization_role", "organizationRole"],
        ),
        workspace_role: find_string_by_keys(&value, &["workspace_role", "workspaceRole"]),
        rate_limit_tier: find_string_by_keys(&value, &["rate_limit_tier", "rateLimitTier"]),
        display_name: find_string_by_keys(&value, &["display_name", "displayName"]),
        has_extra_usage_enabled: find_bool_by_keys(
            &value,
            &["has_extra_usage_enabled", "hasExtraUsageEnabled"],
        ),
        messages_remaining: find_i64_by_keys(
            &value,
            &[
                "messages_remaining",
                "message_remaining",
                "requests_remaining",
                "remaining_messages",
                "remaining_requests",
            ],
        ),
        messages_limit: find_i64_by_keys(
            &value,
            &[
                "messages_limit",
                "message_limit",
                "requests_limit",
                "messageLimit",
                "requestLimit",
            ],
        ),
        messages_reset_at: find_string_by_keys(
            &value,
            &[
                "messages_reset_at",
                "reset_at",
                "next_reset_at",
                "reset_date",
                "resets_at",
            ],
        ),
        tokens_remaining: find_i64_by_keys(
            &value,
            &["tokens_remaining", "token_remaining", "remaining_tokens"],
        ),
        tokens_limit: find_i64_by_keys(&value, &["tokens_limit", "token_limit"]),
    })
}

async fn fetch_oauth_usage_metadata(access_token: &str) -> Option<OAuthUsageMetadata> {
    let response = match send_oauth_usage_request(access_token).await {
        Ok(response) => response,
        Err(_) => return None,
    };

    if !response.status().is_success() {
        return None;
    }

    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(_) => return None,
    };

    let session_bucket =
        find_json_by_keys(&value, &["five_hour", "current_session", "currentSession"]);
    let week_bucket = find_json_by_keys(&value, &["seven_day", "current_week", "currentWeek"]);
    let extra_usage_bucket = find_json_by_keys(&value, &["extra_usage", "extraUsage"]);

    Some(OAuthUsageMetadata {
        has_extra_usage_enabled: extra_usage_bucket
            .as_ref()
            .and_then(|bucket| find_bool_by_keys(bucket, &["is_enabled", "enabled"])),
        messages_remaining: find_i64_by_keys(
            &value,
            &[
                "messages_remaining",
                "message_remaining",
                "remaining_messages",
                "requests_remaining",
            ],
        ),
        messages_limit: find_i64_by_keys(
            &value,
            &[
                "messages_limit",
                "message_limit",
                "requests_limit",
                "request_limit",
            ],
        ),
        messages_reset_at: find_string_by_keys(
            &value,
            &["messages_reset_at", "reset_at", "next_reset_at", "resets_at"],
        )
        .or_else(|| {
            session_bucket
                .as_ref()
                .and_then(|bucket| find_string_by_keys(bucket, &["resets_at", "reset_at"]))
        }),
        tokens_remaining: find_i64_by_keys(
            &value,
            &["tokens_remaining", "token_remaining", "remaining_tokens"],
        ),
        tokens_limit: find_i64_by_keys(&value, &["tokens_limit", "token_limit"]),
        session_percent_used: session_bucket.as_ref().and_then(|bucket| {
            find_percent_by_keys(
                bucket,
                &[
                    "utilization",
                    "used_percent",
                    "percent_used",
                    "usage_percent",
                    "usedPercentage",
                    "percentageUsed",
                    "used",
                ],
            )
        }),
        session_percent_remaining: session_bucket
            .as_ref()
            .and_then(|bucket| {
                find_percent_by_keys(
                    bucket,
                    &[
                        "remaining_percent",
                        "percent_remaining",
                        "remainingPercentage",
                        "percentageRemaining",
                        "remaining",
                    ],
                )
            })
            .or_else(|| {
                session_bucket.as_ref().and_then(|bucket| {
                    find_percent_by_keys(
                        bucket,
                        &[
                            "utilization",
                            "used_percent",
                            "percent_used",
                            "usage_percent",
                            "usedPercentage",
                            "percentageUsed",
                            "used",
                        ],
                    )
                    .map(|used| 100 - used)
                })
            }),
        session_reset_at_label: session_bucket
            .as_ref()
            .and_then(|bucket| {
                find_string_by_keys(
                    bucket,
                    &[
                        "reset_at",
                        "reset_label",
                        "reset_time",
                        "resets_at",
                        "resetAt",
                        "resetLabel",
                    ],
                )
            }),
        week_percent_used: week_bucket.as_ref().and_then(|bucket| {
            find_percent_by_keys(
                bucket,
                &[
                    "utilization",
                    "used_percent",
                    "percent_used",
                    "usage_percent",
                    "usedPercentage",
                    "percentageUsed",
                    "used",
                ],
            )
        }),
        week_percent_remaining: week_bucket
            .as_ref()
            .and_then(|bucket| {
                find_percent_by_keys(
                    bucket,
                    &[
                        "remaining_percent",
                        "percent_remaining",
                        "remainingPercentage",
                        "percentageRemaining",
                        "remaining",
                    ],
                )
            })
            .or_else(|| {
                week_bucket.as_ref().and_then(|bucket| {
                    find_percent_by_keys(
                        bucket,
                        &[
                            "utilization",
                            "used_percent",
                            "percent_used",
                            "usage_percent",
                            "usedPercentage",
                            "percentageUsed",
                            "used",
                        ],
                    )
                    .map(|used| 100 - used)
                })
            }),
        week_reset_at_label: week_bucket
            .as_ref()
            .and_then(|bucket| {
                find_string_by_keys(
                    bucket,
                    &[
                        "reset_at",
                        "reset_label",
                        "reset_time",
                        "resets_at",
                        "resetAt",
                        "resetLabel",
                    ],
                )
            }),
    })
}

fn find_string_by_keys(value: &Value, keys: &[&str]) -> Option<String> {
    visit_json(value, &mut |key, nested| {
        if keys.iter().any(|candidate| candidate.eq_ignore_ascii_case(key)) {
            nested.as_str().map(String::from)
        } else {
            None
        }
    })
}

fn find_json_by_keys(value: &Value, keys: &[&str]) -> Option<Value> {
    visit_json(value, &mut |key, nested| {
        if keys.iter().any(|candidate| candidate.eq_ignore_ascii_case(key)) {
            Some(nested.clone())
        } else {
            None
        }
    })
}

fn find_i64_by_keys(value: &Value, keys: &[&str]) -> Option<i64> {
    visit_json(value, &mut |key, nested| {
        if !keys.iter().any(|candidate| candidate.eq_ignore_ascii_case(key)) {
            return None;
        }

        nested
            .as_i64()
            .or_else(|| nested.as_u64().and_then(|number| i64::try_from(number).ok()))
            .or_else(|| nested.as_str().and_then(|text| text.parse::<i64>().ok()))
    })
}

fn find_percent_by_keys(value: &Value, keys: &[&str]) -> Option<i64> {
    visit_json(value, &mut |key, nested| {
        if !keys.iter().any(|candidate| candidate.eq_ignore_ascii_case(key)) {
            return None;
        }

        nested
            .as_i64()
            .map(normalize_percent)
            .or_else(|| nested.as_u64().and_then(|number| i64::try_from(number).ok()).map(normalize_percent))
            .or_else(|| nested.as_f64().map(normalize_percent_float))
            .or_else(|| nested.as_str().and_then(parse_percent_text))
    })
}

fn find_bool_by_keys(value: &Value, keys: &[&str]) -> Option<bool> {
    visit_json(value, &mut |key, nested| {
        if !keys.iter().any(|candidate| candidate.eq_ignore_ascii_case(key)) {
            return None;
        }

        nested.as_bool().or_else(|| match nested.as_str() {
            Some("true") => Some(true),
            Some("false") => Some(false),
            _ => None,
        })
    })
}

fn visit_json<T, F>(value: &Value, visitor: &mut F) -> Option<T>
where
    F: FnMut(&str, &Value) -> Option<T>,
{
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                if let Some(found) = visitor(key, nested) {
                    return Some(found);
                }

                if let Some(found) = visit_json(nested, visitor) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => {
            for item in items {
                if let Some(found) = visit_json(item, visitor) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn normalize_percent(value: i64) -> i64 {
    if value <= 1 { value * 100 } else { value.clamp(0, 100) }
}

fn normalize_percent_float(value: f64) -> i64 {
    let normalized = if value <= 1.0 { value * 100.0 } else { value };
    normalized.round().clamp(0.0, 100.0) as i64
}

fn parse_percent_text(value: &str) -> Option<i64> {
    let trimmed = value.trim().trim_end_matches('%');
    trimmed
        .parse::<f64>()
        .ok()
        .map(normalize_percent_float)
}

pub async fn refresh_all_usage(accounts: &[StoredAccount]) -> Vec<UsageInfo> {
    let mut results = Vec::with_capacity(accounts.len());
    for account in accounts {
        match get_account_usage(account).await {
            Ok(info) => results.push(info),
            Err(err) => results.push(UsageInfo::error(account.id.clone(), err.to_string())),
        }
    }
    results
}
