#!/usr/bin/env bash
# Runs nexo-app's repository integration tests against a disposable
# PostgreSQL instance: applies the versioned migrations exactly once, then
# runs the test suite (which connects many times, concurrently, the same
# way a real deployment would after its own one-time migration).
set -euo pipefail
cd "$(dirname "$0")/.."

CONTAINER_NAME="nexo-pg-repository-test"
PORT=55434
DB=nexo_repository_test

cleanup() {
  docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
docker run -d --name "$CONTAINER_NAME" \
  -e POSTGRES_PASSWORD=nexo -e POSTGRES_DB="$DB" \
  -p "${PORT}:5432" postgres:16 >/dev/null

export PGPASSWORD=nexo
ready=0
for _ in $(seq 1 30); do
  if psql -h 127.0.0.1 -p "$PORT" -U postgres -d "$DB" -c 'select 1' >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done
if [ "$ready" -ne 1 ]; then
  echo "FAIL: postgres did not become ready" >&2
  exit 1
fi

echo "== applying migration =="
psql -h 127.0.0.1 -p "$PORT" -U postgres -d "$DB" -v ON_ERROR_STOP=1 \
  -f crates/nexo-app/migrations/0001_init.sql >/dev/null
psql -h 127.0.0.1 -p "$PORT" -U postgres -d "$DB" -v ON_ERROR_STOP=1 \
  -f crates/nexo-app/migrations/0002_audit_chain_v1.sql >/dev/null

export DATABASE_URL="postgres://postgres:nexo@127.0.0.1:${PORT}/${DB}"
echo "== running repository integration tests =="
cargo test -p nexo-app --test repository_test -- --test-threads=8
