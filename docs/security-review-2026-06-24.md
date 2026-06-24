# TypeText Security Review — 2026-06-24

Scope: full workspace (`typetext-core`, `typetext-desktop`, `platform.rs`, `build.rs`,
Scripts, GitHub workflows), with emphasis on the **Windows offline-portable** build.
No code was changed. Reviewed at commit `c4c4c49`.

## Overall

The project is in good security shape. It shows real defense-in-depth: strict input
validation and size caps, atomic file writes, compile-time removal of network/persistence
features for the offline build, a hardened updater, and a locked-down CI pipeline with
pinned action SHAs, dependency auditing, signing/notarization, Defender scanning, and
build attestations. Nothing critical or high-severity was found. The items below are
mostly low-severity hardening suggestions and one correctness question.

## Offline-portable build (focus area)

This is the strongest part of the codebase. The hardening is layered and verified, not
just asserted:

- **Compile-time removal.** `open_url`, `fetch_text`, the GitHub update path, and the
  Registry startup code are all behind `#[cfg(not(feature = "offline-portable"))]` and are
  re-exported only for non-offline builds (`platform.rs` lines ~1850-1859). They are
  excluded from the binary, not merely disabled.
- **Dependency-graph check.** `build-windows-portable.ps1` runs `cargo tree ... --features
  offline-portable` and fails the build if `Win32_System_Registry` or
  `windows-startup-registry` appears.
- **Binary marker scan.** The same script greps the compiled `.exe` for forbidden strings
  (the GitHub API URL, `Invoke-WebRequest`, `url.dll,FileProtocolHandler`, the Run-key
  path) and throws if any are present. These markers are stored as UTF-8 Rust string
  literals, so the ASCII scan does catch them.
- **Strict paths.** `PortablePaths::strictly_beside_executable()` never falls back to
  AppData and errors if the adjacent `data` dir is not writable.
- **Remote-storage refusal.** Startup, import, and export all call
  `storage_security_warning()` and refuse UNC paths and mapped network drives
  (`main.rs` ~585-605, ~898-962).
- **Honest on-disk config.** `settings.offline.json` ships with `checkForUpdates` and
  `openOnStartup` set to false, and the app re-forces them off at runtime
  (`main.rs` 624-627).
- **CI proves the feature config** by running the test suite with
  `--no-default-features --features offline-portable --locked` before building.

Residual notes for the offline build (all low):

1. `storage_security_warning()` is implemented only for Windows; macOS/Linux return
   `None`. That is fine because the offline-portable package is Windows-only, but it means
   the "refuses remote storage" guarantee is a Windows guarantee specifically. Worth
   stating explicitly if the offline concept ever expands to other OSes.
2. Snippet/settings data is plaintext JSON beside the executable. This is already
   documented well in the README. No change needed; rely on OS/full-disk encryption as the
   README advises.

## Findings — desktop app and core

### F1 (Low, correctness) — Repository-owner inconsistency in the updater
`LATEST_RELEASE_API_URL` and the offline forbidden-marker both use
`Joshndroid/TypeText` (`main.rs:31`, `build-windows-portable.ps1:76`), while the README's
attestation instructions and the `validate_update_url` unit tests use
`fruitmac/TypeText`. These should not disagree. Confirm the real owner; a wrong owner
means update checks query the wrong repo, or the documented `gh attestation verify
--repo` command is wrong. Recommend a single source of truth for the repo slug.

### F2 (Low, hardening) — `validate_update_url` does not pin the repo path
The check enforces `https`, `host == github.com`, and empty user/password/port (good — it
correctly rejects `github.com@evil`, `github.com.evil`, and non-default ports). It does
**not** constrain the path, so any `github.com/<anything>` URL passes. Risk is low because
the URL originates from the trusted release API and GitHub constrains real asset URLs, but
adding a `path().starts_with("/<owner>/TypeText/")` check would be cheap defense-in-depth
against a tampered release listing.

