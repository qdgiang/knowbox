# AGENTS.md

Rust workspace. The toolchain is pinned in `rust-toolchain.toml`; cargo picks it up by itself.

## First
- Load the `rust` skill before you write, change or review Rust here.
- `knowbox` is a binary for a Markdown knowledge base on Linux, native Windows, and macOS.
- Test commands against disposable roots only. Never point the development binary at the live `~/knowbox`.

## Agent note changes

Agents must make every note change through the `knowbox` CLI. Never create, edit,
or replace note files with file tools, shell commands, or an editor. Use
`knowbox new` to create notes and `knowbox edit` to change an existing claim
whose first sentence still holds. Use `knowbox touch`, `knowbox verify`,
`knowbox link` and `knowbox supersede OLD --with NEW` for their respective
changes. Use `knowbox verdict` and `knowbox raw-mark` for Review decisions.

Use `knowbox usage` when inspecting note opens or search outcomes. The
usage log and rollup are local data.

## Commands
- After each logical change: `cargo check -p knowbox --message-format=short`.
- One test: `cargo test -p <crate> <test_name>`. Narrowest test first; the whole suite only when shared
  code changed.
- Lints: `cargo clippy --fix --allow-dirty --allow-staged`, then fix the rest by hand.
- Before you finish: `scripts/gate.sh full` must pass. Every warning and failing test is yours.
- Long cargo output: redirect it to a file and read the file. Do not pipe it to `head` or `tail`.
- Never `cargo install`, `rustup default`, `rustup override`, or set `RUSTUP_TOOLCHAIN`. No `--release`
  builds unless asked.

## Dependencies
- Add with `cargo add <crate>`; it fails on an unknown crate or feature. Never hand-edit a version.
- Before you use a dependency's API, read the locked version, not your memory:
  `cargo info <crate>`, then
  `cargo metadata --format-version 1 | jq -r '.packages[] | select(.name == "<crate>") | .manifest_path'`
  and read `src/` beside that manifest.
- Lockfile: `cargo update -p <crate> --precise <version>` only. Never a bare `cargo update`.
- Rust itself: `rustc --explain E0xxx` for an error code, `cargo clippy --explain <lint>` for a lint;
  std's source is under `$(rustc --print sysroot)/lib/rustlib/src/rust/library/`.
- Prefer std and the crates already in the workspace.

## Keep the starting project simple
- Use ordinary dependencies and runtime configuration by default. Database or web API support does
  not need its own Cargo feature flag.
- Do not introduce project feature flags, extra crates, or abstractions for hypothetical future needs.
  Add them only when a concrete requirement justifies the complexity. Enabling a dependency's existing
  features is fine when needed.
- The gate checks default features on the current platform. Add feature or platform checks only when
  the project actually promises those configurations.

## Gate files
Do not edit `scripts/gate.sh`, `deny.toml`, `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`, `.cargo/`
or the `[workspace.lints]` and `[lints]` tables in `Cargo.toml`. Do not add `#[ignore]` or a crate-level
`#![allow]`, and do not delete tests. If a rule blocks the task, or a test contradicts the spec, stop and
say so.

## Snapshots (only if the project uses insta)
- After `cargo insta test`, read every `.snap.new` before `cargo insta accept`. Never accept a changed
  snapshot you cannot explain.

## Claude Code only
- The first `LSP` query of a session can come back empty while rust-analyzer indexes: retry once.
- Files changed by Bash or git are not announced to the LSP: run `cargo check` after such changes.
- Subagents have no `LSP` tool: they use `cargo check`.
