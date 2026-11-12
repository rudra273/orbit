-- Phase 0: search over raw evidence, and cursors for change-feed consumers.

-- One row per piece of evidence; the first 200,000 characters are indexed.
create table evidence_search (
  tenant_id   uuid not null,
  evidence_id uuid not null,
  tsv         tsvector not null,
  primary key (tenant_id, evidence_id),
  foreign key (tenant_id, evidence_id) references evidence
);
create index evidence_search_tsv on evidence_search using gin (tsv);

-- How far each consumer of the change feed (outbox) has read.
create table feed_cursor (
  tenant_id uuid   not null,
  consumer  text   not null,
  last_seq  bigint not null default 0,
  primary key (tenant_id, consumer)
);

alter table evidence_search enable row level security;
alter table evidence_search force row level security;
alter table feed_cursor     enable row level security;
alter table feed_cursor     force row level security;
create policy tenant_isolation on evidence_search using (tenant_id = current_setting('orbit.tenant_id', true)::uuid);
create policy tenant_isolation on feed_cursor     using (tenant_id = current_setting('orbit.tenant_id', true)::uuid);
