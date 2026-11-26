# YummyAnime source

Published source `org.yummyani.source`. Build and test without a signing key with
`pwsh -File scripts/build.ps1 -Test -Unsigned -ModuleId org.yummyani.source`.
See the [release guide](../../docs/releasing.md) for signed packages.

Implements the JSON API of `old.yummyani.me` (every response is wrapped into
`{"response": …}` or `{"error": …}`): catalog `GET /api/anime`, search
`GET /api/search?q=…`, release `GET /api/anime/{id}`, episodes and players
`GET /api/anime/{id}/videos`. Release ids are the numeric site `anime_id`;
`anime_url` is only a web-route slug and never an API key. Catalog pages hold
20 items and request one extra row (`limit=21&offset=(page-1)*20`) to prove
the next page; the extra row only proves `has_next` and is never shown. Only
the real ascending site sorts (`year`, `title`, `rating`, `views`, `id`) are
offered; the API enum rejects descending and date-added values, and the only
offered filter is the validated `status` select (`ongoing`, `released`,
`announcement`) sent as `status=…` on `/api/anime`. Search behavior beyond
`q`/`limit`/`offset` is not verified, so no filter or sort is advertised for
search (the site client does send `offset`, but whether the API honors it
past the first page is unconfirmed).

Reserved feeds: the site has no updates feed and rejects descending sorts, so
`get_latest` and `get_popular` return a documented successful empty page —
an empty page means "unsupported operation", never fictitious rows, never
least-viewed titles mislabelled as popular, and never a global home error.

Videos are grouped by the site `data.player_id` (1 Aksor, 2 Alloha, 3 CVH,
4 Kodik) into independent candidates with one dub variant per exact voice —
never a fallback episode or voice. `get_players` accepts only the canonical
episode id of the requested release (`{anime_id}-{number}`). Embed URLs are
validated (HTTPS, exact host, no credentials, no non-default port) before
they reach the players. The CVH candidate requires an iframe whose `episode`
matches the row and whose `dubbing_code` is non-empty and unambiguous; it
carries the real integration context (`anime_id` as CVH media id,
`dubbing_code` voice, exact `episode`, publisher `745`, aggregator `mali`,
season 1) with referer `https://ru.yummyani.me/`, Kodik with
`https://old.yummyani.me/`. Alloha rows must carry every required parameter
(`token_movie`, `translation`, `season`, `episode`, `token`) exactly once,
never blank, and a clearly numeric `episode` must match the row; the same
rules are enforced again by the `org.alloha.player` module. Parameter values
are never inspected or printed. `get_players` always requests a fresh videos
list: Alloha tokens are ephemeral playback data, never cached or logged by
this module, and release/episode metadata stays persistable in the host
output cache (no opaque `sourceRef`). Optional `mirror` configuration still
requires the mirror host to be present in the module's network permissions.

See [the reusable provider contract](../../docs/contracts/provider-embed.md).
