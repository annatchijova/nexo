# Authenticated audit chain v2 and export v3 contract

`audit_chain/v2` is an explicit cutover from v1. The v3 migration refuses to
reinterpret a non-empty v1 chain. An operator must perform a reviewed
retention and cutover procedure before enabling v2 on a database with history.

## Authentication format

The structural event digest is `audit-chain-v2` and binds the chain identity,
version, sequence, predecessor digest, and complete canonical event. Each event
also carries an `entry_hmac` computed as HMAC-SHA-256 over a canonical record
containing:

- the full event;
- chain ID and chain version;
- sequence and predecessor digest;
- the structural entry digest;
- authentication scheme and public key version.

Each checkpoint has a separate HMAC over its chain ID/version, key version,
sequence, entry count, and tip digest. This makes both event records and the
retained completeness checkpoint key-bound.

The database stores only `auth_scheme`, public `hmac_key_version`, digests, and
HMAC tags. It never stores HMAC key material. Authentication is mandatory for
v2; unknown, missing, or mismatched key configuration fails closed. The
verifier never downgrades v2 to unauthenticated SHA-only verification.

## Key custody and rotation

The application receives the current key through `NEXO_AUDIT_HMAC_KEY` and its
public version through `NEXO_AUDIT_HMAC_KEY_VERSION`. Optional previous keys
for verification use `NEXO_AUDIT_HMAC_PREVIOUS_KEYS` with the format
`version=key;version=key`. Keys must be at least 32 bytes. These variables
describe delivery, not custody: production custody must be an operator-owned
secret manager or equivalent authority that is not shared with PostgreSQL.

During rotation, the new current key and the old key are both supplied. The
application switches the chain's future key version without rewriting prior
events; prior events retain their public version and HMAC. The old key remains
required until the retained history no longer needs verification.

No key, raw or derived, is allowed in events, exports, logs, SQL, or source
control. The verifier reports `hmac_checked` and `hmac_ok` separately.

## Export version and completeness witness

The authenticated chain remains `audit_chain/v2`; its materialized export is
`audit-export-v3`. Export-v3 adds an authenticated `chain_state` containing the
database chain's current sequence, tip digest, and tip HMAC. The independent
verifier requires that state to match the complete event list and checkpoint.
This detects removal of a database tail together with its latest checkpoint
when the chain row itself is retained. The older audit-export-v2 format remains
parseable for compatibility but is not reported as `complete_history`, because
it has no authenticated chain-state witness.

## Threat-model boundary

This version addresses a PostgreSQL-only compromise when the attacker cannot
read or use the application deployment's HMAC keyring. Such an attacker can
delete or alter local data, but cannot create a matching authenticated event or
checkpoint; verification rejects the result or fails closed.

This version does not protect against an attacker who controls the application,
its runtime environment, and the HMAC secret. That attacker can recompute the
authenticated history. Independent checkpoint retention and RFC3161/external
anchoring are deliberately deferred to a later version with an explicit
external authority; no local HMAC tag is presented as external evidence.
