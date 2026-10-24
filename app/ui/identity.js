const LucilleIdentityUI = {
  validate(p) { if (!p.name.trim() || new TextEncoder().encode(p.name).length>80) throw new Error('Informe um nome de até 80 bytes.'); return p; },
  status(saved,legacy) { return saved ? 'Perfil privado salvo. Aplicado à próxima solicitação ao modelo.' : legacy ? 'Perfil legado encontrado. O arquivo original será preservado; salvar confirma os campos abaixo como perfil permanente.' : 'Perfil padrão Lucille. Salve para personalizar nome, personalidade e instruções.'; },
};
if(typeof module !== 'undefined') module.exports=LucilleIdentityUI;
if(typeof window !== 'undefined') (()=>{
  const el=id=>document.getElementById(`identity-${id}`),call=(action,args={})=>window.__TAURI__.core.invoke('memory_action',{action,args});
  let dirty=false,loaded=false;
  for(const key of ['name','description','personality','instructions']) el(key).addEventListener('input',()=>{dirty=true;});
  async function load(){if(dirty)return;try{const d=await call('identity_get');for(const key of ['name','description','personality','instructions'])el(key).value=d.profile[key];el('status').textContent=LucilleIdentityUI.status(d.saved,d.legacy_present);loaded=true;}catch(e){el('status').textContent=String(e);}}
  el('form').addEventListener('submit',async e=>{e.preventDefault();if(!loaded)return;try{const p={};for(const key of ['name','description','personality','instructions'])p[key]=el(key).value;await call('identity_save',LucilleIdentityUI.validate(p));dirty=false;await load();}catch(err){el('status').textContent=String(err);}});
  window.addEventListener('aba',e=>{if(e.detail==='config')load();});
})();
