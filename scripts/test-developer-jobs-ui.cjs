const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const jobs = require('../app/ui/developer-jobs.js');
test('only live jobs offer cancellation; restart cannot turn interruption into success', () => {
  for (const state of ['pending', 'awaiting_approval', 'running']) assert.equal(jobs.active(state), true);
  for (const state of ['completed', 'failed', 'cancelled', 'timed_out', 'interrupted', 'unknown']) assert.equal(jobs.active(state), false);
});
test('job operations exclude installers and arbitrary shells', () => {
  assert.equal(jobs.operations.length, 9);
  assert.ok(jobs.operations.includes('rust_fmt'));
  for (const name of ['npm_install', 'shell', 'cargo_install', 'rustup']) assert.ok(!jobs.operations.includes(name));
});
test('job display uses inert text and native shared approvals', () => {
  const s = fs.readFileSync('app/ui/developer-jobs.js', 'utf8');
  assert.ok(!s.includes('innerHTML'));
  assert.match(s, /developer_job_submit/);
  assert.match(s, /job_cancel/);
  assert.match(s, /job_get/);
  assert.ok(!s.includes('developer_decide'));
});
