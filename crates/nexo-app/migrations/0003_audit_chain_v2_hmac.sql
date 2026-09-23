-- Persistent audit_chain/v2. This migration is intentionally a cutover, not
-- a reinterpretation of v1 rows. A non-empty v1 chain requires an explicit,
-- reviewed operational migration before authenticated history can begin.

do $$
begin
    if exists (select 1 from audit_log)
       or exists (select 1 from audit_checkpoints)
       or exists (select 1 from audit_chains where current_sequence <> 0) then
        raise exception
            'audit_chain/v2 requires an explicit cutover for non-empty v1 history';
    end if;
end
$$ language plpgsql;

alter table audit_chains
    add column if not exists auth_scheme text,
    add column if not exists hmac_key_version text,
    add column if not exists current_tip_hmac bytea;

alter table audit_log
    add column if not exists auth_scheme text,
    add column if not exists hmac_key_version text,
    add column if not exists entry_hmac bytea;

alter table audit_checkpoints
    add column if not exists auth_scheme text,
    add column if not exists hmac_key_version text,
    add column if not exists tip_hmac bytea;

update audit_chains
set chain_version = 2,
    auth_scheme = 'hmac-sha256',
    hmac_key_version = 'v1',
    current_tip_hmac = decode(repeat('00', 32), 'hex')
where chain_id = 'mutations';

alter table audit_chains
    alter column auth_scheme set not null,
    alter column hmac_key_version set not null,
    alter column current_tip_hmac set not null;

alter table audit_log
    alter column auth_scheme set not null,
    alter column hmac_key_version set not null,
    alter column entry_hmac set not null;

alter table audit_checkpoints
    alter column auth_scheme set not null,
    alter column hmac_key_version set not null,
    alter column tip_hmac set not null;

alter table audit_chains
    add constraint audit_chains_auth_scheme_hmac check (auth_scheme = 'hmac-sha256'),
    add constraint audit_chains_hmac_length check (octet_length(current_tip_hmac) = 32);

alter table audit_log
    add constraint audit_log_auth_scheme_hmac check (auth_scheme = 'hmac-sha256'),
    add constraint audit_log_entry_hmac_length check (octet_length(entry_hmac) = 32);

alter table audit_checkpoints
    add constraint audit_checkpoints_auth_scheme_hmac check (auth_scheme = 'hmac-sha256'),
    add constraint audit_checkpoints_tip_hmac_length check (octet_length(tip_hmac) = 32);

insert into nexo_schema_migrations (version)
values ('0003_audit_chain_v2_hmac')
on conflict (version) do nothing;
