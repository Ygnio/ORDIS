const {test}=require('node:test');
const assert=require('node:assert/strict');
const ui=require('../app/ui/active-memory.js');
test('learning mode defaults are explicit and category constrained',()=>{assert.deepEqual(ui.policy('manual',[]),{mode:'manual',categories:[]});assert.throws(()=>ui.policy('controlled',['instructions']));assert.throws(()=>ui.policy('admin',[]));assert.equal(ui.policy('controlled',['projects','projects']).categories.length,1);});
test('forget requires a specific human confirmation',()=>{const r={id:2,target:3,operation:'forget'};assert.throws(()=>ui.forget(r,false));assert.deepEqual(ui.forget(r,true),{request_id:2,target_id:3,confirmed:true});assert.throws(()=>ui.forget({...r,operation:'update'},true));});
test('functional labels cover eleven categories',()=>{assert.equal(Object.keys(ui.labels).length,11);assert.ok(ui.labels.temporary);assert.ok(ui.labels.experiences);});
test('rendered forget request is inert and requires exact human confirmation',async()=>{
  const vm=require('node:vm'),fs=require('node:fs');const nodes=new Map(),calls=[];let confirmed=false;
  class Element{constructor(){this.children=[];this.value='';this.textContent='';}append(...x){this.children.push(...x);}replaceChildren(){this.children=[];}querySelectorAll(){return [];}set innerHTML(v){throw new Error('Unsafe HTML');}}
  const get=id=>{if(!nodes.has(id))nodes.set(id,new Element());return nodes.get(id);};
  const request={id:17,target:23,operation:'forget',reason:'<img src=x onerror=alert(1)>',content:'Synthetic private fixture',candidate:null};
  const invoke=async(command,{action,args})=>{calls.push({command,action,args});if(action==='learning_get')return {policy:{mode:'manual',categories:[]},auto_categories:['projects']};if(action==='requests')return [request];if(action==='history')return {tasks:[]};return true;};
  const context={document:{getElementById:get,createElement:()=>new Element(),createTextNode:text=>({textContent:text})},window:{__TAURI__:{core:{invoke}},addEventListener(){},dispatchEvent(){},confirm:()=>confirmed},CustomEvent:function(){}};
  vm.runInNewContext(fs.readFileSync(require.resolve('../app/ui/active-memory.js'),'utf8'),context);
  await get('learning-refresh').onclick();const article=get('learning-requests').children[0];assert.match(article.children[0].textContent,/<img/);
  await article.children[1].onclick();assert.equal(calls.filter(x=>x.action==='forget_confirm').length,0);
  confirmed=true;await article.children[1].onclick();const sent=calls.find(x=>x.action==='forget_confirm');assert.equal(sent.args.request_id,17);assert.equal(sent.args.target_id,23);assert.equal(sent.args.confirmed,true);
});
