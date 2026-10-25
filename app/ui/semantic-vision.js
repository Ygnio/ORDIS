(function () {
  const { invoke } = window.__TAURI__.core;
  const $ = id => document.getElementById(id);
  const { avisar } = window.ordis;
  let vision = null, semantic = null;
  async function load() {
    try {
      vision = await invoke('vision_action', { action: 'load', args: {} });
      semantic = await invoke('memory_action', { action: 'semantic_get', args: {} });
      for (const k of ['enabled','screenshot']) $('vs-'+k).checked = vision[k];
      for (const k of ['max_mb','max_edge','retention_days','history_images']) $('vs-'+k).value = vision[k];
      $('sv-enabled').checked = semantic.config.enabled;
      for (const k of ['endpoint','model_id','model_path','dimension','query_prefix']) $('sv-'+k).value = semantic.config[k];
      $('sv-status').textContent = JSON.stringify(semantic.index, null, 2) + '\nBusca exata até 10.000 memórias; fusão RRF. Provedor local separado, sem downloads ou inicialização automática.';
      $('capturar-tela').disabled = !vision.enabled || !vision.screenshot;
    } catch (e) { avisar(String(e), true); }
  }
  $('vs-save').addEventListener('click', async () => {
    try {
      const args = { enabled: $('vs-enabled').checked, screenshot: $('vs-screenshot').checked };
      for (const k of ['max_mb','max_edge','retention_days','history_images']) args[k] = Number($('vs-'+k).value);
      await invoke('vision_action', { action: 'save', args }); await load(); avisar('Preferências de visão salvas. EXIF removido na importação.');
    } catch (e) { avisar(String(e), true); }
  });
  $('sv-save').addEventListener('click', async () => {
    try {
      const args = { enabled: $('sv-enabled').checked };
      for (const k of ['endpoint','model_id','model_path','query_prefix']) args[k] = $('sv-'+k).value;
      args.dimension = Number($('sv-dimension').value);
      await invoke('memory_action', { action: 'semantic_save', args }); await load();
    } catch (e) { avisar(String(e), true); }
  });
  $('sv-refresh').addEventListener('click', load);
  $('sv-reindex').addEventListener('click', async () => {
    if (!confirm('Descartar os vetores e reindexar as memórias confirmadas? O texto permanece intacto.')) return;
    try { await invoke('memory_action', { action: 'semantic_reindex', args: {} }); await load(); }
    catch (e) { avisar(String(e), true); }
  });
  $('sv-compare').addEventListener('click', async () => {
    try {
      const r = await invoke('memory_action', { action: 'semantic_compare', args: { query: $('sv-query').value, project_id: $('sv-project').value ? Number($('sv-project').value) : null, category: $('sv-category').value || null } });
      $('sv-results').textContent = JSON.stringify(r, null, 2);
    } catch (e) { avisar(String(e), true); }
  });
  $('vs-diagnostic').addEventListener('click', async () => {
    try { $('vs-status').textContent = JSON.stringify(await invoke('vision_action', { action: 'diagnostic', args: {} }), null, 2); }
    catch (e) { avisar(String(e), true); }
  });
  $('vs-probe').addEventListener('click', async () => {
    $('vs-probe').disabled = true;
    $('vs-status').textContent = 'Enviando apenas formas geométricas sintéticas ao backend local…';
    try { $('vs-status').textContent = JSON.stringify(await invoke('vision_action', { action: 'probe', args: {} }), null, 2); }
    catch (e) { $('vs-status').textContent = String(e); }
    finally { $('vs-probe').disabled = false; }
  });
  $('capturar-tela').addEventListener('click', async () => {
    if (!confirm('Abrir a Ferramenta de Captura? Escolha apenas a região autorizada. Depois cole no chat e revise a miniatura antes de enviar.')) return;
    try { const r = await invoke('vision_action', { action: 'capture', args: {} }); avisar(r.message); }
    catch (e) { avisar(String(e), true); }
  });
  window.LucilleVision = { getConfig: () => vision, load };
  load();
})();
