(function () {
  const { invoke } = window.__TAURI__.core;
  const { el, avisar } = window.ordis;
  const $ = (id) => document.getElementById(id);
  const known = (value) => value == null || value === '' ? 'Desconhecido' : String(value);
  const bytes = (value) => typeof value === 'number' && value > 0 ? `${(value / 1024 ** 3).toFixed(1)} GiB` : 'Desconhecido';
  const boolean = (value) => value == null ? 'Desconhecido' : value ? 'Disponível' : 'Não disponível';
  let preferences = null;
  let diagnosticBusy = false;
  let comfyBusy = false;

  function facts(target, rows) {
    $(target).replaceChildren(...rows.map(([label, value]) => {
      const card = el('div', 'capability-item');
      const list = el('dl');
      list.append(el('dt', '', label), el('dd', '', String(value)));
      card.append(list);
      return card;
    }));
  }

  async function diagnostic() {
    if (diagnosticBusy) return;
    diagnosticBusy = true;
    $('diagnostico-atualizar').disabled = true;
    $('diagnostico-estado').textContent = 'Consultando hardware e serviços…';
    try {
      const data = await invoke('hardware_diagnostics');
      const cpu = data.cpu || {};
      const ins = cpu.instructions || {};
      const backend = data.backend || {};
      const comfy = data.comfy || {};
      facts('diagnostico-dados', [
        ['Processador', `${known(cpu.name)}\n${known(cpu.architecture)} · ${known(cpu.physical_cores)} núcleos / ${known(cpu.threads)} threads`],
        ['Instruções utilizáveis', `SSE4.2: ${boolean(ins.sse42)}\nAVX: ${boolean(ins.avx)}\nAVX2: ${boolean(ins.avx2)}\nFMA: ${boolean(ins.fma)}`],
        ['RAM', `Total: ${bytes(data.ram?.total_bytes)}\nDisponível: ${bytes(data.ram?.available_bytes)}`],
        ...((data.gpus || []).length ? data.gpus.map((gpu) => [gpu.name, `VRAM total: ${(gpu.total_mib / 1024).toFixed(1)} GiB\nEm uso: ${(gpu.used_mib / 1024).toFixed(1)} GiB\nLivre: ${(gpu.free_mib / 1024).toFixed(1)} GiB\nDriver: ${gpu.driver}`]) : [['GPU', data.gpu_detection]]),
        ['Windows / WebView2', `${known(data.windows)}\nWebView2: ${known(data.webview2)}`],
        ['Inferência ativa', `${backend.backend === 'llama_cpp' ? 'llama.cpp' : 'TabbyAPI'} · ${backend.connected ? 'conectado' : 'desconectado'}\n${known(backend.model)}\n${known(backend.endpoint)}`],
        ['ComfyUI', `${comfy.connected ? 'Conectado' : 'Desconectado'} · ${comfy.managed ? 'instalação gerenciada' : 'serviço externo'}\n${known(comfy.endpoint)}\nCheckpoints: ${comfy.checkpoints == null ? 'Desconhecidos' : comfy.checkpoints.length}\nModelo/fluxo selecionado: ${boolean(comfy.model_available)}`],
        ['Proxmox', `${data.proxmox?.configured ? 'Configurado' : 'Não configurado'}\nCA: ${boolean(data.proxmox?.ca_available)}\n${data.proxmox?.connection || ''}`],
        ['Modo Desenvolvedor', 'Planejamento · permissões desativadas · sem execução de comandos'],
      ]);
      $('diagnostico-avisos').replaceChildren(...(data.warnings || []).map((text) => el('p', '', text)));
      $('diagnostico-estado').textContent = `Atualizado às ${new Date().toLocaleTimeString('pt-BR')}. ${data.privacy}`;
      window.dispatchEvent(new CustomEvent('backend-status', { detail: backend }));
      publishComfy(comfy);
    } catch (error) {
      $('diagnostico-estado').textContent = `Diagnóstico indisponível: ${error}`;
    } finally {
      diagnosticBusy = false;
      $('diagnostico-atualizar').disabled = false;
    }
  }

  function publishComfy(status) {
    const count = status.checkpoints == null ? 'Checkpoints desconhecidos' : status.checkpoints.length ? `${status.checkpoints.length} checkpoint(s) encontrado(s)` : 'Nenhum checkpoint encontrado';
    $('comfy-estado').textContent = `${status.connected ? 'Conectado' : 'Desconectado'} · ${status.managed ? 'gerenciado pelo ORDIS' : 'externo'} · ${count}${status.management_requested && status.connected && !status.process_owned ? ' · Processo externo detectado e preservado; não foi iniciado pelo ORDIS' : ''}${status.error ? ` · ${status.error}` : ''}`;
    window.dispatchEvent(new CustomEvent('comfy-status', { detail: status }));
  }

  async function refreshComfy() {
    if (comfyBusy) return;
    comfyBusy = true;
    $('comfy-testar').disabled = true;
    $('comfy-estado').textContent = 'Consultando serviço salvo…';
    try { publishComfy(await invoke('comfy_status')); }
    catch (error) { $('comfy-estado').textContent = `Consulta indisponível: ${error}`; }
    finally { comfyBusy = false; $('comfy-testar').disabled = false; }
  }

  async function load() {
    preferences = await invoke('local_settings');
    $('comfy-url').value = preferences.comfy_url;
    $('comfy-manage').checked = preferences.manage_comfy;
    const d = preferences.developer;
    for (const name of ['workspace', 'shell', 'git', 'python', 'node', 'cargo']) $(`dev-${name}`).value = d[name];
    $('dev-repositories').value = d.repositories.join('\n');
    $('dev-timeout').value = d.timeout_seconds;
    $('dev-long-task').value = d.long_task_minutes;
    $('dev-logs').checked = d.local_logs;
    $('dev-estado').textContent = 'Preferências locais; nenhuma permissão ativa.';
  }

  $('diagnostico-atualizar').addEventListener('click', diagnostic);
  $('comfy-testar').addEventListener('click', refreshComfy);
  $('comfy-url').addEventListener('input', () => {
    if ($('comfy-url').value.trim().replace(/\/$/, '') !== 'http://127.0.0.1:8188') $('comfy-manage').checked = false;
  });
  $('comfy-salvar').addEventListener('click', async () => {
    const button = $('comfy-salvar');
    button.disabled = true;
    try {
      const current = await invoke('local_settings');
      const value = { ...current, comfy_url: $('comfy-url').value, manage_comfy: $('comfy-manage').checked };
      await invoke('local_settings_save', { value });
      preferences = value;
      await refreshComfy();
      window.dispatchEvent(new CustomEvent('comfy-settings-saved'));
      avisar('Endereço do ComfyUI salvo.');
    } catch (error) { avisar(String(error), true); }
    finally { button.disabled = false; }
  });
  $('dev-salvar').addEventListener('click', async () => {
    const button = $('dev-salvar');
    button.disabled = true;
    try {
      const current = await invoke('local_settings');
      const d = { ...current.developer };
      for (const name of ['workspace', 'shell', 'git', 'python', 'node', 'cargo']) d[name] = $(`dev-${name}`).value.trim();
      d.repositories = $('dev-repositories').value.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
      d.timeout_seconds = Number($('dev-timeout').value);
      d.long_task_minutes = Number($('dev-long-task').value);
      d.local_logs = $('dev-logs').checked;
      await invoke('local_settings_save', { value: { ...current, developer: d } });
      $('dev-estado').textContent = 'Planejamento salvo localmente. Permissões continuam desativadas.';
      avisar('Preferências de planejamento salvas.');
    } catch (error) { avisar(String(error), true); }
    finally { button.disabled = false; }
  });
  window.addEventListener('aba', (event) => {
    if (event.detail === 'config') {
      load().catch((error) => avisar(String(error), true));
      refreshComfy();
    } else if (event.detail === 'imagens') refreshComfy();
  });
})();
