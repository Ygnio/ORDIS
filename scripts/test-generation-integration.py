"""Synthetic CLI integration. Never reads an existing ORDIS profile or real backend."""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading


def run(cli):
    mode = ['normal']
    requests = []

    class Backend(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            value = {'data': [{'id': 'SyntheticGeneration'}]} if self.path == '/v1/models' else {
                'default_generation_settings': {'n_ctx': 65536},
                'chat_template_caps': {'supports_tool_calls': True}}
            data = json.dumps(value).encode()
            self.send_response(200)
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(body)
            assert body['max_tokens'] > 0
            assert 'n_ctx' not in body
            assert 'chat_template_kwargs' not in body
            name = mode[0]
            continued = any(m['role'] == 'tool' for m in body['messages'])
            delta = {'content': 'Synthetic final.'}
            finish = 'stop'
            if name == 'partial':
                delta, finish = {'content': 'Synthetic partial.'}, 'length'
            elif name == 'no_final':
                delta, finish = {'reasoning_content': 'Synthetic thought.'}, 'length'
            elif name in ('invalid_tool', 'truncated_tool', 'duplicate_tool', 'valid_tool') and not continued:
                calls = [{'index': 0, 'id': 'synthetic-call', 'function': {
                    'name': 'read', 'arguments': '{"path":"synthetic.txt"}'}}]
                if name == 'invalid_tool':
                    calls[0]['function']['arguments'] = '{"path":'
                if name == 'duplicate_tool':
                    calls.append(dict(calls[0], index=1))
                delta, finish = {'tool_calls': calls}, 'length' if name == 'truncated_tool' else 'tool_calls'
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Connection', 'close')
            self.end_headers()
            wire = 'data: ' + json.dumps({'choices': [{'delta': delta, 'finish_reason': finish}]}) + '\n\ndata: [DONE]\n\n'
            self.wfile.write(wire.encode())
            self.wfile.flush()
            self.close_connection = True

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Backend)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    tested = []
    try:
        with tempfile.TemporaryDirectory(prefix='lucille-generation-synthetic-') as temporary:
            root = Path(temporary)
            (root / 'config').mkdir()
            (root / 'config/paths.json').write_text('{"runtime":"runtime"}', encoding='utf-8')
            inference = root / 'runtime/inference'
            inference.mkdir(parents=True)
            (inference / 'generation.json').write_text('{"selected_profile":"long"}', encoding='utf-8')
            workspace = root / 'agente/workspace'
            workspace.mkdir(parents=True)
            (workspace / 'synthetic.txt').write_text('Synthetic fixture only.', encoding='utf-8')
            endpoint = f'http://127.0.0.1:{server.server_port}'
            local = root / 'isolated-localappdata'
            local.mkdir()
            env = dict(os.environ, ORDIS_PROJECT=str(root), LOCALAPPDATA=str(local), APPDATA=str(local / 'roaming'))
            for backend in ('llama_cpp', 'tabbyapi'):
                (inference / 'backend.json').write_text(json.dumps({
                    'selected': backend, 'llama_url': endpoint, 'tabby_url': endpoint}), encoding='utf-8')
                for case in ('normal', 'partial', 'no_final', 'invalid_tool', 'truncated_tool', 'duplicate_tool', 'valid_tool'):
                    mode[0] = case
                    before = len(requests)
                    p = subprocess.run([str(cli), 'conversar'], input=json.dumps({
                        'text': 'Synthetic generation validation.', 'attachments': []}),
                        env=env, capture_output=True, text=True, encoding='utf-8', timeout=30)
                    assert p.returncode == 0, 'Synthetic CLI failed; captured content not exported'
                    sessions = list((root / 'runtime/agente/sessoes').glob('*.json'))
                    newest = max(sessions, key=lambda x: x.stat().st_mtime_ns)
                    # Reopen from disk, rather than relying on the process that generated it.
                    session = json.loads(newest.read_text(encoding='utf-8-sig'))
                    assistants = [m for m in session['mensagens'] if m['papel'] == 'assistant']
                    tools = [m for m in session['mensagens'] if m['papel'] == 'tool']
                    expected = {'partial': 'length', 'no_final': 'length', 'invalid_tool': 'protocol_error',
                                'truncated_tool': 'length', 'duplicate_tool': 'protocol_error'}.get(case, 'completed')
                    assert assistants[-1]['metricas']['state'] == expected, (backend, case, 'wrong terminal state')
                    assert assistants[-1]['metricas']['backend'] == backend
                    assert len(tools) == (1 if case == 'valid_tool' else 0), 'Unexpected tool execution'
                    assert len(requests) - before == (2 if case == 'valid_tool' else 1), 'Retry or invalid agent continuation'
                    assert all(r['max_tokens'] == (16384 if backend == 'llama_cpp' else 4096)
                               for r in requests[before:]), 'Profile or explicit Tabby limit was overwritten'
                    if case == 'partial':
                        assert assistants[-1]['texto'] == 'Synthetic partial.'
                    if case == 'no_final':
                        assert not assistants[-1]['texto']
                    if case == 'valid_tool':
                        assert tools[0]['ferramenta'] == 'read'
                        assert not tools[0].get('erro', False)
                        assert requests[-1]['messages'][-1]['role'] == 'tool'
                    tested.append({'backend': backend, 'case': case, 'state': expected})
            assert (local / 'Lucille/ORDIS/runtime/memory/lucille.sqlite3').is_file()
        print(json.dumps({'synthetic_cli_integration': 'passed', 'scenarios': tested,
                          'real_backend_requests': 0, 'personal_profile_accessed': False}))
    finally:
        server.shutdown()
        server.server_close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--cli', required=True, type=Path)
    run(parser.parse_args().cli.resolve(strict=True))
