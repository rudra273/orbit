-- Phase 0: dense (embedding) search over raw evidence.
--
-- Vectors from different models cannot be compared, so each model and size is its own *space*.
-- A new space is built in the background from the start of the change feed and becomes the
-- active one only when it has caught up; the old one is then retired. Vectors are as sensitive
-- as the text they come from (docs/architecture/11-security-privacy.md).

create extension if not exists vector;

create table embedding_space (
  tenant_id    uuid    not null,
  space_id     text    not null,                 -- '<model>@<dims>', e.g. 'qwen/qwen3-embedding-8b@4096'
  model        text    not null,
  dims         int     not null check (dims between 1 and 16000),
  query_prefix text    not null default '',      -- fixed for the life of the space
  doc_prefix   text    not null default '',
  hosted       boolean not null,                 -- text leaves the server; `sensitive` evidence is never sent
  state        text    not null check (state in ('building', 'active', 'retired')),
  created_at   timestamptz not null default now(),
  activated_at timestamptz,
  primary key (tenant_id, space_id)
);
create unique index embedding_space_one_active on embedding_space (tenant_id) where state = 'active';

-- One row per chunk of a piece of evidence. `v` is half precision; its length is checked against
-- the space by the indexer. `bits` is its 1-bit copy (the sign of each number). Search scans
-- `bits` by Hamming distance and re-ranks the best candidates with `v`: on the owner's history
-- that lost no accuracy at 4096 dimensions, and at 50,000 chunks it takes ~55 ms against
-- ~250 ms for an exact scan of `v`. No ANN index is needed at one person's scale; an HNSW index
-- over `bits` is the next step if a brain outgrows the scan.
create table evidence_embedding (
  tenant_id   uuid        not null,
  space_id    text        not null,
  evidence_id uuid        not null,
  chunk       int         not null,
  byte_start  int         not null,
  byte_end    int         not null check (byte_end > byte_start),
  v           halfvec     not null,
  bits        bit varying not null,
  primary key (tenant_id, space_id, evidence_id, chunk),
  foreign key (tenant_id, space_id) references embedding_space,
  foreign key (tenant_id, evidence_id) references evidence
);

alter table embedding_space    enable row level security;
alter table embedding_space    force row level security;
alter table evidence_embedding enable row level security;
alter table evidence_embedding force row level security;
create policy tenant_isolation on embedding_space    using (tenant_id = current_setting('orbit.tenant_id', true)::uuid);
create policy tenant_isolation on evidence_embedding using (tenant_id = current_setting('orbit.tenant_id', true)::uuid);

-- Erasing evidence removes everything derived from its text: vectors can be inverted back into
-- much of the text, and the full-text vector holds its words.
create function evidence_erase_derived() returns trigger language plpgsql as $$
begin
  delete from evidence_embedding where tenant_id = new.tenant_id and evidence_id = new.evidence_id;
  delete from evidence_search    where tenant_id = new.tenant_id and evidence_id = new.evidence_id;
  return new;
end $$;
create trigger evidence_erase_derived after update of erased_at on evidence
  for each row when (old.erased_at is null and new.erased_at is not null)
  execute function evidence_erase_derived();
