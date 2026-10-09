<div align="center">

# CybCore

**Локальный API для агентов, задач и общей памяти**

[![Checks](https://github.com/c1cad4/CybCore/actions/workflows/ci.yml/badge.svg)](https://github.com/c1cad4/CybCore/actions/workflows/ci.yml)

[Запуск](#быстрый-старт) · [API](#http-api) · [Для агентов](AGENTS.md) · [Экосистема](https://github.com/c1cad4/cybOS) · [Лицензия](LICENSE)

</div>

CybCore соединяет [CybAgents](https://github.com/c1cad4/CybAgents), [CybRegistry](https://github.com/c1cad4/CybRegistry), [CybMemory](https://github.com/c1cad4/CybMemory) и [CybSwarm](https://github.com/c1cad4/CybSwarm) в один локальный HTTP-сервис. Через браузер или API можно сохранить знание с источником, а затем найти его после перезапуска.

Прототип работает без модели LLM. Встроенный агент `keeper` поддерживает `memory.remember` и `memory.recall`.

## Быстрый старт

Нужны Python 3.12+, Git и доступ к GitHub для подготовки четырёх соседних компонентов.

```bash
git clone https://github.com/c1cad4/CybCore.git
cd CybCore
python3 scripts/bootstrap_components.py
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements.lock.txt
.venv/bin/python -m unittest discover -s tests -v
.venv/bin/python run.py
```

Откройте **http://127.0.0.1:8010**. Интерактивная документация API доступна на **http://127.0.0.1:8010/docs**.

Выбрать своё хранилище можно через `CYBCORE_DATABASE`:

```bash
CYBCORE_DATABASE=/path/to/data/knowledge.sqlite3 .venv/bin/python run.py
```

Bootstrap использует закреплённые SHA из `components.lock.json`, загружает одну ревизию во временную папку и публикует checkout после успеха. При ошибке загрузки запуск можно повторить. Существующие checkout и локальные изменения сохраняются. `--verify` сравнивает HEAD с lock-файлом без сети; чистота рабочего дерева отдельно не проверяется. Для полной истории компонента выполните в нём `git fetch --unshallow origin`.

## HTTP API

- `GET /` — интерфейс сохранения и поиска.
- `GET /health` — состояние SQLite и доступные возможности.
- `GET /agents` — зарегистрированные агенты.
- `POST /agents` — регистрация агента с разрешёнными возможностями.
- `POST /tasks` — выполнение задачи указанной командой агентов.

Сохранить знание:

```bash
curl --fail-with-body http://127.0.0.1:8010/tasks \\
  -H 'Content-Type: application/json' \\
  --data '{"task_id":"observe-1","agent_ids":["keeper"],"capability":"memory.remember","content":"Пасека использует солнечную энергию","source":"журнал пасеки"}'
```

Найти знание:

```bash
curl --fail-with-body http://127.0.0.1:8010/tasks \\
  -H 'Content-Type: application/json' \\
  --data '{"task_id":"search-1","agent_ids":["keeper"],"capability":"memory.recall","query":"солнечную"}'
```

Повтор `task_id` с тем же содержимым возвращает сохранённый результат. Другой payload с тем же идентификатором возвращает `409`. Для нового поиска используйте новый `task_id`.

## Память и ограничения

Знания и завершённые задачи сохраняются в SQLite. Пользовательские регистрации агентов хранятся в памяти процесса; `keeper` регистрируется при каждом старте.

Сервер слушает только loopback. Аутентификация для публичного сервиса с несколькими пользователями пока не реализована. Ядро выполняет операции памяти; произвольное исполнение shell и управление оборудованием не входят в доступные возможности.

## Разработка

[AGENTS.md](AGENTS.md) описывает устройство проекта, команды и контракты. Полная проверка выполняется командой из быстрого старта. Для работы только над bootstrap достаточно стандартной библиотеки Python и Git:

```bash
python3 -m unittest discover -s tests -p test_bootstrap.py -v
```

[Источники и лицензии](UPSTREAM.md) · [Карта экосистемы](https://github.com/c1cad4/cybOS)
