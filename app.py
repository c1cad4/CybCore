"""Local development API joining agents, registry, persistent memory and swarm."""
import os
from pathlib import Path
import sys
from typing import Literal

ROOT = Path(__file__).resolve().parent.parent
for repo in ('CybAgents', 'CybRegistry', 'CybMemory', 'CybSwarm'):
    sys.path.insert(0, str(ROOT / repo))
from cybagents import Agent, CAPABILITIES
from cybregistry import Registry
from cybmemory import Memory, Conflict
from cybswarm import Swarm
from fastapi import FastAPI, HTTPException
from fastapi.responses import FileResponse
from pydantic import BaseModel, ConfigDict, Field, model_validator

class Registration(BaseModel):
    model_config = ConfigDict(extra='forbid')
    id: str = Field(pattern=r'^[a-zA-Z0-9_-]{1,64}$')
    capabilities: list[Literal['memory.remember', 'memory.recall']] = Field(min_length=1, max_length=2)

class Task(BaseModel):
    model_config = ConfigDict(extra='forbid')
    task_id: str = Field(pattern=r'^[a-zA-Z0-9_-]{1,128}$')
    agent_ids: list[str] = Field(min_length=1, max_length=8)
    capability: Literal['memory.remember', 'memory.recall']
    content: str = Field(default='', max_length=8000)
    source: str = Field(default='', max_length=256)
    query: str = Field(default='', max_length=256)
    @model_validator(mode='after')
    def required_fields(self):
        if self.capability == 'memory.remember' and (not self.content.strip() or not self.source.strip()):
            raise ValueError('content and source required')
        if self.capability == 'memory.recall' and not self.query.strip():
            raise ValueError('query required')
        return self

def create_app(database=None):
    app = FastAPI(title='CybCore', version='0.1.0')
    memory = Memory(database or os.environ.get('CYBCORE_DATABASE', str(ROOT / '.onboarding/cybcore/knowledge.sqlite3')))
    registry = Registry()
    registry.register(Agent('keeper', tuple(sorted(CAPABILITIES))))
    swarm = Swarm(registry, memory)
    @app.get('/')
    def home():
        return FileResponse(Path(__file__).parent / 'web/index.html')
    @app.get('/health')
    def health():
        with memory.connect() as db:
            db.execute('SELECT 1').fetchone()
        return {'status': 'ok', 'capabilities': sorted(CAPABILITIES)}
    @app.get('/agents')
    def agents():
        return [{'id': a.id, 'capabilities': a.capabilities} for a in registry.list()]
    @app.post('/agents', status_code=201)
    def register(body: Registration):
        try:
            agent = registry.register(Agent(body.id, tuple(body.capabilities)))
            return {'id': agent.id, 'capabilities': agent.capabilities}
        except ValueError as e:
            raise HTTPException(409, str(e)) from e
    @app.post('/tasks')
    def execute(body: Task):
        try:
            return swarm.execute(body.task_id, body.model_dump(exclude={'task_id'}))
        except Conflict as e:
            raise HTTPException(409, str(e)) from e
        except ValueError as e:
            raise HTTPException(400, str(e)) from e
    return app
