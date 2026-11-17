# Haven — the commands that matter. Everything else is `cargo` directly.

SHELL := /bin/bash
.DEFAULT_GOAL := help

ifneq (,$(wildcard .env))
include .env
export
endif

BACKEND := backend

# The address a device on the same Wi-Fi dials. It is a DHCP lease, not a fact about the machine,
# which is why the broker certificate is reissued when it moves and the firmware takes it as a
# build flag rather than a constant.
LAN_IP ?= $(shell ipconfig getifaddr en0 2>/dev/null || ipconfig getifaddr en1 2>/dev/null)

# One CA for everything: the API signs with it, the broker trusts it, and `make broker-tls` issues
# the broker's own certificate from it. Pinned to the repository root here because a relative
# default follows the process's working directory — starting the API from `backend/` once created
# a second CA there, and every device certificate it issued was refused by the broker.
export HAVEN_CA_DIR ?= $(CURDIR)/.haven/ca
# Cargo 1.99 takes `--manifest-path` after the subcommand, so every recipe
# changes directory instead of threading a flag through a variable.
CARGO := cd $(BACKEND) && cargo
# The suite runs against `haven_test`, never the dev database. Otherwise a worker
# left running on the dev database leases the automations a test just seeded — the
# test then reports "never leased" and the code looks broken when the environment
# is. The test database is created by `make testdb`, which `check` and `test` depend on.
# `MIGRATE_DATABASE_URL` matters here too: the scheduler tests seed fixtures through
# the app role and lease through the owner connection, so both must name the test
# database. Leaking the dev URL into one of them made the tests lease a different
# database and report "never leased".
# All three URLs name the same database, which is the invariant CI relies on: a test
# that seeds through one connection and reads through another must not be talking to
# two databases. `DATABASE_URL` is the app role (so RLS applies), `MIGRATE_DATABASE_URL`
# the owner (so fixtures can write DDL-adjacent rows).
# The assignments come *after* `cd … &&` on purpose: `VAR=x cd dir && cargo` sets VAR
# for `cd`, which ignores it, and cargo then runs with the dev database from .env.
CARGO_TEST := cd $(BACKEND) && DATABASE_URL=$(TEST_DATABASE_URL) \
	TEST_DATABASE_URL=$(TEST_DATABASE_URL) MIGRATE_DATABASE_URL=$(TEST_MIGRATE_DATABASE_URL) \
	TEST_REDIS_URL=$(TEST_REDIS_URL) cargo

.PHONY: help
help: ## show this help
	@# `include .env` puts a second file in MAKEFILE_LIST, and grep then prefixes
	@# every line with its filename — which made this list read "Makefile  format".
	@# `[a-zA-Z_-]+` hid any target with a digit in it — `e2e` never appeared.
	@grep -E '^[a-zA-Z0-9_-]+:.*?## ' $(firstword $(MAKEFILE_LIST)) | \
	  awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

.PHONY: env-check
env-check: ## do the credentials agree with each other and with the broker config?
	@scripts/env-check.sh

.PHONY: up
# The check runs first because the failure it catches is silent: a mismatched MQTT token starts
# a stack whose broker closes every connection, and the only symptom is "Connection closed by
# peer abruptly" from a service that looks healthy.
up: env-check broker-tls ## start postgres, valkey, vernemq, seaweedfs
	docker compose -f deploy/docker-compose.yml up -d
	@echo "waiting for postgres..." && \
	  until docker exec haven-db pg_isready -U haven -d haven >/dev/null 2>&1; do sleep 1; done

.PHONY: down
down: ## stop the stack (keeps volumes)
	docker compose -f deploy/docker-compose.yml down

.PHONY: reset
reset: ## stop the stack and delete its volumes
	docker compose -f deploy/docker-compose.yml down -v

.PHONY: contrast
contrast: ## every text pair in the app palette meets its WCAG threshold
	@cd app && node scripts/check-contrast.mjs

.PHONY: tokens
tokens: ## no colour value in the app lives outside shared/config/tokens.ts
	@cd app && node scripts/check-tokens.mjs

.PHONY: time-serde
time-serde: ## every timestamp a client receives is an RFC 3339 string, not a field tuple
	@python3 scripts/time-serde.py

