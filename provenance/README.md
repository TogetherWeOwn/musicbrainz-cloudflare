# Clean-export provenance and retained obligations

## Source and contribution mapping

The export is pinned to
[`TogetherWeOwn/nntunev2@3ecdd743b26c0881ac17520883aab717fb813ec6`](https://github.com/TogetherWeOwn/nntunev2/tree/3ecdd743b26c0881ac17520883aab717fb813ec6).
No moving-main or unmerged source content is silently substituted.
[`source-export.json`](source-export.json) maps each copied file's original
Git blob, last-touch contribution commit, pinned contribution-history link,
destination and SHA-256. Original authorship remains in that linked history;
the export commit does not claim to author the existing implementation.
LICENSE is the original TogetherWeOwn MIT notice verbatim. No unrelated
product history, contributor email addresses or filtered history is copied.
This audit covers the selected payload, not historical secret clearance.

The worker lock is promoted to root `Cargo.lock` with unchanged package
versions/checksums. Only the two required packages join the new workspace.
Source package names and paths stay compatible. The art mapping substitutes
unprovisioned example names; every independent assertion remains, with the
production example tightened to a separate bucket instead of reproducing
a source deployment drift. Config and operational docs are newly scoped,
not a wholesale deploy/root configuration copy. Generated live-dump golden
sets, source seed SQL and spike deployment/measurement scripts are excluded.
The existing fold table is a deterministic source normalization table, not
a music dataset or dump-derived heldout fixture.

Org workflow inputs are reviewed from
[`TogetherWeOwn/.github@20d95fd8c249c7192c364f8b41519fca3fc873df`](https://github.com/TogetherWeOwn/.github/tree/20d95fd8c249c7192c364f8b41519fca3fc873df):
`workflow-templates/starter-ci.yml`, `gitleaks.yml`, `pr-lint.yml`,
`pr-size.yml`, `test-guard.yml` and `.github/pull_request_template.md`.
The starter's placeholders are replaced with actual Rust tests/lint/builds.
Source Rust cache/action pins are retained and every callable workflow uses
a literal reviewed SHA; no floating action tag or paid scanner wrapper.

## Tests and fixture provenance

All 22 independent `workers/mb/tests/*.rs` targets, inline worker/error/
snapshot tests, search-pagination source module, logging inline tests and
both logging integration targets are exported. The MB registry retains its
bidirectional docs/status/retryability test without copying other workers.
`api_errors_cross_worker.rs` stays in the source repository: its subject is
consistency across api/ingest/stream/repair and MB, not an independent MB
assertion. Source consumer and NNTune E2E/SQL tests likewise remain there.
No assertions are deleted to make the standalone crate compile.

- `workers/mb/testdata/search_pagination_golden.json`: 50 synthetic IDs,
  numbered Golden Song labels and deterministic scores; no live query.
- `workers/mb/tests/fixtures/logging_golden.json`: synthetic timestamp,
  trace and cache-hit event, not a captured tail.
- Inline fixtures in exported Rust tests: literal/fake repositories,
  caches and object stores; credential/IP/email-shaped strings are inert
  negative privacy-validation vectors, not usable credentials or people.
- No public Actions artifact contains a live dump, production log, artwork,
  user record or unverified external fixture. No scan suppression is added.

Destination configuration tests preserve environment separation, disabled
workers.dev and binding-name pins. The example intentionally has no usable
provider IDs and is not deployment qualification. Public-reference and
provenance guards run on actual tracked payloads, not a placeholder file.

## Retained source PR — rework/split/reuse, not merge or discard

[`nntunev2#931`](https://github.com/TogetherWeOwn/nntunev2/pull/931) is retained
at `509f6f3843141bb55697fe77845b623ac9675c5d`. None of its unmerged changes
is exported here, and this package does not claim its delivery or terminal
disposition. Its original Security review, rounds and source owner remain
unchanged.

| Unique retained work/obligation | Successor or retained-source mapping |
|---|---|
| CC0 phase-2 projections; aliases, labels, works and relations | Retained in that exact source PR; later importer/projection package must deliberately reuse the safe subset with contribution mapping and equivalent tests. |
| Validator/admission invariants and atomic projection | Retained source; later importer/validator package must port or consume normally merged source, not invent an independent replacement here. |
| DLQ, prune, replay and projection tests | Retained source tests, to be mapped explicitly during the importer package; current pure worker ledger tests do not replace them. |
| Migration `0035` and real applied-ledger/staging obligations | Unapplied; no SQL target or ledger receipt is invented. Backend adoption and source disposition stay with the existing source owners. |

Importers and WS/2 packages can reuse the root workspace/path layout.
Complete ordered media, search/count semantics, generation/promotion,
commercial/supplementary and optional-hourly requirements remain future
acceptance, not silently dropped by extracting this synthetic foundation.
