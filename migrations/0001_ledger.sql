-- Phase 0: the evidence ledger (L0) and the change feed.
-- Reference schema: docs/system-design/13-data-model.md. Requires PostgreSQL 18 (uuidv7).

create type integrity   as enum ('external', 'agent', 'user_relayed', 'user_artifact', 'user_direct');
create type sensitivity as enum ('public', 'work', 'personal', 'sensitive');

create table source (
  tenant_id    uuid not null,
  source_id    uuid not null default uuidv7(),
  kind         text not null,
  external_ref text not null,
  app          text,
  model        text,
  title        text,
  project_id   uuid,
  meta         jsonb not null default '{}',
  created_at   timestamptz not null default now(),
  primary key (tenant_id, source_id),
  unique (tenant_id, kind, external_ref)
);

create table evidence (
  tenant_id       uuid not null,
  evidence_id     uuid not null,
  source_id       uuid not null,
  seq             int  not null,
  episode_id      uuid,
  author_role     text not null check (author_role in ('user', 'assistant', 'tool', 'system', 'third_party')),
  author_ref      text,
  occurred_at     timestamptz not null,
  recorded_at     timestamptz not null default now(),
  content         bytea,
  content_sha256  bytea not null,
  integrity       integrity   not null,
  sensitivity     sensitivity not null,
  taint           text[] not null default '{}',
  idempotency_key text not null,
  chain_seq       bigint not null,
  prev_hash       bytea,
  row_hash        bytea not null,
  erased_at       timestamptz,
  primary key (tenant_id, evidence_id),
  unique (tenant_id, idempotency_key),
  unique (tenant_id, source_id, seq),
  unique (tenant_id, chain_seq),
  foreign key (tenant_id, source_id) references source (tenant_id, source_id)
);

-- INV-1: evidence is never edited. The only changes allowed are assigning an
-- episode and erasing content at the owner's request.
create function evidence_append_only() returns trigger language plpgsql as $$
begin
  if tg_op = 'DELETE' then
    raise exception 'evidence is append-only';
  end if;
  if (to_jsonb(new) - 'episode_id' - 'erased_at' - 'content')
     is distinct from (to_jsonb(old) - 'episode_id' - 'erased_at' - 'content') then
    raise exception 'evidence is append-only';
  end if;
  if new.content is distinct from old.content and not (new.content is null and new.erased_at is not null) then
    raise exception 'evidence content can only be erased, never changed';
  end if;
  return new;
end $$;

create trigger evidence_append_only before update or delete on evidence
  for each row execute function evidence_append_only();

create table outbox (
  tenant_id  uuid   not null,
  tenant_seq bigint not null,
  topic      text   not null,
  payload    jsonb  not null,
  created_at timestamptz not null default now(),
  primary key (tenant_id, tenant_seq)
);

-- One brain per deployment today; row-level security is kept as the seam for more.
-- Note: a superuser connection bypasses these policies.
alter table source   enable row level security;
alter table evidence enable row level security;
alter table outbox   enable row level security;
alter table source   force row level security;
alter table evidence force row level security;
alter table outbox   force row level security;

create policy tenant_isolation on source   using (tenant_id = current_setting('orbit.tenant_id', true)::uuid);
create policy tenant_isolation on evidence using (tenant_id = current_setting('orbit.tenant_id', true)::uuid);
create policy tenant_isolation on outbox   using (tenant_id = current_setting('orbit.tenant_id', true)::uuid);
