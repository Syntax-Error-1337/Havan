# AGENTS.md — how to work in this repo

Read `docs/00-VISION.md` through `docs/07-ROADMAP.md` before changing anything.
They are the spec; code follows them, not the other way round.

## Rules that are not negotiable

1. **Only Apache-2.0, MIT, BSD, ISC, PostgreSQL or 0BSD dependencies.** No BSL,
   SSPL, RSALv2, TSL or AGPL in anything we host (`docs/adr/ADR-002`). A new
   dependency needs a licence line in the PR description.
2. **`cargo fmt` + `clippy -D warnings` + `cargo test --workspace`** before every
   commit. `make check` runs all three.
3. **No `unwrap`/`expect`/`panic!`** outside tests and `main`.
4. **Every tenant table carries `vendor_id` and an RLS policy.** A query that
   forgets to filter must return zero rows, never another vendor's data.
5. **Topics, envelopes and capabilities live in `protocol`.** Nothing else
   formats a topic by hand.
6. **Frontend imports point down the FSD layers** (`app → pages → widgets →
   features → entities → shared`). Siblings never import each other.
7. **Commits are Conventional Commits** and describe the decision, not the
   keystrokes: `fix(pairing): a retried claim returns the same device`.

## Layout

| Path | What lives there |
|---|---|
| `backend/crates/protocol` | wire types, capability catalog, thing-model rules |
| `backend/crates/kernel` | config, errors, ids, tenant context — no frameworks |
| `backend/crates/domain` | feature slices: use cases + repository traits |
| `backend/crates/infra` | Postgres, Valkey, MQTT, object storage |
| `backend/crates/api` | axum routes, middlewares, OpenAPI (bin `haven-api`) |
| `backend/crates/worker` | ingest, scheduler, OTA, fan-out (bin `haven-worker`) |
| `backend/crates/migrate` | schema owner (bin `haven-migrate`) |
| `backend/migrations` | SQL, applied in order, never edited after merge |
| `app` | Expo customer app, feature-sliced under `src/` |
| `portal` | React vendor portal, same layers |
| `sdk` | device SDK: C component, Rust crate |
| `deploy` | compose, broker config, Postgres roles |

## Local stack

```bash
cp .env.example .env
make up && make migrate && make api    # :8080
make worker
make app                               # Expo
```

The app connects to `http://localhost:8080/v1`. Ports: Postgres 5433, Valkey
6380, VerneMQ 1883/8883, SeaweedFS 9000. The API owns 8080 — the broker's status
page is on 8888.

## Definition of done for a slice

- A test that fails without the change and passes with it.
- A route (or a worker handler) wired into the composition root.
- The docs updated **in the same commit** if a contract moved.
- `make check` green.
