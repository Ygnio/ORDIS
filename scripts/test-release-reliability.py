"""Synthetic Phase 3.2 release validation; never uses existing backends or conversations."""
import argparse
from contextlib import closing
import ctypes
from ctypes import wintypes
import http.server
import io
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import threading
import time
import zipfile


def run(package, metrics=True):
    base = Path(os.environ['LOCALAPPDATA']) / 'Lucille' / 'ORDIS-Teste'
    base.mkdir(parents=True, exist_ok=True)
    fixture = Path(tempfile.mkdtemp(prefix='validacao-sintetica-', dir=base))
    assert fixture.resolve().parent == base.resolve()
    staging = Path(tempfile.mkdtemp(prefix='ordis-release-synthetic-'))
    requests = []
    memory_peaks = []

    class Counters(ctypes.Structure):
        _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] + [
            (name, ctypes.c_size_t) for name in ['PeakWorkingSetSize', 'WorkingSetSize', 'QuotaPeakPagedPoolUsage',
            'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage', 'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage', 'PrivateUsage']]
    get_memory = ctypes.WinDLL('psapi', use_last_error=True).GetProcessMemoryInfo
    get_memory.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    get_memory.restype = wintypes.BOOL

    class Backend(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass  # No request bodies, paths or private data in logs.

        def do_GET(self):
            if self.path == '/v1/models':
                obj = {'data': [{'id': 'SyntheticModel', 'architecture': {'input_modalities': ['text']}}]}
            elif self.path == '/props':
                obj = {'default_generation_settings': {'n_ctx': 8192}}
            else:
                obj = {}
            body = json.dumps(obj).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            assert self.path == '/v1/chat/completions'
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(request)
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Connection', 'close')
            self.end_headers()
            for text in ['Resposta ', 'sintética.']:
                chunk = {'model': 'SyntheticModel', 'choices': [{'delta': {'content': text}}]}
                self.wfile.write(('data: ' + json.dumps(chunk) + '\n\n').encode())
                self.wfile.flush()
                time.sleep(.16)
            chunk = {'choices': [], 'usage': {'prompt_tokens': 900, 'completion_tokens': 17, 'total_tokens': 917},
                     'timings': {'predicted_n': 17, 'predicted_ms': 320, 'predicted_per_second': 53.125}}
            self.wfile.write(('data: ' + json.dumps(chunk) + '\n\ndata: [DONE]\n\n').encode())
            self.wfile.flush()
            self.close_connection = True

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Backend)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        archive = zipfile.ZipFile(package)
        if 'ORDIS-Windows-Test.zip' in archive.namelist():
            archive = zipfile.ZipFile(io.BytesIO(archive.read('ORDIS-Windows-Test.zip')))
        # Own CI artifacts only; inspect before extracting executable code.
        for name in archive.namelist():
            assert not Path(name).is_absolute() and '..' not in Path(name).parts
            assert not name.lower().endswith(('.sqlite3', '.db', '.dpapi', '.gguf', '.log'))
        archive.extractall(staging)
        (fixture / 'config').mkdir()
        (fixture / 'config/paths.json').write_text('{"runtime":"runtime"}', encoding='utf-8')
        (fixture / 'runtime/inference').mkdir(parents=True)
        (fixture / 'runtime/agente/sessoes').mkdir(parents=True)
        legacy = fixture / 'runtime/agente/sessoes/synthetic-legacy.json'
        legacy_bytes = b'{"id":"synthetic-legacy","titulo":"Legacy","criada":0,"atualizada":0,"mensagens":[]}'
        legacy.write_bytes(legacy_bytes)
        config = fixture / 'runtime/inference/backend.json'
        endpoint = f'http://127.0.0.1:{server.server_port}'
        env = dict(os.environ, ORDIS_PROJECT=str(fixture))
        exe = staging / 'app/ordis-cli.exe'

        def select(backend):
            config.write_text(json.dumps({'selected': backend, 'tabby_url': endpoint, 'llama_url': endpoint}), encoding='utf-8')

        def ask(text):
            process = subprocess.Popen([str(exe), 'perguntar', text], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            stop = threading.Event()
            peak = [0]
            def sample():
                while not stop.is_set():
                    counters = Counters(); counters.cb = ctypes.sizeof(counters)
                    if get_memory(int(process._handle), ctypes.byref(counters), counters.cb):
                        peak[0] = max(peak[0], counters.PeakWorkingSetSize)
                    stop.wait(.01)
            sampler = threading.Thread(target=sample, daemon=True); sampler.start()
            try:
                stdout, stderr = process.communicate(timeout=40)
                assert process.returncode == 0, 'Release CLI failed; output retained privately, not printed.'
                assert 'sintética'.encode() in stdout
            finally:
                if process.poll() is None:
                    process.kill(); process.communicate()
                stop.set(); sampler.join(timeout=5)
            memory_peaks.append(round(peak[0]/1024**2, 2))

        def mem(action,args=None):
            result=subprocess.run([str(exe),'memoria',action],input=json.dumps(args or {}),env=env,capture_output=True,text=True,encoding='utf-8',timeout=40)
            assert result.returncode==0, 'Synthetic memory operation failed; no private output logged.'
            return json.loads(result.stdout)
        def memory_context():
            return '\n'.join(str(m.get('content','')) for m in requests[-1]['messages'] if m.get('role')=='user' and str(m.get('content','')).startswith('<lucille_memory_data>'))
        select('llama_cpp')
        workspace = fixture / 'agente/workspace'
        workspace.mkdir(parents=True)
        private_prompt = workspace / 'AGENTS.local.md'
        private_bytes = 'Seja breve e gentil. Perfil sintético de validação.'.encode('utf-8')
        private_prompt.write_bytes(private_bytes)
        ask('Prefiro respostas sintéticas do Atlas em Rust.')
        first_system = requests[-1]['messages'][0]['content']
        assert 'Nome: Lucille' in first_system
        assert 'Seja breve e gentil' in first_system
        assert 'Você é ORDIS, assistente técnico local do usuário.' not in first_system
        assert not any(m.get('role') == 'user' and str(m.get('content', '')).startswith('<lucille_memory_data>') for m in requests[-1]['messages'])
        db_path = fixture / 'runtime/memory/lucille.sqlite3'
        assert db_path.is_file()
        with closing(sqlite3.connect(db_path)) as db:
            assert db.execute('pragma user_version').fetchone()[0] == 4
            assert db.execute('select state from memories').fetchone()[0] == 'candidate'
            assert db.execute("select count(*) from messages").fetchone()[0] == 2
        candidate=mem('list',{'kind':'preference','state':'candidate'})[0]
        mem('approve',{'id':candidate['id'],'replace':False})
        mem('identity_save',{'name':'Aurora Sintética','description':'Descrição fictícia','personality':'Personalidade fictícia','instructions':'Instrução fictícia permanente'})
        ask('Bom dia.')
        assert 'Nome: Aurora Sintética' in requests[-1]['messages'][0]['content']
        assert 'Instrução fictícia permanente' in requests[-1]['messages'][0]['content']
        assert all('_lucille_ids' not in m for m in requests[-1]['messages'])
        assert any(m.get('role') == 'user' and str(m.get('content', '')).startswith('<lucille_memory_data>') and 'Atlas' in str(m.get('content', '')) for m in requests[-1]['messages'])
        select('tabbyapi')  # External mock endpoint; never manage a real Tabby process.
        ask('Relembre Atlas Rust.')
        assert any(m.get('role') == 'user' and str(m.get('content', '')).startswith('<lucille_memory_data>') for m in requests[-1]['messages'])
        assert legacy.read_bytes() == legacy_bytes
        assert private_prompt.read_bytes() == private_bytes
        assert len(requests) == 3
        sessions = [json.loads(p.read_text(encoding='utf-8')) for p in legacy.parent.glob('*.json') if p != legacy]
        assert len(sessions) == 3
        assert sum(bool(s['mensagens'][-1].get('memoria_ids')) for s in sessions) == 2
        if metrics:
            for s in sessions:
                m = s['mensagens'][-1]['metricas']
                assert m['completion_tokens'] == 17 and m['token_source'] == 'usage'
                assert m['state'] == 'completed' and m['ttft_ms'] is not None
                assert m['total_ms'] >= 250 and m['generation_ms'] == 320
                assert m['tokens_per_second'] == 53.125
                assert m['estimated_tokens'] is None
                assert requests[0]['stream_options']['include_usage'] is True
        with closing(sqlite3.connect(db_path)) as db:
            assert db.execute('pragma integrity_check').fetchone()[0] == 'ok'
            assert db.execute("select count(*) from messages").fetchone()[0] == 6
            assert db.execute("select count(*) from conversations").fetchone()[0] == 3
        # A/B/C: genuine public memory operations, each ask starts a new CLI process.
        options={'retrieval':True,'extraction':True,'token_budget':1024,'project_id':None,'auto_consolidation':True}
        mem('options',options)
        select('llama_cpp')
        ask('Lucille, o nome do meu projeto fictício é Girassol Azul.')
        assert 'Girassol Azul' not in memory_context()
        blue=mem('list',{'query':'Girassol','kind':'fact'})[0]
        assert blue['state']=='candidate'
        mem('approve',{'id':blue['id'],'replace':False})
        ask('Qual é o projeto fictício?')
        assert 'Girassol Azul' in memory_context()
        ask('Lucille, o nome do meu projeto fictício é Girassol Verde.')
        versions=mem('list',{'query':'Girassol','kind':'fact'})
        green=next(m for m in versions if 'Verde' in m['content'])
        assert all(m['state']=='conflict' for m in versions)
        mem('approve',{'id':green['id'],'replace':True})
        select('tabbyapi')
        ask('Qual é o projeto fictício?')
        assert 'Girassol Verde' in memory_context() and 'Girassol Azul' not in memory_context()
        mem('delete',{'id':green['id']})
        mem('consolidate')
        ask('Qual é o projeto fictício?')
        assert 'Girassol Verde' not in memory_context() and 'Girassol Azul' not in memory_context()
        # E: project metadata and scope survive separate processes.
        pid=mem('project_save',{'name':'Boreal sintético','goal':'Organizar teste fictício','architecture':'Decisão: usar filas locais','next_tasks':'Validar tarefa de cancelamento','execution_state':'Em teste'})
        options['project_id']=pid;mem('options',options)
        ask('Qual a tarefa de cancelamento?')
        assert 'Validar tarefa de cancelamento' in memory_context()
        assert 'filas locais' in memory_context()
        # Measured overhead, not a target or an inference benchmark: same synthetic transport.
        import statistics
        samples={}
        for enabled in [False,True]:
            options['retrieval']=enabled;mem('options',options)
            measured=[]
            for _ in range(3):
                before={p.name for p in legacy.parent.glob('*.json')}
                started=time.perf_counter();ask('Qual a tarefa de cancelamento?');wall=(time.perf_counter()-started)*1000
                added=[p for p in legacy.parent.glob('*.json') if p.name not in before];assert len(added)==1
                reply=json.loads(added[0].read_text(encoding='utf-8'))['mensagens'][-1]
                measured.append({'memory_ms':reply['memoria_recuperacao_ms'],'ttft_http_ms':reply['metricas']['ttft_ms'],'total_cli_ms':wall})
            samples['enabled' if enabled else 'disabled']={k:round(statistics.median(r[k] for r in measured),3) for k in measured[0]}
        technical=mem('technical')
        assert not any(t in json.dumps(technical) for t in ['Girassol','Aurora','Boreal','Atlas',str(fixture)])
        exported=Path(mem('technical_export')['path']);assert exported.is_relative_to(fixture)
        assert json.loads(exported.read_text(encoding='utf-8'))['schema']==4
        print(json.dumps({'reliability_release':'passed','scenarios':['persistence','update','delete','backend_independence','projects','privacy'], 'synthetic_requests':len(requests),'latency_medians_ms':samples,'cli_peak_working_set_mib':memory_peaks}))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
        for path, allowed, prefix in [(fixture, base, 'validacao-sintetica-'), (staging, Path(tempfile.gettempdir()), 'ordis-release-synthetic-')]:
            resolved = path.resolve()
            assert resolved.parent == allowed.resolve() and resolved.name.startswith(prefix) and not path.is_symlink()
            shutil.rmtree(path)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('package', type=Path)
    parser.add_argument('--metrics', action='store_true')
    args = parser.parse_args()
    run(args.package, True)
