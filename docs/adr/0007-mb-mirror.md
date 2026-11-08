# Standalone legacy mirror boundary

A clean, pinned source export preserves the worker and logging contracts
without transferring the source application's database, deployment,
consumer bindings or history. Compatible package paths let importer and
WS/2 packages land without competing workspace roots.

The art mapping uses the unprovisioned example labels
`musicbrainz-example-art-staging` and `musicbrainz-example-art-prod`.
No cover-art fetching, proxy activation or redistribution is authorized.
Stored release URLs remain unchanged. Environment separation is tested,
but is not proof of deployed resource ownership.

The legacy SQL adapter and snapshot experiment are not the accepted final
WS/2 service. Backend adoption, importer/schema lineage, complete ordered
media and official wire contracts require separate implementation and
qualification. See [ops](../ops.md) and [provenance](../../provenance/README.md).
