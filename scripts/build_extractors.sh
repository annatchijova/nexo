#!/usr/bin/env bash
# Builds every sandboxed extractor image from source: a static musl binary
# per extractor, packaged into a `FROM scratch` image. Run this before any
# test or code path that calls nexo-sandbox::run_extraction with a real
# extractor image, and before building the test-only hostile images
# nexo-sandbox's own test suite depends on.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== musl target (idempotent; required by every cargo build below) =="
rustup target add x86_64-unknown-linux-musl

echo "== nexo-extractor-plaintext =="
cargo build --release --target x86_64-unknown-linux-musl -p nexo-extractor-plaintext
docker build -t nexo-extractor-plaintext:local -f crates/nexo-extractor-plaintext/Dockerfile .

echo "== nexo-extractor-eml =="
cargo build --release --target x86_64-unknown-linux-musl -p nexo-extractor-eml
docker build -t nexo-extractor-eml:local -f crates/nexo-extractor-eml/Dockerfile .

echo "== nexo-extractor-pdf =="
cargo build --release --target x86_64-unknown-linux-musl -p nexo-extractor-pdf
docker build -t nexo-extractor-pdf:local -f crates/nexo-extractor-pdf/Dockerfile .

echo "== test-only hostile images (nexo-sandbox's own test suite) =="
docker build -t nexo-sandbox-test-sleep:local crates/nexo-sandbox/testdata/hostile-sleep

echo "== done =="
docker images --filter "reference=nexo-*"
