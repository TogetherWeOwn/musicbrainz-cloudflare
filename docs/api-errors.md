# Legacy MB error registry

This is the custom `/v1/mb` contract, not the future WS/2 error/help shape.
Errors render exactly `code`, `message`, `retryable`; the HTTP status is
not duplicated inside the JSON body. Server failures and unknown codes
use fixed public text, never driver, query or secret details.

| Code | HTTP status | Retryable | Workers | Meaning |
|---|---|---|---|---|
| `invalid_request` | 400 | false | mb | Malformed supported input |
| `MB_SEARCH_EMPTY_QUERY` | 400 | false | mb | Empty search query |
| `not_found` | 404 | false | mb | No matching entity |
| `internal_error` | 500 | true | mb | Generic unexpected failure |

The module's bidirectional registry test rejects missing, mismatched and
undeclared MB documentation rows. Full cross-worker taxonomy consistency
remains source-owned; no unrelated worker is copied to satisfy isolation.
