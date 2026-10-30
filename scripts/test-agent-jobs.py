"""Exercise model->job routing, approval, timeout flags and external cancellation on a mock."""
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


def run(cli, tools, output):
    requests = []
    results = []

    class Backend(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            data = json.dumps({'data': [{'id': 'SyntheticAgentJobs'}]} if self.path == '/v1/models' else {
                'default_generation_settings': {'n_ctx': 65536}, 'chat_template_caps': {'supports_tool_calls': True}}).encode()
            self.send_response(200); self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data)

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(body)
            prompt = next(m['content'] for m in reversed(body['messages']) if m['role'] == 'user' and isinstance(m['content'], str) and m['content'].startswith('Synthetic agent job '))
            case = prompt.split()[-1]
            self.send_response(200); self.send_header('Content-Type', 'text/event-stream'); self.send_header('Connection', 'close'); self.end_headers()
            if body['messages'][-1]['role'] == 'tool':
                delta, finish = {'content': 'Synthetic final after native job.'}, 'stop'
            else:
                assert any(t['function']['name'] == 'developer_job' for t in body['tools'])
                project, op = ('node-output', 'node_test') if case == 'stalled' else (
                    ('python-wait', 'python_test') if case == 'timeout' else ('python-pass', 'python_test'))
                args = dict(project=project, operation=op, reason='Synthetic model tool request')
                delta, finish = {'tool_calls': [{'index': 0, 'id': 'synthetic-job-call', 'function': {
                    'name': 'developer_job', 'arguments': json.dumps(args)}}]}, 'tool_calls'
            try:
                self.wfile.write(('data: ' + json.dumps({'model': 'SyntheticAgentJobs', 'choices': [{'delta': delta, 'finish_reason': finish}]}) + '\n\ndata: [DONE]\n\n').encode()); self.wfile.flush()
            except (BrokenPipeError, ConnectionError):
                pass
            self.close_connection = True

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Backend)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix='ordis-agent-jobs-synthetic-') as tmp:
            root = Path(tmp); local = root / 'local'; home = local / 'Lucille/Interface'; workspace = root / 'project'
            (home / 'config').mkdir(parents=True); (home / 'runtime/inference').mkdir(parents=True); workspace.mkdir()
            (home / 'config/paths.json').write_text('{"runtime":"runtime"}', encoding='utf-8')
            (local / 'synthetic-staging.txt').write_text('ORDIS synthetic staging\n', encoding='utf-8')
            endpoint = f'http://127.0.0.1:{server.server_port}'
            (home / 'runtime/inference/backend.json').write_text(json.dumps(dict(selected='llama_cpp', llama_url=endpoint)), encoding='utf-8')
            env = dict(os.environ, LOCALAPPDATA=str(local), APPDATA=str(local / 'roaming'), ORDIS_PROJECT=str(home), LUCILLE_STAGING_ROOT=str(local), TEMP=str(root), TMP=str(root))

            def action(name, value):
                p = subprocess.run([str(cli), 'desenvolvedor', name], input=json.dumps(value), env=env, capture_output=True, text=True, encoding='utf-8', timeout=45)
                assert p.returncode == 0, p.stderr
                return json.loads(p.stdout)

            cfg = action('load', {}); cfg['permissions'].update(enabled=True, allow_read=True, allow_write=True, allow_execute=True, workspace=str(workspace))
            cfg['approval_timeout_seconds'] = 2; cfg['max_output_bytes'] = 256000
            cfg['jobs'].update(tools, enabled=True, seconds=3, subprocess_seconds=3)
            action('save', cfg)
            for language, case in [('python', 'pass'), ('python', 'wait'), ('node', 'output')]:
                action('job_fixture', dict(language=language, case=case))
            for case in ('allow', 'deny', 'timeout', 'external-cancel', 'stalled'):
                prompt = f'Synthetic agent job {case}'
                p = subprocess.Popen([str(cli), 'conversar'], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8', bufsize=1)
                p.stdin.write(json.dumps(dict(text=prompt, attachments=[])) + '\n'); p.stdin.flush()
                lines = queue.Queue(); stop_reading = threading.Event()

                def reader():
                    for line in p.stdout:
                        lines.put(line)
                        if case == 'stalled' and '[developer] ' in line and 'awaiting_approval' in line:
                            stop_reading.wait(60)
                    lines.put(None)
                thread = threading.Thread(target=reader, daemon=True); thread.start()
                identity = None; deadline = time.monotonic() + 60
                while time.monotonic() < deadline:
                    try:
                        line = lines.get(timeout=.2)
                    except queue.Empty:
                        if p.poll() is not None: break
                        continue
                    if line is None: break
                    if '[developer] ' not in line: continue
                    event = json.loads(line.split('[developer] ', 1)[1])
                    if event['state'] == 'awaiting_approval':
                        identity = event['id']
                        if case == 'external-cancel':
                            action('job_cancel', dict(id=identity))
                        else:
                            p.stdin.write(json.dumps(dict(type='approval', id=identity, decision='deny' if case == 'deny' else 'allow')) + '\n'); p.stdin.flush()
                if p.poll() is None:
                    try: p.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        p.kill(); p.wait(); raise AssertionError('Agent job or stdout monitor exceeded harness deadline')
                stop_reading.set(); thread.join(timeout=2)
                assert identity, (case, p.stderr.read())
                record = action('job_get', dict(id=identity))
                expected = dict(allow='completed', deny='failed', timeout='timed_out', **{'external-cancel': 'cancelled'})
                if case == 'stalled':
                    assert record['state'] in ('failed', 'cancelled', 'timed_out') and p.returncode != 0
                else:
                    assert record['state'] == expected[case], (case, record)
                    session=None
                    for file in (home / 'runtime/agente/sessoes').glob('*.json'):
                        candidate=json.loads(file.read_text(encoding='utf-8'))
                        if any(m.get('papel')=='user' and m.get('texto')==prompt for m in candidate['mensagens']):
                            session=candidate;break
                    assert session is not None,case
                    tool = next(m for m in session['mensagens'] if m.get('ferramenta') == 'developer_job')
                    assert bool(tool.get('erro')) == (case != 'allow'), (case, tool)
                results.append(dict(case=case, state=record['state'], returncode=p.returncode, error_flag_verified=case != 'stalled'))
                output.write_text(json.dumps(dict(results=results, requests=len(requests)), indent=2), encoding='utf-8')
            assert any(m['role'] == 'tool' and 'trusted_executor' in m['content'] and 'untrusted_output' in m['content'] for b in requests for m in b['messages'])
        print('5 mock agent job scenarios PASS; no real model used')
    finally:
        server.shutdown(); server.server_close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('--allow-elevated-blocked', action='store_true'); parser.add_argument('--cli', type=Path, required=True); parser.add_argument('--tools', type=Path, required=True); parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.allow_elevated_blocked:
        import importlib.util
        spec=importlib.util.spec_from_file_location('job_policy_test',Path(__file__).with_name('test-developer-jobs.py'))
        policy=importlib.util.module_from_spec(spec);spec.loader.exec_module(policy)
        if policy.elevated():
            args.output.write_text(json.dumps({'positive_execution':'BLOCKED_ELEVATED','real_model_requests':0},indent=2),encoding='utf-8')
            print('Agent job execution BLOCKED_ELEVATED; native refusal covered by developer-jobs suite')
            raise SystemExit(0)
    run(args.cli.resolve(), json.loads(args.tools.read_text(encoding='utf-8-sig')), args.output)
