#!/usr/bin/env bash
# Applies crates/nexo-app/migrations/0001_init.sql to a disposable PostgreSQL
# instance and asserts the invariants docs/APPLICATION_LAYER_CONTRACT.md
# claims for it: a full happy-path case-to-preparation flow commits, and
# four specific violations are rejected by the schema itself (not by
# application code that does not exist yet).
#
# Requires docker. Safe to run repeatedly; the container is disposable and
# torn down on exit regardless of pass/fail.
set -euo pipefail

CONTAINER_NAME="nexo-pg-schema-test"
PORT=55433
DB=nexo_schema_test

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

psql_q() {
  psql -h 127.0.0.1 -p "$PORT" -U postgres -d "$DB" -v ON_ERROR_STOP=1 "$@"
}

echo "== applying migration =="
psql_q -f "$(dirname "$0")/../crates/nexo-app/migrations/0001_init.sql"

echo "== happy path: case -> artifact -> observation -> bundle -> claim -> route -> evaluation -> preparation =="
psql_q <<'SQL'
begin;
insert into actors (external_identity) values ('anna');
insert into cases (owner_actor_id) values (1);
insert into digests (algorithm, hex) values ('sha256', repeat('ab', 32));
insert into provenance_records (case_id, channel, actor_id, recorded_at) values (1, 'user_provided', 1, now());
insert into ingestion_records (case_id, received_at, declared_filename, declared_mime, byte_size)
  values (1, now(), 'evidence.pdf', 'application/pdf', 1024);
insert into case_nodes (case_id, node_id, kind) values (1, 1, 'artifact');
insert into artifact_nodes (case_id, node_id, digest_id, size_bytes, ingestion_record_id, source_provenance_id)
  values (1, 1, 1, 1024, 1, 1);
insert into tools (name) values ('nexo-ocr');
insert into tool_versions (tool_id, version) values (1, 1);
insert into case_nodes (case_id, node_id, kind) values (1, 2, 'observation');
insert into observation_nodes (case_id, node_id, artifact_case_id, artifact_node_id, extractor_tool_version, locator, recorded_at)
  values (1, 2, 1, 1, 1, 'page:1,line:4', now());
insert into digests (algorithm, hex) values ('sha256', repeat('cd', 32));
insert into provenance_records (case_id, channel, actor_id, recorded_at) values (1, 'web_fetch', 1, now());
insert into normative_sources (authority_kind, acquisition_channel, issuer, locator, retrieved_at, captured_artifact_digest_id, digest_id, provenance_id)
  values ('primary_official', 'web_fetch', 'Boletin Oficial', 'https://example.gov.ar/ley', now(), 2, 2, 2);
insert into digests (algorithm, hex) values ('sha256', repeat('ef', 32));
insert into provenance_records (case_id, channel, actor_id, recorded_at) values (1, 'imported_bundle', 1, now());
insert into policy_bundles (jurisdiction, schema_version, policy_version, validity_from, captured_artifact_digest_id, digest_id, provenance_id)
  values ('AR', 1, '2026.1', '2026-01-01', 3, 3, 3);
insert into policy_bundle_activations (policy_bundle_id, activated_by_actor_id) values (1, 1);
insert into normative_claims (policy_bundle_id, proposition, jurisdiction, validity_from)
  values (1, 'right to request data access', 'AR', '2026-01-01');
insert into normative_claim_sources (claim_id, source_id, role, ordinal) values (1, 1, 'primary', 0);
insert into action_routes (policy_bundle_id, jurisdiction, title)
  values (1, 'AR', 'request personal data access');
insert into action_route_claims (route_id, claim_id, ordinal) values (1, 1, 0);
insert into action_evaluations (case_id, route_id, policy_bundle_id, evaluator_version, result_kind, action_status, result_schema_version, result_payload)
  values (1, 1, 1, '0.1.0', 'actionable', 'supported', 1, '{"factual_support":[{"artifact":1}]}');
insert into preparations (action_evaluation_id, kind) values (1, 'draft_request');
commit;
SQL

expect_failure() {
  local label="$1"
  shift
  if psql_q "$@" >/tmp/nexo_schema_test_out 2>&1; then
    echo "FAIL: expected '$label' to be rejected by the schema, but it succeeded" >&2
    cat /tmp/nexo_schema_test_out >&2
    exit 1
  fi
  echo "ok: $label was rejected as expected"
}

expect_failure "preparation against a non-actionable evaluation" <<'SQL'
insert into action_evaluations (case_id, route_id, policy_bundle_id, evaluator_version, result_kind, non_actionable_variant, result_schema_version, result_payload)
  values (1, 1, 1, '0.1.0', 'non_actionable', 'insufficient_facts', 1, '{"missing":["proof_of_identity"]}');
insert into preparations (action_evaluation_id, kind) values (2, 'draft_request');
SQL

expect_failure "actionable result with a non_actionable_variant set" <<'SQL'
insert into action_evaluations (case_id, route_id, policy_bundle_id, evaluator_version, result_kind, action_status, non_actionable_variant, result_schema_version, result_payload)
  values (1, 1, 1, '0.1.0', 'actionable', 'supported', 'abstain', 1, '{}');
SQL

expect_failure "observation referencing an artifact node id from another case" <<'SQL'
insert into case_nodes (case_id, node_id, kind) values (1, 99, 'observation');
insert into observation_nodes (case_id, node_id, artifact_case_id, artifact_node_id, extractor_tool_version, locator, recorded_at)
  values (1, 99, 1, 999, 1, 'x', now());
SQL

expect_failure "tool version zero" <<'SQL'
insert into tool_versions (tool_id, version) values (1, 0);
SQL

echo "== all schema invariants verified =="
