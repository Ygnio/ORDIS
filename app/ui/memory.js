/* Private memory UI. All stored content uses textContent, never HTML or Markdown. */
const MemoryUI = {
  nullableId(value) { return value === '' ? null : Number(value); },
  options(retrieval, extraction, budget, project) {
    const token_budget = Number(budget);
    if (!Number.isInteger(token_budget) || token_budget < 128 || token_budget > 4096) throw new Error('Orçamento: 128 a 4096 tokens estimados.');
    return { retrieval: !!retrieval, extraction: !!extraction, token_budget, project_id: this.nullableId(project) };
  },
  restoreArgs(name, digest, confirmed) {
    if (!confirmed || !name || !digest) throw new Error('Restauração não confirmada.');
    return { name, digest, confirmed: true };
  },
};
if (typeof module !== 'undefined') module.exports = MemoryUI;
if (typeof window !== 'undefined') (() => {
  const get = id => document.getElementById(`mem-${id}`);
  const invoke = window.__TAURI__.core.invoke;
  const call = (action, args = {}) => invoke('memory_action', { action, args });
  const labels = { fact:'Fato', preference:'Preferência', hardware:'Hardware', project:'Projeto', decision:'Decisão', solution:'Solução', goal:'Objetivo', task:'Tarefa interrompida/pendente', constraint:'Restrição', summary:'Resumo extrativo', hypothesis:'Hipótese', candidate:'Candidato', active:'Ativa (aprovada)', conflict:'Contradição', obsolete:'Obsoleta' };
  const projectLabels = { name:'Nome', goal:'Objetivo', repository:'Repositório (referência)', branch:'Branch (referência)', files:'Arquivos relevantes', architecture:'Decisões de arquitetura', issues:'Problemas encontrados', solutions:'Soluções aplicadas', tests:'Testes executados', next_tasks:'Próximas tarefas', execution_state:'Estado de execução/retomada' };
  let loaded = false, projects = [], editing = null, projectId = null;
  function message(text) { get('status').textContent = text; }
  function bind(id, fn) { get(id).addEventListener('click', async () => { try { await fn(); } catch (e) { message(String(e)); } }); }
  function select(node, items, empty = null) {
    const old = node.value; node.replaceChildren();
    if (empty !== null) node.add(new Option(empty, ''));
    for (const [value, text] of items) node.add(new Option(text, String(value)));
    if ([...node.options].some(o => o.value === old)) node.value = old;
  }
  async function refresh() {
    const data = await call('load'); projects = data.projects;
    for (const id of ['active-project','filter-project','project','project-edit']) select(get(id), projects.map(p => [p.id,p.name]), id === 'filter-project' ? 'Todos os projetos' : 'Sem projeto');
    for (const id of ['kind','filter-kind']) select(get(id), data.kinds.map(k => [k, labels[k]]), id === 'filter-kind' ? 'Todas as categorias' : null);
    for (const id of ['state','filter-state']) select(get(id), data.states.map(k => [k, labels[k]]), id === 'filter-state' ? 'Todos os estados' : null);
    const o = data.options; get('retrieval').checked = o.retrieval; get('extraction').checked = o.extraction; get('budget').value = o.token_budget; get('active-project').value = o.project_id ?? '';
    const s = data.stats; get('stats').textContent = `Banco: ${s.path}\nSchema ${s.schema} · ${s.memories} memórias · ${s.messages} mensagens novas · ${s.projects} projetos\nSQLite: ${s.database_bytes} bytes + WAL ${s.wal_bytes} bytes · cache configurado: ${s.cache_kib} KiB · auditorias: ${s.retrievals}`;
    loaded = true; await search();
  }
  function editor(m = null) {
    editing = m?.id ?? null; get('editor-title').textContent = editing ? `Editar memória #${editing}` : 'Criar memória';
    get('kind').value = m?.kind ?? 'fact'; get('project').value = m?.project_id ?? ''; get('state').value = m?.state ?? 'active'; get('importance').value = m?.importance ?? 3; get('key').value = m?.fact_key ?? ''; get('content').value = m?.content ?? ''; get('form').hidden = false; get('content').focus();
  }
  async function search() {
    const rows = await call('list', { query:get('query').value, project_id:MemoryUI.nullableId(get('filter-project').value), kind:get('filter-kind').value, state:get('filter-state').value });
    const list = get('list'); list.replaceChildren();
    if (!rows.length) { const p = document.createElement('p'); p.textContent = 'Nenhuma memória encontrada.'; list.append(p); }
    for (const m of rows) {
      const article = document.createElement('article'), title = document.createElement('h3'), content = document.createElement('pre'), source = document.createElement('p');
      title.textContent = `#${m.id} · ${labels[m.kind]} · ${labels[m.state]} · ${projects.find(p => p.id === m.project_id)?.name ?? 'Geral'}`;
      content.textContent = m.content;
      source.textContent = `Origem: ${m.provenance} · criado ${new Date(m.created_at * 1000).toLocaleString()} · atualizado ${new Date(m.updated_at * 1000).toLocaleString()} · fontes: ${JSON.stringify(m.sources)}`;
      article.append(title,content,source);
      const edit = document.createElement('button'); edit.textContent = 'Revisar / editar'; edit.onclick = () => editor(m); article.append(edit);
      const del = document.createElement('button'); del.textContent = 'Excluir'; del.onclick = async () => { if (!window.confirm(`Excluir memória #${m.id}? Fontes JSON e backups existentes permanecerão.`)) return; try { await call('delete',{id:m.id}); await refresh(); message('Memória excluída.'); } catch(e) { message(String(e)); } }; article.append(del); list.append(article);
    }
  }
  bind('search',search); bind('new',() => editor()); bind('close',() => {get('form').hidden = true;});
  bind('options',async () => { await call('options',MemoryUI.options(get('retrieval').checked,get('extraction').checked,get('budget').value,get('active-project').value)); message('Opções salvas.'); await refresh(); });
  get('form').addEventListener('submit',async e => { e.preventDefault(); try { const memory = {kind:get('kind').value,content:get('content').value,project_id:MemoryUI.nullableId(get('project').value),fact_key:get('key').value,state:get('state').value,importance:Number(get('importance').value)}; await call('save',{id:editing,memory}); get('form').hidden = true; await refresh(); message('Memória salva. Confira o estado após a detecção de contradições.'); } catch(err) { message(String(err)); } });
  function projectEditor(p = {}) {
    projectId = p.id ?? null; const fields = get('project-fields'); fields.replaceChildren();
    for (const [key,label] of Object.entries(projectLabels)) { const l = document.createElement('label'), input = document.createElement('textarea'); l.textContent = label; input.id = `mem-p-${key}`; input.value = p[key] ?? ''; input.rows = 2; input.maxLength = 1500; if (key === 'name') input.required = true; l.append(input); fields.append(l); }
    get('project-form').hidden = false;
  }
  bind('project-new',() => projectEditor()); bind('project-load',() => {const p=projects.find(p => p.id === Number(get('project-edit').value)); if(p) projectEditor(p);});
  get('project-form').addEventListener('submit',async e => {e.preventDefault(); try { const p={id:projectId}; for(const k of Object.keys(projectLabels)) p[k]=document.getElementById(`mem-p-${k}`).value; await call('project_save',p); get('project-form').hidden=true; await refresh(); message('Projeto salvo como referência local.'); } catch(err) {message(String(err));} });
  bind('consolidate',async () => {get('consolidate').disabled = true; try { const r=await call('consolidate'); await refresh(); message(`${r.processed} conversas consolidadas${r.interrupted ? ' · interrompido' : ''}. Revise os candidatos.`); } finally {get('consolidate').disabled = false;} });
  bind('stop',async () => {await invoke('memory_stop');message('Interrupção solicitada; o lote atual será concluído.');});
  async function backups() {select(get('backup-list'),(await call('backups')).map(x => [x,x]),'Selecione um backup');}
  bind('backups-refresh',backups);
  bind('backup',async () => {const r=await call('backup');await backups();message(`Backup privado criado: ${r.directory}\\${r.name}`);});
  bind('export',async () => {const r=await call('export');message(`Exportação privada: ${r.path}`);});
  bind('restore',async () => { const name=get('backup-list').value; const validated=await call('validate_backup',{name}); const confirmed=window.confirm(`Backup validado: ${validated.path}\nSubstituir a memória SQLite atual? Será criado um backup de segurança antes da restauração. Históricos JSON não serão modificados.`); if(!confirmed) return; const r=await call('restore',MemoryUI.restoreArgs(name,validated.digest,confirmed)); await refresh(); await backups(); message(`Memória restaurada. Backup anterior preservado: ${r.safety_backup}`); });
  window.addEventListener('aba',async e => {if(e.detail === 'memoria') {try {await refresh();} catch(err) {message(String(err));}}});
})();
