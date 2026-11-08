# Contributing

Follow the organization [contribution contract](https://github.com/TogetherWeOwn/.github/blob/20d95fd8c249c7192c364f8b41519fca3fc873df/CONTRIBUTING.md)
and [pipeline standard](https://github.com/TogetherWeOwn/.github/blob/20d95fd8c249c7192c364f8b41519fca3fc873df/STANDARD.md).
Use the PR template verbatim, including Attestation and Model Used.
Conventional Commits titles, squash merges and public-safe descriptions
are required. Never include internal tracker references, private host
paths, credentials or unreviewed live data in repository/GitHub content.

## GitHub Actions first

Full suites and CI gates execute only in GitHub Actions. Open a draft PR
and push to its branch; record run/job URLs and conclusions for the exact
head SHA. Local execution is limited to focused reproduction of one
failing test or crate. Heavy focused reproduction requires the approved
guarded offload path, source-only inputs, no environment forwarding and
verified cleanup; no raw shared-host heavy build or full-suite fallback.
Formatting alone does not compile and can be applied locally.

Actions runs:

- Native tests: `cargo test --locked --workspace --all-targets`.
- Strict lint: `cargo clippy --locked --workspace --all-targets -- -D warnings`.
- Format: `cargo fmt --all -- --check`.
- Wasm: `cargo build --locked -p nntune-mb --target wasm32-unknown-unknown --release`.
- Export/provenance, public-reference and configuration guards, workflow lint,
  full-history secret scan and the honest `ci-ok` aggregate.

Default CodeQL setup remains the supported scanner configuration. Do not
add a conflicting advanced workflow, fake a review status or bypass a
missing first-target analysis. Real required review/scan/check evidence,
independent QA/Security and answered findings remain merge gates. Never
trigger or wait for an advisory review to start. Reply to each finding in
its existing thread using `Fixed in <sha>`, `Rejected: <proof>` or
`Duplicate of <link>`.

Before marking ready, record a fresh-context diff review and all finding
dispositions plus exact-head Actions evidence. No full suite transcript
from the local host substitutes for Actions. Final review performs the
exact-head pinned squash merge after all applicable release gates.

## Public data boundary

Fixtures, logs and Actions artifacts must be synthetic or individually
verified CC0. No live dump/feed acquisition, customer records, provider
resource IDs, source environment inventory or linked artwork/video bytes
belong in this extraction. Keep original MIT notices and per-file source
provenance. A fixture URL is inert test text, not permission to fetch it.
Changes to extracted files update their provenance digest and adaptation
record; tests are never weakened to make isolation pass.

## Dependency controls

See [supply-chain.md](docs/supply-chain.md). Do not install dependencies
just to discover their hooks; lock changes and build scripts need review.
No registry package publication, deployment or spend is implied by a PR.
Preview/staging changes must satisfy [zero-idle ops](docs/ops.md).
