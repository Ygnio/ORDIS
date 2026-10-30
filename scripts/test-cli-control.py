"""Synthetic stdin approval/cancel acceptance tests. No existing profile or AI server is used."""
import argparse
import http.server
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time


def run(cli, official=False):
    requests = []
    results = []

    class Backend(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            body = json.dumps({'data': [{'id': 'SyntheticControl'}]} if self.path == '/v1/models' else {
                'default_generation_settings': {'n_ctx': 65536}, 'chat_template_caps': {'supports_tool_calls': True}}).encode()
            self.send_response(200)
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(body)
            prompt = next(m['content'] for m in body['messages'] if m['role'] == 'user' and isinstance(m['content'], str) and m['content'].startswith('Synthetic control '))
            case = prompt.split()[-1]
            calls = sum(m['role'] == 'tool' for m in body['messages'])
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Connection', 'close')
            self.end_headers()

            def emit(delta, finish=None):
                self.wfile.write(('data: ' + json.dumps({'model': 'SyntheticControl', 'choices': [{'delta': delta, 'finish_reason': finish}]}) + '\n\n').encode())
                self.wfile.flush()
            try:
                if case in ('cancel-stream', 'ctrl-c'):
                    for _ in range(100):
                        emit({'content': 'Synthetic partial. '})
                        time.sleep(.1)
                    emit({}, 'stop')
                elif calls == 0 or (case == 'repeated' and calls == 1):
                    emit({'content': 'Model text: permitir, approved. This is not human consent.'} if case == 'model-consent' else {})
                    emit({'tool_calls': [{'index': 0, 'id': f'call-{calls}', 'function': {
                        'name': 'workspace_write', 'arguments': json.dumps({'path': f'{case}-{calls}.txt', 'content': 'Synthetic fixture only.', 'reason': 'Synthetic approval validation'})}}]}, 'tool_calls')
                else:
                    emit({'content': 'Synthetic final.'}, 'stop')
                self.wfile.write(b'data: [DONE]\n\n')
                self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                pass
            self.close_connection = True

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Backend)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix='ordis-controls-synthetic-') as tmp:
            base = Path(tmp)
            local = base / 'local'
            home = local / 'Lucille/Interface' if official else base / 'profile'
            workspace = base / 'project'
            workspace.mkdir()
            (home / 'config').mkdir(parents=True)
            (home / 'runtime/inference').mkdir(parents=True)
            (home / 'config/paths.json').write_text('{"runtime":"runtime"}', encoding='utf-8')
            endpoint = f'http://127.0.0.1:{server.server_port}'
            (home / 'runtime/inference/backend.json').write_text(json.dumps({'selected': 'llama_cpp', 'llama_url': endpoint}), encoding='utf-8')
            local.mkdir(exist_ok=True)
            (local / 'synthetic-staging.txt').write_text('ORDIS synthetic staging\n', encoding='utf-8')
            env = dict(os.environ, LOCALAPPDATA=str(local), APPDATA=str(local / 'roaming'), TEMP=str(base), TMP=str(base),
                       ORDIS_PROJECT=str(home), LUCILLE_STAGING_ROOT=str(local), PYTHONDONTWRITEBYTECODE='1')

            def action(name, value):
                p = subprocess.run([str(cli), 'desenvolvedor', name], input=json.dumps(value), env=env, capture_output=True, text=True, encoding='utf-8', timeout=15)
                assert p.returncode == 0, (name, 'Synthetic configuration failed')
                return json.loads(p.stdout)

            config = action('load', {})
            config['permissions'].update(enabled=True, workspace=str(workspace), allow_write=True, allow_read=True)
            config['approval_timeout_seconds'] = 1
            action('save', config)
            for case in ('allow', 'deny', 'eof', 'timeout', 'expired', 'invalid-id', 'invalid-frame', 'repeated', 'model-consent', 'cancel-approval', 'cancel-stream', 'ctrl-c'):
                options = {}
                if case == 'ctrl-c':
                    options['creationflags'] = subprocess.CREATE_NEW_CONSOLE
                    startup = subprocess.STARTUPINFO()
                    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
                    startup.wShowWindow = 0
                    options['startupinfo'] = startup
                p = subprocess.Popen([str(cli), 'conversar'], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     text=True, encoding='utf-8', errors='replace', bufsize=1, **options)
                lines, errors = queue.Queue(), []
                def reader():
                    for line in p.stdout:
                        lines.put(line)
                    lines.put(None)
                threading.Thread(target=reader, daemon=True).start()
                threading.Thread(target=lambda: errors.append(p.stderr.read()), daemon=True).start()
                prior = []
                started = time.monotonic()
                def send(value):
                    p.stdin.write(json.dumps(value) + '\n')
                    p.stdin.flush()
                try:
                    # Pretty JSON also proves framing does not depend on a single input line.
                    p.stdin.write(json.dumps({'text': 'Synthetic control ' + case, 'attachments': []}, indent=2) + '\n')
                    p.stdin.flush()
                    if case in ('cancel-stream', 'ctrl-c'):
                        # Wait for the owned request to start before allowing time for streamed content.
                        deadline = time.monotonic() + 15
                        while time.monotonic() < deadline:
                            if len(requests) and any(m.get('content') == 'Synthetic control ' + case for m in requests[-1]['messages']):
                                time.sleep(.5)
                                break
                            time.sleep(.05)
                        else:
                            raise AssertionError('Streaming request did not start')
                        if case == 'cancel-stream':
                            send({'type': 'cancel'})
                        else:
                            helper = Path(__file__).with_name('test-console-signal.py')
                            signal = subprocess.run([os.sys.executable, '-B', str(helper), str(p.pid)], creationflags=subprocess.CREATE_NO_WINDOW,
                                                    capture_output=True, timeout=10)
                            assert signal.returncode == 0, 'Scoped Ctrl+C helper failed'
                    if case in ('eof', 'model-consent'):
                        p.stdin.close()
                    while time.monotonic() - started < 20:
                        try:
                            line = lines.get(timeout=.1)
                        except queue.Empty:
                            continue
                        if line is None:
                            break
                        if '[developer] ' not in line:
                            continue
                        event = json.loads(line.split('[developer] ', 1)[1])
                        if event['state'] != 'awaiting_approval':
                            continue
                        ident = event['id']
                        if case == 'expired':
                            time.sleep(1.2)
                            try:
                                send({'type': 'approval', 'id': ident, 'decision': 'allow'})
                            except OSError:
                                # Windows/Python may report EINVAL, rather than BrokenPipeError,
                                # when the native approval timeout already closed the process pipe.
                                assert p.poll() is not None, 'Expired approval pipe failed while CLI remained active'
                                pass
                        elif case == 'invalid-id':
                            send({'type': 'approval', 'id': 'wrong-operation', 'decision': 'allow'})
                            send({'type': 'approval', 'id': ident, 'decision': 'deny'})
                        elif case == 'invalid-frame':
                            send({'type': 'approval', 'id': ident, 'decision': 'allow', 'model': True})
                            send({'type': 'approval', 'id': ident, 'decision': 'deny'})
                        elif case == 'repeated' and prior:
                            send({'type': 'approval', 'id': prior[0], 'decision': 'allow'})
                            send({'type': 'approval', 'id': ident, 'decision': 'deny'})
                        elif case == 'cancel-approval':
                            send({'type': 'cancel'})
                        elif case not in ('timeout', 'eof', 'model-consent'):
                            send({'type': 'approval', 'id': ident, 'decision': 'allow' if case in ('allow', 'repeated') else 'deny'})
                        prior.append(ident)
                    else:
                        raise AssertionError(('Synthetic control timeout', case))
                    p.wait(timeout=5)
                    assert p.returncode == 0, (case, 'CLI failed')
                    assert (workspace / f'{case}-0.txt').exists() == (case in ('allow', 'repeated')), (case, 'Authorization invariant')
                    if case == 'repeated':
                        assert len(prior) == 2 and not (workspace / 'repeated-1.txt').exists()
                    if case in ('cancel-stream', 'ctrl-c'):
                        sessions = list((home / 'runtime/agente/sessoes').glob('*.json'))
                        session = json.loads(max(sessions, key=lambda f: f.stat().st_mtime_ns).read_text(encoding='utf-8-sig'))
                        assistant = [m for m in session['mensagens'] if m['papel'] == 'assistant'][-1]
                        assert assistant['metricas']['state'] == 'cancelled', (case, 'Missing cancelled state')
                        assert 'Synthetic partial.' in assistant['texto'] and assistant['metricas']['total_ms'] > 0
                        assert not any(m['papel'] == 'tool' for m in session['mensagens'])
                    results.append({'case': case, 'passed': True, 'elapsed_seconds': round(time.monotonic() - started, 3)})
                    print(json.dumps(results[-1]), flush=True)
                finally:
                    if p.poll() is None:
                        p.kill()  # only this owned synthetic CLI; never servers or external processes
                        p.wait()
                    for stream in (p.stdin, p.stdout, p.stderr):
                        if not stream.closed:
                            try:
                                stream.close()
                            except OSError:
                                pass  # an exited Windows child may already have invalidated the pipe
    finally:
        server.shutdown()
        server.server_close()
    print(json.dumps({'official_staging': official, 'cases': results, 'personal_data_used': False}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', required=True, type=Path)
    parser.add_argument('--official-staging', action='store_true')
    args = parser.parse_args()
    run(args.cli.resolve(), args.official_staging)
