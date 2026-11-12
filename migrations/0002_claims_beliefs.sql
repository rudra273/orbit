-- Phase 1: episodes, entities, literals (L1 support), claims (L1) and beliefs (L2).
-- Reference schema: docs/system-design/13-data-model.md.

create extension if not exists btree_gist;
create extension if not exists pg_trgm;

create type basis     as enum ('assumed', 'inferred', 'reported', 'observed', 'stated');
create type standing  as enum ('disputed', 'unconfirmed', 'confirmed');
create type lifecycle as enum ('current', 'superseded', 'refined', 'retracted', 'expired');

-- Which engine, model and prompt produced a derived row, so it can be audited and redone.
create table derivation (
  derivation_id  uuid primary key default uuidv7(),
  stage          text not null,              -- extract | verify | link | relate | summarize | embed | revise
  engine_version text not null,
  model_id       text,
  prompt_sha256  bytea,
  params         jsonb not null default '{}',
  epoch          int  not null default 1,
  created_at     timestamptz not null default now()
);

create table episode (
  tenant_id     uuid not null,
  episode_id    uuid not null default uuidv7(),
  parent_id     uuid,
  level         text not null default 'episode' check (level in ('episode', 'general_event', 'period')),
  source_id     uuid,
  project_id    uuid,
  span          tstzrange not null,
  setting       jsonb not null default '{}',
  summary_tpl   text,
  salience      real,
  tier          smallint not null default 0 check (tier between 0 and 2),
  derivation_id uuid references derivation,
  primary key (tenant_id, episode_id),
  foreign key (tenant_id, source_id) references source (tenant_id, source_id)
);

alter table evidence
  add foreign key (tenant_id, episode_id) references episode (tenant_id, episode_id);

create table entity (
  tenant_id      uuid not null,
  entity_id      uuid not null default uuidv7(),
  type           text not null,
  canonical_name text not null,
  category       text,
  external_ids   jsonb not null default '{}',
  cluster_id     uuid not null,                -- current resolution cluster; derived and reversible
  sensitivity    sensitivity not null default 'work',
  created_at     timestamptz not null default now(),
  primary key (tenant_id, entity_id)
);

create table entity_alias (
  tenant_id   uuid not null,
  entity_id   uuid not null,
  alias       text not null,
  norm        text not null,
  evidence_id uuid,
  byte_start  int,
  byte_end    int,
  primary key (tenant_id, entity_id, norm),
  foreign key (tenant_id, entity_id) references entity
);
create index entity_alias_norm on entity_alias (tenant_id, norm);

-- Soft identity: assertions about entities, never destructive merges.
create table entity_link (
  tenant_id     uuid not null,
  a_id          uuid not null,
  b_id          uuid not null,
  relation      text not null check (relation in ('same_as', 'different_from', 'confusable_with')),
  confidence    real not null,
  asserted_by   text not null check (asserted_by in ('system', 'user')),
  derivation_id uuid references derivation,
  primary key (tenant_id, a_id, b_id, relation),
  foreign key (tenant_id, a_id) references entity (tenant_id, entity_id),
  foreign key (tenant_id, b_id) references entity (tenant_id, entity_id)
);

create table mention (
  tenant_id   uuid not null,
  evidence_id uuid not null,
  byte_start  int  not null,
  byte_end    int  not null,
  entity_id   uuid,                           -- null while unresolved
  candidates  jsonb,
  primary key (tenant_id, evidence_id, byte_start),
  foreign key (tenant_id, evidence_id) references evidence,
  foreign key (tenant_id, entity_id) references entity
);

-- One row per distinct exact string; `exact` is only ever copied from an anchor.
create table literal (
  tenant_id  uuid not null,
  literal_id uuid not null default uuidv7(),
  kind       text not null,
  exact      text not null,
  norm       text not null,                   -- lookup key only
  primary key (tenant_id, literal_id),
  unique (tenant_id, kind, exact)
);
create index literal_norm_trgm on literal using gin (norm gin_trgm_ops);

create table literal_occurrence (
  tenant_id   uuid not null,
  literal_id  uuid not null,
  evidence_id uuid not null,
  byte_start  int  not null,
  byte_end    int  not null,
  span_sha256 bytea not null,
  role        text not null check (role in ('typed_by_user', 'suggested_by_assistant', 'tool_output', 'document')),
  primary key (tenant_id, literal_id, evidence_id, byte_start),
  foreign key (tenant_id, literal_id) references literal,
  foreign key (tenant_id, evidence_id) references evidence
);

create table claim (
  tenant_id          uuid not null,
  claim_id           uuid not null,
  source_id          uuid not null,
  episode_id         uuid,
  speaker_role       text not null check (speaker_role in ('user', 'assistant', 'tool', 'system', 'third_party')),
  speaker_ref        text,
  act                text not null check (act in ('assert', 'prefer', 'decide', 'plan', 'commit', 'instruct', 'correct',
                                                  'reject', 'ask', 'hypothesize', 'define', 'report', 'evaluate',
                                                  'rationale', 'ratify')),
  subject_id         uuid,
  predicate          text,
  object_entity_id   uuid,
  object_literal_id  uuid,
  object_value       jsonb,
  negated            boolean not null default false,
  scope              jsonb not null default '{}',
  statement_tpl      text not null,
  asserted_at        timestamptz not null,
  valid_hint         tstzrange,
  explicitness       text not null check (explicitness in ('explicit', 'implicit')),
  refers_to_claim    uuid,
  echo_of_belief     uuid,
  extract_conf       real not null check (extract_conf between 0 and 1),
  entail_score       real,
  integrity          integrity   not null,
  sensitivity        sensitivity not null,
  independence_group text not null,
  derivation_id      uuid not null references derivation,
  retracted_at       timestamptz,
  retract_reason     text,
  primary key (tenant_id, claim_id),
  foreign key (tenant_id, source_id) references source,
  foreign key (tenant_id, episode_id) references episode,
  foreign key (tenant_id, subject_id) references entity (tenant_id, entity_id),
  foreign key (tenant_id, object_entity_id) references entity (tenant_id, entity_id),
  foreign key (tenant_id, object_literal_id) references literal (tenant_id, literal_id)
);
create index claim_slot on claim (tenant_id, subject_id, predicate) where retracted_at is null;

