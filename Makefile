# Haven — the commands that matter. Everything else is `cargo` directly.

SHELL := /bin/bash
.DEFAULT_GOAL := help

ifneq (,$(wildcard .env))
include .env
export
endif

BACKEND := backend
# Cargo 1.99 takes `--manifest-path` after the subcommand, so every recipe
# changes directory instead of threading a flag through a variable.
CARGO := cd $(BACKEND) && cargo

.PHONY: help
help: ## show this help
	@# `include .env` puts a second file in MAKEFILE_LIST, and grep then prefixes
	@# every line with its filename — which made this list read "Makefile  format".
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(firstword $(MAKEFILE_LIST)) | \
	  awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

.PHONY: up
up: ## start postgres, redis, vernemq, minio
	docker compose -f deploy/docker-compose.yml up -d
	@echo "waiting for postgres..." && \
	  until docker exec haven-db pg_isready -U haven -d haven >/dev/null 2>&1; do sleep 1; done

.PHONY: down
down: ## stop the stack (keeps volumes)
	docker compose -f deploy/docker-compose.yml down

.PHONY: reset
reset: ## stop the stack and delete its volumes
	docker compose -f deploy/docker-compose.yml down -v

.PHONY: testdb
testdb: ## create, migrate and grant the test database
	@docker exec haven-db psql -U haven -d postgres -tc \
	  "SELECT 1 FROM pg_database WHERE datname='haven_test'" | grep -q 1 || \
	  docker exec haven-db createdb -U haven haven_test
	@MIGRATE_DATABASE_URL=$(subst /haven,/haven_test,$(MIGRATE_DATABASE_URL)) \
	  DATABASE_URL=$(TEST_DATABASE_URL) $(CARGO) run -q -p migrate
	@docker exec -i haven-db psql -U haven -d haven_test -q -c \
	  "GRANT USAGE ON SCHEMA public TO haven_app, haven_system; \
	   GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO haven_app, haven_system; \
	   GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO haven_app, haven_system;"
	@echo "haven_test ready"

.PHONY: migrate
migrate: ## apply migrations
	$(CARGO) run -q -p migrate

.PHONY: api
api: ## run the HTTP API (foreground)
	$(CARGO) run -p api

.PHONY: worker
worker: ## run the worker (foreground)
	$(CARGO) run -p worker

.PHONY: check
check: ## fmt + clippy + tests
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(CARGO) test --workspace

.PHONY: test
test: ## tests only
	$(CARGO) test --workspace

.PHONY: fmt
fmt: ## format
	$(CARGO) fmt --all

.PHONY: e2e
e2e: ## end-to-end cloud flow (needs the stack up)
	$(CARGO) test -p api --test e2e -- --nocapture

.PHONY: openapi
openapi: ## regenerate docs/openapi.json from the code
	$(CARGO) run -q -p api --bin haven-api -- --print-openapi > docs/openapi.json

.PHONY: app
app: ## start the Expo app
	cd app && npm start

.PHONY: portal
portal: ## start the vendor portal
	cd portal && npm run dev

.PHONY: app-web
app-web: ## serve the customer app in a browser (no simulator needed)
	cd app && EXPO_PUBLIC_BLE_MOCK=1 EXPO_PUBLIC_API_URL=http://127.0.0.1:8080/v1 \
	  npx expo start --web --port 8082

.PHONY: ci-backend
ci-backend: ## the backend suite as CI runs it: scratch db, deploy roles, haven_app
	./scripts/ci-backend.sh

.PHONY: bench
bench: ## baseline the read paths for the current home
	./scripts/bench.sh /tmp/tok.txt "$$(cat /tmp/hid.txt)" "$$(cat /tmp/did.txt)"

.PHONY: seed
seed: ## demo vendor, portal login and three products (development only)
	$(CARGO) run -p devseed

.PHONY: db-reset
db-reset: ## drop and rebuild the dev database from migrations
	@# FORCE, because the API, the worker and any open psql hold connections and a
	@# plain DROP refuses — which is what a reset is supposed to survive.
	@docker exec -i haven-db psql -U haven -d postgres -c 'DROP DATABASE IF EXISTS haven WITH (FORCE);'
	docker exec -i haven-db psql -U haven -d postgres -c 'CREATE DATABASE haven OWNER haven;'
	docker exec -i haven-db psql -U haven -d haven -q < deploy/postgres/init.sql
	$(MAKE) migrate

