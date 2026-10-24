const {test}=require('node:test');const assert=require('node:assert/strict');const fs=require('node:fs');
const ui=require('../app/ui/developer.js');
test('only explicit allow authorizes operation',()=>{assert.equal(ui.decision('allow'),true);for(const v of ['deny','cancel','true',true,{},undefined])assert.equal(ui.decision(v),false);});
test('task states distinguish refusal interruption and failure',()=>{for(const s of ['awaiting_approval','running','completed','failed','cancelled','refused','timeout','interrupted'])assert.notEqual(ui.label(s),s);});
test('untrusted previews are text, approval goes through native IPC',()=>{const s=fs.readFileSync('app/ui/developer.js','utf8');assert.ok(!s.includes('innerHTML'));assert.match(s,/developer_decide/);assert.match(s,/interromper/);assert.match(s,/developer_action/);assert.match(s,/Abrir conversa/);});