.PHONY: testdb
testdb: ## create, migrate and grant the test database
	@docker exec haven-db psql -U haven -d postgres -tc \
	  "SELECT 1 FROM pg_database WHERE datname='haven_test'" | grep -q 1 || \
	  docker exec haven-db createdb -U haven haven_test
	@cd $(BACKEND) && MIGRATE_DATABASE_URL=$(TEST_MIGRATE_DATABASE_URL) \
	  DATABASE_URL=$(TEST_DATABASE_URL) cargo run -q -p migrate
	@# The migration run used to fail silently and leave a half-built database —
	@# the suite then tested last week's schema. Count, compare, refuse.
	@applied=$$(docker exec haven-db psql -U haven -d haven_test -tAc 'SELECT count(*) FROM _sqlx_migrations;'); \
	 files=$$(ls backend/migrations/*.sql | wc -l | tr -d ' '); \
	 if [ "$$applied" != "$$files" ]; then \
	   echo "haven_test has $$applied of $$files migrations — the run did not finish" >&2; exit 1; \
	 fi
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
api-lan: ## run the API on all interfaces, so a phone on the LAN can reach it
	@echo "  listening on every interface — use this only on a network you trust"
	cd $(BACKEND) && HAVEN_BIND=0.0.0.0:8080 cargo run -p api

.PHONY: api-lan
worker: ## run the worker (foreground)
	$(CARGO) run -p worker

.PHONY: broker-tls
broker-tls: ## the broker's TLS certificate, signed by the device CA (covers the LAN address)
	@if [ -f deploy/certs/server.crt ] && [ -n "$(LAN_IP)" ] && \
	    openssl x509 -in deploy/certs/server.crt -noout -text | grep -q "DNS:$(LAN_IP)"; then \
	  echo "broker certificate already covers $(LAN_IP)"; \
	else \
	  echo "issuing the broker certificate from the device CA for $(LAN_IP)"; \
	  cd $(BACKEND) && cargo run -q -p haven-cert --bin haven-cert-broker -- \
	    ../.haven/ca ../deploy/certs "$(LAN_IP)" "$(LAN_IP)"; \
	fi

.PHONY: sdk
sdk: ## build and test the C component (host compiler, no board needed)
	@$(MAKE) -s -C sdk/c test

.PHONY: check
check: testdb ## fmt + clippy + tests + no secrets + routes match handlers + readable palette
	$(CARGO) fmt --all --check
	bash scripts/no-secrets.sh
	python3 scripts/route-arity.py --self-test
	$(MAKE) contrast
	$(MAKE) tokens
	$(MAKE) time-serde
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(CARGO_TEST) test --workspace

# The same gate without the database suites, for the loop between edits.
#
# Measured where the five minutes actually go: the fifty-eight suites execute in 12.8 s. The other four
# minutes are compilation — one small change to `infra` relinks every test binary that depends on it, and
# there are fifty of those. Sharding would not help; it would only add build contention. This target exists
# so the fast feedback loop is a make target rather than a personal recipe, and so nobody spends an afternoon
# "optimising" the part that is already four percent of the time.
check-fast: ## fmt + lints + the cheap gates, no database suites (~90 s on a warm tree)
	$(CARGO) fmt --all --check
	bash scripts/no-secrets.sh
	python3 scripts/route-arity.py --self-test
	$(MAKE) contrast
	$(MAKE) tokens
	$(MAKE) time-serde
	$(CARGO) clippy --workspace --all-targets -- -D warnings

.PHONY: audit
audit: ## dependency advisories: shipped code must be clean, tooling must be clean at high
	bash scripts/audit.sh

.PHONY: vendor-smoke
# The surfaces behind settings screens that no test clicks through by hand. The notifications bug
# passed every test in the repo and answered 404 to the app; a request is the only thing that says so.
vendor-smoke: ## live pass over vendor two-factor, SSO, invoices, webhooks and support bundle
	python3 scripts/vendor-smoke.py

.PHONY: conformance
# A vendor runs this before asking us to certify a product; we run it before we say yes. It drives
# a real device through the app's contract, so it is the only check that can answer "does this
# product actually work", and it reads its answers from the platform rather than from its own log.
conformance: ## run the device conformance suite: make conformance VENDOR=<uuid> PRODUCT=<uuid>
	@test -n "$(VENDOR)" -a -n "$(PRODUCT)" || \
	  { echo "usage: make conformance VENDOR=<uuid> PRODUCT=<uuid>"; exit 2; }
	python3 scripts/conformance.py --vendor $(VENDOR) --product $(PRODUCT)

.PHONY: test
test: testdb ## tests only
	$(CARGO_TEST) test --workspace

.PHONY: backup
backup: ## verified database dump into .haven/backups
	@scripts/backup.sh

.PHONY: restore-drill
restore-drill: ## restore the newest dump into a throwaway database and compare counts
	@scripts/restore-drill.sh

.PHONY: fmt
fmt: ## format
	$(CARGO) fmt --all

.PHONY: e2e
e2e: ## the customer journey over HTTP (needs stack, seed, api and worker up)
	./scripts/e2e.sh

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
licences: ## every dependency is permissively licensed (AGENTS.md 1 / ADR-002)
	@bash scripts/licences.sh

load: ## how much ingest this stack takes, and whether it loses any (RATE= SECONDS=)
	@bash scripts/load.sh $(RATE) $(SECONDS)

api-replicas: ## two API instances: do they agree about a dependency that died?
	@bash scripts/api-replicas.sh

sockets: ## one home, N phones: subscriptions per pod, memory per socket, and does every socket hear one frame (COUNT= PORTS=)
	@bash scripts/sockets.sh $(COUNT) $(PORTS)

.PHONY: broker-auth
broker-auth: ## assert the broker authenticates a device and confines it to its own topics
	@bash scripts/broker-auth.sh

.PHONY: wifi-diag
wifi-diag: ## flash the wifi probe and read what the board's radio sees (then read the comments)
	@pio run -d sdk/esp8266 -e scan -t upload
	@PORT=$${PORT:-/dev/cu.usbserial-A5069RR4} bash scripts/wifi-diag.sh

multi-replica: ## two workers, one fleet: exclusive ingest and a single redelivery
	@bash scripts/multi-replica.sh

chaos: ## What breaks when a dependency dies (stops and starts the local containers)
	@bash scripts/chaos.sh $(WHAT)

bench: ## baseline the read paths on a fresh 100-device bench home
	./scripts/bench.sh

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

