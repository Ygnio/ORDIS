const {test}=require('node:test');const assert=require('node:assert/strict');
const ui=require('../app/ui/identity.js');
test('profile keeps permanent fields distinct',()=>{const p={name:'Lucille',description:'Synthetic',personality:'Brief',instructions:'Synthetic policy'};assert.deepEqual(ui.validate(p),p);});
test('empty and oversized identity names rejected',()=>{assert.throws(()=>ui.validate({name:' '}));assert.throws(()=>ui.validate({name:'é'.repeat(41)}));});
test('legacy, saved and default states explained',()=>{assert.match(ui.status(false,true),/preservado/);assert.match(ui.status(true,false),/salvo/);assert.match(ui.status(false,false),/Lucille/);});
