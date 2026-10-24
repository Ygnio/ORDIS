(function (root, factory) {
  const status = factory();
  root.ordisBackendStatus = status;
  if (typeof module !== 'undefined' && module.exports) module.exports = status;
})(globalThis, function () {
  function footer(status, serviceState, swapping) {
    if (!status) return '';
    const backend = status.backend === 'llama_cpp' ? 'llama.cpp' : 'TabbyAPI';
    if (status.connected) return `${status.model || 'Modelo desconhecido'} · ${backend} · conectado`;
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
    const external = backend === 'llama_cpp';
    const managed = (services || []).filter((service) =>
      external ? service.nome === 'imagens' : service.nome === 'inferencia' || service.nome === 'imagens',
    );
    const active = managed.some((service) => service.situacao !== 'desligado');
    return {
      active,
      enabled: external ? active : true,
      label: external ? (active ? 'Desligar serviços do ORDIS' : 'llama.cpp externo') : (active ? 'Desligar' : 'Iniciar'),
      summary: external
        ? `llama.cpp externo ${services?.some((service) => service.nome === 'inferencia' && service.situacao === 'pronto') ? 'conectado' : 'desconectado'}${services?.some((service) => service.nome === 'imagens' && service.situacao !== 'desligado') ? ' · ComfyUI ligado' : ''}. O ORDIS não controla o processo llama.cpp.`
        : null,
      action: external ? (active ? 'stop' : 'none') : (active ? 'stop' : 'start'),
      stoppedNotice: external ? 'Serviços do ORDIS encerrados. llama.cpp externo continua ativo.' : 'ORDIS desligado. GPU livre.',
      busyLabel: external ? 'Encerrando serviços do ORDIS…' : (active ? 'Desligando o ORDIS…' : 'Iniciando o ORDIS…'),
    };
  }

  return { footer, tabbySettingsVisible, knownOrUnknown, serviceControl };
});
