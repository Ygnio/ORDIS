(function () {
  'use strict';
  const operations = ['rust_check', 'rust_test', 'rust_fmt', 'rust_clippy', 'node_version', 'node_syntax', 'node_test', 'python_syntax', 'python_test'];
  const fields = { rust_bin: 'Pasta dos binários Rust nativos', gnu_bin: 'Pasta MinGW-w64', msvc_bin: 'Pasta MSVC (opcional)', node: 'node.exe instalado', python: 'python.exe instalado', target: 'Target Rust instalado', seconds: 'Tempo total (s)', subprocess_seconds: 'Tempo por subprocesso (s)', memory_mib: 'Memória (MiB)', cpu_percent: 'CPU máxima (%)', processes: 'Processos simultâneos', storage_mib: 'Armazenamento de saída (MiB)' };
  const labels = { pending: 'Pendente', awaiting_approval: 'Aguardando aprovação', running: 'Executando', completed: 'Concluído', failed: 'Falhou', cancelled: 'Cancelado', timed_out: 'Tempo esgotado', interrupted: 'Interrompido ao reiniciar' };
  function label(state) { return labels[state] || state; }
  function active(state) { return ['pending', 'awaiting_approval', 'running'].includes(state); }
  if (typeof module !== 'undefined') { module.exports = { operations, active, label }; return; }
  const $ = id => document.getElementById(`jobs-${id}`);
  const { invoke } = window.__TAURI__.core;
  const action = (name, args = {}) => invoke('developer_action', { action: name, args });
  const report = e => { $('state').textContent = String(e); };
  const node = (tag, text) => { const n = document.createElement(tag); n.textContent = text; return n; };
  for (const [key, text] of Object.entries(fields)) {
    const label = node('label', text); const input = document.createElement('input'); input.id = `jobs-${key}`;
    if (['seconds', 'subprocess_seconds', 'memory_mib', 'cpu_percent', 'processes', 'storage_mib'].includes(key)) input.type = 'number';
    label.append(input); $('settings').append(label);
  }
  for (const op of operations) $('operation').add(new Option(op, op));
  async function projects() {
    const list = await action('job_projects'); const old = $('project').value; $('project').replaceChildren();
    for (const p of list) $('project').add(new Option(p.project, p.project));
    if ([...$('project').options].some(o => o.value === old)) $('project').value = old;
  }
  async function history() {
    const list = await action('job_history'); $('history').replaceChildren();
    for (const job of list.jobs) {
      const row = node('details', ''); row.append(node('summary', `${job.operation} · ${job.project} · ${label(job.state)}`), node('pre', JSON.stringify(job, null, 2)));
      const logs = node('button', 'Consultar resultado e logs'); const output = node('pre', '');
      logs.addEventListener('click', async () => { try { output.textContent = JSON.stringify(await action('job_get', { id: job.id }), null, 2); } catch (e) { report(e); } });
      row.append(logs, output);
      if (active(job.state)) {
        const stop = node('button', 'Cancelar job'); stop.addEventListener('click', async () => { try { await action('job_cancel', { id: job.id }); stop.disabled = true; report('Cancelamento solicitado ao executor próprio.'); } catch (e) { report(e); } }); row.append(stop);
      }
      $('history').append(row);
    }
    if (!list.jobs.length) $('history').append(node('p', 'Nenhum job registrado. Nada é executado ao abrir esta página.'));
  }
  async function load() {
    const cfg = await action('load'); $('enabled').checked = cfg.jobs.enabled;
    for (const key of Object.keys(fields)) $(key).value = cfg.jobs[key];
    await history(); if (cfg.permissions.enabled) await projects();
  }
  $('save').addEventListener('click', async () => {
    try {
      const cfg = await action('load'); cfg.jobs.enabled = $('enabled').checked;
      for (const key of Object.keys(fields)) cfg.jobs[key] = $(key).type === 'number' ? Number($(key).value) : $(key).value.trim();
      await action('save', cfg); report('Ferramentas verificadas e hashes fixados. Nenhuma instalação realizada.'); await load();
    } catch (e) { report(e); }
  });
  $('fixture').addEventListener('click', async () => {
    try { const r = await action('job_fixture', { language: $('language').value, case: $('case').value }); await projects(); $('project').value = r.project; report(`Projeto criado: ${r.project}`); } catch (e) { report(e); }
  });
  $('submit').addEventListener('click', async () => {
    $('submit').disabled = true;
    try {
      report('Preparando plano. A execução exige aprovação no cartão nativo.');
      const r = await invoke('developer_job_submit', { args: { project: $('project').value, operation: $('operation').value, reason: $('reason').value } });
      report(`Job encerrado: ${label(r.state)}. Consulte o resultado no histórico.`);
    } catch (e) { report(e); }
    finally { $('submit').disabled = false; await history().catch(report); }
  });
  $('refresh').addEventListener('click', () => history().catch(report));
  $('panel').addEventListener('toggle', () => { if ($('panel').open) load().catch(report); });
})();
