# musicbrainz-cloudflare

A Cloudflare-native MusicBrainz data service with a MusicBrainz WS/2-compatible API.

- Serves MusicBrainz data from our own storage on Cloudflare Workers; it does not run the MusicBrainz server.
- Phase 1 covers the endpoints NNTune needs, using the official WS/2 request paths, parameters and response format.
- Long-term goal: full WS/2 mirror parity, so any client configured for a MusicBrainz mirror can use it.
- Only CC0 core data is ingested by default. Data under other licences (for example CC BY-NC-SA derived tables and the Live Data Feed) stays disabled unless the required MetaBrainz licence is in place.

MusicBrainz is a trademark of the MetaBrainz Foundation. This project is not affiliated with or endorsed by MetaBrainz.

## Direction

Last reviewed: 2026-11-16.

This project is in early development. The introduction above describes intended
behavior and goals, not a service that is currently available. Development is
currently deferred pending prerequisites; there is no committed service date.
The source is public, while the intended service is private. Anonymous access to
any hosted service is not promised.

The initial intention is to support the MusicBrainz WS/2 requests needed by
NNTune on Cloudflare. Before claiming broader mirror compatibility, the project
will define an explicit supported-request matrix covering paths, parameters,
paging and response formats. Full mirror parity is a long-term goal, not a
current capability or delivery commitment.

Running the MusicBrainz server itself and making all MusicBrainz data available
regardless of licence are not goals. The licence boundaries above continue to
apply. These intentions may change and do not imply a delivery schedule.

Questions and feedback are welcome through [GitHub issues](https://github.com/TogetherWeOwn/musicbrainz-cloudflare/issues).

## License

MIT. See [LICENSE](LICENSE).
