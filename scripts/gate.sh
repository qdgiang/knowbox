#!/bin/sh
# gate.sh file <path> | full: this repo's verification gate (gate convention: kb-0175). Silent on pass.
set -eu
cd "$(dirname "$0")/.."
case "${1:-}" in
  file) ;; # no per-file check: rust-analyzer and `cargo check` cover single edits
  full)
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -q -- -D warnings
    INSTA_UPDATE=no cargo test --workspace --locked -q
    cargo deny --workspace --locked check
    ;;
  *) echo "usage: gate.sh file <path> | full" >&2; exit 2 ;;
esac
