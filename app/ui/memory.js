/* Private memory UI. All stored content uses textContent, never HTML or Markdown. */
const MemoryUI = {
  nullableId(value) { return value === '' ? null : Number(value); },
  options(retrieval, extraction, budget, project, auto=false) {
    const token_budget = Number(budget);
    if (!Number.isInteger(token_budget) || token_budget < 128 || token_budget > 4096) throw new Error('Orçamento: 128 a 4096 tokens estimados.');
    return { auto_consolidation:!!auto, retrieval: !!retrieval, extraction: !!extraction, token_budget, project_id: this.nullableId(project) };
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
    select(get('filter-category'),Object.entries(window.ActiveMemoryUI?.labels||{}),'Todas as finalidades');
    for (const id of ['active-project','filter-project','project','project-edit']) select(get(id), projects.map(p => [p.id,p.name]), id === 'filter-project' ? 'Todos os projetos' : 'Sem projeto');
    for (const id of ['kind','filter-kind']) select(get(id), data.kinds.map(k => [k, labels[k]]), id === 'filter-kind' ? 'Todas as categorias' : null);
    for (const id of ['state','filter-state']) select(get(id), data.states.map(k => [k, labels[k]]), id === 'filter-state' ? 'Todos os estados' : null);
    const o = data.options; get('auto').checked=o.auto_consolidation; get('retrieval').checked = o.retrieval; get('extraction').checked = o.extraction; get('budget').value = o.token_budget; get('active-project').value = o.project_id ?? '';
    const s = data.stats; get('stats').textContent = `Banco: ${s.path}\nSchema ${s.schema} · ${s.memories} memórias · ${s.messages} mensagens novas · ${s.projects} projetos\nSQLite: ${s.database_bytes} bytes + WAL ${s.wal_bytes} bytes · cache configurado: ${s.cache_kib} KiB · auditorias: ${s.retrievals}`;
    const d=await call('dashboard');
    get('overview').textContent=`${d.states.active} confirmadas podem fornecer contexto; ${d.states.candidate} candidatos aguardam sua aprovação; ${d.states.conflict} contradições ficam fora da recuperação; ${d.states.obsolete} obsoletas estão desativadas. Identidade permanente é editada em Configurações.`;
    get('confirmed').textContent=`Confirmadas (${d.states.active})`;get('candidates').textContent=`Aguardando revisão (${d.states.candidate})`;get('conflicts').textContent=`Contradições (${d.states.conflict})`;get('obsolete').textContent=`Obsoletas (${d.states.obsolete})`;
    get('automation-state').textContent=`Recuperação ${o.retrieval?'ativada':'desativada'} · Extração ${o.extraction?'ativada':'desativada'} · Consolidação automática ${o.auto_consolidation?(d.technical.auto_paused?'pausada por interrupção; salve opções para retomar':'ativada; aguarda ociosidade'):'desativada'}. Sem extração habilitada, consolidação automática não executa.`;
    get('extraction-state').textContent=`Mensagens com candidato: ${d.extraction_states.candidate_available||0}; sem informação durável: ${d.extraction_states.no_durable_information||0}; extração desativada: ${d.extraction_states.extraction_disabled||0}. Não encontrar uma memória é um resultado normal.`;
    get('diagnostics').textContent=JSON.stringify(d.technical,null,2);
    const recent=get('recent');recent.replaceChildren();
    if(!d.recent.length)recent.textContent='Nenhuma recuperação registrada. Aprove uma preferência e inicie outra conversa com recuperação habilitada.';
    for(const r of d.recent){const p=document.createElement('p');p.textContent=`${new Date(r.date*1000).toLocaleString()} · ${r.ids.length ? r.ids.map(id=>'#'+id).join(', ') : 'Nenhuma memória selecionada'} · ${r.tokens} tokens estimados`;recent.append(p);}
    const states={idle:'Ainda não executada',running:'Em execução',interrupted:'Interrompida; pode retomar',completed:'Último lote concluído',error:'Último lote falhou; fontes preservadas'};
    get('consolidation-state').textContent=`${(d.maintenance.state==='running'&&!d.running?'Execução anterior interrompida; pode retomar':states[d.maintenance.state])||d.maintenance.state} · ${d.maintenance.processed} lotes processados · ${d.pending_messages} mensagens aguardando consolidação · última atualização: ${d.maintenance.date ? new Date(d.maintenance.date*1000).toLocaleString() : "ainda não executada"}. Resumos e decisões extraídos exigem revisão.`;
    get('project-summary').textContent=projects.length ? projects.map(p=>`${p.name}: ${p.execution_state||'estado não informado'}; próxima tarefa: ${p.next_tasks||'não informada'}`).join(' | ') : 'Nenhum projeto cadastrado. Cadastre objetivo e tarefas de retomada abaixo.';
    loaded = true; await search();
  }
  function editor(m = null) {
    editing = m?.id ?? null; get('editor-title').textContent = editing ? `Editar memória #${editing}` : 'Criar memória';
    get('kind').value = m?.kind ?? 'fact'; get('project').value = m?.project_id ?? ''; get('state').value = m?.state ?? 'active'; get('importance').value = m?.importance ?? 3; get('key').value = m?.fact_key ?? ''; get('content').value = m?.content ?? ''; get('form').hidden = false; get('content').focus();
  }
  async function search() {
    const rows = (await call('active_list', { query:get('query').value, project_id:MemoryUI.nullableId(get('filter-project').value), category:get('filter-category').value, state:get('filter-state').value })).filter(m=>!get('filter-kind').value||m.kind===get('filter-kind').value);
    const list = get('list'); list.replaceChildren();
    if (!rows.length) { const p = document.createElement('p'); p.textContent = 'Nenhuma memória encontrada.'; list.append(p); }
    for (const m of rows) {
      const article = document.createElement('article'), title = document.createElement('h3'), content = document.createElement('pre'), source = document.createElement('p');
      title.textContent = `#${m.id} · ${labels[m.kind]} · ${labels[m.state]} · ${projects.find(p => p.id === m.project_id)?.name ?? 'Geral'}`;
      content.textContent = m.content;
      source.textContent = `Origem: ${m.provenance} · criado ${new Date(m.created_at * 1000).toLocaleString()} · atualizado ${new Date(m.updated_at * 1000).toLocaleString()} · fontes: ${JSON.stringify(m.sources)}`;
      article.append(title,content,source);
      const meta=document.createElement('p');meta.textContent=`Finalidade: ${window.ActiveMemoryUI?.labels[m.classification.category]||m.classification.category} · ${m.classification.form} · etiquetas: ${m.classification.tags.join(', ')} · política: ${m.classification.policy} · validade: ${m.classification.expires_at?new Date(m.classification.expires_at*1000).toLocaleString():'duradoura'}`;article.append(meta);
      const history=document.createElement('button');history.textContent='Histórico de versões';history.onclick=async()=>{try{const p=document.createElement('pre');p.textContent=JSON.stringify(await call('versions',{id:m.id}),null,2);article.append(p);history.disabled=true;}catch(e){message(String(e));}};article.append(history);
      const classify=document.createElement('button');classify.textContent='Reclassificar / etiquetas';classify.onclick=async()=>{try{const category=window.prompt('Finalidade: '+Object.keys(window.ActiveMemoryUI.labels).join(', '),m.classification.category);if(!category)return;const tags=window.prompt('Etiquetas separadas por vírgula',m.classification.tags.join(', '));if(tags===null)return;let expires_at=null;if(category==='temporary'){const date=window.prompt('Validade (ISO: AAAA-MM-DD)',new Date(Date.now()+86400000).toISOString().slice(0,10));if(!date)return;expires_at=Math.floor(Date.parse(date)/1000);}await call('reclassify',{id:m.id,category,tags:tags.split(',').map(x=>x.trim()).filter(Boolean),expires_at});await search();}catch(e){message(String(e));}};article.append(classify);
      if(m.classification.policy==='controlled-v1'){const undo=document.createElement('button');undo.textContent='Desfazer confirmação automática';undo.onclick=async()=>{await call('undo_auto',{id:m.id});await refresh();};article.append(undo);}
      if(m.state==='candidate'||m.state==='conflict'){const reject=document.createElement('button');reject.textContent='Rejeitar proposta';reject.onclick=async()=>{try{await call('reject',{id:m.id});await refresh();}catch(e){message(String(e));}};article.append(reject);}
      if(m.state==='candidate'||m.state==='conflict'){const approve=document.createElement('button');approve.textContent=m.state==='conflict'?'Usar esta versão; desativar anteriores':'Aprovar';approve.onclick=async()=>{const replace=m.state==='conflict';if(replace&&!window.confirm('Confirmar esta versão e marcar as outras versões da mesma chave e projeto como obsoletas?'))return;try{await call('approve',{id:m.id,replace});await refresh();message('Memória aprovada.');}catch(e){message(String(e));}};article.append(approve);}
      const edit = document.createElement('button'); edit.textContent = 'Revisar / editar'; edit.onclick = () => editor(m); article.append(edit);
      const del = document.createElement('button'); del.textContent = 'Excluir'; del.onclick = async () => { if (!window.confirm(`Excluir memória #${m.id}? Fontes JSON e backups existentes permanecerão.`)) return; try { await call('delete',{id:m.id}); await refresh(); message('Memória excluída.'); } catch(e) { message(String(e)); } }; article.append(del); list.append(article);
    }
  }
  for(const [id,state] of [['confirmed','active'],['candidates','candidate'],['conflicts','conflict'],['obsolete','obsolete']])bind(id,async()=>{get('filter-state').value=state;await search();});
  bind('search',search); bind('new',() => editor()); bind('close',() => {get('form').hidden = true;});
  bind('options',async () => { await call('options',MemoryUI.options(get('retrieval').checked,get('extraction').checked,get('budget').value,get('active-project').value,get('auto').checked)); message('Opções salvas.'); await refresh(); });
  get('form').addEventListener('submit',async e => { e.preventDefault(); try { const memory = {kind:get('kind').value,content:get('content').value,project_id:MemoryUI.nullableId(get('project').value),fact_key:get('key').value,state:get('state').value,importance:Number(get('importance').value)}; await call('save',{id:editing,memory}); get('form').hidden = true; await refresh(); message('Memória salva. Confira o estado após a detecção de contradições.'); } catch(err) { message(String(err)); } });
  function projectEditor(p = {}) {
    projectId = p.id ?? null; const fields = get('project-fields'); fields.replaceChildren();
    for (const [key,label] of Object.entries(projectLabels)) { const l = document.createElement('label'), input = document.createElement('textarea'); l.textContent = label; input.id = `mem-p-${key}`; input.value = p[key] ?? ''; input.rows = 2; input.maxLength = 1500; if (key === 'name') input.required = true; l.append(input); fields.append(l); }
    get('project-form').hidden = false;
  }
  bind('project-new',() => projectEditor()); bind('project-load',() => {const p=projects.find(p => p.id === Number(get('project-edit').value)); if(p) projectEditor(p);});
  get('project-form').addEventListener('submit',async e => {e.preventDefault(); try { const p={id:projectId}; for(const k of Object.keys(projectLabels)) p[k]=document.getElementById(`mem-p-${k}`).value; await call('project_save',p); get('project-form').hidden=true; await refresh(); message('Projeto salvo como referência local.'); } catch(err) {message(String(err));} });
  bind('consolidate',async () => {get('consolidate').disabled = true; try { const r=await call('consolidate'); await refresh(); message(r.deferred?'Aguarde a resposta terminar antes de consolidar manualmente.':`${r.processed} lotes consolidados${r.interrupted ? ' · interrompido' : ''}. Revise os candidatos.`); } finally {get('consolidate').disabled = false;} });
  bind('stop',async () => {await invoke('memory_stop');message('Interrupção solicitada; o lote atual será concluído.');});
  async function backups() {select(get('backup-list'),(await call('backups')).map(x => [x,x]),'Selecione um backup');}
  bind('technical-export',async()=>{const r=await call('technical_export');message(`Relatório sanitizado salvo localmente: ${r.path}. Contém somente códigos, contagens e tempos.`);});
  bind('backups-refresh',backups);
  bind('backup',async () => {const r=await call('backup');await backups();message(`Backup privado criado: ${r.directory}\\${r.name}`);});
  bind('export',async () => {const r=await call('export');message(`Exportação privada: ${r.path}`);});
  bind('restore',async () => { const name=get('backup-list').value; const validated=await call('validate_backup',{name}); const confirmed=window.confirm(`Backup validado: ${validated.path}\nSubstituir a memória SQLite atual? Será criado um backup de segurança antes da restauração. Históricos JSON não serão modificados.`); if(!confirmed) return; const r=await call('restore',MemoryUI.restoreArgs(name,validated.digest,confirmed)); await refresh(); await backups(); message(`Memória restaurada. Backup anterior preservado: ${r.safety_backup}`); });
  window.addEventListener('aba',async e => {if(e.detail === 'memoria') {try {await refresh();} catch(err) {message(String(err));}}});
})();
