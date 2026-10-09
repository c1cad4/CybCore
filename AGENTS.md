# CybCore guide for coding agents

Applies to the whole repository. Follow more specific subtree instructions when present.

## Scope

- CybCore owns a local FastAPI HTTP API and browser UI joining CybAgents, CybRegistry, CybMemory and CybSwarm.
- app.py defines request validation and endpoints; run.py binds the development server; web/index.html owns the UI; local_model.py implements the optional model adapter.
- Keep component behavior in its owning sibling repository; read components.lock.json for pinned revisions.
- This prototype saves and searches knowledge without requiring an LLM; an optional loopback model supports source-backed answers. User agent registrations, knowledge and completed task results persist in SQLite.

## Setup and checks

- Python 3.12+ and Git are required. Run commands from the repository root.
- Prepare siblings: python3 scripts/bootstrap_components.py.
- Create an isolated environment: python3 -m venv .venv.
- Install pinned requirements: .venv/bin/python -m pip install -r requirements.lock.txt.
- Run the complete suite: .venv/bin/python -m unittest discover -s tests -v.
- Run bootstrap-only tests without API dependencies: python3 -m unittest discover -s tests -p test_bootstrap.py -v.
- Start: .venv/bin/python run.py; use CYBCORE_DATABASE to select an isolated SQLite path.
- HTTP UI: http://127.0.0.1:8010; API schema: /docs; health: /health.

## Contracts

- Keep the server loopback-only; public multi-user deployment requires a separate authenticated design.
- Preserve task idempotency: the same task_id and payload returns the stored result; a changed payload returns 409.
- Validate the whole requested agent team before writing memory; denied capabilities must not produce partial writes.
- Keep /ask model access bounded and loopback-only; return 503 on model failure, and do not store answers automatically.
- Preserve source provenance and reject undeclared request fields and capabilities.
- Do not describe recorded knowledge as model reasoning or hardware observations without evidence.
- Do not reset existing sibling working trees. Publish component commits before updating integration pins.
- Attribute reused upstream code and preserve license notices in UPSTREAM.md.

## Delivery

- Check restart persistence, idempotency, permission failures and UI/API compatibility for relevant changes.
- State which checks actually ran, and any unavailable checks. Update README when behavior changes.
- Keep changes reviewable; separate release or hardware work from API maintenance.

