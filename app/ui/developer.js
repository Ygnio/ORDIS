(function () {
  'use strict';
  const states = { awaiting_approval: 'Aguardando aprovação', running: 'Executando', output: 'Saída', completed: 'Concluído', failed: 'Falhou', cancelled: 'Cancelado', refused: 'Recusado', timeout: 'Tempo esgotado', interrupted: 'Interrompido ao reiniciar', progress: 'Executando', preparing: 'Verificando fontes e ferramentas', timed_out: 'Tempo esgotado' };
  function label(state) { return states[state] || state; }
  function decision(value) { return value === 'allow'; }
  function approvalRemaining(expires, now = Date.now()) {
    return Number.isFinite(expires) && Number.isFinite(now) ? Math.max(0, Math.ceil((expires - now) / 1000)) : 0;
  }
  if (typeof module !== 'undefined') { module.exports = { label, decision, approvalRemaining }; return; }
  const $ = (id) => document.getElementById(id);
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  const { avisar } = window.ordis;
  const cards = new Map();
  let config;
  const action = (name, args = {}) => invoke('developer_action', { action: name, args });
  function node(tag, text) { const n = document.createElement(tag); n.textContent = text; return n; }
  async function load() {
    config = await action('load');
    const p = config.permissions;
    for (const [id, key] of [['enabled', 'enabled'], ['read', 'allow_read'], ['write', 'allow_write'], ['execute', 'allow_execute']]) $(`dev-${id}`).checked = p[key];
    for (const key of ['workspace', 'python', 'shell']) $(`dev-${key}`).value = p[key];
    $('dev-timeout').value = p.timeout_seconds;
    $('dev-approval-timeout').value = config.approval_timeout_seconds;
    for (const [id, key] of [['calls', 'max_calls'], ['file-limit', 'max_file_bytes'], ['read-limit', 'max_read_bytes'], ['output-limit', 'max_output_bytes']]) $(`dev-${id}`).value = config[key];
    $('dev-estado').textContent = p.enabled ? 'Ativo neste workspace; escritas e execuções aguardam aprovação individual.' : 'Desativado. Nenhuma ferramenta de desenvolvimento é enviada ao modelo.';
  }
  async function history() {
    const h = await action('history');
    const target = $('dev-history'); target.replaceChildren();
    target.append(node('p', 'Tarefas locais. Abrir uma conversa não reinicia operações: envie um novo pedido para continuar.'));
    for (const task of h.tasks) {
      const row = node('div', `${task.title} · ${label(task.state)} `);
      const b = node('button', 'Abrir conversa');
      if (!task.id.startsWith('job-')) { b.addEventListener('click', () => window.ordisOpenDeveloperTask(task.id)); row.append(b); }
      target.append(row);
    }
    for (const op of h.operations) {
      const d = document.createElement('details');
      d.append(node('summary', `${op.tool} · ${label(op.state)}`), node('pre', JSON.stringify(op, null, 2))); target.append(d);
    }
    if (!h.tasks.length && !h.operations.length) target.append(node('p', 'Nenhuma operação registrada.'));
  }
  $('dev-salvar').addEventListener('click', async () => {
    try {
      if (!config) await load();
      const p = { ...config.permissions };
      for (const [id, key] of [['enabled', 'enabled'], ['read', 'allow_read'], ['write', 'allow_write'], ['execute', 'allow_execute']]) p[key] = $(`dev-${id}`).checked;
      for (const key of ['workspace', 'python', 'shell']) p[key] = $(`dev-${key}`).value.trim();
      p.timeout_seconds = Number($('dev-timeout').value);
      const next = { ...config, permissions: p };
      next.approval_timeout_seconds = Number($('dev-approval-timeout').value);
      for (const [id, key] of [['calls', 'max_calls'], ['file-limit', 'max_file_bytes'], ['read-limit', 'max_read_bytes'], ['output-limit', 'max_output_bytes']]) next[key] = Number($(`dev-${id}`).value);
      await action('save', next); await load(); avisar('Permissões salvas apenas neste computador.');
    } catch (error) { avisar(String(error), true); }
  });
  $('dev-history-refresh').addEventListener('click', () => history().catch((e) => avisar(String(e), true)));
  window.addEventListener('aba', ({ detail }) => { if (detail === 'config') Promise.all([load(), history()]).catch((e) => avisar(String(e), true)); });
  listen('agente', ({ payload }) => {
    if (payload.tipo !== 'developer') return;
    const d = payload.dados;
    const panel = $('dev-approval-panel'); panel.hidden = false;
    let card = cards.get(d.id);
    if (!card) {
      const root = document.createElement('section'); root.className = 'cartao';
      const title = node('h3', 'Operação local'); const state = node('p', ''); const deadline = node('p', '');
      const details = document.createElement('details'); details.open = d.state === 'awaiting_approval';
      const preview = node('pre', JSON.stringify(d.proposal || {}, null, 2)); const output = node('pre', '');
      details.append(node('summary', 'Arquivos, diff, comando e saída'), preview, output);
      const buttons = document.createElement('div'); root.append(title, state, deadline, details, buttons);
      panel.append(root); card = { root, title, state, deadline, preview, output, buttons, timer: null }; cards.set(d.id, card);
    }
    card.state.textContent = label(d.state);
    if (d.proposal?.tool === 'developer_job') card.job = true;
    if (d.proposal) { card.title.textContent = d.proposal.tool; card.preview.textContent = JSON.stringify(d.proposal, null, 2); }
    if (d.elapsed_ms != null) card.state.textContent = `${label(d.state)} · ${(d.elapsed_ms / 1000).toFixed(1)} s`;
    if (d.text && card.output.textContent.length < 256000) card.output.textContent += d.text;
    if (d.result) card.output.textContent = JSON.stringify(d.result, null, 2);
    if (d.state === 'awaiting_approval') {
      clearInterval(card.timer);
      card.buttons.replaceChildren();
      for (const [text, value] of [['Permitir esta operação', 'allow'], ['Recusar', 'deny'], ['Cancelar tarefa', 'cancel']]) {
        const b = node('button', text); b.addEventListener('click', async () => {
          for (const button of card.buttons.children) button.disabled = true;
          try {
            if (value === 'cancel') await (card.job ? action('job_cancel', { id: d.id }) : invoke('interromper'));
            else if (approvalRemaining(d.approval_expires_at_ms) > 0) await invoke('developer_decide', { id: d.id, allow: decision(value) });
            else avisar('Aprovação expirada. Solicite uma nova operação.', true);
          }
          catch (error) { avisar(String(error), true); }
        }); card.buttons.append(b);
      }
      const tick = () => {
        const remaining = approvalRemaining(d.approval_expires_at_ms);
        card.deadline.textContent = remaining > 0 ? `Prazo de aprovação: ${remaining} s restantes.` : 'Prazo de aprovação esgotado.';
        if (!remaining) { for (const b of card.buttons.children) b.disabled = true; clearInterval(card.timer); }
      };
      card.timer = setInterval(tick, 250); tick();
    } else if (!['output', 'progress', 'preparing'].includes(d.state)) {
      clearInterval(card.timer); card.timer = null;
      card.deadline.textContent = d.state === 'timeout' ? 'Prazo esgotado; a operação não foi autorizada.' : '';
      card.buttons.replaceChildren();
      const b = node('button', d.state === 'running' ? 'Cancelar tarefa' : 'Fechar cartão');
      b.addEventListener('click', async () => { if (d.state === 'running') await (card.job ? action('job_cancel', { id: d.id }) : invoke('interromper')); else { card.root.remove(); cards.delete(d.id); panel.hidden = cards.size === 0; } }); card.buttons.append(b);
    }
  });
})();
