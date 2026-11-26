# Provider embed playback

A source returns one player candidate per provider for a specific episode.
AniMex picks a player module by the candidate's `protocol` and passes the
candidate's opaque JSON payload to it. The player lists dubs in payload order,
marks the first as default, and resolves the selected dub by its exact ID.

| Protocol | Sources in this repository | Player result |
| --- | --- | --- |
| `cvh.embed.v1` | AnimeGO, YummyAnime | Native DASH or HLS with a supplied CVH context, iframe without it |
| `kodik.embed.v1` | AnimeGO, YummyAnime | Native HLS with the provider's available renditions |
| `aniboom.embed.v1` | AnimeGO | Native HLS or DASH |
| `alloha.embed.v1` | YummyAnime | Official Alloha iframe |
| `aksor.embed.v1` | YummyAnime | Aksor video iframe |

The shared payload contains an optional `referer` and a `variants` array of
1–128 entries. Each variant contains `id`, `label`, and `url`, with an optional
`language`. IDs are non-empty and unique within a payload. Labels must
contain text. URLs and referers use absolute HTTPS without credentials.

A CVH variant can also contain `cvh` with `mediaId`, `publisherId`,
`aggregator`, `season`, `episode`, and `voice`. The source supplies this
integration context. The CVH player requires a match for season, episode,
and voice for native playback. For provider-specific URL checks, see the
[Alloha](../../modules/player-alloha/README.md) and
[Aksor](../../modules/player-aksor/README.md) READMEs.

Playback payloads, signatures, and resolved URLs are transient. The host
enforces each module's network permissions. The Rust wire types are in
[provider_embed.rs](../../vendor/sdk/crates/anm-sdk/src/protocol/provider_embed.rs).
