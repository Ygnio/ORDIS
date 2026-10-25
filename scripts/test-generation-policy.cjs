const { test } = require('node:test');
const assert = require('node:assert/strict');
const status = require('../app/ui/generation-policy.js');
test('normal completion and legacy history never get an error notice',()=>{assert.equal(status.text(null),'');assert.equal(status.text({state:'completed'}),'');});
test('length preserves the distinction between partial final and no final',()=>{
  assert.match(status.text({state:'length',has_final_content:true}),/conteúdo acima está incompleto/);
  assert.match(status.text({state:'length',has_final_content:false}),/Não houve conteúdo final/);
});
test('voluntary cancellation is distinct from timeout and connection failure',()=>{
  assert.match(status.text({state:'cancelled'}),/cancelada por você/);
  assert.match(status.text({state:'timeout'}),/tempo total/);
  assert.match(status.text({state:'first_token_timeout'}),/primeiro token/);
  assert.match(status.text({state:'idle_timeout'}),/inatividade/);
  assert.match(status.text({state:'transport_error'}),/conexão/);
});
test('protocol failure and repetition warning never claim tool execution or automatic cancellation',()=>{
  assert.match(status.text({state:'protocol_error'}),/Nenhuma chamada deste lote foi executada/);
  assert.match(status.text({state:'streaming',repetition_warning:true}),/nenhum cancelamento automático/);
});
test('notices use textContent and remain independent of hidden metrics',()=>{
  const div={setAttribute(){}};const container={querySelector(){return null;},append(v){this.div=v;}};
  global.document={createElement(){return div;}};
  status.show(container,{state:'length',has_final_content:true});assert.match(div.textContent,/incompleto/);assert.equal(div.className.includes('response-metrics'),false);
  delete global.document;
});