-- INV-2: every claim cites exact bytes.
create table claim_anchor (
  tenant_id   uuid not null,
  claim_id    uuid not null,
  evidence_id uuid not null,
  byte_start  int  not null,
  byte_end    int  not null check (byte_end > byte_start),
  span_sha256 bytea not null,
  role        text not null default 'content' check (role in ('content', 'referenced')),
  primary key (tenant_id, claim_id, evidence_id, byte_start),
  foreign key (tenant_id, claim_id) references claim,
  foreign key (tenant_id, evidence_id) references evidence
);

create table predicate_def (
  tenant_id           uuid not null,
  predicate           text not null,
  cardinality         text not null check (cardinality in ('functional', 'multi')),
  temporality         text not null check (temporality in ('fluent', 'rigid')),
  default_kind        text not null,
  min_integrity       integrity not null,
  default_sensitivity sensitivity not null,
  primary key (tenant_id, predicate)
);

-- One row per belief VERSION. Rows are never edited in place except to close an interval
-- or change lifecycle, standing and confidence when the slot is recomputed.
create table belief (
  tenant_id         uuid not null,
  belief_id         uuid not null default uuidv7(),
  slot_key          text not null,             -- subject | predicate | scope key (| object, for multi-valued)
  version_key       uuid not null,             -- the claim that opened it; stable across replays
  kind              text not null,
  subject_id        uuid not null,
  predicate         text not null,
  object_entity_id  uuid,
  object_literal_id uuid,
  object_value      jsonb,
  negated           boolean not null default false,
  scope             jsonb not null default '{}',
  scope_key         text not null,
  statement_tpl     text not null,
  valid             tstzrange not null,
  believed          tstzrange not null default tstzrange(now(), null),
  basis             basis     not null,
  standing          standing  not null default 'unconfirmed',
  lifecycle         lifecycle not null default 'current',
  confidence        real not null check (confidence between 0 and 1),
  integrity         integrity   not null,
  sensitivity       sensitivity not null,
  compartment_id    uuid,
  stability         text check (stability in ('state', 'phase', 'trait')),
  importance        real not null default 0,
  links             jsonb not null default '[]',
  status_detail     jsonb,
  derivation_id     uuid not null references derivation,
  primary key (tenant_id, belief_id),
  unique (tenant_id, version_key, believed),
  foreign key (tenant_id, subject_id) references entity (tenant_id, entity_id),
  foreign key (tenant_id, object_entity_id) references entity (tenant_id, entity_id),
  foreign key (tenant_id, object_literal_id) references literal (tenant_id, literal_id),

  -- INV-4: at most one current, undisputed, still-believed version per slot at any valid instant.
  -- Disputed versions coexist on purpose: both sides are kept until the owner decides.
  constraint belief_one_current_per_slot exclude using gist (
    tenant_id with =, slot_key with =, valid with &&
  ) where (lifecycle = 'current' and standing <> 'disputed' and upper_inf(believed))
);
create index belief_slot  on belief (tenant_id, subject_id, predicate, scope_key) where upper_inf(believed);
create index belief_kind  on belief (tenant_id, kind, compartment_id) where lifecycle = 'current';
create index belief_valid on belief using gist (tenant_id, valid);

-- The truth-maintenance graph: which claims justify which belief versions.
create table justification (
  tenant_id   uuid not null,
  belief_id   uuid not null,
  claim_id    uuid not null,
  role        text not null check (role in ('supports', 'attacks', 'rationale', 'ratifies', 'corrects')),
  primary key (tenant_id, belief_id, claim_id, role),
  foreign key (tenant_id, belief_id) references belief,
  foreign key (tenant_id, claim_id) references claim
);
create index justification_claim on justification (tenant_id, claim_id);

-- Cached relation judgements, so replay is deterministic.
create table judgment (
  tenant_id     uuid not null,
  a_id          uuid not null,
  b_id          uuid not null,
  derivation_id uuid not null references derivation,
  relation      text not null check (relation in ('same', 'refines', 'contradicts', 'unrelated')),
  confidence    real not null,
  rationale_tpl text,
  primary key (tenant_id, a_id, b_id, derivation_id)
);

do $$
declare t text;
begin
  foreach t in array array['episode', 'entity', 'entity_alias', 'entity_link', 'mention', 'literal',
                           'literal_occurrence', 'claim', 'claim_anchor', 'predicate_def', 'belief',
                           'justification', 'judgment']
  loop
    execute format('alter table %I enable row level security', t);
    execute format('alter table %I force row level security', t);
    execute format('create policy tenant_isolation on %I using (tenant_id = current_setting(''orbit.tenant_id'', true)::uuid)', t);
  end loop;
end $$;
