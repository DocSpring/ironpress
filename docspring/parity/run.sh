#!/usr/bin/env bash
# Run a command in the parity suite's runtime (docspring/parity/Dockerfile), with this checkout at /src.
#
#   docspring/parity/run.sh scripts/parity.sh
#   docspring/parity/run.sh env PARITY_ONLY=inline-block cargo test --test feature_parity -- --ignored --nocapture
#   docspring/parity/run.sh scripts/parity-gen-refs.sh layout
#
# It needs an amd64 Linux host with Docker (docspring-dev). The build cache and cargo registry live in volumes.
# The container gets 8 CPUs (PARITY_CPUS to change it): docspring-dev's host resets under sustained all-core load.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE=ironpress-parity

docker build --quiet --tag "$IMAGE" --file "$ROOT/docspring/parity/Dockerfile" "$ROOT" >/dev/null
exec docker run --rm --init \
  --cpus "${PARITY_CPUS:-8}" \
  --volume "$ROOT:/src" \
  --volume ironpress-parity-target:/src/target \
  --volume ironpress-parity-cargo:/usr/local/cargo/registry \
  --workdir /src \
  --env FONTCONFIG_FILE=/src/tests/parity/fonts/fonts.conf \
  --env PARITY_PDFTOPPM=/usr/bin/pdftoppm \
  --env CARGO_TERM_COLOR=never \
  "$IMAGE" "$@"
