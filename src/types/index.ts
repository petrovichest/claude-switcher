export type AuthMode = "claude_oauth";

export interface AccountInfo {
  id: string;
  name: string;
  email: string | null;
  plan_type: string | null;
  auth_mode: AuthMode;
  is_active: boolean;
  created_at: string;
  last_used_at: string | null;
}

export interface UsageInfo {
  account_id: string;
  plan_type: string | null;
  rate_limit_tier: string | null;
  email: string | null;
  display_name: string | null;
  account_uuid: string | null;
  organization_uuid: string | null;
  organization_name: string | null;
  organization_role: string | null;
  workspace_role: string | null;
  has_extra_usage_enabled: boolean | null;
  messages_remaining: number | null;
  messages_limit: number | null;
  messages_reset_at: string | null;
  tokens_remaining: number | null;
  tokens_limit: number | null;
  session_percent_used: number | null;
  session_percent_remaining: number | null;
  session_reset_at_label: string | null;
  week_percent_used: number | null;
  week_percent_remaining: number | null;
  week_reset_at_label: string | null;
  usage_source: string | null;
  usage_note: string | null;
  error: string | null;
}

export interface OAuthLoginInfo {
  auth_url: string;
  callback_port: number;
}

export interface AccountWithUsage extends AccountInfo {
  usage?: UsageInfo;
  usageLoading?: boolean;
}

export interface ClaudeProcessInfo {
  count: number;
  background_count: number;
  can_switch: boolean;
  pids: number[];
}

export interface WarmupSummary {
  total_accounts: number;
  warmed_accounts: number;
  failed_account_ids: string[];
}

export interface ScheduledWarmupSettings {
  enabled: boolean;
  local_time: string;
  account_ids: string[];
  last_run_local_date: string | null;
  last_missed_prompt_local_date: string | null;
}

export interface ScheduledWarmupStatus {
  schedule: ScheduledWarmupSettings | null;
  valid_account_ids: string[];
  missed_run_today: boolean;
  next_run_local_iso: string | null;
}

export interface ScheduledWarmupEvent {
  summary: WarmupSummary;
  trigger: string;
}

export interface ImportAccountsSummary {
  total_in_payload: number;
  imported_count: number;
  skipped_count: number;
}

export type ExportSecurityMode = "less_secure" | "passphrase" | "keychain";

export interface AppSettings {
  version: number;
  export_security_mode: ExportSecurityMode | null;
  scheduled_warmup: ScheduledWarmupSettings | null;
}
