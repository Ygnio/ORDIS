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
