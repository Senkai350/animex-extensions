# Aksor

Reusable player for `aksor.embed.v1`. See [provider embed contract](../../docs/contracts/provider-embed.md).

Returns the Aksor iframe at `https://player.aksor.tv/video/<32 lowercase
hex>` after validating the exact host and the exact `/video/<id>` path
shape. HTTPS only, no credentials, no non-default ports, no native
capabilities.
