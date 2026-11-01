# Module output cache

AniMex caches successful metadata returned by source and tracker modules at the
application boundary. The cache belongs to the host; modules do not need to
implement output persistence to benefit from it. Installing another module with
the same supported metadata interfaces automatically uses the same mechanism.
This cache is separate from the guest's optional HTTP cache and from media cache.

## Lifetime settings

| Setting | Default | Applies to |
|---|---:|---|
| `cache.source_days` | 1 day | All source metadata reads |
| `cache.tracker_days` | 3 days | Tracker title details, search, episodes and audience statistics |
| `cache.tracker_static_days` | 7 days | Tracker characters, staff, videos, screenshots, related and similar titles |

All values are integers from 1 to 365, editable under **Настройки → Кэширование
данных**. Invalid input is rejected by both the UI and backend. If settings are
missing or malformed, defaults apply. Freshness is calculated from the record's
stored time using the current policy on every read, so editing a lifetime also
affects existing records. A day is 24 hours.

## IPC

```ts
invoke<CachedModuleResult<T>>("module_data", {
  moduleId: "org.myanimelist.tracker",
  method: "tracker_anime",
  args: { id: "20" },
  refresh: false,
});

interface CachedModuleResult<T> {
  data: T;
  cache: {
    moduleId: string;
    method: string;
    key: string;       // opaque host cache key
    cached: boolean;   // true only when this response came from the output cache
    storedAt: number;  // Unix milliseconds
    expiresAt: number; // Unix milliseconds, using the current lifetime setting
  };
}
```

`method` uses existing metadata command names (`source_release`,
`tracker_characters`, `titles_related`, `titles_similar`, `module_statistics`,
etc.). `args` contains that method's normal arguments excluding the module ID;
related/similar/statistics use `{ release: { id, payload } }`. A missing/null
module ID resolves to the active source/tracker for methods with that role.
Related/similar/statistics calls identify their module explicitly.

Frontend API wrappers unwrap `data` and retain their existing typed return values.
They expose a final optional refresh argument and retain the cache envelope for
provenance UI. Legacy metadata IPC commands share the same output cache while
preserving their original raw response shape.

## Identity, lifetime and refresh

The host namespaces keys by module identity, version and WASM fingerprint, method,
full normalized arguments and cache schema. Different providers, pages, search
parameters and opaque release references therefore cannot reuse one another's
results. Cache keys are digests; avoid logging raw request payloads.

Ordinary successful metadata persists between app launches in bounded host cache
storage. A result recursively containing a non-null `sourceRef` carries an opaque
playback payload: the complete result is cached only in bounded host memory,
never on disk. For example, a source release or an episode list with playback
references can be reused during the current app session, but must be requested
again after restarting. Null/absent references do not prevent persistence.
The host likewise excludes non-null `payload`, `handle`, `episodeRef` and
`releaseRef` fields from disk persistence. It does not remove or rewrite these
fields in the returned data.

Empty arrays and null capability results are valid successful values. Errors
are not cached. `refresh: true` bypasses the stored output and replaces it only
after a successful module execution. Failed refreshes return an error while the
previous successful record remains available; they are not reported as fresh data.

Guest data caches honor the scoped host configuration key `cache.force_refresh`.
Only successful data reads bypass their cache in this mode. Pacing, bounded
retries, rate-limit cooldowns and circuit breakers still apply. Updating a
module changes its cache namespace automatically.

## User interface

Cached sections provide one small outline refresh button in the top-right heading
or carousel controls. It refreshes every provider contributing to that section,
including providers with successful empty or unsupported results. Provider names
and backend cache timestamps appear in the button tooltip instead of separate
attribution rows. A muted `Данные получены DD.MM.YYYY HH:mm` label to the left
of the button shows the oldest successful backend timestamp among the section's
providers, so partial failures never make older results appear newly fetched. Loading and error feedback must not replace successful data
with an empty state while a refresh is running; failures remain available from
the refresh control and section error messages.

Global **Очистить кэш** clears host output cache, guest cache and frontend
provenance. Frontend in-memory stores must not bypass the output cache's expiry
or ignore explicit refresh requests.

Playback candidates, opaque playback handles, resolved stream URLs, credentials,
mutating commands and authentication state are outside this metadata cache.
