-- NEXO application-layer schema.
-- Implements docs/APPLICATION_LAYER_CONTRACT.md. Every table here durably
-- records state that nexo-core already validated in memory; it does not
-- re-derive or loosen any nexo-core invariant.

-- ---------------------------------------------------------------------
-- Actors and cases
-- ---------------------------------------------------------------------

create table actors (
    id                  bigint generated always as identity primary key,
    external_identity   text not null unique,
    created_at          timestamptz not null default now()
);

create table cases (
    id                  bigint generated always as identity primary key,
    owner_actor_id      bigint not null references actors (id),
    created_at          timestamptz not null default now()
);

create index cases_owner_actor_id_idx on cases (owner_actor_id);

-- ---------------------------------------------------------------------
-- Content digests, ingestion, provenance
-- ---------------------------------------------------------------------

create table digests (
    id                  bigint generated always as identity primary key,
    algorithm           text not null,
    hex                 text not null,
    unique (algorithm, hex)
);

create type acquisition_channel as enum (
    'web_fetch',
    'official_api',
    'user_provided',
    'research_connector',
    'imported_bundle'
);

create table provenance_records (
    id                  bigint generated always as identity primary key,
    case_id             bigint not null references cases (id),
    channel             acquisition_channel not null,
    actor_id            bigint references actors (id),
    recorded_at         timestamptz not null,
    detail              jsonb not null default '{}'::jsonb
);

create index provenance_records_case_id_idx on provenance_records (case_id);

create type sandbox_status as enum (
    'pending',
    'accepted',
    'rejected'
);

-- Declared (untrusted) ingestion metadata, kept separate from any sandbox
-- worker extraction result per docs/SANDBOX.md: nothing here is parsed by
-- the application process, only recorded.
create table ingestion_records (
    id                  bigint generated always as identity primary key,
    case_id             bigint not null references cases (id),
    received_at         timestamptz not null,
    declared_filename   text,
    declared_mime       text,
    byte_size           bigint not null check (byte_size >= 0),
    status              sandbox_status not null default 'pending'
);

create index ingestion_records_case_id_idx on ingestion_records (case_id);

-- ---------------------------------------------------------------------
-- Tools and versions (ToolVersion: version zero is unrepresentable)
-- ---------------------------------------------------------------------

create table tools (
    id                  bigint generated always as identity primary key,
    name                text not null unique
);

create table tool_versions (
    id                  bigint generated always as identity primary key,
    tool_id             bigint not null references tools (id),
    version             bigint not null check (version > 0),
    unique (tool_id, version)
);

-- ---------------------------------------------------------------------
-- Case graph: node kind discriminator plus one payload table per kind.
-- NodeId is scoped per case and assigned sequentially starting at 1,
-- matching nexo_core::CaseGraph. case_nodes is the sole id-allocation
-- source of truth; see docs/APPLICATION_LAYER_CONTRACT.md transaction
-- contract item 2.
-- ---------------------------------------------------------------------

create type case_node_kind as enum (
    'artifact',
    'observation',
    'user_assertion',
    'derived_fact',
    'inference'
);

create table case_nodes (
    case_id             bigint not null references cases (id),
    node_id             bigint not null check (node_id > 0),
    kind                case_node_kind not null,
    created_at          timestamptz not null default now(),
    primary key (case_id, node_id)
);

create table artifact_nodes (
    case_id                 bigint not null,
    node_id                 bigint not null,
    digest_id               bigint not null references digests (id),
    size_bytes              bigint not null check (size_bytes >= 0),
    ingestion_record_id     bigint not null references ingestion_records (id),
    source_provenance_id    bigint not null references provenance_records (id),
    primary key (case_id, node_id),
    foreign key (case_id, node_id) references case_nodes (case_id, node_id)
);

create table observation_nodes (
    case_id                 bigint not null,
    node_id                 bigint not null,
    artifact_case_id        bigint not null,
    artifact_node_id        bigint not null,
    extractor_tool_version  bigint not null references tool_versions (id),
    locator                 text not null check (locator <> ''),
    recorded_at             timestamptz not null,
    primary key (case_id, node_id),
    foreign key (case_id, node_id) references case_nodes (case_id, node_id),
    -- An Observation references exactly one Artifact in the same case
    -- (docs/CASE_GRAPH_CONTRACT.md reference rule 2).
    check (artifact_case_id = case_id),
    foreign key (artifact_case_id, artifact_node_id) references artifact_nodes (case_id, node_id)
);

create type confirmation_state as enum ('confirmed', 'unconfirmed');

create table user_assertion_nodes (
    case_id             bigint not null,
    node_id             bigint not null,
    actor_id            bigint not null references actors (id),
    recorded_at         timestamptz not null,
    confirmation        confirmation_state not null,
    primary key (case_id, node_id),
    foreign key (case_id, node_id) references case_nodes (case_id, node_id)
);

