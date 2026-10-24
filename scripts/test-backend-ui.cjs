const test = require('node:test');
const assert = require('node:assert/strict');
const status = require('../app/ui/backend-status.js');

test('chat footer identifies the connected model and backend', () => {
  assert.equal(
    status.footer({ backend: 'llama_cpp', connected: true, model: 'Lucille' }, 'pronto', false),
    'Lucille · llama.cpp · conectado',
  );
});

test('chat footer reflects disconnection and startup', () => {
  assert.equal(status.footer({ backend: 'llama_cpp', connected: false }, 'desligado', false), 'llama.cpp desconectado');
  assert.equal(status.footer({ backend: 'tabbyapi', connected: false }, 'iniciando', false), 'TabbyAPI iniciando…');
});

test('model settings stay specific to TabbyAPI and missing facts remain unknown', () => {
  assert.equal(status.tabbySettingsVisible('tabbyapi'), true);
  assert.equal(status.tabbySettingsVisible('llama_cpp'), false);
  assert.equal(status.knownOrUnknown(null), 'desconhecido');
  assert.equal(status.knownOrUnknown(false), 'não');
});

test('external llama.cpp is never presented as a service ORDIS can stop', () => {
  const llamaAndComfy = [
    { nome: 'inferencia', situacao: 'pronto' },
    { nome: 'imagens', situacao: 'pronto' },
  ];
  assert.deepEqual(status.serviceControl('llama_cpp', llamaAndComfy), {
    active: true,
    enabled: true,
    label: 'Desligar serviços do ORDIS',
    summary: 'llama.cpp externo conectado · ComfyUI ligado. O ORDIS não controla o processo llama.cpp.',
    action: 'stop',
    stoppedNotice: 'Serviços do ORDIS encerrados. llama.cpp externo continua ativo.',
    busyLabel: 'Encerrando serviços do ORDIS…',
  });

  const llamaOnly = status.serviceControl('llama_cpp', [llamaAndComfy[0], { nome: 'imagens', situacao: 'desligado' }]);
  assert.equal(llamaOnly.enabled, false);
  assert.equal(llamaOnly.action, 'none');
  assert.match(llamaOnly.summary, /continua|não controla/);
});

test('backend settings serialize both custom endpoints with the Rust field names', () => {
  const urls = { tabbyapi: 'http://127.0.0.1:5050', llama_cpp: 'http://127.0.0.1:8090' };
  const saved = status.backendSettings('llama_cpp', urls);
  assert.deepEqual(JSON.parse(JSON.stringify(saved)), {
    selected: 'llama_cpp', tabby_url: urls.tabbyapi, llama_url: urls.llama_cpp,
  });
  assert.equal(status.backendSettings('tabbyapi', urls).llama_url, urls.llama_cpp);
});

test('capability cards keep unknown context, advertised support and validation separate', () => {
  const rows = status.inferenceRows({ backend: 'llama_cpp', connected: true, model: 'Lucille', capabilities: { tools_protocol: true, tools_advertised: true } });
  assert.equal(rows.find(([name]) => name === 'Contexto configurado')[1], 'Desconhecido');
  const tools = rows.find(([name]) => name === 'Ferramentas')[1];
  assert.match(tools, /Anunciado pelo servidor/);
  assert.match(tools, /Ainda não testado/);
  assert.match(status.capabilityEvidence(true, false, false), /Servidor anuncia ausência de suporte/);
  assert.match(status.capabilityEvidence(true, false, false), /Falhou no teste/);
});

test('external ComfyUI cannot enable a managed stop action or generation while disconnected', () => {
  const services = [{ nome: 'inferencia', situacao: 'pronto', managed: false }, { nome: 'imagens', situacao: 'pronto', managed: false }];
  assert.equal(status.serviceControl('llama_cpp', services).action, 'none');
  assert.equal(status.imageGenerationAllowed({ connected: false, managed: false, model_available: null }), false);
  assert.equal(status.imageGenerationAllowed({ connected: true, managed: false, model_available: false }), false);
  assert.equal(status.imageGenerationAllowed({ connected: false, managed: true, model_available: true }), true);
});
