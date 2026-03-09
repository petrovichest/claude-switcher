import { useState, useRef, useEffect } from "react";
import type { AccountWithUsage } from "../types";
import { UsageBar } from "./UsageBar";

interface AccountCardProps {
  account: AccountWithUsage;
  onSwitch: () => void;
  onWarmup: () => Promise<void>;
  onDelete: () => void;
  onRefresh: () => Promise<void>;
  onRename: (newName: string) => Promise<void>;
  switching?: boolean;
  switchDisabled?: boolean;
  warmingUp?: boolean;
  masked?: boolean;
  onToggleMask?: () => void;
}

function formatLastRefresh(date: Date | null): string {
  if (!date) return "Never";
  const now = new Date();
  const diff = Math.floor((now.getTime() - date.getTime()) / 1000);
  if (diff < 5) return "Just now";
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return date.toLocaleDateString();
}

function BlurredText({ children, blur }: { children: React.ReactNode; blur: boolean }) {
  return (
    <span
      className={`transition-all duration-200 select-none ${blur ? "blur-sm" : ""}`}
      style={blur ? { userSelect: "none" } : undefined}
    >
      {children}
    </span>
  );
}

export function AccountCard({
  account,
  onSwitch,
  onWarmup,
  onDelete,
  onRefresh,
  onRename,
  switching,
  switchDisabled,
  warmingUp,
  masked = false,
  onToggleMask,
}: AccountCardProps) {
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(
    account.usage && !account.usage.error ? new Date() : null
  );
  const [isEditing, setIsEditing] = useState(false);
  const [editName, setEditName] = useState(account.name);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  const handleRefresh = async () => {
    setIsRefreshing(true);
    try {
      await onRefresh();
      setLastRefresh(new Date());
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleRename = async () => {
    const trimmed = editName.trim();
    if (trimmed && trimmed !== account.name) {
      try {
        await onRename(trimmed);
      } catch {
        setEditName(account.name);
      }
    } else {
      setEditName(account.name);
    }
    setIsEditing(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleRename();
    } else if (e.key === "Escape") {
      setEditName(account.name);
      setIsEditing(false);
    }
  };

  const planDisplay = account.plan_type
    ? account.plan_type.charAt(0).toUpperCase() + account.plan_type.slice(1)
    : "Claude";

  const planColors: Record<string, string> = {
    max: "bg-stone-100 text-stone-800 border-stone-300",
    pro: "bg-indigo-50 text-indigo-700 border-indigo-200",
    team: "bg-blue-50 text-blue-700 border-blue-200",
    enterprise: "bg-amber-50 text-amber-700 border-amber-200",
    free: "bg-gray-50 text-gray-600 border-gray-200",
  };

  const planKey = account.plan_type?.toLowerCase() || "free";
  const planColorClass = planColors[planKey] || planColors.free;

  return (
    <div
      className={`relative mx-auto flex h-full w-full max-w-sm flex-col rounded-2xl border p-4 transition-all duration-200 ${
        account.is_active
          ? "border-emerald-400 bg-white shadow-sm"
          : "border-gray-200 bg-white hover:border-gray-300"
      }`}
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="mb-2 flex items-center gap-2">
            {account.is_active && (
              <span className="flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-2 w-2 rounded-full bg-green-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
              </span>
            )}
            <span
              className={`px-2.5 py-1 text-xs font-medium rounded-full border ${planColorClass}`}
            >
              {planDisplay}
            </span>
          </div>
          <div className="space-y-1.5">
            {isEditing ? (
              <input
                ref={inputRef}
                type="text"
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                onBlur={handleRename}
                onKeyDown={handleKeyDown}
                className="w-full rounded border border-gray-300 bg-gray-100 px-2 py-1 text-base font-semibold text-gray-900 focus:border-gray-500 focus:outline-none"
              />
            ) : (
              <h3
                className="cursor-pointer truncate text-base font-semibold text-gray-900 hover:text-gray-600"
                onClick={() => !masked && setIsEditing(true)}
                title={masked ? undefined : "Click to rename"}
              >
                <BlurredText blur={masked}>{account.name}</BlurredText>
              </h3>
            )}
            {(account.email || account.usage?.email || account.usage?.display_name) && (
              <p className="truncate text-sm text-gray-500">
                <BlurredText blur={masked}>
                  {account.email || account.usage?.email || account.usage?.display_name}
                </BlurredText>
              </p>
            )}
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {onToggleMask && (
            <button
              onClick={onToggleMask}
              className="rounded-lg p-1.5 text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-600"
              title={masked ? "Show info" : "Hide info"}
            >
              {masked ? (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                </svg>
              ) : (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                </svg>
              )}
            </button>
          )}
        </div>
      </div>

      <div className="mb-3">
        <UsageBar usage={account.usage} loading={isRefreshing || account.usageLoading} />
      </div>

      <div className="mb-4 text-xs text-gray-400">
        Last updated: {formatLastRefresh(lastRefresh)}
      </div>

      <div className="mt-auto flex flex-col gap-2">
        {account.is_active ? (
          <button
            disabled
            className="w-full rounded-lg border border-gray-200 bg-gray-100 px-4 py-2 text-sm font-medium text-gray-500 cursor-default"
          >
            ✓ Active
          </button>
        ) : (
          <button
            onClick={onSwitch}
            disabled={switching || switchDisabled}
            className={`w-full rounded-lg px-4 py-2 text-sm font-medium transition-colors disabled:opacity-50 ${
              switchDisabled
                ? "bg-gray-200 text-gray-400 cursor-not-allowed"
                : "bg-gray-900 hover:bg-gray-800 text-white"
            }`}
            title={switchDisabled ? "Close all Claude Code processes first" : undefined}
          >
            {switching ? "Switching..." : switchDisabled ? "Claude Running" : "Switch"}
          </button>
        )}
        <div className="grid grid-cols-3 gap-2">
          <button
            onClick={() => {
              void onWarmup();
            }}
            disabled={warmingUp}
            className={`rounded-lg px-3 py-2 text-sm transition-colors ${
              warmingUp
                ? "bg-amber-100 text-amber-500"
                : "bg-amber-50 text-amber-700 hover:bg-amber-100"
            }`}
            title={warmingUp ? "Sending warm-up request..." : "Send minimal warm-up request"}
          >
            ⚡
          </button>
          <button
            onClick={handleRefresh}
            disabled={isRefreshing}
            className={`rounded-lg px-3 py-2 text-sm transition-colors ${
              isRefreshing
                ? "bg-gray-200 text-gray-400"
                : "bg-gray-100 text-gray-600 hover:bg-gray-200"
            }`}
            title="Refresh usage"
          >
            <span className={isRefreshing ? "inline-block animate-spin" : ""}>↻</span>
          </button>
          <button
            onClick={onDelete}
            className="rounded-lg bg-red-50 px-3 py-2 text-sm text-red-600 transition-colors hover:bg-red-100"
            title="Remove account"
          >
            ✕
          </button>
        </div>
      </div>
    </div>
  );
}
