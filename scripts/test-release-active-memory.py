"""Active memory acceptance with an isolated synthetic profile; prints aggregates only.
--real contacts the already running localhost server without changing its configuration.
"""
import argparse
import http.server
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import zipfile

def run(package,real=False):
    base=Path(os.environ['LOCALAPPDATA'])/'Lucille'/'ORDIS-Teste';base.mkdir(parents=True,exist_ok=True)
    home=Path(tempfile.mkdtemp(prefix='active-acceptance-synthetic-',dir=base))
    stage=Path(tempfile.mkdtemp(prefix='active-release-synthetic-'))
    requests=[];mode=['learn'];target=[None];calls={};server=None
    text={'learn':'Lucille, meu projeto fictício se chama Aurora.','update':'Renomeei Aurora para Horizonte.','forget':'Esqueça o nome desse projeto fictício.','recall':'Qual era o nome do projeto fictício?'}
    class Backend(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_GET(self):
            obj={'data':[{'id':'SyntheticActiveMemory'}]} if self.path=='/v1/models' else {'default_generation_settings':{'n_ctx':8192},'chat_template_caps':{'supports_tool_calls':True}}
            body=json.dumps(obj).encode();self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        def do_POST(self):
            req=json.loads(self.rfile.read(int(self.headers['Content-Length'])));requests.append(req)
            names=[x['function']['name'] for x in req['tools']]
            assert all(n in names for n in ['memory_search','memory_propose','memory_update','memory_forget','memory_inspect'])
            assert not any(n in names for n in ['learning_save','forget_confirm','approve'])
            key=mode[0];calls[key]=calls.get(key,0)+1;delta={'content':'Resultado sintético; confira aprovação na página Memória.'}
            if calls[key]==1:
                name={'learn':'memory_propose','update':'memory_update','forget':'memory_forget','recall':'memory_search'}[key]
                args={'learn':{'content':text['learn'],'category':'projects'},'update':{'id':target[0],'content':text['update'],'category':'projects','reason':'Renomeação sintética'},'forget':{'id':target[0],'reason':'Pedido sintético explícito'},'recall':{'query':'Horizonte'}}[key]
                delta={'tool_calls':[{'index':0,'id':'synthetic-active-call','type':'function','function':{'name':name,'arguments':json.dumps(args)}}]}
            self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
            self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode());self.wfile.flush();self.close_connection=True
    try:
        with zipfile.ZipFile(package) as outer:
            blob=outer.read('ORDIS-Windows-Test.zip') if 'ORDIS-Windows-Test.zip' in outer.namelist() else Path(package).read_bytes()
        with zipfile.ZipFile(io.BytesIO(blob)) as archive:
            for name in archive.namelist():assert not Path(name).is_absolute() and '..' not in Path(name).parts
            archive.extractall(stage)
        (home/'config').mkdir();(home/'config/paths.json').write_text('{"runtime":"runtime"}',encoding='utf-8')
        (home/'runtime/inference').mkdir(parents=True)
        if not real:
            tabby=home/'runtime/tabbyAPI';tabby.mkdir()
            (tabby/'config.yml').write_text('model:\n  tool_format: qwen3_5\n',encoding='utf-8')
            (tabby/'api_tokens.yml').write_text('api_key: synthetic-fixture-only\n',encoding='utf-8')
        if real:endpoint='http://127.0.0.1:8080'
        else:
            server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Backend);threading.Thread(target=server.serve_forever,daemon=True).start();endpoint=f'http://127.0.0.1:{server.server_port}'
        cfg=home/'runtime/inference/backend.json';env=dict(os.environ,ORDIS_PROJECT=str(home));exe=stage/'app/ordis-cli.exe'
        def select(backend):cfg.write_text(json.dumps({'selected':backend,'llama_url':endpoint,'tabby_url':endpoint}),encoding='utf-8')
        def mem(action,args=None):
            r=subprocess.run([str(exe),'memoria',action],input=json.dumps(args or {}),env=env,text=True,encoding='utf-8',capture_output=True,timeout=40)
            assert r.returncode==0,'Synthetic memory action failed; private output not exported'
            return json.loads(r.stdout)
        def ask(key,extra=''):
            mode[0]=key;calls[key]=0
            r=subprocess.run([str(exe),'perguntar',text[key]+extra],env=env,text=True,encoding='utf-8',capture_output=True,timeout=600 if real else 50)
            assert r.returncode==0,'Synthetic conversation failed; private output not exported'
        select('llama_cpp');assert mem('learning_get')['policy']['mode']=='manual'
        ask('learn',' Use memory_propose com um trecho literal e categoria projects.' if real else '')
        proposals=mem('active_list',{'category':'projects','state':'candidate'})
        assert proposals,'No classified proposal from model';first=proposals[0]['id'];mem('approve',{'id':first,'replace':False})
        ask('recall');assert mem('list',{'state':'active'})
        if not real:assert any('Aurora' in str(m.get('content','')) and str(m.get('content','')).startswith('<lucille_memory_data>') for m in requests[-1]['messages'])
        target[0]=first
        ask('update',f' Use memory_update no ID {first}, trecho literal, categoria projects e motivo renomeação.' if real else '')
        pending=[r for r in mem('requests') if r['operation']=='update'];assert pending,'No native update proposal'
        second=pending[0]['candidate'];mem('approve',{'id':second,'replace':True});assert mem('versions',{'id':first})
        select('llama_cpp' if real else 'tabbyapi');ask('recall')
        if not real:assert any('Horizonte' in str(m.get('content','')) for m in requests[-1]['messages'])
        target[0]=second
        ask('forget',f' Use memory_forget para solicitar exclusão somente do ID {second}; aguardarei a interface.' if real else '')
        pending=[r for r in mem('requests') if r['operation']=='forget'];assert pending,'No native forget request'
        req=pending[0];assert any(m['id']==second for m in mem('list'))
        mem('forget_confirm',{'request_id':req['id'],'target_id':second,'confirmed':True})
        assert all(m['id']!=second for m in mem('list'))
        assert not mem('versions',{'id':second})
        ask('recall');assert not mem('list',{'state':'active'})
        assert mem('technical')['schema']==6
        print(json.dumps({'active_memory':'passed','mode':'real-localhost' if real else 'synthetic-backends','learning':True,'update':True,'exact_human_forget':True,'process_restart_persistence':True,'private_output_exported':False,'requests':None if real else len(requests)}))
    finally:
        if server:server.shutdown();server.server_close()
        assert home.resolve().parent==base.resolve() and home.name.startswith('active-acceptance-synthetic-')
        assert stage.resolve().parent==Path(tempfile.gettempdir()).resolve() and stage.name.startswith('active-release-synthetic-')
        shutil.rmtree(home);shutil.rmtree(stage)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('package');p.add_argument('--real',action='store_true');args=p.parse_args();run(args.package,args.real)
