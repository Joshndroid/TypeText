# TypeText Security Review — 2026-07-29

Scope: the full Rust workspace, desktop platform integrations, data handling,
Windows packaging and security-review PowerShell scripts, and GitHub Actions
workflows. This review updates the historical 2026-06-24 review after the
subsequent hardening and user-interface changes.

## Overall

No critical, high, or medium-severity security issue was identified in the
reviewed source. The earlier F1–F4 findings are resolved:

- The repository owner is consistently `Joshndroid/TypeText`.
- Update links must use HTTPS, `github.com`, no credentials or non-default
  port, and the `/Joshndroid/TypeText/` path prefix.
- macOS text insertion sends snippet content to `osascript` over standard input
  instead of exposing it in process arguments.
- portable path resolution no longer falls back to the current directory.

The earlier F5 predictable macOS lock-file note remains a very-low-risk local
denial-of-service consideration. The lock is stored in a per-user temporary
directory where supported; it does not expose snippet contents or grant access
to TypeText data.

## Data and runtime security

- Loaded JSON is size-limited, UTF-8 validated, schema-validated, and subject
  to bounded group, snippet, title, body, and token limits.
- Saves use uniquely created temporary files, flush data before replacement,
  and clean up failed temporary writes.
- Snippet data remains plaintext by design. TypeText documentation warns
  against storing secrets and recommends operating-system or full-disk
  encryption.
- Windows typing verifies the foreground window while inserting text to reduce
  accidental disclosure after focus changes.
- Import, export, and portable storage reject Windows UNC paths and mapped
  network drives.

## Updates and external capabilities

- Update metadata is fetched from the fixed GitHub releases API endpoint.
- Release and asset URLs are validated against the trusted repository path
  before TypeText offers to open them.
- TypeText does not automatically download, execute, or replace application
  binaries.
- The Windows offline-portable feature compile-time removes update,
  URL-opening, and startup-registration capabilities and forces the related
  settings off.

## Build, CI, and PowerShell assurance

GitHub Actions use pinned action revisions, restricted default permissions,
locked Rust dependencies, formatting checks, tests, Clippy with warnings
denied, RustSec auditing, platform packaging, release checksums, artifact
attestations, and Windows Defender scanning.

`Scripts/run-windows-security-review.ps1` now mirrors both relevant Rust
quality matrices before packaging:

1. normal/default-feature workspace tests;
2. normal/all-feature workspace Clippy with warnings denied;
3. offline-portable workspace tests; and
4. offline-portable workspace Clippy with warnings denied.

The harness also performs the RustSec audit when `cargo-audit` is installed,
invokes the locked Windows packaging pipeline, verifies standard/offline
capability-marker contrast, scans the PE import table, runs Microsoft Defender,
captures Process Monitor evidence, and behaviorally checks typing, token
expansion, settings persistence, network activity, Registry mutations, child
processes, and file mutations.

`-SkipBuild` remains available for reviewing a previously built executable.
Reports generated in that mode correctly mark the complete release pipeline as
requiring review rather than claiming it passed.

## Residual considerations

- The offline-portable remote-storage refusal is specifically a Windows
  guarantee; no offline-portable package is produced for macOS or Linux.
- Local users or processes with equivalent access can read plaintext TypeText
  data. This is an explicit product limitation, not a secrets vault.
- Windows signing remains optional and operator-controlled. Official release
  assurance depends on the release workflow and its configured signing
  credentials.

## Release assessment

The reviewed source and controls are suitable for release, subject to the
normal requirement that the relevant CI and release jobs pass for the exact
release commit and artifacts.
