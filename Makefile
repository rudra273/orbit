.PHONY: db db-down run import test test-db check

db:            ## start the local PostgreSQL
	docker compose up -d --wait

db-down:       ## stop it (data is kept in the orbit-pg volume)
	docker compose down

run:           ## run the engine; needs .env
	set -a && . ./.env && set +a && cargo run -p orbit-server

import:        ## import Claude Code and Codex history into the running server
	set -a && . ./.env && set +a && cargo run -q -p orbit-cli -- import claude-code && cargo run -q -p orbit-cli -- import codex

test:          ## unit tests (no database needed)
	cargo test --workspace

test-db:       ## database tests; needs `make db` and .env
	set -a && . ./.env && set +a && cargo test --workspace -- --ignored

check:         ## what CI runs, minus the database
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
