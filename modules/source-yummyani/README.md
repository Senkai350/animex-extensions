# YummyAnime source

Source module `org.yummyani.source`. See [the release workflow](../../docs/releasing.md).

The module uses `GET /api/anime` for the catalog and title search,
`GET /api/anime/{id}` for releases, and `GET /api/anime/{id}/videos` for
episodes and players. Release IDs are numeric `anime_id` values. Catalog
pages show 20 titles and request one extra row to detect the next page.

The catalog offers the site's genre inclusion and exclusion, types, statuses,
year bounds, seasons, episode count bounds and age ratings. The genre IDs and
labels come from the site's public filter form. The API combines these filters
with `q` and supports either direction through `sort_forward`. The default
puts the most relevant results first, orders titles A–Z, and uses descending
order for numeric sorts. The random-title action uses
`sort=random&limit=1` with the same active filters.

The reserved `get_latest` and `get_popular` feeds return a successful empty page.

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
