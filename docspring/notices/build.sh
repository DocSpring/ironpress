#!/usr/bin/env bash
# Writes THIRD_PARTY_NOTICES: the licenses of the crates the WebAssembly build links (cargo-about, with this folder's
# about.toml and notices.hbs) and of the Liberation fonts it embeds. scripts/prepare-wasm-package.mjs puts it in the npm
# package. Run it after changing dependencies or Cargo.lock; CI fails when the file is out of date (--check).
#
#   cargo install cargo-about --locked --version 0.8.4
#   docspring/notices/build.sh [--check]
set -euo pipefail

cd "$(dirname "$0")/../.."
output="$(mktemp)"
trap 'rm -f "$output"' EXIT

cargo about generate --config docspring/notices/about.toml --locked \
  --no-default-features --features wasm --target wasm32-unknown-unknown \
  docspring/notices/notices.hbs -o "$output"
{
  printf '\n================================================================================\n'
  printf 'SIL Open Font License 1.1 (OFL-1.1)\n\nUsed by:\n- Liberation Sans, Serif and Mono (assets/Liberation*.ttf)\n\n'
  cat assets/LICENSE-LiberationFonts
} >>"$output"

if [[ "${1:-}" == "--check" ]]; then
  diff -u THIRD_PARTY_NOTICES "$output" || {
    echo "THIRD_PARTY_NOTICES is out of date: run docspring/notices/build.sh" >&2
    exit 1
  }
else
  cp "$output" THIRD_PARTY_NOTICES
fi
