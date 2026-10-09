# CybCore

Серверная инфраструктура, API, узлы сети и координация сервисов.

## Реализовано

HTTP API объединяет CybAgents, CybRegistry, CybMemory и CybSwarm. Прототип работает локально, без модели LLM. Агент keeper выполняет две реальные операции: сохранение и поиск знаний. Полный сценарий проверяет рестарт, идемпотентность и разрешения.

## Запуск

Требуются Python 3.12+ и четыре sibling checkout-папки.

```bash
python3 scripts/bootstrap_components.py
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements.lock.txt
CYBCORE_DATABASE=/path/to/data/knowledge.sqlite3 .venv/bin/python run.py
```

Сервер слушает только loopback на порту 8010. Главная страница — интерфейс
сохранения/поиска; `/docs` — OpenAPI, `/health` — состояние SQLite,
`GET/POST /agents` — реестр, `POST /tasks` — координация.
Публичная multi-user публикация с authentication не реализована.

```json
{"task_id":"observe-1","agent_ids":["keeper"],"capability":"memory.remember","content":"Пасека использует солнечную энергию","source":"журнал пасеки"}
```

Повтор task_id с тем же payload возвращает прежний результат; другой payload — 409.
Для поиска отправьте новую задачу с `capability: "memory.recall"` и `query`.
Знания и завершённые задачи переживают рестарт. Пользовательские регистрации
агентов остаются в памяти процесса; keeper регистрируется на каждом старте.

## Проверка

```bash
.venv/bin/python -m unittest discover -s tests -v
```

[Upstream и лицензии](UPSTREAM.md). Локальное ядро не исполняет shell,
не управляет оборудованием и не выдаёт сохранённый текст за вывод модели.

## Интеграция

[Сквозной API и UI](https://github.com/c1cad4/CybCore) · [Карта экосистемы](https://github.com/c1cad4/cybOS)
