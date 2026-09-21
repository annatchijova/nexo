#!/usr/bin/env bash
# Builds every sandboxed extractor image from source: a static musl binary
# per extractor, packaged into a `FROM scratch` image. Run this before any
# test or code path that calls nexo-sandbox::run_extraction with a real
# extractor image, and before building the test-only hostile images
# nexo-sandbox's own test suite depends on.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== nexo-extractor-plaintext =="
cargo build --release --target x86_64-unknown-linux-musl -p nexo-extractor-plaintext
docker build -t nexo-extractor-plaintext:local -f crates/nexo-extractor-plaintext/Dockerfile .

echo "== test-only hostile images (nexo-sandbox's own test suite) =="
docker build -t nexo-sandbox-test-sleep:local crates/nexo-sandbox/testdata/hostile-sleep

echo "== done =="
docker images --filter "reference=nexo-*"
