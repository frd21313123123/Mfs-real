# RealFlow Traffic 0.3.1 — Rust

Нативный прототип трафика и ATC для MSFS 2020: OpenSky ADS-B, синтетические
самолёты, наземное движение и локальные FSLTL-модели. **Python для запуска и
сборки не требуется.** Движок и лаунчер написаны на Rust.

Нативная интеграция SimConnect, FSLTL-анимации, COM/XPDR и звук всё ещё требуют
проверки внутри MSFS 2020. Офлайн-тесты не подтверждают работоспособность в игре.

## Запуск готовой Windows-сборки

Скачайте `RealFlow-Windows-x64.zip` со страницы
[GitHub Releases](https://github.com/frd21313123123/Mfs-real/releases/latest). Распакуйте
`RealFlow-Windows-x64.zip` и откройте `realflow-launcher.exe` или
`start-launcher.bat`. Держите `realflow.exe` рядом с лаунчером. Rust toolchain,
Python, pip и отдельный Visual C++ runtime для этих программ не требуются.

Лаунчер редактирует совместимые JSON-профили, запускает диагностику, сканер
FSLTL, офлайн-превью карты, демо трафика/ATC и экспериментальный MSFS-сеанс.
Вывод запуска отображается в окне; кнопка остановки даёт движку выполнить
штатную очистку своих самолётов. Радио пилота принимает текстовые команды.

По умолчанию профиль лаунчера: `%USERPROFILE%\.realflow\config.json`.
Можно открыть существующий `config.json`. Профиль записывается атомарно;
относительные пути FSLTL/аэропорта разрешаются относительно профиля.
Изменения применяются при следующем запуске. Ключи OpenSky в профиль не пишутся.

CLI работает самостоятельно, из любой папки. Вымышленная демонстрационная
геометрия встроена в бинарник:

```powershell
.\realflow.exe doctor
.\realflow.exe --config config.json demo --steps 600 --output demo-results.json
.\realflow.exe atc-demo --steps 600 --output atc-demo-results.json
.\realflow.exe scan --fsltl "D:\MSFS\Community\fsltl-traffic-base"
```

Демо никогда не создаёт самолёты в MSFS. На Linux доступны CLI, лаунчер и
офлайн-движок; SimConnect и нативный голосовой ввод/вывод требуют Windows.

## Скачать модели трафика через лаунчер

Откройте «Скачать модели трафика» под полем FSLTL и нажмите
«Скачать FlyByWire Installer» (Windows). Выберите место сохранения: скачивание
выполняется в фоне с отображением прогресса и отменой, файл проверяется по
SHA-512 из официального релиза. Установщик не запускается автоматически.
Нажмите «Запустить скачанный установщик» и установите в нём **FSLTL Traffic
Base Models**. Затем вернитесь в RealFlow и нажмите «Найти установленные
модели и сохранить путь». Если Community находится в нестандартной папке,
выберите `fsltl-traffic-base` вручную и сохраните профиль.

Для скачивания нужен интернет. При ошибке доступна ссылка на официальную
страницу. Модели распространяет и устанавливает FlyByWire Installer; RealFlow
не содержит копию пакета FSLTL и не запускает сторонний установщик без нажатия
кнопки пользователем. Linux предоставляет ссылку; интеграция MSFS требует Windows.

## Сборка из исходников

Фиксированная версия Rust: **1.90.0**, зависимости зафиксированы в `Cargo.lock`.
Установите Rust через официальный rustup. Для Windows выберите MSVC toolchain
и установите Visual Studio 2022 C++ Build Tools с Windows SDK.

```powershell
rustup target add x86_64-pc-windows-msvc
.\scripts\build.ps1
```

Скрипт выполняет форматирование, Clippy, Rust-тесты, release-сборку, запуск
готовых бинарников и упаковку. Результат: `release/RealFlow-Windows-x64.zip`
и `release/SHA256SUMS.txt`. MSVC собирается с `+crt-static`; программы не
зависят от Python или распространяемого отдельно VC runtime. Системные
Windows DLL, драйверы графики и MSFS runtime остаются необходимыми.

Linux (для сборки: C-компилятор, pkg-config, OpenSSL development headers;
для графического запуска: X11/Wayland, OpenGL и desktop file portal):

```bash
# Ubuntu/Debian, development dependencies:
sudo apt-get install build-essential pkg-config libssl-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev
./scripts/build.sh
# or:
cargo test --locked --all-targets
cargo build --release --locked --bins
./target/release/realflow-launcher
```

Для серверной/CLI-сборки без графических зависимостей:
`cargo build --release --locked --no-default-features --bin realflow`.
GitHub Actions автоматически публикует артефакты `RealFlow-Linux-x64-*`
для Linux и `RealFlow-Windows-x64-*` для Windows при push и pull request.
Windows-сборка также запускается вручную через workflow_dispatch.

При отправке тега `v<версия из Cargo.toml>` workflow **Publish compiled releases**
собирает и проверяет обе платформы, затем публикует Windows ZIP, Linux tar.gz
и общий `SHA256SUMS.txt` в GitHub Releases. Например, для версии 0.3.0:
`git tag v0.3.0` и `git push origin v0.3.0`. Релиз появляется только после
успешной сборки обеих платформ.

Linux-бинарники используют системные libc/OpenSSL/графические библиотеки;
пакет Linux не заявляется универсальной статической сборкой.

## Сценарии аэропортов, поиск по ICAO

В Windows-лаунчере есть вкладка **«Сценарии аэропортов»**. Введите ICAO
(например, `PATK`, `SAQU` или `SADQ`), выберите найденный пакет и
нажмите **«Скачать и установить»**. Приложение находит готовый ZIP в
GitHub Releases автора, скачивает его через HTTPS, проверяет структуру
пакетов MSFS и переносит папки в выбранное место. Для каждого пакета
`manifest.json` и `layout.json` должны лежать в его корне.

**Папка назначения.** Кнопка «Найти Community» читает
`InstalledPackagesPath` из `UserCfg.opt` Steam/Microsoft Store и выбирает
активную `Community`. Любую другую существующую папку можно выбрать
вручную. Сценарии, скачанные самостоятельно, устанавливаются кнопкой
**«Установить свой ZIP…»**.

**Доступные изначально freeware-пакеты:**

- `PATK`, `2AK7`, `AK19`, `51AK`, `AK44`, `0AA5`: Talkeetna Region Pack,
  © [julysfire](https://github.com/julysfire/MSFS2020-Talkeetna_Region/releases), GPL-3.0.
- `SAQU`: Ezpeleta, © [Cronch / Agnalim](https://github.com/Cronch/fs2020-saqu-scenary/releases), MIT.
- `SADQ`: Quilmes, © [Cronch / Agnalim](https://github.com/Cronch/fs2020-sadq-scenary/releases), MIT.

Список источников расширяется через `scenery/catalog.json`, но это **не**
каталог абсолютно всех аэропортов мира. Проект не собирает платные
сценарии и не обходит авторизацию сторонних сайтов, включая Flightsim.to.
Для пакетов без прямого официального релиза используется ручная загрузка
и импорт ZIP. Исходники из GitHub не принимаются за готовый игровой
сценарий.

Установщик ограничивает размер ZIP (1 ГБ), суммарную распаковку (4 ГБ)
и количество файлов; распаковывает во временную папку с проверкой путей,
не удаляет и не перезаписывает существующие пакеты. Для обновления
сделайте резервную копию старой версии и удалите её вручную перед
установкой. Скачивание можно отменить. ZIP должен содержать готовые
пакеты MSFS, а не один архив с исходным кодом. Установка через GUI
поддерживается на Windows, реальные сценарии и их совместимость необходимо
проверять в MSFS 2020 после перезапуска игры.

## MSFS 2020

Требуются Windows x64, уже запущенный MSFS 2020, установленный FSLTL Traffic
Base Models и легитимный `SimConnect.dll` из runtime/SDK. DLL и модели не
включены в архив. Конкурирующие AI-инжекторы следует отключить на время теста.

```powershell
.\realflow.exe --config config.json run --bridge simconnect --fsltl "D:\MSFS\Community\fsltl-traffic-base" --duration 60
```

Запись позиций включается только явным `--allow-motion` (либо соответствующим
параметром лаунчера). Для наземного движения передайте проверенный граф:
`--airport airport.json --allow-motion`. Граф с меткой FICTIONAL отвергается
в нативном режиме. Никакие dummy-модели не подставляются при запуске MSFS.
Ctrl+C, `/quit`, таймер или кнопка остановки лаунчера завершают сеанс с очисткой.

## OpenSky, данные и аэропорт

OpenSky поддерживает анонимный доступ и OAuth2 через `OPENSKY_CLIENT_ID` /
`OPENSKY_CLIENT_SECRET`. HTTPS использует проверку сертификатов и системный
proxy; опрос выполняется в отдельном потоке, с интервалом ≥60 секунд,
повторным получением токена и задержкой после 401/403/429.
Реальные ADS-B самолёты наблюдаются и экстраполируются, ATC ими не управляет.
Для подбора типа можно передать `--metadata aircraft.csv` с колонками
`icao24,icao_type`. Файлы самолётов FSLTL только читаются.

```powershell
.\realflow.exe convert-airport --xml examples/sample_airport_source.xml --output airport.json
.\realflow.exe atc update
.\realflow.exe atc frequencies --icao UUEE
.\realflow.exe atc session --icao UUEE --destination ULLI --callsign AFL101 --runway 24L --com1 121.900
```

Конвертер принимает исходный scenery XML, не compiled BGL. Геометрию необходимо
проверить в игре. OurAirports загружается только явно командой `atc update`;
это неофициальные community-данные, частоты необходимо сверять с AIP.
Неизвестные частоты не выдумываются. Для собственного CSV: `atc --csv file.csv …`.

## ATC и голос без Python

`run --atc-csv airport-frequencies.csv --airport airport.json --atc-airborne`
координирует собственные наземные и воздушные AI через общий контроллер полос.
Параметры `--pilot-callsign`, `--pilot-destination`, `--pilot-runway` добавляют
радио пилота; COM1/COM2/XPDR читаются из SimConnect. На mock используйте
`--atc-monitor-frequency 121.900`. `/status`, `/tune 121.900` (offline), `/quit`.

`--atc-voice` использует встроенный **Windows SAPI**, без платных API.
Распознавание речи включается через `--pilot-voice-model` либо
`atc session --voice --vosk-model`. Нужны локальная английская 16 kHz модель
Vosk и официальная нативная `libvosk.dll` со всеми её companion DLL рядом с
`realflow.exe` (или путь `VOSK_DLL`). Они не скачиваются автоматически.
Пустой Enter записывает 6 секунд PCM16 через Windows multimedia API и передаёт
звук локальной Vosk. Звук не отправляется в облако. Голос не заменяет внутренний
MSFS ATC audio bus, не подключается к VATSIM и требует проверки на Windows.

## Настройки и ограничения

Сохранены поля исходного JSON: `mode`, `airborne_limit` (0–35), `ground_limit`
(0–30), `sync_real_flights`, `fetch_seconds`, `max_radius_km`, `fsltl_path`,
`airport_graph`, `enable_simconnect_position_writes`, `realistic_taxi`,
`runway_control`, `fps_target`. Последние три сохраняются для совместимости,
но не переключают поведение движка: блокировки полос/дорожек работают всегда;
целевой FPS не управляет производительностью игры. Это отмечено в лаунчере.

Поведенческие Rust-тесты проверяют лимиты, приоритет LIVE, устаревшие данные,
маршруты, разделение наземных самолётов, конфликты полос, радио/readback,
go-around, ограничения кинематики и передачу ground↔air с сохранением ID.
CI выполняет тесты на Linux/Windows и запускает скомпилированные демо.
Эти проверки не заменяют MSFS-in-the-loop испытания.

Ранее написанный Python-код и тесты сохранены в `legacy/python` как reference
для переноса; они не участвуют в нативной сборке или запуске.

MIT. Microsoft SimConnect runtime, FSLTL assets, Vosk DLL и модели не
перераспределяются. Это экспериментальная симуляция, не авиационное средство
навигации и не проверенный игровой релиз.
