/* Human-only learning controls. Tool results cannot click, approve or configure these. */
const ActiveMemoryUI = {
  policy(mode, categories) {
    if (!['manual','assisted','controlled'].includes(mode) || !Array.isArray(categories) || categories.some(c=>!['technical_profile','projects','decisions','tasks'].includes(c))) throw new Error('Política inválida');
    return {mode,categories:[...new Set(categories)]};
  },
  forget(request, confirmed) {
    if (!confirmed || request.operation!=='forget' || !Number.isInteger(request.id) || !Number.isInteger(request.target)) throw new Error('Confirmação específica obrigatória');
    return {request_id:request.id,target_id:request.target,confirmed:true};
  },
  labels: {user_identity:'Identidade do usuário',preferences:'Preferências',technical_profile:'Perfil técnico',projects:'Projetos',decisions:'Decisões',tasks:'Tarefas',knowledge:'Conhecimento',experiences:'Experiências',relationships:'Relacionamentos',instructions:'Instruções persistentes (dados sem privilégios)',temporary:'Temporárias'},
};
if(typeof module!=='undefined')module.exports=ActiveMemoryUI;
if(typeof window!=='undefined')(()=>{
  const get=id=>document.getElementById('learning-'+id);
  const call=(action,args={})=>window.__TAURI__.core.invoke('memory_action',{action,args});
  function status(text){get('status').textContent=text;}
  async function refresh(){
    const data=await call('learning_get');get('mode').value=data.policy.mode;
    const choices=get('categories');choices.replaceChildren();
    for(const c of data.auto_categories){const label=document.createElement('label'),input=document.createElement('input');input.type='checkbox';input.value=c;input.checked=data.policy.categories.includes(c);label.append(input,document.createTextNode(ActiveMemoryUI.labels[c]));choices.append(label);}
    const requests=await call('requests'),list=get('requests');list.replaceChildren();
    if(!requests.length)list.textContent='Nenhuma atualização ou exclusão aguardando confirmação.';
    for(const r of requests){const article=document.createElement('article'),text=document.createElement('pre');text.textContent=`${r.operation==='forget'?'Esquecer':'Substituir'} #${r.target}\n${r.content}\nMotivo proposto (não confiável): ${r.reason}`;article.append(text);
      const approve=document.createElement('button');approve.textContent=r.operation==='forget'?'Confirmar exclusão deste ID':'Revisar e aprovar substituição';approve.onclick=async()=>{try{
        if(r.operation==='forget'){if(!window.confirm(`Excluir SOMENTE #${r.target}?\n${r.content}\nMensagens originais e backups permanecem.`))return;await call('forget_confirm',ActiveMemoryUI.forget(r,true));}
        else{const candidate=await call('active_get',{id:r.candidate});if(!window.confirm(`Substituir #${r.target}:\n${r.content}\nPor #${r.candidate}:\n${candidate.content}`))return;await call('approve',{id:r.candidate,replace:true});}
        status('Decisão registrada localmente.');await refresh();window.dispatchEvent(new CustomEvent('aba',{detail:'memoria'}));
      }catch(e){status(String(e));}};
      const reject=document.createElement('button');reject.textContent='Recusar solicitação';reject.onclick=async()=>{try{if(r.candidate)await call('reject',{id:r.candidate});else await call('request_reject',{id:r.id});await refresh();}catch(e){status(String(e));}};article.append(approve,reject);list.append(article);
    }
    const history=await window.__TAURI__.core.invoke('developer_action',{action:'history',args:{}});
    get('operational').textContent=history.tasks.length?history.tasks.map(t=>`${t.title} · ${t.state} · ${new Date(t.updated*1000).toLocaleString()}`).join('\n'):'Nenhuma tarefa no diário Desenvolvedor. Estados não autorizam execução.';
  }
  get('save').onclick=async()=>{try{const p=ActiveMemoryUI.policy(get('mode').value,[...get('categories').querySelectorAll('input:checked')].map(x=>x.value));if(p.mode==='controlled'&&!window.confirm('Autorizar confirmação automática apenas das categorias marcadas? Atualizações conflitantes, exclusões e dados sensíveis continuam exigindo revisão.'))return;await call('learning_save',p);status('Política privada salva.');await refresh();}catch(e){status(String(e));}};
  get('refresh').onclick=()=>refresh().catch(e=>status(String(e)));
  window.addEventListener('aba',e=>{if(e.detail==='memoria')refresh().catch(e=>status(String(e)));});
  window.ActiveMemoryUI=ActiveMemoryUI;
})();
