# musicbrainz-cloudflare

A Cloudflare-native MusicBrainz data service with a MusicBrainz WS/2-compatible API.

- Serves MusicBrainz data from our own storage on Cloudflare Workers; it does not run the MusicBrainz server.
- Phase 1 covers the endpoints NNTune needs, using the official WS/2 request paths, parameters and response format.
- Long-term goal: full WS/2 mirror parity, so any client configured for a MusicBrainz mirror can use it.
- Only CC0 core data is ingested by default. Data under other licences (for example CC BY-NC-SA derived tables and the Live Data Feed) stays disabled unless the required MetaBrainz licence is in place.

MusicBrainz is a trademark of the MetaBrainz Foundation. This project is not affiliated with or endorsed by MetaBrainz.

## License

MIT. See [LICENSE](LICENSE).
