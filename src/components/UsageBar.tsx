import type { UsageInfo } from "../types";

interface UsageBarProps {
  usage?: UsageInfo;
  loading?: boolean;
}

function toTitleCase(value: string | null | undefined) {
  if (!value) return null;
  return value
    .split(/[_\s-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function formatQuota(remaining: number | null, limit: number | null) {
  if (remaining == null && limit == null) return null;
  if (remaining != null && limit != null) return `${remaining}/${limit} left`;
  if (remaining != null) return `${remaining} left`;
  return `limit ${limit}`;
}

function formatResetAt(value: string | null | undefined) {
  if (!value) return null;

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return date.toLocaleString();
}

function UsagePercentCard({
  label,
  used,
  remaining,
  resetLabel,
}: {
  label: string;
  used: number | null;
  remaining: number | null;
  resetLabel: string | null;
}) {
  if (used == null) return null;

  const safeUsed = Math.max(0, Math.min(100, used));
  const safeRemaining =
    remaining == null ? Math.max(0, 100 - safeUsed) : Math.max(0, Math.min(100, remaining));
  const formattedResetLabel = formatResetAt(resetLabel);

  return (
    <div className="space-y-1 rounded-xl border border-slate-200 bg-slate-50/80 p-3">
      <div className="flex items-center justify-between gap-3">
        <div className="text-xs font-medium text-slate-800">{label}</div>
        <div className="text-xs text-slate-500">{safeRemaining}% left</div>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-slate-200">
        <div
          className="h-full rounded-full bg-emerald-500 transition-[width]"
          style={{ width: `${safeUsed}%` }}
        />
      </div>
      <div className="text-xs text-slate-600">{safeUsed}% used</div>
      {formattedResetLabel && (
        <div className="text-xs text-slate-500">Resets: {formattedResetLabel}</div>
      )}
    </div>
  );
}

export function UsageBar({ usage, loading }: UsageBarProps) {
  if (loading) {
    return (
      <div className="space-y-2">
        <div className="h-5 w-32 rounded-full bg-gray-100 animate-pulse" />
        <div className="h-4 w-56 rounded-full bg-gray-100 animate-pulse" />
      </div>
    );
  }

  if (!usage || usage.error) {
    return (
      <div className="text-xs text-gray-400 italic py-1">
        {usage?.error || "Usage unavailable"}
      </div>
    );
  }

  const planLabel = toTitleCase(usage.plan_type) || "Unknown plan";
  const messageQuota = formatQuota(usage.messages_remaining, usage.messages_limit);
  const tokenQuota = formatQuota(usage.tokens_remaining, usage.tokens_limit);
  const resetAt = formatResetAt(usage.messages_reset_at);
  const hasCliPercentUsage =
    usage.session_percent_used != null || usage.week_percent_used != null;

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-2">
        <span className="rounded-full border border-emerald-200 bg-emerald-50 px-2.5 py-1 text-xs font-medium text-emerald-800">
          Plan: {planLabel}
        </span>
      </div>

      {hasCliPercentUsage && (
        <div className="flex flex-col gap-2">
          <UsagePercentCard
            label="Current session"
            used={usage.session_percent_used}
            remaining={usage.session_percent_remaining}
            resetLabel={usage.session_reset_at_label}
          />
          <UsagePercentCard
            label="Current week"
            used={usage.week_percent_used}
            remaining={usage.week_percent_remaining}
            resetLabel={usage.week_reset_at_label}
          />
        </div>
      )}

      {(messageQuota || tokenQuota || resetAt || usage.usage_note) && (
        <div className="space-y-1">
          {messageQuota && (
            <div className="text-xs text-gray-700">Messages: {messageQuota}</div>
          )}
          {tokenQuota && (
            <div className="text-xs text-gray-700">Tokens: {tokenQuota}</div>
          )}
          {resetAt && (
            <div className="text-xs text-gray-500">Resets: {resetAt}</div>
          )}
          {usage.usage_note && (
            <div className="text-xs text-gray-400 italic">{usage.usage_note}</div>
          )}
        </div>
      )}
    </div>
  );
}
