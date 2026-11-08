# Operations boundary

## No configured deployment

This foundation adds no deploy workflow, public route, live feed/dump
acquisition, upload, database/DDL or scheduled runtime. It does not change
production. `workers/mb/wrangler.toml` is an unconfigured example, not a
resource inventory or deploy recipe. Resource IDs are deliberately absent.
No source installation's resources, secrets or environment bindings transfer
with the code. The example names below are unprovisioned, not live assets.

### R2 buckets — example contract only

| Bucket label | Environment | State |
|---|---|---|
| `musicbrainz-example-art-staging` | staging | Unprovisioned example |
| `musicbrainz-example-art-prod` | prod | Unprovisioned example |

### Environments matrix — example contract only

| Concern | Staging label | Production label |
|---|---|---|
| R2 artwork / MB Worker | `musicbrainz-example-art-staging` | `musicbrainz-example-art-prod` |

### Binding names

| Binding | Contract |
|---|---|
| `ART_CACHE` | Example labels `musicbrainz-example-art-staging` and `musicbrainz-example-art-prod`; no art activation or redistribution rights implied. |
| `MB_DB` | Legacy SQL adapter's Hyperdrive connection; no target, schema or applied ledger here. |
| `MB_CACHE` | Legacy successful-response KV cache; no namespace provisioned. |
| `MB_SNAPSHOT` | Opt-in snapshot backend's R2 object store; not bound or activated by the example. |

`BUILD_SHA` is public build identity, not a credential. `ENVIRONMENT` is a
label, not authorization. `MB_BACKEND` selects the repository adapter;
`MB_SNAPSHOT_PREFIX` would select a snapshot object prefix. Secret names
alone do not confer permission to fetch, mint, place or rotate values.
`/healthz` does not establish data readiness. There is no activation recipe
until the supported backend, rights, ownership and security gates are met.

## On-demand preview and staging; zero idle runtime cost

No staging/preview runtime may incur recurring always-on charges without
an explicit dated owner cost exception. A sleeping container is not enough:
inventory Containers, Durable Objects (including alarms/keep-alive),
scheduled Workers, Stream inputs, database compute and every other
applicable billable runtime before introducing an environment. None is
provisioned by this PR.

Any future pipeline must:

1. Own trusted on-demand startup and readiness before qualification/e2e,
   bound to the exact tested SHA and environment. Liveness alone is not
   storage/schema/generation readiness.
2. Grant a bounded active-use window. Record its start, deadline, tested
   SHA, environment and all billable resources as qualification evidence.
3. Use native stop/scale-to-zero plus native cleanup/expiry, including
   failed and cancelled runs. A final cleanup step alone is not a guarantee
   under hard cancellation; platform expiry must cover that case.
4. Retain startup/readiness, bounded-window and cleanup/expiry evidence.
   If the platform cannot enforce a control, route that prerequisite before
   activation rather than claim it works. Do not add host timers or another
   scheduler to compensate.
5. Avoid alarms, probes, schedules and keep-alive that resurrect resources
   or sustain charges outside the authorized window. Preserve required
   health, security and qualification checks during active use.

An always-on exception must identify owner authorization, date, resource
and environment scope, recurring cost, and expiry/review date. No new
spend, credential or security weakening is authorized here. Sleeping never
waives exact-head Actions/e2e, security review, migration/rollback evidence
or release gates. Production remains untouched.

## Rollback and later adoption

The initial rollback is a source revert through normal PR gates; there is
no runtime to roll back. A future deployment must identify the exact
previous known-good artifact and data generation, migration ledger (or an
explicit no-migration statement), atomic promotion strategy and tested
rollback. Do not infer an applied SQL migration from a source file or
synthetic packet ledger. Never apply the retained source's migration 0035
as part of extraction.

Before any merge, inspect automatic production workflow paths **and** the
default-head hosting-provider deployments/checks. Unknown classification
is production. Every production deploy or production-changing merge
requires exact-head release acceptance; non-production source changes keep
the normal CI/review gate. No merge or publish permission is encoded in
this document.
