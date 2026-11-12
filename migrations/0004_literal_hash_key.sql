-- Literals can be long (code blocks), beyond what a btree can index directly.
-- Identity moves to a hash of the exact string.

alter table literal add column exact_sha256 bytea;
update literal set exact_sha256 = sha256(convert_to(exact, 'UTF8'));
alter table literal alter column exact_sha256 set not null;
alter table literal drop constraint literal_tenant_id_kind_exact_key;
alter table literal add constraint literal_identity unique (tenant_id, kind, exact_sha256);
