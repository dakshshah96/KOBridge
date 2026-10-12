# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

KOBridge is a macOS app (Tauri 2 + Svelte 5 + Vite, Rust backend) that manages KOReader on a Kindle over SSH. UI copy is Spanish (voseo: "Conectá", "Reiniciá").

## Commands

```sh
bun install
bunx tauri dev                          # app with HMR; Rust changes rebuild automatically
bunx tauri build                        # .app + .dmg in src-tauri/target/release/bundle/
bunx tauri build --bundles app          # .app only (faster)
bun run check                           # svelte-check + tsc; must stay at 0 errors / 0 warnings

cd src-tauri
cargo test                              # unit tests (Lua parser, Markdown, path allowlist, quoting)
cargo test parses_sidecar               # single test by name
cargo test -- --ignored --nocapture     # live tests against the real Kindle
cargo test kindle_size -- --ignored --nocapture
```

The `--ignored` tests in `src-tauri/src/lib.rs` (`mod live`) need the Kindle online. `kindle_writes` creates and then deletes files: `/tmp/kob test/` and `koreader/patches/9-kob-test*.lua.disabled`.

Tauri CLI comes from `@tauri-apps/cli` via `bunx`. It is not installed with `cargo install`.

## Architecture

**SSH is the system binary.** `src-tauri/src/ssh.rs` spawns `/usr/bin/ssh kindle` with `BatchMode`, `ConnectTimeout=5` and `ControlMaster`/`ControlPersist=120` (socket `~/.ssh/kob-%C`). Everything about reaching the device lives in the user's `~/.ssh/config`, so there is no IP setting in the app:
- `Match originalhost kindle exec "nc -z ..."` blocks pick the Kindle's IP for the current Wi-Fi.
- `HostKeyAlias kindle` keeps one known_hosts entry across IPs.

Do not replace this with a Rust SSH crate.

Four transport primitives in `ssh.rs`; every feature is a thin wrapper:
- `run`: stdout as a String.
- `run_with_input`: writes remote files through `cat >` on stdin, so file content is never shell-quoted.
- `pull_to_file`: streams stdout to `<dest>.part`, then renames it; used by backup and stats.
- `upload`: streams a local file into `cat > name.kob-part && mv`. It avoids `scp`, whose remote side word-splits book names with spaces and parentheses.

`stream` spawns a long-running command such as `tail -f` for the log view.

**Remote commands are shell strings.** Interpolate any value only through `ssh::sq()` (single-quote escaping). `lib.rs::allowed()` restricts read, write, rename and delete to:
- `koreader/patches/<name>`
- `/mnt/us/fonts/<name>`
- `koreader/defaults.custom.lua`
- `koreader/settings.reader.lua`

Extend that allowlist rather than bypassing it.

**Commands (`lib.rs`)** map one-to-one onto the sidebar views in `src/lib/views/`. They are async Tauri commands that return `Result<T, String>`; the String is shown to the user, so write it in Spanish. Streaming uses `tauri::ipc::Channel`: backup bytes and log lines. App settings are a JSON file in `app_config_dir()` (struct `Settings`, `#[serde(default)]` so new fields are backward compatible). Caches live in `app_cache_dir()`: the `statistics.sqlite3` snapshot and `highlights.json`. Stats and highlights therefore render offline.

**Device facts the code depends on:**
- **Busybox userland.** `tar` supports `-T` and the device has `sqlite3`, but no `rsync`.
- **Backup paths.** `backup_list_cmd()` writes `/tmp/kob-list`; both `backup_run` and `backup_size` (`du -sk` plus an 8 MB `dd` speed probe) use it.
- **Stats snapshot.** Stats are copied with `sqlite3 ... '.backup /tmp/kob-stats.db'`, never by copying the live DB. `stats.rs` queries `page_stat_data`, not the `page_stat` view.
- **Sidecar location.** Sidecars sit next to books (`*.sdr/metadata.*.lua`, `document_metadata_folder = "doc"`).
- **Highlights fetch.** One ssh call cats every sidecar that has annotations, each prefixed by an `@@KOB@@ <path>` line (`highlights::FETCH_CMD`).
- **Lua parsing.** `lua.rs` is a hand-written parser for KOReader's `%q` serialization subset. Only the modern `annotations` table is read; the device has 0 legacy `highlight` tables.
- **`settings.reader.lua` writes.** KOReader overwrites this file on exit, so `write_remote` refuses it while `reader.lua` is running. The check is server side, not just in the UI.
- **No remote restart.** KOReader cannot be restarted from outside: `koreader.sh` only relaunches on an internal exit code 85. The UI tells the user to restart it instead.
- **SSH server lifetime.** The dropbear SSH server survives a KOReader exit.

**Frontend.**
- **Shared state.** `src/lib/store.svelte.ts` (Svelte 5 runes) holds global state: connection status polled every 30 s, settings and backup progress. It also runs the auto-backup: one attempt per day, and `last_auto_backup` is saved before the run so a failure doesn't retry every poll. `explain()` maps ssh stderr to a user-facing hint.
- **Typed wrappers.** `src/lib/api.ts` has the typed `invoke` wrappers and the formatters, which use Spanish decimal commas.
- **Data views.** Views that read the device load the cache on mount, then fetch once per visit through an `$effect` that waits for `app.status`. While loading they show `Loading.svelte` (spinner, step text, elapsed seconds).
- **Uploads.** `Dropzone.svelte` (Tauri `onDragDropEvent` plus the dialog plugin, sequential queue) serves both Send books and Fonts.

## Design rules

- **Tokens.** All colors are tokens in `src/app.css`, with light values and dark values under `prefers-color-scheme`. The palette is e-ink greys and ink-black actions. The only color, highlighter yellow `--mark`, is reserved for highlighted text.
- **Fonts.** System fonts only: `-apple-system` for UI, `ui-serif` (New York) for book titles and quotes. No web fonts or icon/CSS libraries; icons are inline Lucide path strings in `App.svelte`.
- **No accent rails.** The user rejects them as AI slop: no `border-left` or inset-shadow bars on selected items or quotes. Selection is a background change.
- **No inert waits.** Every async action shows feedback (button label change, spinner, or `Loading`).
- **Window chrome.** `titleBarStyle: Overlay` with `.titlebar` drag regions. Only `.pane` scrolls (`html`/`body` are `overflow: hidden`).

## Release / install

- **Local Network permission.** `src-tauri/Info.plist` adds `NSLocalNetworkUsageDescription`. Without it, macOS may silently block the ssh child from reaching the LAN when the app is launched from Finder, even though the same command works from Terminal.
- **Installing.** Copy `src-tauri/target/release/bundle/macos/KOBridge.app` over `/Applications/KOBridge.app`.
- **Icon.** Source is `assets/icon.svg`; regenerate with `rsvg-convert -w 1024 -h 1024 assets/icon.svg -o assets/icon.png && bunx tauri icon assets/icon.png`, then delete `src-tauri/icons/{android,ios}`.

## Out of scope

Calibre integration (sshfs + "Connect to folder", or a Calibre device plugin) is planned as a separate project. Don't add it here.
