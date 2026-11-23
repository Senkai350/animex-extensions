# Alloha

Reusable player for `alloha.embed.v1`. See [provider embed contract](../../docs/contracts/provider-embed.md).

Returns the official Alloha iframe at `https://alloha.yani.tv/` after
validating the exact host, the exact `/` path and the required
`token_movie`, `translation`, `season`, `episode`, `token` parameters.
HTTPS only, no credentials, no non-default ports, no native capabilities;
tokens stay inside the transient playback payload.
