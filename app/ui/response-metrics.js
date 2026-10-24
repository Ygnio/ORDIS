const ResponseMetrics = {
  number(value, digits = 1) { return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value.toLocaleString('pt-BR', {maximumFractionDigits:digits}) : null; },
  text(m) {
    if (!m) return '';
    const parts = [m.model || 'Assistente'];
    if (m.state === 'cancelled') parts.push('Cancelada');
    else if (m.state === 'streaming') parts.push('Gerando…');
    else if (m.state === 'interrupted') parts.push('Interrompida');
    if (this.number(m.completion_tokens,0) !== null) parts.push(`${this.number(m.completion_tokens,0)} tokens gerados`);
    else if (m.state !== 'cancelled' && this.number(m.estimated_tokens,0) !== null) parts.push(`≈${this.number(m.estimated_tokens,0)} tokens (estimativa local)`);
    else parts.push('tokens indisponíveis');
    if (this.number(m.total_ms) !== null) parts.push(`${this.number(m.total_ms/1000)} s total`);
    if (this.number(m.tokens_per_second,2) !== null) parts.push(`${m.rate_source === 'local_estimate' ? '≈' : ''}${this.number(m.tokens_per_second,2)} tok/s${m.rate_source === 'local_estimate' ? ' (estimativa)' : ''}`);
    if (this.number(m.ttft_ms) !== null) parts.push(`TTFT ${this.number(m.ttft_ms/1000)} s`);
    if (this.number(m.generation_ms) !== null) parts.push(`${this.number(m.generation_ms/1000)} s ${m.generation_time_source === 'backend' ? 'geração (backend)' : 'após primeiro token (local)'}`);
    return parts.join(' · ');
  },
  enabled() { try { return localStorage.getItem('ordis.response-metrics.show') !== 'false'; } catch (_) { return true; } },
  setEnabled(value) { try { localStorage.setItem('ordis.response-metrics.show', String(!!value)); } catch (_) { /* display still changes this session */ } document.documentElement.classList.toggle('metrics-hidden', !value); },
  mount(container, data) {
    const div = document.createElement('div'); div.className = 'response-metrics'; this.update(div,data); container.append(div); return div;
  },
  update(div, data) {
    div.textContent = this.text(data);
    div.title = `Tokens: ${data.token_source || 'indisponíveis'}. Taxa: ${data.rate_source || 'indisponível'}. Total/TTFT medidos no cliente desde o envio HTTP, incluindo espera e rede; TTFT considera também raciocínio e ferramentas. Tempos do backend são separados. As contagens do backend podem incluir raciocínio e tool calling. Estimativa local: bytes de saída/4, sem tokenizer. Não inclui tokens do prompt.`;
  },
};
if (typeof module !== 'undefined') module.exports = ResponseMetrics;
if (typeof window !== 'undefined') { window.ordisResponseMetrics = ResponseMetrics; ResponseMetrics.setEnabled(ResponseMetrics.enabled()); }
