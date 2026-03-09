<p align="center">
  <img src="src-tauri/icons/logo.svg" alt="Claude Switcher" width="128" height="128">
</p>

<h1 align="center">Claude Switcher</h1>

<p align="center">
  A desktop app for managing multiple Claude Code accounts.<br>
  Switch the active account, inspect live usage and plan data, run warmups, and keep local Claude credentials in sync.
</p>

## Features

- **Multi-Account Management** - Add and manage multiple Claude Code accounts in one place
- **Quick Switching** - Switch the active Claude Code account with a single click
- **Usage Insights** - Refresh plan details, rate-limit tier, and available message/token quota
- **Scheduled Warmups** - Run daily warmups for selected accounts while the app is open
- **OAuth + Import** - Sign in via Claude OAuth or import an existing `.credentials.json`
- **Encrypted Backups** - Export/import account bundles with optional passphrase or keychain protection
- **Process Awareness** - Detect running Claude processes before switching accounts

## Installation

### Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/)

### Build from Source

```bash
git clone git@github.com:petrovichest/claude-switcher.git
cd claude-switcher
pnpm install
pnpm tauri dev
```

Production bundles will be written under `src-tauri/target/release/bundle/`.

## Notes

- Active switching writes Claude OAuth credentials to `~/.claude/.credentials.json`
- Account metadata is also synced to `~/.claude.json` so Claude Code sees the matching account profile
- Usage data comes from Claude OAuth usage/profile endpoints when available and falls back to the last known values in the app
- Scheduled warmups only run while the desktop app is open and trigger once per local day
