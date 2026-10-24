(function (root, factory) {
  const status = factory();
  root.ordisBackendStatus = status;
  if (typeof module !== 'undefined' && module.exports) module.exports = status;
})(globalThis, function () {
  function footer(status, serviceState, swapping) {
    if (!status) return '';
    const backend = status.backend === 'llama_cpp' ? 'llama.cpp' : 'TabbyAPI';
    if (status.connected) return `${status.model || 'Modelo desconhecido'} · ${backend} · ${status.details?.sleeping ? 'em repouso' : 'conectado'}`;
    if (serviceState === 'iniciando' || swapping) return `${backend} iniciando…`;
    return `${backend} desconectado`;
  }

  function tabbySettingsVisible(backend) {
    return backend !== 'llama_cpp';
  }

  function knownOrUnknown(value) {
    return value == null ? 'desconhecido' : value ? 'sim' : 'não';
  }

  function serviceControl(backend, services) {
    const external = backend === 'llama_cpp' || services?.some((service) => service.nome === 'inferencia' && service.managed === false);
    const externalName = backend === 'llama_cpp' ? 'llama.cpp' : 'TabbyAPI';
    const managed = (services || []).filter((service) =>
      service.managed === undefined ? (external ? service.nome === 'imagens' : service.nome === 'inferencia' || service.nome === 'imagens') : service.managed,
    );
    const active = managed.some((service) => service.situacao !== 'desligado');
    return {
      active,
      enabled: external ? active : true,
      label: external ? (active ? 'Desligar serviços do ORDIS' : `${externalName} externo`) : (active ? 'Desligar' : 'Iniciar'),
      summary: external
        ? `${externalName} externo ${services?.some((service) => service.nome === 'inferencia' && service.situacao === 'pronto') ? 'conectado' : 'desconectado'}${services?.some((service) => service.nome === 'imagens' && service.situacao !== 'desligado') ? ' · ComfyUI ligado' : ''}. O ORDIS não controla o processo ${externalName}.`
        : null,
      action: external ? (active ? 'stop' : 'none') : (active ? 'stop' : 'start'),
      stoppedNotice: external ? `Serviços do ORDIS encerrados. ${externalName} externo continua ativo.` : 'Serviços do ORDIS encerrados. Confira o uso de GPU no painel.',
      busyLabel: external ? 'Encerrando serviços do ORDIS…' : (active ? 'Desligando o ORDIS…' : 'Iniciando o ORDIS…'),
    };
  }

  function capabilityEvidence(protocol, advertised, validated, configured) {
    const parts = [];
    if (protocol != null) parts.push(`Protocolo: ${protocol ? 'disponível' : 'indisponível'}`);
    if (configured != null) parts.push(`Configuração: ${configured ? 'habilitada' : 'desabilitada'}`);
    parts.push(advertised == null ? 'Não anunciado pelo servidor' : advertised ? 'Anunciado pelo servidor' : 'Servidor anuncia ausência de suporte');
    parts.push(validated == null ? 'Ainda não testado com este modelo/template' : validated ? 'Validado neste modelo/template' : 'Falhou no teste deste modelo/template');
    return parts.join('\n');
  }

  function inferenceRows(status) {
    const c = status.capabilities || {};
    const d = status.details || {};
    const metric = d.metrics?.['llamacpp:predicted_tokens_seconds'];
    return [
      ['Conexão', `${status.connected ? 'Conectado' : 'Desconectado'}${d.sleeping ? ' · servidor em repouso' : ''}\n${status.endpoint || 'Endereço desconhecido'}${status.error ? `\n${status.error}` : ''}`],
      ['Modelo', status.model || 'Desconhecido'],
      ['Contexto configurado', status.context_tokens == null ? 'Desconhecido' : `${status.context_tokens.toLocaleString('pt-BR')} tokens\n${d.context_source || 'Fonte: configuração do backend'}`],
      ['Limite padrão de resposta', status.response_max_tokens == null ? 'Desconhecido' : `${status.response_max_tokens.toLocaleString('pt-BR')} tokens\n${d.response_limit_source || 'Configuração do backend'}`],
      ['Streaming', capabilityEvidence(c.streaming, c.streaming_advertised, c.streaming_validated)],
      ['Ferramentas', capabilityEvidence(c.tools_protocol, c.tools_advertised, c.tools_validated, c.tools_configured)],
      ['Raciocínio separado', capabilityEvidence(true, c.reasoning, c.reasoning_validated, c.reasoning_configured)],
      ['Entrada de imagens', capabilityEvidence(true, c.image_input_advertised, null, c.image_input_configured)],
      ['Métricas', c.metrics ? `Endpoint disponível${metric == null ? '\nVelocidade não anunciada' : `\nMédia de geração do servidor: ${metric.toFixed(1)} tokens/s`}` : 'Não anunciadas ou endpoint indisponível'],
      ['Gerenciamento', status.externally_managed ? 'Servidor externo · o ORDIS não controla seu processo' : 'Processo local gerenciado pelo ORDIS'],
      ['Flash Attention', knownOrUnknown(d.flash_attention)],
      ['KV cache K / V', `${d.cache_type_k || 'Desconhecido'} / ${d.cache_type_v || 'Desconhecido'}`],
    ];
  }

  function backendSettings(selected, urls) {
    return { selected, tabby_url: urls.tabbyapi, llama_url: urls.llama_cpp };
  }

  function imageGenerationAllowed(status) {
    return !status || (status.model_available !== false && (status.connected || status.managed));
  }

  return { footer, tabbySettingsVisible, knownOrUnknown, serviceControl, capabilityEvidence, inferenceRows, backendSettings, imageGenerationAllowed };
});
