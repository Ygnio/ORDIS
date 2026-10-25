/* Termination states are application notices, never assistant answer text. */
const GenerationStatus = {
  text(m) {
    if (!m) return '';
    const states = {
      length: 'A geração atingiu o limite configurado antes de concluir.',
      timeout: 'A solicitação foi interrompida por exceder o tempo total permitido.',
      first_token_timeout: 'A solicitação excedeu o prazo para o primeiro token.',
      idle_timeout: 'A solicitação foi interrompida por inatividade da geração.',
      cancelled: 'Geração cancelada por você.',
      transport_error: 'A conexão foi interrompida antes da conclusão.',
      backend_error: 'O backend recusou ou falhou ao processar a solicitação.',
      protocol_error: 'A resposta terminou com protocolo ou chamada de ferramenta inválida. Nenhuma chamada deste lote foi executada.',
      no_final: 'A Lucille não produziu conteúdo final nesta resposta.',
    };
    let text = states[m.state] || '';
    if (text && m.has_final_content) text += ' O conteúdo acima está incompleto.';
    else if (text && m.state !== 'cancelled') text += ' Não houve conteúdo final disponível.';
    if (m.repetition_warning) text += `${text ? ' ' : ''}Possível repetição observada no raciocínio; nenhum cancelamento automático por repetição foi aplicado.`;
    return text;
  },
  show(container, metrics) {
    const text = this.text(metrics);
    if (!text || !container) return;
    let div = container.querySelector('.generation-notice');
    if (!div) { div = document.createElement('p'); div.className = 'generation-notice sutil pequeno'; div.setAttribute('role', 'status'); container.append(div); }
    div.textContent = text;
  },
};
if (typeof module !== 'undefined') module.exports = GenerationStatus;
if (typeof window !== 'undefined') window.lucilleGenerationStatus = GenerationStatus;

if (typeof window !== 'undefined' && window.__TAURI__) (function () {
  const { invoke } = window.__TAURI__.core;
  const $ = id => document.getElementById(id);
  const fields = ['max_tokens', 'total_seconds', 'first_token_seconds', 'idle_seconds'];
  const names = { conversation: 'Conversa comum', vision: 'Perguntas visuais', long: 'Programação e respostas longas', agent: 'Ferramentas e agentes' };
  let config = null;
  async function load() {
    try {
      config = await invoke('generation_config', { action: 'load', value: null });
      $('generation-profile').value = config.selected_profile;
      $('generation-observe').checked = config.observe_repetition;
      const rows = $('generation-profiles'); rows.replaceChildren();
      for (const [profile, label] of Object.entries(names)) {
        const group = document.createElement('fieldset'); const title = document.createElement('legend'); title.textContent = label; group.append(title);
        for (const [i, key] of fields.entries()) {
          const label = document.createElement('label'); label.textContent = ['Tokens de geração', 'Prazo total (s)', 'Primeiro token (s)', 'Inatividade (s)'][i];
          const input = document.createElement('input'); input.type = 'number'; input.id = `generation-${profile}-${key}`;
          input.min = key === 'max_tokens' ? '256' : key === 'total_seconds' ? '10' : '5'; input.max = key === 'max_tokens' ? '65536' : '3600'; input.step = '1'; input.value = config[profile][key];
          label.append(input); group.append(label);
        }
        rows.append(group);
      }
      $('generation-state').textContent = 'Limites do cliente carregados. O servidor não foi reconfigurado.';
    } catch (_) { $('generation-state').textContent = 'Não foi possível carregar os limites. O arquivo existente foi preservado.'; }
  }
  $('generation-save').addEventListener('click', async () => {
    if (!config) return;
    const next = JSON.parse(JSON.stringify(config)); next.selected_profile = $('generation-profile').value; next.observe_repetition = $('generation-observe').checked;
    for (const profile of Object.keys(names)) for (const key of fields) next[profile][key] = Number($(`generation-${profile}-${key}`).value);
    const button = $('generation-save'); button.disabled = true;
    try { await invoke('generation_config', { action: 'save', value: next }); config = next; $('generation-state').textContent = 'Limites locais salvos para as próximas requisições. O servidor continua intacto.'; }
    catch (_) { $('generation-state').textContent = 'Não foi possível salvar: confira os intervalos e mantenha primeiro token/inatividade até o prazo total.'; }
    finally { button.disabled = false; }
  });
  window.addEventListener('aba', e => { if (e.detail === 'config') load(); });
})();
