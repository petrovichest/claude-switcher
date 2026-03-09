//! Account switching logic for Claude Code credentials

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::types::{
    AuthData, ClaudeAiOauthData, ClaudeConfigJson, ClaudeOauthAccount, CredentialsDotJson,
    StoredAccount,
};

use super::fs_utils::{write_bytes_atomic, FileLock};

pub fn get_claude_config_dir() -> Result<PathBuf> {
    if let Ok(value) = std::env::var("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(value));
    }

    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(".claude"))
}

pub fn get_claude_credentials_file() -> Result<PathBuf> {
    Ok(get_claude_config_dir()?.join(".credentials.json"))
}

pub fn get_claude_settings_file() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(".claude.json"))
}

pub fn switch_to_account(account: &StoredAccount) -> Result<()> {
    let config_dir = get_claude_config_dir()?;
    fs::create_dir_all(&config_dir).with_context(|| {
        format!(
            "Failed to create Claude config directory: {}",
            config_dir.display()
        )
    })?;

    let credentials = create_credentials_json(account)?;
    let credentials_path = get_claude_credentials_file()?;
    let _credentials_lock = FileLock::acquire(&credentials_path)?;
    let credentials_content =
        serde_json::to_string_pretty(&credentials).context("Failed to serialize credentials")?;
    write_bytes_atomic(&credentials_path, credentials_content.as_bytes(), true).with_context(
        || format!("Failed to write Claude credentials: {}", credentials_path.display()),
    )?;
    set_secure_permissions(&credentials_path)?;

    let settings_path = get_claude_settings_file()?;
    let mut settings = read_claude_settings().unwrap_or(None).unwrap_or_default();
    settings.oauth_account = create_oauth_account_metadata(account);

    let _settings_lock = FileLock::acquire(&settings_path)?;
    let settings_content =
        serde_json::to_string_pretty(&settings).context("Failed to serialize Claude config")?;
    write_bytes_atomic(&settings_path, settings_content.as_bytes(), true).with_context(|| {
        format!(
            "Failed to write Claude settings metadata: {}",
            settings_path.display()
        )
    })?;
    set_secure_permissions(&settings_path)?;

    Ok(())
}

fn create_credentials_json(account: &StoredAccount) -> Result<CredentialsDotJson> {
    match &account.auth_data {
        AuthData::ClaudeOAuth {
            access_token,
            refresh_token,
            expires_at_ms,
            scopes,
            rate_limit_tier,
            ..
        } => Ok(CredentialsDotJson {
            claude_ai_oauth: Some(ClaudeAiOauthData {
                access_token: access_token.clone(),
                refresh_token: refresh_token.clone(),
                expires_at: *expires_at_ms,
                scopes: scopes.clone(),
                subscription_type: account.plan_type.clone(),
                rate_limit_tier: rate_limit_tier.clone(),
            }),
        }),
    }
}

fn create_oauth_account_metadata(account: &StoredAccount) -> Option<ClaudeOauthAccount> {
    match &account.auth_data {
        AuthData::ClaudeOAuth {
            account_uuid,
            organization_uuid,
            display_name,
            has_extra_usage_enabled,
            ..
        } => Some(ClaudeOauthAccount {
            account_uuid: account_uuid.clone(),
            email_address: account.email.clone(),
            organization_uuid: organization_uuid.clone(),
            display_name: display_name.clone(),
            has_extra_usage_enabled: *has_extra_usage_enabled,
            ..ClaudeOauthAccount::default()
        }),
    }
}

pub fn import_from_credentials_file(path: &str, account_name: String) -> Result<StoredAccount> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read Claude credentials file: {path}"))?;
    let credentials: CredentialsDotJson = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse Claude credentials file: {path}"))?;

    let oauth = credentials
        .claude_ai_oauth
        .context("Claude credentials file does not contain claudeAiOauth")?;

    let maybe_settings = maybe_read_matching_settings(Path::new(path)).ok().flatten();
    let oauth_account = maybe_settings.and_then(|settings| settings.oauth_account);

    Ok(StoredAccount::new_claude(
        account_name,
        oauth_account.as_ref().and_then(|value| value.email_address.clone()),
        oauth.subscription_type.clone(),
        oauth.access_token,
        oauth.refresh_token,
        oauth.expires_at,
        oauth.scopes,
        oauth_account.as_ref().and_then(|value| value.account_uuid.clone()),
        oauth_account
            .as_ref()
            .and_then(|value| value.organization_uuid.clone()),
        oauth.rate_limit_tier,
        oauth_account.as_ref().and_then(|value| value.display_name.clone()),
        oauth_account
            .as_ref()
            .and_then(|value| value.has_extra_usage_enabled),
    ))
}

fn maybe_read_matching_settings(credentials_path: &Path) -> Result<Option<ClaudeConfigJson>> {
    let current_credentials = get_claude_credentials_file()?;
    if credentials_path != current_credentials {
        return Ok(None);
    }

    read_claude_settings()
}

pub fn read_current_credentials() -> Result<Option<CredentialsDotJson>> {
    let path = get_claude_credentials_file()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read Claude credentials: {}", path.display()))?;
    let credentials = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse Claude credentials: {}", path.display()))?;
    Ok(Some(credentials))
}

pub fn read_claude_settings() -> Result<Option<ClaudeConfigJson>> {
    let path = get_claude_settings_file()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read Claude settings: {}", path.display()))?;
    let settings = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse Claude settings: {}", path.display()))?;
    Ok(Some(settings))
}

pub fn has_active_login() -> Result<bool> {
    match read_current_credentials()? {
        Some(credentials) => Ok(credentials.claude_ai_oauth.is_some()),
        None => Ok(false),
    }
}

fn set_secure_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}
