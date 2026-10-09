import tempfile
from pathlib import Path
import unittest
from fastapi.testclient import TestClient
from app import create_app
class ApiTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.database=Path(self.tmp.name)/'knowledge.sqlite3'
        self.client=TestClient(create_app(self.database));self.addCleanup(self.client.close)
    def task(self,identifier='t1',**kwargs):
        return self.client.post('/tasks',json={'task_id':identifier,'agent_ids':['keeper'],'capability':'memory.remember','content':'Пасека использует солнечную энергию','source':'журнал пасеки',**kwargs})
    def test_full_flow_survives_restart(self):
        first=self.task();self.assertEqual(first.status_code,200)
        with TestClient(create_app(self.database)) as restarted:
            result=restarted.post('/tasks',json={'task_id':'search','agent_ids':['keeper'],'capability':'memory.recall','query':'солнечную'})
        rows=result.json()['events'][0]['result'];self.assertEqual(rows[0]['source'],'журнал пасеки')
        self.assertEqual(rows[0]['id'],first.json()['events'][0]['result']['id'])
    def test_idempotent_task_and_conflicting_payload(self):
        first=self.task().json();self.assertEqual(self.task().json(),first)
        self.assertEqual(self.task(content='different').status_code,409)
    def test_capability_denied_has_no_partial_writes(self):
        self.client.post('/agents',json={'id':'reader','capabilities':['memory.recall']})
        self.assertEqual(self.task(agent_ids=['keeper','reader']).status_code,400)
        result=self.task('search',capability='memory.recall',query='Пасека').json()
        self.assertEqual(result['events'][0]['result'],[])
    def test_register_team_and_execute(self):
        self.assertEqual(self.client.post('/agents',json={'id':'archivist','capabilities':['memory.remember']}).status_code,201)
        result=self.task(agent_ids=['keeper','archivist']);self.assertEqual(result.status_code,200)
        self.assertEqual([e['agent_id'] for e in result.json()['events']],['keeper','archivist'])
    def test_unknown_agent(self):self.assertEqual(self.task(agent_ids=['missing']).status_code,400)
    def test_empty_provenance(self):self.assertEqual(self.task(source=' ').status_code,422)
    def test_arbitrary_execution_and_extra_fields_rejected(self):
        self.assertEqual(self.task(capability='shell.execute').status_code,422)
        self.assertEqual(self.task(command='echo test').status_code,422)
    def test_duplicate_registration(self):
        self.assertEqual(self.client.post('/agents',json={'id':'keeper','capabilities':['memory.recall']}).status_code,409)
    def test_user_agent_and_permissions_survive_restart(self):
        response = self.client.post('/agents',json={'id':'reader','capabilities':['memory.recall']})
        self.assertEqual(response.status_code,201)
        with TestClient(create_app(self.database)) as restarted:
            agents = restarted.get('/agents').json()
            self.assertEqual(next(a for a in agents if a['id']=='reader')['capabilities'],['memory.recall'])
            denied = restarted.post('/tasks',json={'task_id':'denied-after-restart','agent_ids':['reader'],'capability':'memory.remember','content':'must not be saved','source':'test'})
            self.assertEqual(denied.status_code,400)
            recall = restarted.post('/tasks',json={'task_id':'recall-after-restart','agent_ids':['reader'],'capability':'memory.recall','query':'saved'})
            self.assertEqual(recall.status_code,200)
            self.assertEqual(recall.json()['events'][0]['result'],[])
    def test_ui_and_health(self):
        self.assertEqual(self.client.get('/health').json()['status'],'ok')
        self.assertIn('Память',self.client.get('/').text)
