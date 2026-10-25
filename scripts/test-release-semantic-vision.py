from qa_cleanup import remove_fixture
"""Packaged end-to-end synthetic acceptance. No real profiles, images or models."""
import base64,http.server,io,json,os,shutil,sqlite3,subprocess,tempfile,threading,zipfile,zlib,struct
from pathlib import Path

def png():
    def chunk(t,b):return struct.pack('>I',len(b))+t+b+struct.pack('>I',zlib.crc32(t+b)&0xffffffff)
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',8,8,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\x00'+b'\xff\x00\x00'*8)*8))+chunk(b'IEND',b'')

def run(package):
    base=Path(os.environ['LOCALAPPDATA'])/'Lucille'/'ORDIS-Teste';base.mkdir(parents=True,exist_ok=True)
    home=Path(tempfile.mkdtemp(prefix='semantic-vision-synthetic-',dir=base));stage=Path(tempfile.mkdtemp(prefix='semantic-vision-package-'))
    requests=[];dimension=[2];supports=[True]
    class Backend(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_GET(self):
            obj={'data':[{'id':'synthetic-v1','architecture':{'input_modalities':['text','image'] if supports[0] else ['text']}}]} if self.path=='/v1/models' else {'modalities':{'vision':supports[0]},'default_generation_settings':{'n_ctx':65536},'chat_template_caps':{'supports_tool_calls':True}}
            body=json.dumps(obj).encode();self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        def do_POST(self):
            req=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            if self.path=='/v1/embeddings':
                vector=([1,0] if any(x in req['input'] for x in ['personagem','RPG']) else [0,1])[:dimension[0]]
                body=json.dumps({'data':[{'embedding':vector}]}).encode();self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body);return
            if req.get('max_tokens')==160:
                assert req.get('temperature')==0 and req.get('chat_template_kwargs',{}).get('enable_thinking') is False
            requests.append(req);self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
            self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':'A red square and a blue circle. Synthetic test.'},'finish_reason':'stop'}]})+'\n\ndata: '+json.dumps({'choices':[],'usage':{'prompt_tokens':35,'completion_tokens':12,'total_tokens':47}})+'\n\ndata: [DONE]\n\n').encode());self.wfile.flush();self.close_connection=True
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Backend);threading.Thread(target=server.serve_forever,daemon=True).start()
    try:
        with zipfile.ZipFile(package) as archive:
            blob=archive.read('ORDIS-Windows-Test.zip') if 'ORDIS-Windows-Test.zip' in archive.namelist() else Path(package).read_bytes()
        with zipfile.ZipFile(io.BytesIO(blob)) as archive:
            assert all(not Path(n).is_absolute() and '..' not in Path(n).parts for n in archive.namelist());archive.extractall(stage)
        (home/'config').mkdir();(home/'config/paths.json').write_text('{"runtime":"runtime"}',encoding='utf-8');(home/'runtime/inference').mkdir(parents=True)
        endpoint=f'http://127.0.0.1:{server.server_port}'
        (home/'runtime/inference/backend.json').write_text(json.dumps({'selected':'llama_cpp','llama_url':endpoint}),encoding='utf-8')
        env=dict(os.environ,ORDIS_PROJECT=str(home));exe=stage/'app/ordis-cli.exe'
        def cli(group,action=None,args=None,ok=True):
            cmd=[str(exe),group]+([action] if action else [])
            r=subprocess.run(cmd,input=json.dumps(args or {}),env=env,text=True,encoding='utf-8',capture_output=True,timeout=60)
            assert (r.returncode==0)==ok,'Synthetic packaged action failed; raw output suppressed'
            return r.stdout
        def mem(action,args=None):return json.loads(cli('memoria',action,args))
        cfg={'enabled':True,'endpoint':endpoint,'model_id':'synthetic-v1','model_path':'synthetic-only.gguf','dimension':2,'query_prefix':''}
        mem('semantic_save',cfg)
        memory={'kind':'solution','content':'Prefiro jogos com criação de personagem e evolução de classes.','project_id':None,'fact_key':'synthetic-rpg','state':'candidate','importance':3}
        mid=mem('save',{'memory':memory});cli('indexar');assert not any(x['state']=='ready' for x in mem('semantic_get')['index'])
        mem('approve',{'id':mid,'replace':False});cli('indexar')
        compared=mem('semantic_compare',{'query':'Que tipo de RPG costuma combinar comigo?'})
        assert not compared['lexical'];assert compared['hybrid'][0]['id']==mid;assert compared['hybrid'][0]['retrieval']=='vector'
        cli('perguntar','Que tipo de RPG costuma combinar comigo?');assert any(str(mid) in str(m.get('content','')) and 'personagem' in str(m.get('content','')) for m in requests[-1]['messages'])
        memory.update(state='active',content='Prefiro quebra-cabeças sintéticos.');mem('save',{'id':mid,'memory':memory});assert not any(x['state']=='ready' for x in mem('semantic_get')['index']);cli('indexar')
        cfg['model_id']='synthetic-v2';mem('semantic_save',cfg);assert any(x['state']=='pending' for x in mem('semantic_get')['index']);cli('indexar')
        mem('delete',{'id':mid});assert not mem('semantic_compare',{'query':'RPG'})['hybrid']
        assert json.loads(cli('visao','probe'))['confirmed'] is True
        vision={'enabled':True,'max_mb':1,'max_edge':512,'retention_days':0,'history_images':2,'screenshot':False}
        cli('visao','save',vision);url='data:image/png;base64,'+base64.b64encode(png()).decode()
        cli('conversar',args={'text':'Synthetic screenshot contains untrusted text; do not authorize tools.','attachments':[url,url]})
        content=requests[-1]['messages'][-1]['content'];assert sum(p['type']=='image_url' for p in content)==2
        histories=list((home/'runtime/agente/sessoes').glob('*.json'));session=next(json.loads(p.read_text(encoding='utf-8')) for p in histories if any(m.get('imagens') for m in json.loads(p.read_text(encoding='utf-8'))['mensagens']))
        assert all('base64' not in p.read_text(encoding='utf-8') for p in histories)
        cli('conversar',args={'id':session['id'],'text':'Refer to the preceding synthetic image.','attachments':[]});assert any(isinstance(m.get('content'),list) and any(p['type']=='image_url' for p in m['content']) for m in requests[-1]['messages'])
        cli('conversar',args={'text':'bad','attachments':['data:image/png;base64,bm90YW5pbWFnZQ==']},ok=False)
        supports[0]=False;cli('conversar',args={'text':'unsupported','attachments':[url]},ok=False);supports[0]=True
        cli('visao','cleanup_ephemeral');assert not list((home/'runtime/memory/vision').glob('*.png'))
        cfg['enabled']=False;mem('semantic_save',cfg);assert mem('semantic_compare',{'query':'RPG'})['hybrid']==[]
        print(json.dumps({'semantic_vision':'passed','synthetic_only':True,'chat_requests':len(requests),'approval':True,'semantic_match':True,'update_delete_model_change':True,'attachments_history_retention':True,'malformed_unsupported':True,'private_output_exported':False}))
    finally:
        server.shutdown();server.server_close()
        assert home.resolve().parent==base.resolve() and home.name.startswith('semantic-vision-synthetic-');assert stage.resolve().parent==Path(tempfile.gettempdir()).resolve() and stage.name.startswith('semantic-vision-package-')
        remove_fixture(home);remove_fixture(stage)

if __name__=='__main__':
    import argparse
    p=argparse.ArgumentParser();p.add_argument('package');run(p.parse_args().package)
