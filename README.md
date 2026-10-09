<div align="center">

# CybCore

**Локальный API для агентов, задач и общей памяти**

[![Checks](https://github.com/c1cad4/CybCore/actions/workflows/ci.yml/badge.svg)](https://github.com/c1cad4/CybCore/actions/workflows/ci.yml)

[Запуск](#быстрый-старт) · [API](#http-api) · [Для агентов](AGENTS.md) · [Экосистема](https://github.com/c1cad4/cybOS) · [Лицензия](LICENSE)

</div>

CybCore соединяет [CybAgents](https://github.com/c1cad4/CybAgents), [CybRegistry](https://github.com/c1cad4/CybRegistry), [CybMemory](https://github.com/c1cad4/CybMemory) и [CybSwarm](https://github.com/c1cad4/CybSwarm) в один локальный HTTP-сервис. Через браузер или API можно сохранить знание с источником, а затем найти его после перезапуска.

Операции памяти работают без обязательной модели LLM. Помощник с контекстом может обращаться к отдельно запущенной локальной модели через loopback OpenAI-совместимый сервер. Встроенный агент `keeper` поддерживает `memory.remember` и `memory.recall`.

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
- `GET /model` — состояние локальной модели.
- `POST /ask` — ответ модели с контекстом из памяти.

Сохранить знание:

```bash
curl --fail-with-body http://127.0.0.1:8010/tasks \
  -H 'Content-Type: application/json' \
  --data '{"task_id":"observe-1","agent_ids":["keeper"],"capability":"memory.remember","content":"Пасека использует солнечную энергию","source":"журнал пасеки"}'
```

Найти знание:

```bash
curl --fail-with-body http://127.0.0.1:8010/tasks \
  -H 'Content-Type: application/json' \
  --data '{"task_id":"search-1","agent_ids":["keeper"],"capability":"memory.recall","query":"солнечную"}'
```

Повтор `task_id` с тем же содержимым возвращает сохранённый результат. Другой payload с тем же идентификатором возвращает `409`. Для нового поиска используйте новый `task_id`.

## Память и ограничения

Знания и завершённые задачи сохраняются в SQLite. Пользовательские регистрации агентов и их возможности сохраняются в SQLite; `keeper` и `advisor` регистрируются при каждом старте.

Сервер слушает только loopback. Аутентификация для публичного сервиса с несколькими пользователями пока не реализована. Ядро выполняет операции памяти; произвольное исполнение shell и управление оборудованием не входят в доступные возможности.

## Разработка

[AGENTS.md](AGENTS.md) описывает устройство проекта, команды и контракты. Полная проверка выполняется командой из быстрого старта. Для работы только над bootstrap достаточно стандартной библиотеки Python и Git:

```bash
python3 -m unittest discover -s tests -p test_bootstrap.py -v
```

[Источники и лицензии](UPSTREAM.md) · [Карта экосистемы](https://github.com/c1cad4/cybOS)

## Помощник с контекстом памяти

`POST /ask` принимает `question` (до 4000 символов) и `knowledge_query`
(до 256 символов). Агент advisor ищет по заданным словам через memory.recall,
передаёт модели до пяти фрагментов / 12000 символов и возвращает ответ
с `used_context`. Это список переданных данных, а не доказательство
правильности ответа или того, что модель использовала каждый источник.
Поиск лексический FTS5, не semantic/vector retrieval.

```json
{"question":"Как пасека получает энергию?","knowledge_query":"пасека"}
```

По умолчанию используется `http://127.0.0.1:8080/v1`, совпадающий с портом
Qwen в CybOS-demo. Переопределения: CYBMODEL_URL (literal loopback /v1),
CYBMODEL_NAME (необязательно; иначе GET /models), CYBMODEL_TIMEOUT (0–30 секунд,
по умолчанию 30; больше нуля). URL не принимается из HTTP-запроса.
`GET /model` показывает online/offline; `/health` отдельно проверяет память.
Ошибка модели возвращает 503 без генерации подставного ответа.

Требуется отдельно запущенный Qwen/MLX или другой совместимый сервер
с установленными весами. Модель не скачивается и не запускается этим проектом.
Протокол проверяется HTTP-фикстурами. Для моделей с режимом thinking
адаптер запрашивает chat_template_kwargs.enable_thinking=false, чтобы получить
конечный ответ в пределах лимита генерации. Ответ с finish_reason=length
отклоняется как незавершённый.

Соединение SQLite закрывается до вызова модели. Ответ не записывается
автоматически в память; модель не имеет tools или разрешения выполнять действия.

