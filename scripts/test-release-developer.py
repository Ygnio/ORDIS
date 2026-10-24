"""Synthetic developer acceptance. --real uses only an existing localhost llama server.
No conversations, profiles, source outputs or backend responses are published.
The harness acts as the trusted human test controller for known synthetic operations.
"""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import zipfile

BUG='def add(a,b): return a-b\n'
FIX='def add(a,b): return a+b\n'
TEST='import unittest\nfrom calc import add\nclass Calc(unittest.TestCase):\n def test_add(self):\n  self.assertEqual(add(2,3),5)\nunittest.main()\n'

def elevated():
    token=wintypes.HANDLE();value=wintypes.DWORD();size=wintypes.DWORD()
    api=ctypes.WinDLL('advapi32');kernel=ctypes.WinDLL('kernel32')
    kernel.GetCurrentProcess.restype=wintypes.HANDLE
    kernel.CloseHandle.argtypes=[wintypes.HANDLE]
    api.OpenProcessToken.argtypes=[wintypes.HANDLE,wintypes.DWORD,ctypes.POINTER(wintypes.HANDLE)]
    api.GetTokenInformation.argtypes=[wintypes.HANDLE,ctypes.c_int,ctypes.c_void_p,wintypes.DWORD,ctypes.POINTER(wintypes.DWORD)]
    api.OpenProcessToken(kernel.GetCurrentProcess(),8,ctypes.byref(token))
    ok=api.GetTokenInformation(token,20,ctypes.byref(value),4,ctypes.byref(size));kernel.CloseHandle(token)
    return not ok or bool(value.value)

