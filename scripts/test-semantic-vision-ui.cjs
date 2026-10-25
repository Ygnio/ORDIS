const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
function fixture(){
  const nodes=new Map(),calls=[];let confirmed=false;
  const get=id=>{if(!nodes.has(id))nodes.set(id,{value:'',checked:false,textContent:'',disabled:false,events:{},addEventListener(n,f){this.events[n]=f;},set innerHTML(_){throw Error('Unsafe HTML');}});return nodes.get(id);};
  const invoke=async(command,{action,args})=>{calls.push({command,action,args});if(command==='vision_action'&&action==='load')return {enabled:true,screenshot:true,max_mb:12,max_edge:2048,retention_days:0,history_images:4};if(action==='semantic_get')return {config:{enabled:false,endpoint:'http://127.0.0.1:8081',model_id:'synthetic',model_path:'',dimension:2,query_prefix:''},index:[]};if(action==='capture')return {message:'Select and paste'};if(action==='diagnostic')return {model:'<img src=x onerror=alert(1)>'};return true;};
  const context={document:{getElementById:get},window:{__TAURI__:{core:{invoke}},ordis:{avisar(){}}},confirm:()=>confirmed};
  vm.runInNewContext(fs.readFileSync(require.resolve('../app/ui/semantic-vision.js'),'utf8'),context);
  return {get,calls,confirm(v){confirmed=v;},ready:()=>new Promise(r=>setImmediate(r))};
}
test('capture requires human consent and never auto-sends',async()=>{const f=fixture();await f.ready();await f.get('capturar-tela').events.click();assert(!f.calls.some(x=>x.action==='capture'));f.confirm(true);await f.get('capturar-tela').events.click();assert.equal(f.calls.filter(x=>x.action==='capture').length,1);assert(!f.calls.some(x=>x.command==='enviar'));});
test('vision settings persist actual numeric controls and ephemeral default',async()=>{const f=fixture();await f.ready();await f.get('vs-save').events.click();const call=f.calls.find(x=>x.action==='save');assert.equal(call.args.retention_days,0);assert.equal(call.args.max_mb,12);assert.equal(call.args.enabled,true);});
test('reindex consent and diagnostic text remain inert',async()=>{const f=fixture();await f.ready();await f.get('sv-reindex').events.click();assert(!f.calls.some(x=>x.action==='semantic_reindex'));f.confirm(true);await f.get('sv-reindex').events.click();assert.equal(f.calls.filter(x=>x.action==='semantic_reindex').length,1);await f.get('vs-diagnostic').events.click();assert.match(f.get('vs-status').textContent,/<img/);});
