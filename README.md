# Antigravity CLI Statusline for Windows 11

TUI statusline and window title formatter for Antigravity CLI on Windows 11, implemented in Rust.

## Layout Adaptations

The output format alters layout density based on terminal width limits:

### 1. High-Density Layout (Width >= 160 columns)

```text
╭─ [READY] | Gemini 3.5 Flash | ⚡ [-----] (~3h12m) |  /path/to/my-project |  main* (+12/-5) | 󰘚 [========>-] 14.8% (148.0K/1.0M) |  rd:115.8K/wr:0 | 󰧑 4 | 󰚩 1 | 󰔛 2
```

### 2. Standard Layout (Width >= 80 columns)

```text
╭─ [READY] | Claude Sonnet 3.5 | ⚡ [-----] (~3h12m) |  my-project |  main* (+12/-5)
╰─ 󰘚 [====>---] 65.0% (free:35.0%/350.0K) |  rd:115.8K/wr:0 | 󰧑 4
```

### 3. Low-Density Layout (Width < 80 columns)

```text
[THINKING] | Sonnet 3.5 | ⚡ 65%
󰘚 [====>-] 65.0% (650.0K/1.0M)
```

### 4. Window Title Layout

```text
 idle | my-project
```

*Note: Directory paths and Git branch badges embed OSC8 escape sequences to enable terminal-based hyperlinks redirection.*

## Technical Architecture Specifications

- **Native Compilation**: Optimizations utilize `opt-level = "z"`, Link-Time Optimization (LTO), and stripped symbols.
- **Inter-Process Communication (IPC)**: Uses Windows Shared Memory (`CreateFileMappingW`) and Named Mutexes (`CreateMutexW`). The IPC buffer (`SharedVcsInfo`) employs a fixed memory layout (`#[repr(C)]`) matching protocol version 6.
- **Mtime-Based Refresh Policy**: Queries Git database times (`.git/HEAD`, `.git/index`). Foreground execution skips background refresh spawns when cached states match active files and the elapsed cache lifetime is below the 10-second threshold.
- **Command-line Interface**: Dispatch logic routes actions based on parameters (`--config`, `--theme`, `--title`, `--refresh`).
- **Terminal Hyperlink Redirections**: Outputs OSC8 escape sequences for working directories and repository paths. The display engine employs a terminal escape parser state machine to compute visual text bounds omitting escape sequence lengths.
- **Secure Credential Storage**: Integrates with the Win32 `CredReadW` and `CredWriteW` APIs under `gemini:antigravity:<alias>` targets, avoiding plaintext token storage in configuration files.
- **Out-of-Quota Handling**: Overrides quota labels with `[EXHAUSTED] (Switch via --config)` wrapping in an OSC 8 terminal hyperlink when model usage reaches limit thresholds.

## Configuration System

### 1. Integration Settings (`~/.gemini/antigravity-cli/settings.json`)

Integration into the CLI utilizes the following fields in `settings.json`:

```json
{
  "...": "...",
  "statusLine": {
    "type": "command",
    "command": "C:/path/to/statusline.exe",
    "enabled": true
  },
  "title": {
    "type": "command",
    "command": "C:/path/to/statusline.exe --title",
    "enabled": true
  }
}
```

The executable runs in title formatting mode when the `--title` argument is passed.

### 2. Parameter Specifications (`~/.gemini/antigravity-cli/statusline/statusline.json`)

The application configures runtime toggles and visual parameters via `statusline.json`.

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `theme` | `string` | `"frost"` | Selects active palette theme (`"frost"`, `"pastel"`, `"neon"`). |
| `show_vcs` | `boolean` | `true` | Configures visibility of Git repository and branch details. |
| `show_quota` | `boolean` | `true` | Configures visibility of Google API model subscription quota trackers. |
| `show_context` | `boolean` | `true` | Configures visibility of context window memory tokens progress bar. |
| `show_settings` | `boolean` | `true` | Configures visibility of settings configuration indicator. |
| `saved_accounts` | `array of strings` | `[]` | Registers Credential Manager backup profiles aliases. |

### 3. Interactive Configuration Utility (`--config`)

Executing the application with `--config` (or interactively without input pipes) starts the console configuration utility.

```text
==================================================================
                 Antigravity Statusline Config                    
==================================================================
--- PREVIEW ------------------------------------------------------
╭─ [READY] | Gemini 3.5 Flash | ⚡ [====>-] (~3h12m) |  my-project |  main* (+12/-5)
------------------------------------------------------------------

[SETTINGS]
   Theme: frost
 ❯ Show Git VCS Info: ● Enabled
   Show Model Quota: ● Enabled
   Show Context Token Bar: ● Enabled
   Show Settings Gear Icon: ● Enabled
   Account Management
   Save & Exit
   Cancel & Exit

  Use ↑/↓ to navigate • Enter to toggle/select • Esc to abort
==================================================================
```

#### Account Management Submenu

Selecting **Account Management** displays the profile manager:

- **Saved Accounts List**: Triggers credential switching by mapping selected alias strings back to target Credential Manager slots.
- **Save Current Account**: Encrypts and writes current environment tokens into a local backup alias target.
- **Add Account**: Polls for credential inputs during terminal auth procedures.

```text
==============================================================
                      Account Management
==============================================================
  Current Active: work (Saved)
--------------------------------------------------------------
   work
 ❯ personal
   [Save Current Account] (Saved as 'work')
   [Add Account] (Login via 'agy')
   [Back to Settings]

  Use ↑/↓ to navigate • Enter to confirm • Esc to cancel
==============================================================
```

## Compilation & Execution

### Compilation Command

```powershell
cargo build --release
```

Output binary: `target/release/statusline.exe`

### Command Options

| Command Option | Parameter | Description |
| :--- | :--- | :--- |
| `--config` | *None* | Launches the interactive console setup. |
| `--theme` | `<name>` | Changes color scheme (`frost`, `pastel`, `neon`). |
| `--title` | *None* | Generates formatted output for terminal titles. |
| `--refresh` | *None* | Spawns background caching of VCS and quota information. |
| `--cwd` | `<path>` | Sets working path associated with `--refresh` calls. |

### Security Verification

```powershell
gh attestation verify statusline.exe --repo <github-username>/antigravity-statusline-win11-rs
```

## Requirements

- **Operating System**: Windows 11.
- **Dependencies**: None.

## Legacy JS Version

The legacy JavaScript implementation is removed.

## References

- [Antigravity CLI Statusline Documentation](https://antigravity.google/docs/cli-statusline)
- [Antigravity CLI Title Documentation](https://antigravity.google/docs/cli-title)
- [Google Antigravity CLI Examples](https://github.com/google-antigravity/antigravity-cli/tree/main/examples)

## License

MIT
