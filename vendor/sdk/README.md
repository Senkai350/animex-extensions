# AniMex SDK

https://github.com/Senkai350/animex-sdk


Самостоятельный Cargo workspace для авторов WASM-расширений AniMex.

- `wit/animex.wit`: публичный контракт ABI (API 2).
- `crates/anm-sdk`: host bindings, protocol types и helpers.
- `crates/anm-types`: сериализуемые DTO.
- `crates/anm-manifest`: проверка манифеста и Ed25519.
- `crates/anm-repository`: подписанный контракт каталогов v1.
- `tools/anmx`: подпись/упаковка расширений и генерация каталога.

```sh
cargo test --workspace --locked
cargo run --locked -p anmx -- public-key
```

CLI читает `ANMX_SIGNING_KEY` из окружения (32-byte Ed25519 private seed,
`ed25519:<base64>`). Никогда не добавляйте приватный ключ в исходники.

```sh
cargo run --locked -p anmx -- pack module.json module.wasm extension-1.0.0.anmx
cargo run --locked -p anmx -- index org.example.extensions "Example" https://example.com/extensions/ dist dist
```

Каталог подписывается отдельным payload с domain separation. `pack` заменяет
публичный ключ автора реальным ключом подписи и подписывает весь Manifest и WASM.
Версии и разрешения берутся из module.json. `index` извлекает именно подписанный
манифест из пакета, проверяет подпись и формирует ссылки и SHA-256.

В монорепозитории исходники SDK редактируются здесь, а снимок для самостоятельной
сборки расширений обновляется командой `npm run sdk:sync` из корня. В экспортированном
SDK зависимости от исходников приложения отсутствуют. Требуются stable Rust и
wasm32-wasip2 target для сборки самих расширений.

## Module data caching

Source/tracker output metadata is cached by the host. Do not implement frontend TTL
caches in modules. Any internal module data cache must honor scoped host configuration
`cache.force_refresh`, while retaining rate limits, retry limits and circuit breakers.
Never put playback credentials, resolved stream URLs or player handles in this cache.
A result containing a non-null sourceRef anywhere is memory-only at the host and must
not be modified to make it persistable. Ordinary successful empty/null capability
outputs are valid cache results. SDK and WIT changes must remain compatible with the
host API version or explicitly increment it.
