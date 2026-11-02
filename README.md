# AniMex Extensions

Репозиторий: https://github.com/Senkai350/animex-extensions

URL для подключения в AniMex (VPS): **https://senkai.fun/extensions/repo.json**

GitHub Pages (когда репозиторий публичный): **https://senkai350.github.io/animex-extensions/repo.json**

Публичный ключ и его SHA-256: [repository-key.json](repository-key.json).
SDK: https://github.com/Senkai350/animex-sdk


Независимые WASM-расширения: источники, трекеры и плееры. Приложение AniMex для
сборки не требуется: версионированный снимок SDK и контракта находится в vendor/sdk.

`modules.json` перечисляет публикуемые расширения. Метаданные, версия и разрешения
каждого расширения хранятся в его module.json. Общие файлы плееров находятся в
modules/player-embed-common. Dummy-модули — conformance fixtures, player-animego —
сохранённая старая реализация, они не публикуются в каталог.

```powershell
rustup target add wasm32-wasip2
# Тесты и сборка без ключа:
pwsh -File scripts/build.ps1 -Test -Unsigned
# Подписанная сборка (ANMX_SIGNING_KEY в окружении):
pwsh -File scripts/build.ps1 -Test
cargo run --locked --manifest-path vendor/sdk/Cargo.toml -p anmx -- index org.example.extensions "My extensions" https://example.com/extensions/ dist dist
```

Публикуйте всё содержимое dist/ по указанному HTTPS URL. Для подключения в AniMex
вставьте ссылку на repo.json в Модули → Каталог и репозитории.
Используйте чистый staging dist/ для каждого выпуска. Один ID должен иметь одну
актуальную версию в каталоге. Не перезаписывайте опубликованную версию другими байтами.

## VPS и GitHub Pages

Workflow подписывает один набор `.anmx` и создаёт два независимых подписанных
каталога с разными базовыми URL. Артефакт `vps-extensions` публикуется при каждом
push в main и ручном запуске; VPS проверяет новые успешные сборки каждые две минуты,
проверяет подпись и хеши и атомарно переключает каталог. Исходники не раздаются.

В приватном репозитории Pages пропускается и не блокирует публикацию VPS.
После открытия репозитория включите Settings → Pages → GitHub Actions:
публикация Pages возобновится автоматически; VPS продолжит работать.
Оба каталога используют существующий `ANMX_SIGNING_KEY`; ключ менять не нужно.
Файлы на VPS публичны, даже пока исходный GitHub-репозиторий приватный.
Инструкции сервера: [deploy/README.md](deploy/README.md).

## GitHub Pages

1. Загрузите этот проект в отдельный публичный Git-репозиторий с main branch.
2. Добавьте Actions secret ANMX_SIGNING_KEY: случайный 32-byte приватный seed
   Ed25519 в формате ed25519:base64. Храните ключ вне репозитория.
3. Выберите GitHub Actions в Settings → Pages.
4. Workflow publish.yml проверяет PR без секретов, а main собирает/подписывает
   пакеты и публикует repo.json, index.json и .anmx на Pages.

URL подключения: https://OWNER.github.io/REPO/repo.json. Публичный отпечаток ключа
следует опубликовать для пользователей отдельно от самого каталога.

Для создания нового расширения используйте существующий модуль того же типа как
пример, назначьте уникальный ID, задайте минимальные network/storage permissions,
добавьте его путь в modules.json. Источники и трекеры используют общий кеш метаданных
хоста; не добавляйте отдельные frontend-кеши. Собственные внутренние кеши должны
обходиться при scoped host config cache.force_refresh. Никогда не кешируйте resolved
stream URLs и credentials в metadata cache. Подробности контракта смотрите в
docs/contracts/module-cache.md и документации SDK.


Генерация ключа в приватный файл вне репозитория (существующий ключ не перезаписывается):

```powershell
powershell -File scripts/new-signing-key.ps1
$env:ANMX_SIGNING_KEY = (Get-Content -Raw "$env:USERPROFILE/.animex/signing-key.private").Trim()
```

