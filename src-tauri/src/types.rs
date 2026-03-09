//! Core types for Claude Switcher

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

fn default_app_settings_version() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportSecurityMode {
    LessSecure,
    Passphrase,
    Keychain,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ScheduledWarmupSettings {
    pub enabled: bool,
    pub local_time: String,
    pub account_ids: Vec<String>,
    pub last_run_local_date: Option<String>,
    pub last_missed_prompt_local_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    #[serde(default = "default_app_settings_version")]
    pub version: u32,
    #[serde(default)]
    pub export_security_mode: Option<ExportSecurityMode>,
    #[serde(default)]
    pub scheduled_warmup: Option<ScheduledWarmupSettings>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: 1,
            export_security_mode: None,
            scheduled_warmup: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountsStore {
    pub version: u32,
    pub accounts: Vec<StoredAccount>,
    pub active_account_id: Option<String>,
}

impl Default for AccountsStore {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: Vec::new(),
            active_account_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub auth_mode: AuthMode,
    pub auth_data: AuthData,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl StoredAccount {
    #[allow(clippy::too_many_arguments)]
    pub fn new_claude(
        name: String,
        email: Option<String>,
        plan_type: Option<String>,
        access_token: String,
        refresh_token: String,
        expires_at_ms: i64,
        scopes: Vec<String>,
        account_uuid: Option<String>,
        organization_uuid: Option<String>,
        rate_limit_tier: Option<String>,
        display_name: Option<String>,
        has_extra_usage_enabled: Option<bool>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            email,
            plan_type,
            auth_mode: AuthMode::ClaudeOAuth,
            auth_data: AuthData::ClaudeOAuth {
                access_token,
                refresh_token,
                expires_at_ms,
                scopes,
                account_uuid,
                organization_uuid,
                rate_limit_tier,
                display_name,
                has_extra_usage_enabled,
            },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    ClaudeOAuth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthData {
    ClaudeOAuth {
        access_token: String,
        refresh_token: String,
        expires_at_ms: i64,
        scopes: Vec<String>,
        account_uuid: Option<String>,
        organization_uuid: Option<String>,
        rate_limit_tier: Option<String>,
        display_name: Option<String>,
        has_extra_usage_enabled: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CredentialsDotJson {
    #[serde(rename = "claudeAiOauth", skip_serializing_if = "Option::is_none")]
    pub claude_ai_oauth: Option<ClaudeAiOauthData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAiOauthData {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
    pub scopes: Vec<String>,
    #[serde(rename = "subscriptionType", skip_serializing_if = "Option::is_none")]
    pub subscription_type: Option<String>,
    #[serde(rename = "rateLimitTier", skip_serializing_if = "Option::is_none")]
    pub rate_limit_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClaudeConfigJson {
    #[serde(rename = "oauthAccount", skip_serializing_if = "Option::is_none")]
    pub oauth_account: Option<ClaudeOauthAccount>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClaudeOauthAccount {
    #[serde(rename = "accountUuid", skip_serializing_if = "Option::is_none")]
    pub account_uuid: Option<String>,
    #[serde(rename = "emailAddress", skip_serializing_if = "Option::is_none")]
    pub email_address: Option<String>,
    #[serde(rename = "organizationUuid", skip_serializing_if = "Option::is_none")]
    pub organization_uuid: Option<String>,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(
        rename = "hasExtraUsageEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub has_extra_usage_enabled: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub auth_mode: AuthMode,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl AccountInfo {
    pub fn from_stored(account: &StoredAccount, active_id: Option<&str>) -> Self {
        Self {
            id: account.id.clone(),
            name: account.name.clone(),
            email: account.email.clone(),
            plan_type: account.plan_type.clone(),
            auth_mode: account.auth_mode,
            is_active: active_id == Some(&account.id),
            created_at: account.created_at,
            last_used_at: account.last_used_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub account_id: String,
    pub plan_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub account_uuid: Option<String>,
    pub organization_uuid: Option<String>,
    pub organization_name: Option<String>,
    pub organization_role: Option<String>,
    pub workspace_role: Option<String>,
    pub has_extra_usage_enabled: Option<bool>,
    pub messages_remaining: Option<i64>,
    pub messages_limit: Option<i64>,
    pub messages_reset_at: Option<String>,
    pub tokens_remaining: Option<i64>,
    pub tokens_limit: Option<i64>,
    pub session_percent_used: Option<i64>,
    pub session_percent_remaining: Option<i64>,
    pub session_reset_at_label: Option<String>,
    pub week_percent_used: Option<i64>,
    pub week_percent_remaining: Option<i64>,
    pub week_reset_at_label: Option<String>,
    pub usage_source: Option<String>,
    pub usage_note: Option<String>,
    pub error: Option<String>,
}

impl UsageInfo {
    pub fn error(account_id: String, error: String) -> Self {
        Self {
            account_id,
            plan_type: None,
            rate_limit_tier: None,
            email: None,
            display_name: None,
            account_uuid: None,
            organization_uuid: None,
            organization_name: None,
            organization_role: None,
            workspace_role: None,
            has_extra_usage_enabled: None,
            messages_remaining: None,
            messages_limit: None,
            messages_reset_at: None,
            tokens_remaining: None,
            tokens_limit: None,
            session_percent_used: None,
            session_percent_remaining: None,
            session_reset_at_label: None,
            week_percent_used: None,
            week_percent_remaining: None,
            week_reset_at_label: None,
            usage_source: None,
            usage_note: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupSummary {
    pub total_accounts: usize,
    pub warmed_accounts: usize,
    pub failed_account_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledWarmupStatus {
    pub schedule: Option<ScheduledWarmupSettings>,
    pub valid_account_ids: Vec<String>,
    pub missed_run_today: bool,
    pub next_run_local_iso: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledWarmupEvent {
    pub summary: WarmupSummary,
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAccountsSummary {
    pub total_in_payload: usize,
    pub imported_count: usize,
    pub skipped_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthLoginInfo {
    pub auth_url: String,
    pub callback_port: u16,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ClaudeOAuthProfileResponse {
    pub account: ClaudeProfileAccount,
    pub organization: ClaudeProfileOrganization,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ClaudeProfileAccount {
    pub uuid: Option<String>,
    pub email: Option<String>,
    #[serde(rename = "display_name")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ClaudeProfileOrganization {
    pub uuid: Option<String>,
    #[serde(rename = "organization_type")]
    pub organization_type: Option<String>,
    #[serde(rename = "rate_limit_tier")]
    pub rate_limit_tier: Option<String>,
    #[serde(rename = "has_extra_usage_enabled")]
    pub has_extra_usage_enabled: Option<bool>,
}

pub fn map_claude_organization_type_to_plan_type(value: Option<&str>) -> Option<String> {
    match value {
        Some("claude_max") => Some(String::from("max")),
        Some("claude_pro") => Some(String::from("pro")),
        Some("claude_enterprise") => Some(String::from("enterprise")),
        Some("claude_team") => Some(String::from("team")),
        _ => None,
    }
}