def run(package, real=False):
    base=Path(os.environ['LOCALAPPDATA'])/'Lucille'/'ORDIS-Teste';base.mkdir(parents=True,exist_ok=True)
    home=Path(tempfile.mkdtemp(prefix='developer-acceptance-synthetic-',dir=base))
    workspace=Path(tempfile.mkdtemp(prefix='developer-project-synthetic-'))
    stage=Path(tempfile.mkdtemp(prefix='developer-release-synthetic-'))
    requests=[];approvals=[];results=[];server=None
    def digest(text):return hashlib.sha256(text.encode()).hexdigest()

    class Backend(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_GET(self):
            obj={'data':[{'id':'SyntheticDeveloper'}]} if self.path=='/v1/models' else {'default_generation_settings':{'n_ctx':8192},'chat_template_caps':{'supports_tool_calls':True}}
            body=json.dumps(obj).encode();self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        def do_POST(self):
            req=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(req)
            mode=next(m['content'] for m in req['messages'] if m['role']=='user' and isinstance(m['content'],str) and m['content'].startswith('Synthetic developer ')).split()[-1]
            n=sum(1 for r in requests if any(m.get('content')=='Synthetic developer '+mode for m in r['messages']))
            name=None;args={};text='Synthetic task complete: verified tool results.'
            if mode=='approved':
                if n==1:name='workspace_list'
                elif n==2:name='workspace_read';args={'path':'calc.py'}
                elif n==3:name='workspace_patch';args={'path':'calc.py','expected_sha256':digest(BUG),'old':'a-b','new':'a+b','reason':'Correct synthetic addition'}
                elif n==4:name='developer_execute';args={'executable':'python','arguments':['test_calc.py'],'reason':'Run synthetic unit test'}
            elif mode=='refused':
                if n<=2:name='workspace_write';args={'path':'refused'+str(n)+'.txt','content':'synthetic','reason':'Synthetic refusal test'}
            elif mode=='malformed':
                if n==1:name='workspace_read';args='{'  # Backend sends invalid JSON argument fragment.
                elif n==2:name='workspace_read';args={'path':'calc.py','extra':True}
                elif n==3:name='workspace_read';args={'path':'calc.py'}
            elif mode=='loop':name='workspace_list'
            names=[x['function']['name'] for x in req['tools']]
            assert 'workspace_patch' in names and 'developer_decide' not in names
            delta={'content':text} if not name else {'tool_calls':[{'index':0,'id':f'synthetic-{n}','type':'function','function':{'name':name,'arguments':args if isinstance(args,str) else json.dumps(args)}}]}
            self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
            self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode());self.wfile.flush();self.close_connection=True

    try:
        archive=zipfile.ZipFile(package)
        if 'ORDIS-Windows-Test.zip' in archive.namelist():archive=zipfile.ZipFile(io.BytesIO(archive.read('ORDIS-Windows-Test.zip')))
        for name in archive.namelist():assert not Path(name).is_absolute() and '..' not in Path(name).parts
        archive.extractall(stage)
        (home/'config').mkdir();(home/'config/paths.json').write_text('{"runtime":"runtime"}',encoding='utf-8')
        (home/'runtime/inference').mkdir(parents=True)
        if real:endpoint='http://127.0.0.1:8080'
        else:
            server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Backend)
            threading.Thread(target=server.serve_forever,daemon=True).start();endpoint=f'http://127.0.0.1:{server.server_port}'
        (home/'runtime/inference/backend.json').write_text(json.dumps({'selected':'llama_cpp','llama_url':endpoint,'tabby_url':endpoint}),encoding='utf-8')
        env=dict(os.environ,ORDIS_PROJECT=str(home));exe=stage/'app/ordis-cli.exe'
        def action(name,args=None):
            p=subprocess.run([str(exe),'desenvolvedor',name],input=json.dumps(args or {}),text=True,encoding='utf-8',capture_output=True,env=env,timeout=20)
            assert p.returncode==0,'Synthetic configuration failed; no private output exported.'
            return json.loads(p.stdout)
        cfg=action('load');cfg['permissions'].update({'workspace':str(workspace),'enabled':True,'allow_read':True,'allow_write':True,'allow_execute':True,'python':sys.executable,'shell':str(Path(os.environ['SystemRoot'])/'System32/WindowsPowerShell/v1.0/powershell.exe'),'timeout_seconds':15});cfg['max_calls']=16
        action('save',cfg)
        (workspace/'calc.py').write_text(BUG,encoding='utf-8');(workspace/'test_calc.py').write_text(TEST,encoding='utf-8')
        def ask(text,allow):
            process=subprocess.Popen([str(exe),'perguntar',text],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8',errors='replace',bufsize=1)
            lines=queue.Queue();captured=[]
            def reader():
                for line in process.stdout:lines.put(line)
                lines.put(None)
            def drain_errors():
                # Reasoning is a separate stream; never let its pipe stall the model.
                while process.stderr.read(4096):pass
            stdout_thread=threading.Thread(target=reader,daemon=True);stdout_thread.start()
            stderr_thread=threading.Thread(target=drain_errors,daemon=True);stderr_thread.start()
            deadline=time.monotonic()+(180 if real else 45)
            try:
                while time.monotonic()<deadline:
                    try:line=lines.get(timeout=.2)
                    except queue.Empty:continue
                    if line is None:break
                    captured.append(line)
                    if '[developer] ' in line:
                        event=json.loads(line.split('[developer] ',1)[1]);results.append(event)
                        if event['state']=='awaiting_approval':
                            proposal=event['proposal'];tool=proposal['tool'];approvals.append(tool)
                            # The test controller approves only known fixture files and the configured interpreter.
                            path=proposal.get('path','')
                            permitted=allow and (workspace/path).resolve() in {(workspace/'calc.py').resolve(),(workspace/'test_calc.py').resolve()} and tool in {'workspace_patch','developer_execute'}
                            if tool=='developer_execute':permitted=permitted and os.path.samefile(proposal['executable'],sys.executable)
                            process.stdin.write('permitir\n' if permitted else 'recusar\n');process.stdin.flush()
                else:raise AssertionError('Synthetic task timed out')
                process.wait(timeout=10)
                return process.returncode,''.join(captured)
            finally:
                if process.poll() is None:process.kill();process.wait()
                stdout_thread.join(timeout=5);stderr_thread.join(timeout=5)
                for stream in [process.stdin,process.stdout,process.stderr]:stream.close()
        prompt='Lucille, examine este projeto, encontre o erro, corrija o código e execute os testes para verificar se a correção funcionou. Use workspace_list/read, apresente plano, workspace_patch com hash e developer_execute python test_calc.py. Somente os arquivos sintéticos deste workspace.'
        code,output=ask(prompt if real else 'Synthetic developer approved',True)
        assert code==0,'Approved task failed; private output not printed.'
        changed=(workspace/'calc.py').read_text(encoding='utf-8')
        assert (changed!=BUG if real else changed==FIX),'Model did not apply expected synthetic fix'
        assert 'workspace_patch' in approvals and 'developer_execute' in approvals
        completed=[e.get('result',{}) for e in results if isinstance(e.get('result'),dict) and 'exit_code' in e['result']]
        if not elevated():assert any(v.get('exit_code')==0 and 'Ran 1 test' in v.get('stderr','') for v in completed),'No successful real test execution'
        else:assert any('elevado' in e.get('result',{}).get('error','') for e in results),'Elevated production execution must fail closed'
        approved_count=len(approvals);results.clear();approvals.clear()
        (workspace/'calc.py').write_text(BUG,encoding='utf-8');before=digest(BUG)
        code,_=ask(prompt if real else 'Synthetic developer refused',False)
        assert code==0
        assert digest((workspace/'calc.py').read_text(encoding='utf-8'))==before
        assert not list(workspace.glob('refused*.txt'))
        assert len(approvals)==1,'Refusal must block further mutations this turn'
        if not real:
            code,_=ask('Synthetic developer malformed',False);assert code==0
            cfg['max_calls']=5;action('save',cfg);code,_=ask('Synthetic developer loop',False);assert code!=0
            assert len(requests)<=16
        history=action('history');assert history['tasks'] and history['operations']
        print(json.dumps({'mode':'real-localhost' if real else 'synthetic-backend','approved_operations':approved_count,'refusal_unchanged':True,'history_persisted':True,'execution':'blocked-elevated' if elevated() else 'passed','requests':len(requests),'private_output_exported':False}))
    finally:
        if server:server.shutdown();server.server_close()
        # Exact directories created above, never an existing user installation or project.
        assert home.parent.resolve()==base.resolve() and home.name.startswith('developer-acceptance-synthetic-')
        assert workspace.name.startswith('developer-project-synthetic-') and stage.name.startswith('developer-release-synthetic-')
        shutil.rmtree(home);shutil.rmtree(workspace);shutil.rmtree(stage)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('package');parser.add_argument('--real',action='store_true');args=parser.parse_args();run(args.package,args.real)
