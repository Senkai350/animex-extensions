# AniMex Extensions

Репозиторий: https://github.com/Senkai350/animex-extensions

URL для подключения в AniMex (VPS): **https://senkai.fun/extensions/repo.json**

GitHub Pages (только ручной запасной workflow): **https://senkai350.github.io/animex-extensions/repo.json**

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

## Локальная сборка и публикация (основной способ)

Сборку запускает разработчик или агент на своём компьютере. GitHub Actions
не нужен: автоматические cloud-сборки при push/PR отключены, workflow оставлен
только для ручного запуска как запасной вариант.

```powershell
# Проверить и собрать подписанные пакеты локально:
pwsh -NoProfile -File scripts/release.ps1
# После коммита и push исходников в main — собрать и загрузить GitHub Release:
pwsh -NoProfile -File scripts/release.ps1 -Publish
```

Нужен существующий `ANMX_SIGNING_KEY` или `~/.animex/signing-key.private`.
Скрипт проверяет ключ, запускает тесты SDK и модулей, собирает полный каталог
в новую папку `dist-releases/` и публикует готовые assets через draft → Latest.
Повышайте `version` в `module.json` при изменении плагина.

После первого выпуска можно подключать плагины **напрямую из GitHub**:

**https://github.com/Senkai350/animex-extensions/releases/latest/download/repo.json**

Публичный репозиторий позволяет приложению скачивать пакеты без авторизации.
Адрес каталога постоянный; адреса пакетов закреплены за конкретным выпуском.

VPS может зеркалировать те же готовые пакеты каждые две минуты, без сборки.
Для этого один раз замените старый Actions-based sync-скрипт на версию,
читающую GitHub Releases. Оба каталога используют прежний публичный ключ.

Подробности: [локальный выпуск](docs/releasing.md),
[проверка и установка зеркала VPS](deploy/README.md).

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

