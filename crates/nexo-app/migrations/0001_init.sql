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
    algorithm           text not null default 'sha256',
    hex                 text not null,
    unique (algorithm, hex),
    check (
        algorithm = 'sha256'
        and hex ~ '^[0-9a-f]{64}$'
    )
);

create type acquisition_channel as enum (
    'web_fetch',
    'official_api',
    'user_provided',
    'research_connector',
    'imported_bundle',
    'generated_preparation'
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

create function normative_source_digest_matches_capture()
returns trigger as $$
begin
    if new.digest_id is distinct from new.captured_artifact_digest_id then
        raise exception
            'normative source % digest must match its captured artifact digest',
            new.id;
    end if;
    return new;
end;
$$ language plpgsql;

create trigger normative_source_digest_matches_capture_trigger
    before insert or update on normative_sources
    for each row execute function normative_source_digest_matches_capture();

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

create function policy_bundle_digest_matches_capture()
returns trigger as $$
begin
    if new.digest_id is distinct from new.captured_artifact_digest_id then
        raise exception
            'policy bundle % digest must match its captured artifact digest',
            new.id;
    end if;
    return new;
end;
$$ language plpgsql;

create trigger policy_bundle_digest_matches_capture_trigger
    before insert or update on policy_bundles
    for each row execute function policy_bundle_digest_matches_capture();

create index policy_bundles_jurisdiction_idx on policy_bundles (jurisdiction);

-- Append-only: a bundle is immutable once activated, and history is never
-- overwritten with a "current pointer" (transaction contract item 4).
create table policy_bundle_activations (
    id                  bigint generated always as identity primary key,
    policy_bundle_id    bigint not null references policy_bundles (id),
    activated_at        timestamptz not null default now(),
    activated_by_actor_id bigint not null references actors (id),
    unique (policy_bundle_id)
);

create index policy_bundle_activations_bundle_idx
    on policy_bundle_activations (policy_bundle_id);

create function policy_bundle_activations_are_append_only()
returns trigger as $$
begin
    raise exception 'policy bundle activations are append-only';
end;
$$ language plpgsql;

create trigger policy_bundle_activations_are_append_only_trigger
    before update or delete on policy_bundle_activations
    for each row execute function policy_bundle_activations_are_append_only();

-- Activation freezes the exact policy identity used by later evaluations and
-- receipts. Without this trigger, an UPDATE could silently rewrite the
-- policy behind an already-issued binding evidence row.
create function activated_policy_bundle_is_immutable()
returns trigger as $$
begin
    if exists (
        select 1
        from policy_bundle_activations
        where policy_bundle_id = old.id
    ) then
        raise exception
            'activated policy bundle % is immutable',
            old.id;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger activated_policy_bundle_is_immutable_trigger
    before update on policy_bundles
    for each row execute function activated_policy_bundle_is_immutable();

create table normative_claims (
    id                  bigint generated always as identity primary key,
    policy_bundle_id    bigint not null references policy_bundles (id),
    proposition         text not null check (proposition <> ''),
    jurisdiction         text not null check (jurisdiction <> ''),
    validity_from        date not null,
    validity_to          date,
    check (validity_to is null or validity_to >= validity_from)
);

create function normative_claim_matches_bundle_jurisdiction()
returns trigger as $$
declare
    bundle_jurisdiction text;
begin
    select jurisdiction into bundle_jurisdiction
    from policy_bundles
    where id = new.policy_bundle_id;

    if bundle_jurisdiction is null
       or bundle_jurisdiction is distinct from new.jurisdiction then
        raise exception
            'normative claim % does not match policy bundle jurisdiction',
            new.id;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger normative_claim_matches_bundle_jurisdiction_trigger
    before insert or update on normative_claims
    for each row execute function normative_claim_matches_bundle_jurisdiction();

create function activated_normative_claim_is_immutable()
returns trigger as $$
begin
    if exists (
        select 1
        from policy_bundle_activations
        where policy_bundle_id = old.policy_bundle_id
    ) then
        raise exception
            'normative claim % in an activated policy bundle is immutable',
            old.id;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger activated_normative_claim_is_immutable_trigger
    before update or delete on normative_claims
    for each row execute function activated_normative_claim_is_immutable();

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

create function activated_claim_source_edge_is_immutable()
returns trigger as $$
begin
    if exists (
        select 1
        from policy_bundle_activations activation
        join normative_claims claim
          on claim.policy_bundle_id = activation.policy_bundle_id
        where claim.id = coalesce(old.claim_id, new.claim_id)
    ) then
        raise exception
            'sources of claim % in an activated policy bundle are immutable',
            coalesce(old.claim_id, new.claim_id);
    end if;

    return coalesce(new, old);
end;
$$ language plpgsql;

create trigger activated_claim_source_edge_is_immutable_trigger
    before insert or update or delete on normative_claim_sources
    for each row execute function activated_claim_source_edge_is_immutable();

create function activated_normative_source_is_immutable()
returns trigger as $$
begin
    if exists (
        select 1
        from policy_bundle_activations activation
        join normative_claims claim
          on claim.policy_bundle_id = activation.policy_bundle_id
        join normative_claim_sources claim_source
          on claim_source.claim_id = claim.id
        where claim_source.source_id = old.id
    ) then
        raise exception
            'normative source % used by an activated claim is immutable',
            old.id;
    end if;

    return old;
end;
$$ language plpgsql;

create trigger activated_normative_source_is_immutable_trigger
    before update or delete on normative_sources
    for each row execute function activated_normative_source_is_immutable();

-- ---------------------------------------------------------------------
-- Action routes and evaluations
-- ---------------------------------------------------------------------

create table action_routes (
    id                  bigint generated always as identity primary key,
    policy_bundle_id    bigint not null references policy_bundles (id),
    jurisdiction         text not null,
    title                text not null check (title <> '')
);

create function action_route_matches_bundle_jurisdiction()
returns trigger as $$
declare
    bundle_jurisdiction text;
begin
    select jurisdiction into bundle_jurisdiction
    from policy_bundles
    where id = new.policy_bundle_id;

    if bundle_jurisdiction is null
       or bundle_jurisdiction is distinct from new.jurisdiction then
        raise exception
            'action route % does not match policy bundle jurisdiction',
            new.id;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger action_route_matches_bundle_jurisdiction_trigger
    before insert or update on action_routes
    for each row execute function action_route_matches_bundle_jurisdiction();

create function activated_action_route_is_immutable()
returns trigger as $$
begin
    if exists (
        select 1
        from policy_bundle_activations activation
        join action_routes route on route.policy_bundle_id = activation.policy_bundle_id
        where route.id = coalesce(old.id, new.id)
    ) then
        raise exception
            'action route % in an activated policy bundle is immutable',
            coalesce(old.id, new.id);
    end if;

    return coalesce(new, old);
end;
$$ language plpgsql;

create trigger activated_action_route_is_immutable_trigger
    before update or delete on action_routes
    for each row execute function activated_action_route_is_immutable();

create table action_route_claims (
    route_id            bigint not null references action_routes (id),
    claim_id             bigint not null references normative_claims (id),
    ordinal              integer not null check (ordinal >= 0),
    primary key (route_id, claim_id)
);

-- A route's claims are interpreted under the route's policy bundle. A plain
-- pair of foreign keys would allow a claim from a different bundle, silently
-- mixing policy versions inside one action route.
create function action_route_claim_matches_bundle()
returns trigger as $$
declare
    route_bundle_id bigint;
    claim_bundle_id bigint;
begin
    select policy_bundle_id into route_bundle_id
    from action_routes
    where id = new.route_id;

    select policy_bundle_id into claim_bundle_id
    from normative_claims
    where id = new.claim_id;

    if route_bundle_id is null
       or claim_bundle_id is null
       or route_bundle_id is distinct from claim_bundle_id then
        raise exception
            'action route % claim % does not match route policy bundle',
            new.route_id, new.claim_id;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger action_route_claim_matches_bundle_trigger
    before insert or update on action_route_claims
    for each row execute function action_route_claim_matches_bundle();

create function activated_action_route_claims_are_immutable()
returns trigger as $$
begin
    if exists (
        select 1
        from policy_bundle_activations activation
        join action_routes route on route.policy_bundle_id = activation.policy_bundle_id
        where route.id = coalesce(old.route_id, new.route_id)
    ) then
        raise exception
            'claims of action route % in an activated policy bundle are immutable',
            coalesce(old.route_id, new.route_id);
    end if;

    return coalesce(new, old);
end;
$$ language plpgsql;

create trigger activated_action_route_claims_are_immutable_trigger
    before update or delete on action_route_claims
    for each row execute function activated_action_route_claims_are_immutable();

create table action_route_requirements (
    id                  bigint generated always as identity primary key,
    route_id             bigint not null references action_routes (id),
    description           text not null check (description <> ''),
    ordinal                integer not null check (ordinal >= 0),
    unique (route_id, ordinal)
);

create function activated_action_route_requirements_are_immutable()
returns trigger as $$
begin
    if exists (
        select 1
        from policy_bundle_activations activation
        join action_routes route
          on route.policy_bundle_id = activation.policy_bundle_id
        where route.id = coalesce(old.route_id, new.route_id)
    ) then
        raise exception
            'requirements of route % in an activated policy bundle are immutable',
            coalesce(old.route_id, new.route_id);
    end if;

    return coalesce(new, old);
end;
$$ language plpgsql;

create trigger activated_action_route_requirements_are_immutable_trigger
    before insert or update or delete on action_route_requirements
    for each row execute function activated_action_route_requirements_are_immutable();

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

-- The route carries the policy bundle that defined its claims and meaning.
-- Keep the duplicated bundle reference on an evaluation tied to that route;
-- otherwise a caller could seal a result under one policy while naming a
-- route from another policy.
create function action_evaluation_matches_route_bundle()
returns trigger as $$
declare
    route action_routes%rowtype;
begin
    select * into route
    from action_routes
    where id = new.route_id;

    if route.id is null
       or route.policy_bundle_id is distinct from new.policy_bundle_id then
        raise exception
            'action evaluation % does not match route % policy bundle',
            new.id, new.route_id;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger action_evaluation_matches_route_bundle_trigger
    before insert or update on action_evaluations
    for each row execute function action_evaluation_matches_route_bundle();

-- Binding evidence for a future preparation capability. This receipt is not
-- itself a VerifiedPreparationSnapshot; it records the durable relations the
-- application must prove before crossing into nexo-core preparation.
create table evaluation_receipts (
    id                         bigint generated always as identity primary key,
    action_evaluation_id      bigint not null unique references action_evaluations (id),
    case_id                   bigint not null references cases (id),
    action_route_id           bigint not null references action_routes (id),
    policy_bundle_id          bigint not null references policy_bundles (id),
    input_manifest_digest_id  bigint not null references digests (id),
    result_digest_id          bigint not null references digests (id),
    action_digest_id          bigint not null references digests (id),
    manifest_schema_version   smallint not null check (manifest_schema_version >= 0),
    result_schema_version     smallint not null check (result_schema_version >= 0),
    evaluator_version         text not null check (evaluator_version <> ''),
    recorded_at               timestamptz not null default now()
);

create index evaluation_receipts_case_id_idx on evaluation_receipts (case_id);

create function evaluation_receipt_matches_evaluation()
returns trigger as $$
declare
    evaluation action_evaluations%rowtype;
begin
    select * into evaluation
    from action_evaluations
    where id = new.action_evaluation_id;

    if evaluation.id is null
       or new.case_id <> evaluation.case_id
       or new.action_route_id <> evaluation.route_id
       or new.policy_bundle_id <> evaluation.policy_bundle_id
       or evaluation.result_kind <> 'actionable'
       or evaluation.action_status is distinct from 'supported'::action_status
       or new.result_schema_version <> evaluation.result_schema_version
       or new.evaluator_version <> evaluation.evaluator_version then
        raise exception
            'evaluation receipt does not match action evaluation %',
            new.action_evaluation_id;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger evaluation_receipt_matches_evaluation_trigger
    before insert or update on evaluation_receipts
    for each row execute function evaluation_receipt_matches_evaluation();

create function evaluation_receipts_are_immutable()
returns trigger as $$
begin
    if new.action_evaluation_id is distinct from old.action_evaluation_id
       or new.case_id is distinct from old.case_id
       or new.action_route_id is distinct from old.action_route_id
       or new.policy_bundle_id is distinct from old.policy_bundle_id
       or new.input_manifest_digest_id is distinct from old.input_manifest_digest_id
       or new.result_digest_id is distinct from old.result_digest_id
       or new.action_digest_id is distinct from old.action_digest_id
       or new.manifest_schema_version is distinct from old.manifest_schema_version
       or new.result_schema_version is distinct from old.result_schema_version
       or new.evaluator_version is distinct from old.evaluator_version
       or new.recorded_at is distinct from old.recorded_at then
        raise exception 'evaluation receipt % is immutable', old.id;
    end if;
    return new;
end;
$$ language plpgsql;

create trigger evaluation_receipts_are_immutable_trigger
    before update on evaluation_receipts
    for each row execute function evaluation_receipts_are_immutable();

create function receipted_action_evaluations_are_immutable()
returns trigger as $$
begin
    if exists (
        select 1
        from evaluation_receipts receipt
        where receipt.action_evaluation_id = old.id
    ) and (
        new.case_id is distinct from old.case_id
        or new.route_id is distinct from old.route_id
        or new.policy_bundle_id is distinct from old.policy_bundle_id
        or new.evaluated_at is distinct from old.evaluated_at
        or new.evaluator_version is distinct from old.evaluator_version
        or new.result_kind is distinct from old.result_kind
        or new.action_status is distinct from old.action_status
        or new.non_actionable_variant is distinct from old.non_actionable_variant
        or new.result_schema_version is distinct from old.result_schema_version
        or new.result_payload is distinct from old.result_payload
    ) then
        raise exception 'action evaluation % is immutable after receipt issuance', old.id;
    end if;
    return new;
end;
$$ language plpgsql;

create trigger receipted_action_evaluations_are_immutable_trigger
    before update on action_evaluations
    for each row execute function receipted_action_evaluations_are_immutable();

create function invalidate_preparations_on_receipt_delete()
returns trigger as $$
begin
    update preparations
    set status = 'invalidated'::preparation_status,
        invalidation_reason = 'evaluation receipt removed'
    where action_evaluation_id = old.action_evaluation_id
      and status <> 'invalidated'::preparation_status;
    return old;
end;
$$ language plpgsql;

create trigger invalidate_preparations_on_receipt_delete_trigger
    after delete on evaluation_receipts
    for each row execute function invalidate_preparations_on_receipt_delete();

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
    action_route_id           bigint not null references action_routes (id),
    policy_bundle_digest_id   bigint not null references digests (id),
    input_manifest_digest_id  bigint not null references digests (id),
    generator_version         text not null check (generator_version <> ''),
    kind                       preparation_kind not null,
    status                      preparation_status not null default 'prepared',
    prepared_at                 timestamptz not null default now(),
    output_digest_id             bigint not null references digests (id),
    output_provenance_id          bigint not null references provenance_records (id),
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
    status action_status;
    has_receipt boolean;
    evaluation_case_id bigint;
    provenance_case_id bigint;
    receipt_route_id bigint;
    receipt_bundle_id bigint;
    receipt_input_manifest_digest_id bigint;
    bundle_digest_id bigint;
    output_digest_algorithm text;
begin
    select case_id, result_kind, action_status
      into evaluation_case_id, kind, status
    from action_evaluations
    where id = new.action_evaluation_id;
    select action_route_id, policy_bundle_id, input_manifest_digest_id
      into receipt_route_id, receipt_bundle_id, receipt_input_manifest_digest_id
    from evaluation_receipts
    where action_evaluation_id = new.action_evaluation_id;
    has_receipt := receipt_route_id is not null;
    select digest_id into bundle_digest_id
    from policy_bundles
    where id = receipt_bundle_id;
    select algorithm into output_digest_algorithm
    from digests
    where id = new.output_digest_id;
    if new.output_provenance_id is not null then
        select case_id into provenance_case_id
        from provenance_records
        where id = new.output_provenance_id;
    end if;

    if tg_op = 'INSERT'
       or new.status <> 'invalidated'::preparation_status then
        if kind is distinct from 'actionable'
           or status is distinct from 'supported'::action_status
           or not has_receipt
           or new.action_route_id is distinct from receipt_route_id
           or new.policy_bundle_digest_id is distinct from bundle_digest_id
           or new.input_manifest_digest_id is distinct from receipt_input_manifest_digest_id
           or output_digest_algorithm is distinct from 'sha256'
           or (new.output_provenance_id is not null
               and provenance_case_id is distinct from evaluation_case_id) then
            raise exception
                'preparation % lacks a supported evaluation receipt for action_evaluation %',
                new.id, new.action_evaluation_id;
        end if;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger preparations_require_actionable_evaluation_trigger
    before insert or update on preparations
    for each row execute function preparations_require_actionable_evaluation();

create function preparations_preserve_lifecycle()
returns trigger as $$
begin
    if tg_op = 'DELETE' then
        raise exception 'preparation % is append-only', old.id;
    end if;

    if new.action_evaluation_id is distinct from old.action_evaluation_id
       or new.kind is distinct from old.kind
       or new.prepared_at is distinct from old.prepared_at
       or new.action_route_id is distinct from old.action_route_id
       or new.policy_bundle_digest_id is distinct from old.policy_bundle_digest_id
       or new.input_manifest_digest_id is distinct from old.input_manifest_digest_id
       or new.generator_version is distinct from old.generator_version
       or new.output_digest_id is distinct from old.output_digest_id
       or new.output_provenance_id is distinct from old.output_provenance_id then
        raise exception 'preparation % identity is immutable', old.id;
    end if;

    if old.status = 'exported'::preparation_status
       and new.status = 'prepared'::preparation_status then
        raise exception 'exported preparation % cannot return to prepared', old.id;
    end if;
    if old.status = 'invalidated'::preparation_status
       and new.status <> 'invalidated'::preparation_status then
        raise exception 'invalidated preparation % cannot be revived', old.id;
    end if;
    if new.status = 'invalidated'::preparation_status
       and nullif(trim(new.invalidation_reason), '') is null then
        raise exception 'invalidated preparation % requires a reason', new.id;
    end if;

    return new;
end;
$$ language plpgsql;

create trigger preparations_preserve_lifecycle_trigger
    before update or delete on preparations
    for each row execute function preparations_preserve_lifecycle();

-- A new case-graph input changes the manifest from which a preparation was
-- derived. Preserve the material for audit, but stop presenting it as current.
create function invalidate_preparations_on_case_node_insert()
returns trigger as $$
begin
    update preparations preparation
    set status = 'invalidated'::preparation_status,
        invalidation_reason = 'case inputs changed'
    from action_evaluations evaluation
    where preparation.action_evaluation_id = evaluation.id
      and evaluation.case_id = new.case_id
      and preparation.status <> 'invalidated'::preparation_status;
    return new;
end;
$$ language plpgsql;

create trigger invalidate_preparations_on_case_node_insert_trigger
    after insert on case_nodes
    for each row execute function invalidate_preparations_on_case_node_insert();

create function invalidate_preparations_on_policy_activation()
returns trigger as $$
begin
    update preparations preparation
    set status = 'invalidated'::preparation_status,
        invalidation_reason = 'policy bundle changed'
    from action_evaluations evaluation
    join policy_bundles evaluated_bundle
      on evaluated_bundle.id = evaluation.policy_bundle_id
    join policy_bundles activated_bundle
      on activated_bundle.id = new.policy_bundle_id
    where preparation.action_evaluation_id = evaluation.id
      and evaluated_bundle.jurisdiction = activated_bundle.jurisdiction
      and preparation.status <> 'invalidated'::preparation_status;
    return new;
end;
$$ language plpgsql;

create trigger invalidate_preparations_on_policy_activation_trigger
    after insert on policy_bundle_activations
    for each row execute function invalidate_preparations_on_policy_activation();

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
