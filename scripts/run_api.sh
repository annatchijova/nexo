#!/usr/bin/env bash
# Runs nexo-api against a disposable local PostgreSQL for manual testing.
# Not for production use — see docs/API_CONTRACT.md and plan.md Step 9 for
# the real personal-deployment story (persistent database, TLS, backups).
set -euo pipefail
cd "$(dirname "$0")/.."

CONTAINER_NAME="nexo-pg-run-api"
PORT=55436
DB=nexo_dev

cleanup() {
  docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
docker run -d --name "$CONTAINER_NAME" \
  -e POSTGRES_PASSWORD=nexo -e POSTGRES_DB="$DB" \
  -p "${PORT}:5432" postgres:16 >/dev/null

export PGPASSWORD=nexo
for _ in $(seq 1 30); do
  psql -h 127.0.0.1 -p "$PORT" -U postgres -d "$DB" -c 'select 1' >/dev/null 2>&1 && break
  sleep 1
done

if ! docker image inspect nexo-extractor-plaintext:local >/dev/null 2>&1; then
  ./scripts/build_extractors.sh
fi

export DATABASE_URL="postgres://postgres:nexo@127.0.0.1:${PORT}/${DB}"
: "${NEXO_BOOTSTRAP_OWNER:?NEXO_BOOTSTRAP_OWNER must be set; it is never printed by this script}"
export NEXO_OBJECT_STORE_ROOT="${NEXO_OBJECT_STORE_ROOT:-./.dev-data/objects}"
export NEXO_BIND_ADDR="${NEXO_BIND_ADDR:-127.0.0.1:8080}"
export RUST_LOG="${RUST_LOG:-nexo_api=debug,tower_http=debug}"

echo "== listening on ${NEXO_BIND_ADDR} =="
cargo run -p nexo-api
