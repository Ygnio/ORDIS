const { test } = require('node:test');
const assert = require('node:assert/strict');
const ui = require('../app/ui/memory.js');
test('memory toggles remain independent', () => {
  assert.deepEqual(ui.options(true, false, '1024', ''), { auto_consolidation:false,retrieval:true, extraction:false, token_budget:1024, project_id:null });
  assert.equal(ui.options(false, true, 512, '2').project_id, 2);
});
test('memory budget rejects invalid input', () => {
  for (const n of [0,127,4097,NaN,'text',2.5]) assert.throws(() => ui.options(true,true,n,''));
});
test('restore requires explicit confirmation and validation', () => {
  assert.throws(() => ui.restoreArgs('snapshot-1.sqlite3','digest',false));
  assert.throws(() => ui.restoreArgs('snapshot-1.sqlite3','',true));
  assert.deepEqual(ui.restoreArgs('snapshot-1.sqlite3','digest',true), {name:'snapshot-1.sqlite3',digest:'digest',confirmed:true});
});

test('automatic consolidation is explicit and independent of retrieval',()=>{assert.equal(ui.options(false,true,512,'',true).auto_consolidation,true);assert.equal(ui.options(true,true,512,'').auto_consolidation,false);});
