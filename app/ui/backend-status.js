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

  return { footer, tabbySettingsVisible, knownOrUnknown };
});
