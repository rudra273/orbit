.PHONY: db db-down run test

db:            ## start the local PostgreSQL
	docker compose up -d --wait

db-down:       ## stop it (data is kept in the orbit-pg volume)
	docker compose down

run:           ## run the engine; needs .env
	set -a && . ./.env && set +a && cargo run -p orbit-server

test:          ## Rust and Python tests
	cargo test
	cd workers && python3 -m unittest discover -s tests -v
