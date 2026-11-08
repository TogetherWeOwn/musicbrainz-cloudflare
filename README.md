# musicbrainz-cloudflare

Foundation for a Cloudflare-native MusicBrainz data service. This repository
contains a clean, pinned export of the legacy read Worker and its structured
logging dependency. It does **not yet implement the official WS/2 API**.

## Workspace

- [`workers/mb`](workers/mb/README.md): legacy `/v1/mb` read API, native tests,
  and an opt-in R2 snapshot experiment.
- [`crates/nntune-logging`](crates/nntune-logging/README.md): log envelope,
  build identity, trace propagation and redaction contracts.
- [`provenance`](provenance/README.md): source revision, file/blob mappings,
  contribution history, fixture provenance and retained-source obligations.
- [`docs/ops.md`](docs/ops.md): unconfigured bindings, backend limits and
  on-demand, zero-idle-cost preview requirements.

The intended next API subset is release search, release/media hydration,
artist lookup and recording lookup using official WS/2 JSON contracts.
Existing custom responses and relevance scores are not WS/2 equivalents.
Full mirror/XML and client drop-in compatibility are not claimed.

## Verification

Full suites, lint and native/Wasm builds run on GitHub Actions. See
[CONTRIBUTING.md](CONTRIBUTING.md). No database, Cloudflare account, live
MusicBrainz feed or dump is needed by the synthetic test suite. No deploy
workflow or resource activation is included.

## Data and rights

Software is [MIT](LICENSE); see [NOTICE](NOTICE). MusicBrainz core data is
CC0, independently of the software licence. No live dump, supplementary
data, hourly feed, cover-art bytes or customer records are exported here.
Supplementary/hourly licensing and refresh requirements are separate future
work, not removed or satisfied by this package. No account or token is
requested by this foundation. MLHD+ is excluded. Linked artwork and video
remain subject to their own rights; a URL is not a redistribution licence.

MusicBrainz is a trademark of the MetaBrainz Foundation. This project is not
affiliated with or endorsed by MetaBrainz.