create table derived_fact_nodes (
    case_id                     bigint not null,
    node_id                     bigint not null,
    transformation_tool_version bigint not null references tool_versions (id),
    primary key (case_id, node_id),
    foreign key (case_id, node_id) references case_nodes (case_id, node_id)
);

-- Ordered inputs; row-count bound (MAX_DERIVATION_INPUTS = 64) is enforced
-- at the application layer since a bare CHECK cannot count sibling rows.
create table derived_fact_inputs (
    case_id             bigint not null,
    node_id             bigint not null,
    ordinal             integer not null check (ordinal >= 0),
    input_node_id       bigint not null,
    primary key (case_id, node_id, ordinal),
    foreign key (case_id, node_id) references derived_fact_nodes (case_id, node_id),
    foreign key (case_id, input_node_id) references case_nodes (case_id, node_id)
);

create type confidence_bound as enum ('low', 'medium', 'high');

create table inference_nodes (
    case_id             bigint not null,
    node_id             bigint not null,
    method_tool_version bigint not null references tool_versions (id),
    confidence          confidence_bound not null,
    primary key (case_id, node_id),
    foreign key (case_id, node_id) references case_nodes (case_id, node_id)
);

-- MAX_INFERENCE_INPUTS = 64, enforced at the application layer.
create table inference_inputs (
    case_id             bigint not null,
    node_id             bigint not null,
    ordinal             integer not null check (ordinal >= 0),
    input_node_id       bigint not null,
    primary key (case_id, node_id, ordinal),
    foreign key (case_id, node_id) references inference_nodes (case_id, node_id),
    foreign key (case_id, input_node_id) references case_nodes (case_id, node_id)
);

-- ---------------------------------------------------------------------
-- Policy: bundles, activations, normative sources and claims.
-- Mirrors docs/POLICY_BUNDLE_CONTRACT.md field-for-field.
-- ---------------------------------------------------------------------

create type authority_kind as enum (
    'primary_official',
    'official_interpretive',
    'secondary_analysis',
    'unverified'
);

create table normative_sources (
    id                      bigint generated always as identity primary key,
    authority_kind          authority_kind not null,
    acquisition_channel     acquisition_channel not null,
    issuer                  text not null check (issuer <> ''),
    locator                 text not null check (locator <> ''),
    retrieved_at            timestamptz not null,
    captured_artifact_digest_id bigint not null references digests (id),
    digest_id               bigint not null references digests (id),
    provenance_id           bigint not null references provenance_records (id)
);

create table policy_bundles (
    id                      bigint generated always as identity primary key,
    jurisdiction            text not null,
    schema_version          smallint not null check (schema_version >= 0),
    policy_version           text not null check (policy_version <> ''),
    validity_from            date not null,
    validity_to              date,
    captured_artifact_digest_id bigint not null references digests (id),
    digest_id                bigint not null references digests (id),
    provenance_id             bigint not null references provenance_records (id),
    check (validity_to is null or validity_to >= validity_from)
);

create index policy_bundles_jurisdiction_idx on policy_bundles (jurisdiction);

-- Append-only: a bundle is immutable once activated, and history is never
-- overwritten with a "current pointer" (transaction contract item 4).
create table policy_bundle_activations (
    id                  bigint generated always as identity primary key,
    policy_bundle_id    bigint not null references policy_bundles (id),
    activated_at        timestamptz not null default now(),
    activated_by_actor_id bigint not null references actors (id)
);

create index policy_bundle_activations_bundle_idx
    on policy_bundle_activations (policy_bundle_id);

create table normative_claims (
    id                  bigint generated always as identity primary key,
    policy_bundle_id    bigint not null references policy_bundles (id),
    proposition         text not null check (proposition <> ''),
    jurisdiction         text not null check (jurisdiction <> ''),
    validity_from        date not null,
    validity_to          date,
    check (validity_to is null or validity_to >= validity_from)
);

create index normative_claims_bundle_idx on normative_claims (policy_bundle_id);

create type support_role as enum ('primary', 'corroborating');

-- A claim requires at least one source row; enforced at the application
-- layer at claim-insert time (a bare foreign key cannot express "non-empty").
create table normative_claim_sources (
    claim_id            bigint not null references normative_claims (id),
    source_id           bigint not null references normative_sources (id),
    role                support_role not null,
    ordinal             integer not null check (ordinal >= 0),
    primary key (claim_id, source_id)
);

-- ---------------------------------------------------------------------
-- Action routes and evaluations
-- ---------------------------------------------------------------------

