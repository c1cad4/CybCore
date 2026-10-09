# Используемые upstream-компоненты

Подключены через Python packages; исходники сторонних проектов не копируются.

| Проект GitHub | Версия | Лицензия | Использование |
|---|---|---|---|
| [fastapi/fastapi](https://github.com/fastapi/fastapi) | 0.141.1 | MIT | HTTP API и OpenAPI |
| [pydantic/pydantic](https://github.com/pydantic/pydantic) | 2.13.4 | MIT | Ограничения входных данных |
| [encode/uvicorn](https://github.com/encode/uvicorn) | 0.52.1 | BSD-3-Clause | ASGI сервер |
| [encode/httpx](https://github.com/encode/httpx) | 0.28.1 | BSD-3-Clause | Интеграционные HTTP-тесты |

Поиск также рассмотрел omnilib/aiosqlite: отдельный asyncio-адаптер не нужен
для синхронных FastAPI handlers, выполняемых в threadpool. SQLite используется
из стандартной библиотеки, FTS5 обеспечивает поиск; параметры SQL передаются отдельно.
Лицензии FastAPI/Pydantic/Uvicorn/HTTPX проверены через GitHub license endpoint.

Полный transitive lock сформирован из отдельной virtualenv; системные packages не включены.

## Дополнительный поиск GitHub

- langchain-ai/langgraph: MIT, рассматривался для сложных графов задач. В текущей
  вертикали нет LLM/ветвлений, поэтому отдельный orchestration framework пока не добавлен.
- ros2/rclpy: Apache-2.0, кандидат для CybRobot после выбора оборудования и ROS2 SDK.
- eclipse-zenoh/zenoh: проверен как кандидат транспорта; license metadata — NOASSERTION,
  поэтому перед интеграцией нужно отдельно разобрать LICENSE и выбранные features.
- omnilib/aiosqlite: MIT; синхронные SQLite handlers выполняются в threadpool,
  дополнительный async-адаптер пока не требуется.

Это кандидаты для следующих задач, не реализованные возможности текущего приложения.
