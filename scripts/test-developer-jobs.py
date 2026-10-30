"""Run native synthetic jobs through CLI approvals. Never installs tools or calls AI."""
import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time


def elevated():
    token=wintypes.HANDLE();value=wintypes.DWORD();size=wintypes.DWORD()
    api=ctypes.WinDLL('advapi32');kernel=ctypes.WinDLL('kernel32')
    kernel.GetCurrentProcess.restype=wintypes.HANDLE
    kernel.CloseHandle.argtypes=[wintypes.HANDLE]
    api.OpenProcessToken.argtypes=[wintypes.HANDLE,wintypes.DWORD,ctypes.POINTER(wintypes.HANDLE)]
    api.GetTokenInformation.argtypes=[wintypes.HANDLE,ctypes.c_int,ctypes.c_void_p,wintypes.DWORD,ctypes.POINTER(wintypes.DWORD)]
    if not api.OpenProcessToken(kernel.GetCurrentProcess(),8,ctypes.byref(token)):return True
    ok=api.GetTokenInformation(token,20,ctypes.byref(value),4,ctypes.byref(size));kernel.CloseHandle(token)
    return not ok or bool(value.value)


def run(cli, tools, output, languages, allow_elevated_blocked=False):
    results = []
    with tempfile.TemporaryDirectory(prefix='ordis-jobs-synthetic-') as tmp:
        root = Path(tmp)
        local = root / 'local'
        home = local / 'Lucille/Interface'
        workspace = root / 'workspace'
        (home / 'config').mkdir(parents=True)
        (home / 'runtime').mkdir()
        workspace.mkdir()
        local.mkdir(exist_ok=True)
        (home / 'config/paths.json').write_text('{"runtime":"runtime"}', encoding='utf-8')
        (local / 'synthetic-staging.txt').write_text('ORDIS synthetic staging\n', encoding='utf-8')
        env = dict(os.environ, ORDIS_PROJECT=str(home), LOCALAPPDATA=str(local), APPDATA=str(local / 'roaming'),
                   LUCILLE_STAGING_ROOT=str(local), TEMP=str(root), TMP=str(root))

        def action(name, args):
            p = subprocess.run([str(cli), 'desenvolvedor', name], input=json.dumps(args), env=env,
                               capture_output=True, text=True, encoding='utf-8', timeout=30)
            if p.returncode:
                raise AssertionError((name, p.stderr, p.stdout))
            return json.loads(p.stdout)

        cfg = action('load', {})
        cfg['permissions'].update(enabled=True, workspace=str(workspace), allow_read=True, allow_write=True, allow_execute=True)
        cfg['approval_timeout_seconds'] = 2
        cfg['jobs'].update(tools, enabled=True, seconds=15, subprocess_seconds=12, storage_mib=128)
        action('save', cfg)

        def request(project, operation, decision='allow', cancel=False, extra=None, interrupt=False):
            p = subprocess.Popen([str(cli), 'job', 'solicitar'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, env=env, text=True, encoding='utf-8', bufsize=1)
            events = queue.Queue()
            def read():
                for line in p.stdout:
                    events.put(line)
                events.put(None)
            threading.Thread(target=read, daemon=True).start()
            payload = dict(project=project, operation=operation, reason='Synthetic controlled test')
            payload.update(extra or {})
            p.stdin.write(json.dumps(payload) + '\n'); p.stdin.flush()
            frames, identity, cancelled = [], None, False
            started=time.monotonic()
            deadline = started + 180
            while time.monotonic() < deadline:
                try:
                    line = events.get(timeout=.2)
                except queue.Empty:
                    continue
                if line is None:
                    break
                frame = json.loads(line); frame["harness_elapsed_s"]=time.monotonic()-started; frames.append(frame)
                output.write_text(json.dumps(dict(results=results, current=frames), indent=2), encoding="utf-8")
                event = frame.get('data', {})
                if event.get('state') == 'awaiting_approval':
                    identity = event['id']
                    if decision in ('allow', 'deny'):
                        p.stdin.write(json.dumps(dict(type='approval', id=identity, decision=decision)) + '\n'); p.stdin.flush()
                    elif decision == 'eof':
                        p.stdin.close()
                if interrupt and event.get('state') == 'progress':
                    p.kill()  # Only this harness-owned CLI; its own Job Object closes.
                if cancel and event.get('state') == 'progress' and not cancelled:
                    p.stdin.write('{"type":"cancel"}\n'); p.stdin.flush(); cancelled = True
            if p.poll() is None and time.monotonic() < deadline:
                try:
                    p.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    pass
            if p.poll() is None:
                # This is solely the exact CLI process created by this test.
                p.kill(); p.wait()
                output.write_text(json.dumps(dict(results=results, current=frames, stderr=p.stderr.read()), indent=2), encoding='utf-8')
                raise AssertionError('Native job exceeded harness deadline')
            stderr = p.stderr.read()
            result = next((f['data'] for f in reversed(frames) if f['type'] == 'job_result'), None)
            results.append(dict(project=project, operation=operation, decision=decision, frames=frames, stderr=stderr, returncode=p.returncode))
            output.write_text(json.dumps(dict(results=results, count=len(results)), indent=2), encoding='utf-8')
            return result, identity

        if elevated():
            assert allow_elevated_blocked, 'Execution tests require a non-elevated process'
            action('job_fixture', dict(language='node', case='pass'))
            result, identity=request('node-pass','node_test')
            assert result is None and identity
            record=action('job_get',dict(id=identity))
            assert record['state']=='failed' and record['result'] is None
            assert 'elevado' in record['summary']['reason']
            for decision in ('deny','expire'):
                request('node-pass','node_test',decision)
            output.write_text(json.dumps(dict(results=results,positive_execution='BLOCKED_ELEVATED',negative_policy='PASS'),indent=2),encoding='utf-8')
            print('Positive execution BLOCKED_ELEVATED; elevation/refusal/expiry policy PASS')
            return

        for language in languages:
            for case in ('pass', 'test_fail', 'compile_error', 'wait', 'output'):
                action('job_fixture', dict(language=language, case=case))
            operations = dict(node=['node_version', 'node_syntax', 'node_test'], python=['python_syntax', 'python_test'],
                              rust=['rust_check', 'rust_test', 'rust_fmt'])[language]
            for operation in operations:
                result, _ = request(f'{language}-pass', operation)
                assert result and result['state'] == 'completed', results[-1]
            test = dict(node='node_test', python='python_test', rust='rust_test')[language]
            result, _ = request(f'{language}-test_fail', test)
            assert result and result['state'] == 'failed', results[-1]
            syntax = dict(node='node_syntax', python='python_syntax', rust='rust_check')[language]
            result, _ = request(f'{language}-compile_error', syntax)
            assert result and result['state'] == 'failed', results[-1]

        for decision in ('deny', 'eof', 'expire'):
            result, identity = request('node-pass', 'node_test', decision)
            record = action('job_get', dict(id=identity))
            assert record['approval_state'] in ('denied', 'expired')
            assert record['state'] != 'completed'
        result, _ = request('node-wait', 'node_test', cancel=True)
        assert result and result['state'] == 'cancelled', results[-1]
        result, _ = request('python-wait', 'python_test')
        assert result and result['state'] == 'timed_out', results[-1]
        result, _ = request('node-output', 'node_test')
        assert result and result['state'] == 'failed', results[-1]
        for language in ('node','python'):
            action('job_fixture',dict(language=language,case='child'))
            result,_=request(f'{language}-child',f'{language}_test',cancel=True)
            assert result and result['state']=='cancelled',results[-1]
            assert result['trusted_executor']['owned_pids']
        result,identity=request('python-child','python_test',interrupt=True)
        record=action('job_get',dict(id=identity))
        assert result is None and record['state']=='interrupted',record
        assert record['summary']['automatic_retry'] is False
        for project, op, extra in (('../escape', 'node_test', None), ('node-pass', 'npm_install', None),
                                  ('node-pass', 'node_test', {'arguments': ['--eval', 'evil']})):
            result, identity = request(project, op, extra=extra)
            assert result is None and identity is None
        (workspace / 'node-pass/app.cjs').write_text('unreviewed code', encoding='utf-8')
        result, identity = request('node-pass', 'node_test')
        assert result is None and identity is None
        history = action('job_history', {})
        assert not history['automatic_execution']
        assert all(j['state'] not in ('running', 'pending', 'awaiting_approval') for j in history['jobs'])
    output.write_text(json.dumps(dict(results=results, count=len(results)), indent=2), encoding='utf-8')
    print(f'{len(results)} native synthetic job scenarios PASS')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--tools', type=Path, required=True, help='Private JSON: existing native tools only')
    parser.add_argument('--allow-elevated-blocked', action='store_true', help='Report positive execution as BLOCKED on elevated CI, never disable the guard')
    parser.add_argument('--languages', nargs='+', choices=['node', 'python', 'rust'], default=['node', 'python', 'rust'])
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(args.cli.resolve(), json.loads(args.tools.read_text(encoding='utf-8-sig')), args.output, args.languages, args.allow_elevated_blocked)
