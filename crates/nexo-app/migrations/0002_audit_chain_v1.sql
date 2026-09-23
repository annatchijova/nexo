-- Persistent audit_chain/v1. This migration is additive and refuses to
-- reinterpret legacy audit rows: a non-empty pre-v1 audit_log must be
-- migrated by an explicit, reviewed procedure rather than guessed at here.

do $$
begin
    if exists (select 1 from audit_log) then
        raise exception
            'audit_chain/v1 requires an explicit migration for non-empty audit_log';
    end if;
end
$$ language plpgsql;

create table if not exists audit_chains (
    chain_id          text primary key check (chain_id <> ''),
    chain_version     bigint not null check (chain_version > 0),
    genesis_digest    bytea not null check (octet_length(genesis_digest) = 32),
    current_sequence  bigint not null default 0 check (current_sequence >= 0),
    current_tip       bytea not null check (octet_length(current_tip) = 32),
    created_at        timestamptz not null default now(),
    check (current_sequence = 0 or current_tip <> genesis_digest)
);

create table if not exists audit_checkpoints (
    checkpoint_id    bigint generated always as identity primary key,
    chain_id         text not null references audit_chains (chain_id),
    chain_version    bigint not null check (chain_version > 0),
    at_sequence      bigint not null check (at_sequence > 0),
    tip_digest       bytea not null check (octet_length(tip_digest) = 32),
    entry_count      bigint not null check (entry_count > 0),
    created_at       timestamptz not null default now(),
    unique (chain_id, at_sequence)
);

create function audit_checkpoints_are_append_only()
returns trigger as $$
begin
    raise exception 'audit checkpoints are append-only';
end;
$$ language plpgsql;

create trigger audit_checkpoints_are_append_only_trigger
    before update or delete on audit_checkpoints
    for each row execute function audit_checkpoints_are_append_only();

alter table audit_log
    add column if not exists chain_id text,
    add column if not exists chain_version bigint,
    add column if not exists sequence bigint,
    add column if not exists event_id text,
    add column if not exists provenance_refs jsonb;

update audit_log
set chain_id = 'mutations',
    chain_version = 1,
    sequence = id,
    event_id = 'legacy:' || id::text,
    provenance_refs = '[]'::jsonb
where chain_id is null;

alter table audit_log
    alter column chain_id set not null,
    alter column chain_version set not null,
    alter column sequence set not null,
    alter column event_id set not null,
    alter column provenance_refs set not null,
    alter column previous_entry_hash set not null,
    alter column entry_hash set not null;

alter table audit_log
    add constraint audit_log_chain_version_positive check (chain_version > 0),
    add constraint audit_log_sequence_nonnegative check (sequence >= 0),
    add constraint audit_log_event_id_nonempty check (event_id <> ''),
    add constraint audit_log_provenance_refs_object check (jsonb_typeof(provenance_refs) = 'array'),
    add constraint audit_log_previous_digest_length check (octet_length(previous_entry_hash) = 32),
    add constraint audit_log_entry_digest_length check (octet_length(entry_hash) = 32),
    add constraint audit_log_chain_sequence_unique unique (chain_id, sequence),
    add constraint audit_log_chain_event_unique unique (chain_id, event_id);

insert into audit_chains (
    chain_id, chain_version, genesis_digest, current_sequence, current_tip
)
values (
    'mutations', 1, decode(repeat('00', 32), 'hex'), 0,
    decode(repeat('00', 32), 'hex')
)
on conflict (chain_id) do nothing;

create function audit_log_append_matches_chain()
returns trigger as $$
declare
    chain audit_chains%rowtype;
begin
    select * into chain
    from audit_chains
    where chain_id = new.chain_id
    for update;

    if not found then
        raise exception 'audit event references an unknown chain';
    end if;
    if new.chain_version <> chain.chain_version
       or new.sequence <> chain.current_sequence + 1
       or new.previous_entry_hash <> chain.current_tip
       or new.event_id <> new.chain_id || ':' || new.sequence::text then
        raise exception 'audit event does not extend the current chain tip';
    end if;
    return new;
end;
$$ language plpgsql;

create trigger audit_log_append_matches_chain_trigger
    before insert on audit_log
    for each row execute function audit_log_append_matches_chain();

insert into nexo_schema_migrations (version)
values ('0002_audit_chain_v1')
on conflict (version) do nothing;