create table action_routes (
    id                  bigint generated always as identity primary key,
    policy_bundle_id    bigint not null references policy_bundles (id),
    jurisdiction         text not null,
    title                text not null check (title <> '')
);

create table action_route_claims (
    route_id            bigint not null references action_routes (id),
    claim_id             bigint not null references normative_claims (id),
    ordinal              integer not null check (ordinal >= 0),
    primary key (route_id, claim_id)
);

create table action_route_requirements (
    id                  bigint generated always as identity primary key,
    route_id             bigint not null references action_routes (id),
    description           text not null check (description <> ''),
    ordinal                integer not null check (ordinal >= 0),
    unique (route_id, ordinal)
);

create type evaluation_result_kind as enum ('actionable', 'non_actionable');

create type action_status as enum ('supported', 'conditionally_supported');

create type non_actionable_variant as enum (
    'insufficient_facts',
    'contraindicated',
    'out_of_jurisdiction',
    'policy_not_current',
    'abstain'
);

-- The full typed ActionEvaluation is sealed as a canonical, versioned JSON
-- payload rather than re-normalized field by field: nexo-core's evaluator
-- already produced a closed, immutable result, and this layer's job is to
-- keep that result intact, not to reinterpret it in a second schema. See
-- docs/APPLICATION_LAYER_CONTRACT.md, "What is normalized vs. sealed".
create table action_evaluations (
    id                       bigint generated always as identity primary key,
    case_id                  bigint not null references cases (id),
    route_id                 bigint not null references action_routes (id),
    policy_bundle_id         bigint not null references policy_bundles (id),
    evaluated_at             timestamptz not null default now(),
    evaluator_version        text not null check (evaluator_version <> ''),
    result_kind              evaluation_result_kind not null,
    action_status            action_status,
    non_actionable_variant   non_actionable_variant,
    result_schema_version    smallint not null check (result_schema_version >= 0),
    result_payload           jsonb not null,
    check (
        (result_kind = 'actionable'
            and action_status is not null
            and non_actionable_variant is null)
        or
        (result_kind = 'non_actionable'
            and action_status is null
            and non_actionable_variant is not null)
    )
);

create index action_evaluations_case_id_idx on action_evaluations (case_id);
create index action_evaluations_route_id_idx on action_evaluations (route_id);

-- ---------------------------------------------------------------------
-- Preparations (docs/ARCHITECTURE.md "Preparation boundary")
-- ---------------------------------------------------------------------

create type preparation_kind as enum (
    'draft_request',
    'evidence_package',
    'export'
);

create type preparation_status as enum ('prepared', 'exported', 'invalidated');

create table preparations (
    id                       bigint generated always as identity primary key,
    action_evaluation_id      bigint not null references action_evaluations (id),
    kind                       preparation_kind not null,
    status                      preparation_status not null default 'prepared',
    prepared_at                 timestamptz not null default now(),
    output_digest_id             bigint references digests (id),
    output_provenance_id          bigint references provenance_records (id),
    invalidation_reason            text
);

create index preparations_action_evaluation_idx
    on preparations (action_evaluation_id);

-- A preparation can only reference an actionable evaluation; a bare foreign
-- key cannot express that, so it is enforced with a trigger rather than a
-- second, looser copy of the evaluator's decision.
create function preparations_require_actionable_evaluation()
returns trigger as $$
declare
    kind evaluation_result_kind;
begin
    select result_kind into kind
    from action_evaluations
    where id = new.action_evaluation_id;

    if kind is distinct from 'actionable' then
        raise exception
            'preparation % references non-actionable action_evaluation %',
            new.id, new.action_evaluation_id;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger preparations_require_actionable_evaluation_trigger
    before insert or update on preparations
    for each row execute function preparations_require_actionable_evaluation();

-- ---------------------------------------------------------------------
-- Append-only audit log. The hash-chain itself (entry_hash covering the
-- previous entry_hash) is Step 2 / nexo-integrity's contract; this table
-- only guarantees one row per mutating command, in the same transaction.
-- ---------------------------------------------------------------------

create table audit_log (
    id                  bigint generated always as identity primary key,
    case_id              bigint references cases (id),
    actor_id              bigint not null references actors (id),
    occurred_at            timestamptz not null default now(),
    event_kind              text not null check (event_kind <> ''),
    event_payload             jsonb not null,
    previous_entry_hash        bytea,
    entry_hash                  bytea not null
);

create index audit_log_case_id_idx on audit_log (case_id);
create index audit_log_occurred_at_idx on audit_log (occurred_at);

-- No update or delete grants are defined here; enforcing true
-- application-level immutability (revoking UPDATE/DELETE from the runtime
-- role) belongs to the Step 9 deployment hardening pass, once the runtime
-- role itself is defined.
