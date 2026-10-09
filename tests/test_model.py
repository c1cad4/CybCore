"""Protocol fixtures test integration; these responses are not real model inference."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import json
import tempfile
import threading
import unittest
from fastapi.testclient import TestClient
from app import create_app
from local_model import LocalModel, ModelUnavailable

@contextmanager
def server(mode='ok'):
    captured=[]
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args):pass
        def do_GET(self):self.respond({'data':[] if mode=='empty-models' else [{'missing':'identity'}] if mode=='bad-models' else [{'id':'fixture-model'}]})
        def do_POST(self):
            captured.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            if mode=='slow':
                import time
                time.sleep(.1)
            if mode=='error':self.respond({'error':'server detail'},500)
            elif mode=='oversized':self.respond({'choices':[{'message':{'content':'x'*70000}}]})
            elif mode=='reasoning':self.respond({'choices':[{'message':{'reasoning':'internal reasoning'}}]})
            elif mode=='malformed':self.respond({'choices':None})
            elif mode=='truncated':self.respond({'choices':[{'message':{'content':'incomplete'},'finish_reason':'length'}]})
            else:self.respond({'choices':[{'message':{'content':'Ответ протокольной фикстуры'}}]})
        def respond(self,body,status=200):
            data=json.dumps(body,ensure_ascii=False).encode()
            self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers()
            try:self.wfile.write(data)
            except BrokenPipeError:pass
    http=ThreadingHTTPServer(('127.0.0.1',0),Handler)
    thread=threading.Thread(target=http.serve_forever,daemon=True);thread.start()
    try:yield LocalModel(f'http://127.0.0.1:{http.server_port}/v1'),captured
    finally:http.shutdown();http.server_close();thread.join()

class ModelTests(unittest.TestCase):
    def test_loopback_policy(self):
        for url in ['https://example.com/v1','http://localhost/v1','http://127.0.0.1@evil.example/v1','http://127.0.0.1/v1?token=x','http://127.0.0.1/other','http://127.0.0.1:99999/v1','http://127.0.0.1:0/v1']:
            with self.assertRaises(ValueError):LocalModel(url)
    def test_real_http_protocol_and_context(self):
        with server() as (model,captured):
            self.assertEqual(model.status()['status'],'online')
            result=model.answer('Вопрос',[{'source':'field log','content':'data'}])
            self.assertEqual(result['model'],'fixture-model')
            self.assertFalse(captured[0]['stream'])
            self.assertEqual(captured[0]['chat_template_kwargs'], {'enable_thinking': False})
            self.assertEqual(json.loads(captured[0]['messages'][1]['content'])['knowledge'][0]['source'],'field log')
    def test_unloaded_or_invalid_model_is_offline(self):
        for mode in ['empty-models','bad-models']:
            with self.subTest(mode=mode),server(mode) as (model,_):
                self.assertEqual(model.status()['status'],'offline')
                with self.assertRaises(ModelUnavailable):model.answer('question',[])
        with server() as (model,_):
            model.model='not-loaded';self.assertEqual(model.status()['status'],'offline')
    def test_invalid_answers_fail(self):
        for mode in ['error','oversized','reasoning','malformed','truncated']:
            with self.subTest(mode=mode),server(mode) as (model,_):
                with self.assertRaises(ModelUnavailable):model.answer('question',[])
    def test_grounding_does_not_write_answer_to_memory(self):
        with tempfile.TemporaryDirectory() as d,server() as (model,captured),TestClient(create_app(Path(d)/'memory.sqlite3',model)) as client:
            client.post('/tasks',json={'task_id':'record','agent_ids':['keeper'],'capability':'memory.remember','content':'Пасека использует энергию солнца','source':'полевой журнал'})
            result=client.post('/ask',json={'question':'Откуда энергия?','knowledge_query':'Пасека'})
            self.assertEqual(result.status_code,200)
            self.assertEqual(result.json()['used_context'][0]['source'],'полевой журнал')
            self.assertEqual(result.json()['agent_id'],'advisor')
            rows=client.post('/tasks',json={'task_id':'search','agent_ids':['keeper'],'capability':'memory.recall','query':'фикстуры'}).json()['events'][0]['result']
            self.assertEqual(rows,[])
    def test_empty_context_is_explicit(self):
        with tempfile.TemporaryDirectory() as d,server() as (model,_),TestClient(create_app(Path(d)/'memory.sqlite3',model)) as client:
            result=client.post('/ask',json={'question':'Вопрос?','knowledge_query':'отсутствует'})
            self.assertEqual(result.json()['used_context'],[])
    def test_failure_maps_to_503_without_server_details(self):
        with tempfile.TemporaryDirectory() as d,server('error') as (model,_),TestClient(create_app(Path(d)/'memory.sqlite3',model)) as client:
            result=client.post('/ask',json={'question':'Вопрос?','knowledge_query':'данные'})
            self.assertEqual(result.status_code,503);self.assertNotIn('server detail',result.text)
            self.assertEqual(client.get('/health').status_code,200)
    def test_question_validation(self):
        with tempfile.TemporaryDirectory() as d,server() as (model,captured),TestClient(create_app(Path(d)/'memory.sqlite3',model)) as client:
            for body in [{'question':' ','knowledge_query':'text'},{'question':'x'*4001,'knowledge_query':'text'},{'question':'Вопрос','knowledge_query':'text','command':'run'}]:
                self.assertEqual(client.post('/ask',json=body).status_code,422)
            self.assertEqual(captured,[])
    def test_timeout_returns_model_unavailable(self):
        with server('slow') as (model,_):
            model.timeout=.03
            with self.assertRaises(ModelUnavailable):model.answer('question',[])
    def test_offline_probe(self):
        # Closed prebound ephemeral port gives a deterministic connection refusal.
        import socket
        with socket.socket() as sock:
            sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
            self.assertEqual(LocalModel(f'http://127.0.0.1:{port}/v1',timeout=.2).status()['status'],'offline')
