const {test}=require('node:test');
const assert=require('node:assert/strict');
const ui=require('../app/ui/active-memory.js');
test('learning mode defaults are explicit and category constrained',()=>{assert.deepEqual(ui.policy('manual',[]),{mode:'manual',categories:[]});assert.throws(()=>ui.policy('controlled',['instructions']));assert.throws(()=>ui.policy('admin',[]));assert.equal(ui.policy('controlled',['projects','projects']).categories.length,1);});
test('forget requires a specific human confirmation',()=>{const r={id:2,target:3,operation:'forget'};assert.throws(()=>ui.forget(r,false));assert.deepEqual(ui.forget(r,true),{request_id:2,target_id:3,confirmed:true});assert.throws(()=>ui.forget({...r,operation:'update'},true));});
test('functional labels cover eleven categories',()=>{assert.equal(Object.keys(ui.labels).length,11);assert.ok(ui.labels.temporary);assert.ok(ui.labels.experiences);});
