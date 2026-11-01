# AnimeGO source

Bundled source `org.animego.source` (0.1.5). Build and package with
`scripts/build-modules.ps1 -ModuleId org.animego.source`; startup installs it.

Catalog IDs retain the canonical slug so navigation and history reopen valid
release URLs. Episodes are read from paginated player menus; their opaque
references retain the real episode-page IDs. Provider buttons are grouped into
independent CVH, Kodik and AniBoom candidates, with separate dub variants.

See [the reusable provider contract](../../docs/contracts/provider-embed.md).
Metadata uses the host output cache; playback pages and credentials have no
independent persistent cache. Optional `mirror` configuration still requires
the mirror host to be present in the module's network permissions.

The older `player-animego` module is not used by this source.
