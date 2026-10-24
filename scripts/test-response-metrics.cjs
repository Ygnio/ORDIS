const { test } = require('node:test');
const assert = require('node:assert/strict');
const metrics = require('../app/ui/response-metrics.js');
test('real response counts and times remain separate', () => {
  const text = metrics.text({model:'Lucille',state:'completed',completion_tokens:118,total_ms:3300,ttft_ms:1100,generation_ms:2200,generation_time_source:'backend',tokens_per_second:53.636,rate_source:'backend_timings'});
  assert.match(text,/118 tokens gerados/); assert.match(text,/3,3 s total/); assert.match(text,/TTFT 1,1 s/); assert.match(text,/2,2 s geração/); assert.match(text,/53,64 tok\/s/); assert.ok(!text.includes('≈'));
});
test('missing statistics are visibly estimates', () => {
  const text = metrics.text({state:'streaming',estimated_tokens:12,total_ms:2000,tokens_per_second:6,rate_source:'local_estimate'});
  assert.match(text,/Gerando/); assert.match(text,/≈12 tokens \(estimativa local\)/); assert.match(text,/≈6 tok\/s \(estimativa\)/);
});
test('cancelled response never displays a local token estimate', () => {
  const text=metrics.text({state:'cancelled',estimated_tokens:12,total_ms:1000}); assert.match(text,/Cancelada/);assert.match(text,/tokens indisponíveis/);assert.ok(!text.includes('12 tokens'));
});
test('old history and invalid metrics do not invent numbers', () => {
  assert.equal(metrics.text(null),'');assert.equal(metrics.number(NaN),null);assert.equal(metrics.number(Infinity),null);assert.equal(metrics.number(-1),null);
});
test('stored metrics round trip without recalculation on restart', () => {
  const m={model:'Lucille',state:'completed',completion_tokens:4,total_ms:700};assert.equal(metrics.text(JSON.parse(JSON.stringify(m))),metrics.text(m));
});
