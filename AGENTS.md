# AniMex development instructions

## Module data caching

- Source and tracker metadata must use the application's shared output cache by
  default. New modules automatically use this host-owned functionality; do not
  add independent frontend caches with hard-coded lifetimes.
- Route frontend metadata reads through the `module_data` API wrappers and the
  backend's cached metadata dispatcher. When adding a new metadata method,
  register its cache policy and include its full normalized arguments in the key.
  Include module identity/version, module binary fingerprint and cache schema in
  the namespace so updates and different providers cannot share stale results.
- Lifetimes are settings, not per-component constants: sources default to 1 day,
  tracker title/search/episode/statistics data to 3 days, and tracker
  character/staff/video/image/related/similar data to 7 days. Preserve user choices
  and apply lifetime changes to existing records on the next cache read.
- Show compact, muted provenance (`Данные от …`) where cached data is displayed.
  Provide a small outline refresh icon for that exact section/provider, including
  successful empty results. Use cache metadata from the backend for freshness;
  do not infer that data is cached merely because a request finished.
- Explicit refresh must bypass successful data caches at every applicable layer,
  including in-memory UI caches and a module's own data cache. Preserve request
  pacing, retry limits, negative-response protection and circuit breakers. Use
  the scoped `cache.force_refresh` host configuration for first-party module
  caches; new modules with their own data cache must honor it.
- Cache successful output values, including empty lists and unsupported/null
  capability results. Do not cache exceptions or overwrite the last successful
  value after a failed refresh. Retain visible successful data while reporting
  refresh errors. Protect against late responses after navigation or cache clear.
- Keep cache storage bounded. Ordinary metadata persists between app launches;
  results recursively containing a non-null `sourceRef` (opaque playback payload)
  are memory-only and must never be written to disk. This applies to the complete
  result, including nested episode/catalog items. Do not strip or alter the
  playback reference to make a result persistable. Global cache clearing must
  invalidate disk records, host memory and frontend provenance.
- Do not cache playback credentials, resolved stream URLs, player handles,
  side-effecting commands or authentication data through this metadata cache.
- Verify key isolation, expiry, persistence, forced refresh, failed refresh,
  concurrent identical reads and global clearing when changing this layer.

See [the module cache contract](docs/contracts/module-cache.md) for IPC fields,
policy groups and frontend behavior.
