//! Account storage module - manages reading and writing accounts.json

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::types::{AccountsStore, AuthData, StoredAccount};

use super::fs_utils::{write_bytes_atomic, FileLock};

/// Get the path to the claude-switcher config directory
pub fn get_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(".claude-switcher"))
}

/// Get the path to accounts.json
pub fn get_accounts_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("accounts.json"))
}

/// Load the accounts store from disk
pub fn load_accounts() -> Result<AccountsStore> {
    let path = get_accounts_file()?;
    load_accounts_from_path(&path)
}

fn load_accounts_from_path(path: &PathBuf) -> Result<AccountsStore> {
    if !path.exists() {
        return Ok(AccountsStore::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read accounts file: {}", path.display()))?;

    let store: AccountsStore = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse accounts file: {}", path.display()))?;

    Ok(store)
}

/// Save the accounts store to disk
pub fn save_accounts(store: &AccountsStore) -> Result<()> {
    let path = get_accounts_file()?;
    let _lock = FileLock::acquire(&path)?;
    save_accounts_to_path(&path, store)
}

fn save_accounts_to_path(path: &PathBuf, store: &AccountsStore) -> Result<()> {
    // Ensure the config directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content =
        serde_json::to_string_pretty(store).context("Failed to serialize accounts store")?;

    write_bytes_atomic(path, content.as_bytes(), true)
        .with_context(|| format!("Failed to write accounts file: {}", path.display()))?;

    // Set restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

fn update_accounts_store<T, F>(mutate: F) -> Result<T>
where
    F: FnOnce(&mut AccountsStore) -> Result<T>,
{
    let path = get_accounts_file()?;
    let _lock = FileLock::acquire(&path)?;
    let mut store = load_accounts_from_path(&path)?;
    let result = mutate(&mut store)?;
    save_accounts_to_path(&path, &store)?;
    Ok(result)
}

/// Add a new account to the store
pub fn add_account(account: StoredAccount) -> Result<StoredAccount> {
    update_accounts_store(|store| {
        if store.accounts.iter().any(|a| a.name == account.name) {
            anyhow::bail!("An account with name '{}' already exists", account.name);
        }

        let account_clone = account.clone();
        store.accounts.push(account);

        if store.accounts.len() == 1 {
            store.active_account_id = Some(account_clone.id.clone());
        }

        Ok(account_clone)
    })
}

/// Remove an account by ID
pub fn remove_account(account_id: &str) -> Result<()> {
    update_accounts_store(|store| {
        let initial_len = store.accounts.len();
        store.accounts.retain(|a| a.id != account_id);

        if store.accounts.len() == initial_len {
            anyhow::bail!("Account not found: {account_id}");
        }

        if store.active_account_id.as_deref() == Some(account_id) {
            store.active_account_id = store.accounts.first().map(|a| a.id.clone());
        }

        Ok(())
    })
}

/// Update the active account ID
pub fn set_active_account(account_id: &str) -> Result<()> {
    update_accounts_store(|store| {
        if !store.accounts.iter().any(|a| a.id == account_id) {
            anyhow::bail!("Account not found: {account_id}");
        }

        store.active_account_id = Some(account_id.to_string());
        Ok(())
    })
}

/// Get an account by ID
pub fn get_account(account_id: &str) -> Result<Option<StoredAccount>> {
    let store = load_accounts()?;
    Ok(store.accounts.into_iter().find(|a| a.id == account_id))
}

/// Get the currently active account
pub fn get_active_account() -> Result<Option<StoredAccount>> {
    let store = load_accounts()?;
    let active_id = match &store.active_account_id {
        Some(id) => id,
        None => return Ok(None),
    };
    Ok(store.accounts.into_iter().find(|a| a.id == *active_id))
}

/// Update an account's last_used_at timestamp
pub fn touch_account(account_id: &str) -> Result<()> {
    update_accounts_store(|store| {
        if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
            account.last_used_at = Some(chrono::Utc::now());
        }

        Ok(())
    })
}

/// Update an account's metadata (name, email, plan_type)
pub fn update_account_metadata(
    account_id: &str,
    name: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
) -> Result<()> {
    update_accounts_store(|store| {
        if let Some(ref new_name) = name {
            if store
                .accounts
                .iter()
                .any(|a| a.id != account_id && a.name == *new_name)
            {
                anyhow::bail!("An account with name '{new_name}' already exists");
            }
        }

        let account = store
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .context("Account not found")?;

        if let Some(new_name) = name {
            account.name = new_name;
        }

        if email.is_some() {
            account.email = email;
        }

        if plan_type.is_some() {
            account.plan_type = plan_type;
        }

        Ok(())
    })
}

/// Update Claude OAuth tokens for an account and return the updated account.
#[allow(clippy::too_many_arguments)]
pub fn update_account_claude_tokens(
    account_id: &str,
    access_token: String,
    refresh_token: String,
    expires_at_ms: i64,
    scopes: Vec<String>,
    account_uuid: Option<String>,
    organization_uuid: Option<String>,
    rate_limit_tier: Option<String>,
    display_name: Option<String>,
    has_extra_usage_enabled: Option<bool>,
    email: Option<String>,
    plan_type: Option<String>,
) -> Result<StoredAccount> {
    update_accounts_store(|store| {
        let account = store
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .context("Account not found")?;

        match &mut account.auth_data {
            AuthData::ClaudeOAuth {
                access_token: stored_access_token,
                refresh_token: stored_refresh_token,
                expires_at_ms: stored_expires_at_ms,
                scopes: stored_scopes,
                account_uuid: stored_account_uuid,
                organization_uuid: stored_organization_uuid,
                rate_limit_tier: stored_rate_limit_tier,
                display_name: stored_display_name,
                has_extra_usage_enabled: stored_has_extra_usage_enabled,
            } => {
                *stored_access_token = access_token;
                *stored_refresh_token = refresh_token;
                *stored_expires_at_ms = expires_at_ms;
                *stored_scopes = scopes;
                *stored_account_uuid = account_uuid;
                *stored_organization_uuid = organization_uuid;
                *stored_rate_limit_tier = rate_limit_tier;
                *stored_display_name = display_name;
                *stored_has_extra_usage_enabled = has_extra_usage_enabled;
            }
        }

        if let Some(new_email) = email {
            account.email = Some(new_email);
        }

        if let Some(new_plan_type) = plan_type {
            account.plan_type = Some(new_plan_type);
        }

        Ok(account.clone())
    })
}
