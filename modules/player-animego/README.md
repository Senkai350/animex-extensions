# Legacy experimental adapter

Superseded by independent CVH, Kodik and AniBoom players. See
[provider embed contract](../../docs/contracts/provider-embed.md).
This module is not bundled or emitted by the updated AnimeGO source.

# source-animego / player-animego

Third-party AnimeGO (animego.me) source + player pair for AniMex.

- Source: `org.animego.source` — catalog/top-250/search/filters, release
  page, episode schedule, voice players (protocol `animego.embed.v1`).
- Player: `org.animego.player.embed` — resolves `animego.embed.v1`
  payloads: AniBoom (HLS/DASH from the embed page), CVH / CdnVideoHub
  (playlist + video API), Kodik is currently rejected explicitly.

## Build

```bash
cd modules/source-animego && cargo build --release --target wasm32-wasip2
cd ../player-animego   && cargo build --release --target wasm32-wasip2
```

Tests:

```bash
cargo test   # in each module directory (native, offline fixtures)
```

## Package (manual .anmx)

These modules are intentionally NOT added to `scripts/build-modules.ps1`
(they are unsigned third-party). Package by hand:

```bash
id=org.animego.source                      # or org.animego.player.embed
stage=$(mktemp -d)
cp target/wasm32-wasip2/release/anm_source_animego.wasm $stage/module.wasm   # player: anm_player_animego.wasm
cp module.json $stage/
printf '# AnimeGO module\nUnsigned community module: enable Developer mode to install.\n' > $stage/README.md
(cd $stage && zip -q -X /tmp/$id.anmx README.md module.json module.wasm)
```

Requirements: package contains only `module.wasm`, `module.json`,
`README.md`; `api_version` must be 2; `permissions.network` lists exact
hosts (the host allowlist matches the host or any subdomain).

## Install in the app

Manifests here carry no signature. Install with **Settings → Developer
mode** enabled (or accept the developer trust prompt), then reload the
module. Optional config key `mirror` (source module) overrides
`https://animego.me`.

## Verified against

Selectors and endpoints synchronized with live homepage capture and three
community parsers: `anime-parsers-ru` (YaNesyTortiK), `anime-dl-core`
(ialakey), `moe-parsers` (nichind). The live site currently returns 500 on
dynamic pages for datacenter IPs (DDoS-Guard passes; the app itself is
down), so selector regressions should be checked with fixtures in
`src/tests.rs` after the site recovers.
