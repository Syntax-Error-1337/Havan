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
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | \
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
testdb: ## create the test database if it is missing
	@docker exec haven-db psql -U haven -d postgres -tc \
	  "SELECT 1 FROM pg_database WHERE datname='haven_test'" | grep -q 1 || \
	  docker exec haven-db createdb -U haven haven_test

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