### F3 (Low, local info exposure) — macOS typing passes snippet text via `osascript` argv
On macOS, snippet bodies are sent as `osascript -e 'keystroke "<text>"'` arguments
(`platform.rs` ~1207-1232, ~1545-1565). Process arguments are visible to other local users
via `ps`/`/proc`-equivalents for the brief lifetime of the call, so snippet contents are
momentarily observable locally. Windows avoids this by using `SendInput` Unicode events.
The AppleScript escaping itself is correct — only `\` and `"` are special inside a
double-quoted AppleScript literal and both are escaped, so there is no script injection.
Severity is low and consistent with the README's "don't store secrets" guidance; consider
noting it, or feeding text to `osascript` over stdin rather than argv if you want to close
it.

### F4 (Low, robustness) — Non-offline path fallback to `"."`
`PortablePaths::beside_executable().unwrap_or_else(|_| PortablePaths::from_app_dir("."))`
(`main.rs:597-598) falls back to a relative current-working-directory path if the
executable path can't be resolved. That could write `data/` into an unexpected directory.
This does not affect the offline build (which uses the strict resolver and errors out).
Consider failing closed instead of using `"."`.

### F5 (Very low, local DoS) — Predictable macOS single-instance lock
`install_app_mutex()` locks `std::env::temp_dir()/typetext.lock` with `flock`
(`platform.rs` ~1082-1105). On a shared machine another local user could hold that lock to
prevent TypeText from starting. Negligible impact for a single-user desktop tool; noting
for completeness.

## Findings — build / CI

CI is well constructed: `permissions: contents: read` by default, action SHAs pinned,
`cargo audit` at release plus a weekly scheduled RustSec scan, Developer ID signing +
notarization with stapler validation, Microsoft Defender scan of the Windows archives and
their extracted contents (fails closed on detection or unavailable service), and
provenance attestations over the exact published artifacts.

### F6 (Informational) — Toolchain bootstrap pulls unpinned tools
`cargo install cargo-audit` and the `dtolnay/rust-toolchain` install fetch from crates.io
/ the toolchain channel at build time. Versions are pinned (`cargo-audit 0.22.2 --locked`,
toolchain `1.96.0`) but not hash-pinned. Acceptable for this project; only relevant if you
later want fully reproducible, network-isolated builds.

### F7 (Informational) — Optional Windows signing via `cmd.exe` string interpolation
`Invoke-TypeTextOptionalSigning` builds a command by substituting `{file}` into
`$env:TYPETEXT_SIGNTOOL_COMMAND` and runs it through `cmd.exe /c`. This is operator-
controlled input (a CI/maintainer-set env var), not attacker input, so it is not a
vulnerability. Per your note, no Windows release-signing change is suggested.

## Things that are already done well

- `typetext-core` validates all loaded data: byte-size caps on files
  (`read_limited` with a `Take` guard), group/snippet/title/body limits, UTF-8 enforcement,
  and overflow-checked counting.
- Atomic, durable saves: unique temp file via `create_new`, `sync_all`, then `rename`,
  with cleanup on every error path.
- Token expansion walks the string on UTF-8 char boundaries and treats unknown tokens as
  literal — no format-string or interpolation surprises.
- The updater never downloads, executes, or replaces the running app; it only opens a
  validated link and displays the expected SHA-256.
- Windows `type_text` re-checks the foreground window before every character and refuses to
  type if focus moved — prevents leaking snippet text into the wrong window.
- DropText INI/CSV parser is hand-written, bounded, and returns errors rather than
  panicking on malformed input.

## Suggested priority

1. Resolve the `Joshndroid` vs `fruitmac` repo-slug discrepancy (F1).
2. Optionally pin the update URL path in `validate_update_url` (F2).
3. Optionally address F3/F4 if you want to tighten the macOS path and fail-closed behavior.

None of these block a release.
