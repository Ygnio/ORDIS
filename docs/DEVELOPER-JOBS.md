# Fase 8.2A — Arquitetura e fronteira dos jobs

## Reconhecimento anterior à implementação

`developer.rs` concentra Config, Scope (caminhos canônicos e proteção de links), prepare/apply,
aprovação nativa por ID/prazo, orçamentos de chamadas, diário SQLite privado e projeção para memória.
`developer_runner.rs` já executa Python/PowerShell restritos sob Job Object, com stdin de bootstrap,
saída limitada e cancelamento. `agente.rs` usa o mesmo execute e canal de eventos; GUI usa
developer_action/developer_decide e cartões; CLI usa Input/Terminal e quadros JSONL em stdin.
Timeout de execução atual: 1–600 s; aprovação: 1–600 s. Memória recebe dados não confiáveis,
nunca autoridade para aprovar. Não há executor Cargo/Node generalizado nessa base.

## Decisão de segurança

Windows Job Objects controlam recursos e árvore de processos; **não são sandbox de sistema de
arquivos/rede**. Compiladores e testes executam código do projeto. Nesta fase somente projetos
sintéticos do catálogo nativo, com conteúdo integral conferido contra templates fixos, serão
elegíveis. Não haverá chave para transformar um projeto arbitrário em confiável, nem execução
de scripts gerados livremente pelo modelo. Projetos não confiáveis permanecem bloqueados até
uma fase com isolamento de SO efetivo. Isso limita utilidade, mas respeita a fronteira autorizada.

O operador seleciona workspace vazio e ferramentas nativas já instaladas. A GUI/CLI humanas
criam uma fixture do catálogo sem sobrescrever diretórios existentes. O modelo só escolhe uma
operação enumerada e justifica; não escolhe executável, shell, argumento livre, cwd, ambiente,
timeout ou elevação. Configuração e criação de fixtures não serão ferramentas oferecidas ao modelo.

## Integração planejada

- `developer_jobs` fornecerá configuração, catálogo, planos estruturados e persistência de jobs.
  A execução continuará passando por prepare/aprovação/journal/apply de `developer.rs`.
- `developer_runner` reutilizará seu bootstrap Python confiável: ele aguarda stdin antes de
  iniciar qualquer compilador. O pai atribui Job Object e limites antes de liberar esse payload.
  Não haverá janela entre spawn do compilador e associação, pois descendentes já nascem no job.
  Cancelamento/timeout encerram somente o Job Object próprio. Python instalado é requisito
  também para iniciar jobs Cargo/Node por esse mesmo mecanismo, sem um executor paralelo.
- Mutex de perfil impede jobs concorrentes/duplicados; aprovações são específicas e descartáveis.
  Fontes e executáveis são conferidos/abertos sem permitir escrita/remoção durante a execução.
  A execução usa uma cópia privada gerada do catálogo nativo, com fontes conferidas e
  bloqueadas para escrita, em vez de compilar diretamente o workspace mutável.
- Cargo recebe --offline --locked e target explícito; cache/target/TEMP isolados; rustc/rustdoc,
  formatter e C tools por caminhos nativos e hashes. Proxies rustup, install/update, dependências
  ausentes e argumentos extras falham sem instalação. Clippy só quando já instalado.
- Operações: Rust check/test/fmt/clippy; Node version/syntax/test de script fixo; Python
  syntax/test de fixture fixa. O executor restrito anterior permanece disponível e restrito.
- Limites: prazo global aplicável a toda árvore, CPU por processo quando disponível, memória
  do job, hard cap de CPU, processos ativos, stdout/stderr e armazenamento monitorado. Cotas
  de disco monitoradas não serão descritas como isolamento ou quota física infalível.
- Estado e logs limitados ficam privados: pending, awaiting_approval, running, completed,
  failed, cancelled, timed_out, interrupted. IDs existentes não são reutilizados. Reinício
  reconcilia proprietário/PID/criação e marca interrupção, sem reexecutar nada.
- Resultados do executor (estado, exit code, tempos, hashes, limites) são separados de texto e
  diagnósticos não confiáveis dos compiladores. A GUI usa textContent; logs não são instruções.
- CLI possui solicitação/consulta/logs e cancelamento JSONL/Ctrl+C. GUI possui criação de fixture,
  solicitação, cartão de aprovação, saída, resumo, código de saída e cancelamento específico.
  Os dois clientes usam o mesmo serviço e formatos. A validação visual é registrada separadamente.

## Validação

Fixtures Rust/Node/Python cobrirão sucesso, falha sintática/de teste, espera, saída excessiva e
filhos. Testes negativos cobrirão fonte alterada, links, workspace, argumento/executável,
aprovação, ausência de ferramenta, reinício e cancelamento. Elevação continua bloqueada;
se o runner elevado não permitir um caso positivo, registrar BLOCKED, sem relaxar a proteção.
Nada inicia Gemma, WSL ou serviços. O teste real será apenas um plano supervisionado separado.


## Uso local e aprovação

A configuração inicial permanece desativada. Em Configurações → Desenvolvedor, selecione um
workspace sintético, permita leitura/escrita/execução e abra “Jobs de desenvolvimento”. Informe
somente caminhos de ferramentas nativas já instaladas. Salvar fixa hashes, sem instalar nada.
Crie uma fixture pelo seletor; arquivos existentes nunca são sobrescritos. Selecione projeto,
operação e justificativa. O cartão exibe o plano, os limites e o prazo antes da decisão humana.
O painel mostra histórico, resultado, saída não confiável e cancelamento por ID. Abrir o painel
ou consultar um job não o executa novamente.

Na CLI, `desenvolvedor load/save` compartilha a configuração; `job fixture` recebe JSON com
`language` (rust/node/python) e `case` (pass/compile_error/test_fail/wait/output/child/unformatted).
Rust não oferece a fixture child. `job projects` lista fixtures locais. `job solicitar` recebe:

```json
{"project":"node-pass","operation":"node_test","reason":"Validar fixture sintética"}
```

Depois do evento `awaiting_approval`, somente stdin humano pode enviar o quadro abaixo,
substituindo o ID pelo recebido; o texto do modelo não autoriza nada:

```json
{"type":"approval","id":"ID_RECEBIDO","decision":"allow"}
```

`deny`, EOF e expiração impedem execução. `{"type":"cancel"}` ou Ctrl+C cancela a árvore própria.
`job history` recebe `{}`; `job get`, `job logs` e `job cancel` recebem `{"id":"ID_RECEBIDO"}`.
Cada linha de saída de `job solicitar` é JSON independente. O código de saída do processo filho
fica no resultado; um resultado de job failed pode ter sido entregue corretamente pela CLI.
Não confundir a entrega do resultado com aprovação dos testes do projeto.

## Limitações e estado de validação

Job Objects não isolam arquivos/rede e não resistem a um invasor com os privilégios da conta.
Não há suporte a projetos arbitrários ou edição autônoma nesta fase. DLLs e bibliotecas do
runtime instalado continuam na fronteira de confiança da configuração feita pelo operador.
Cotas de disco são monitoradas por amostragem; pode haver ultrapassagem entre verificações.
O prazo de aprovação é separado do prazo de execução da árvore e da verificação dos hashes.
O histórico conserva até 50 jobs; não há exclusão automática nem recuperação que reexecute jobs.

Fontes editáveis continuam recusando hard links. Executáveis instalados fora do workspace
podem usar aliases NTFS, com nome nativo verificado, hash fixado e handle que impede escrita
pelos aliases e remoção do caminho selecionado enquanto executa. O teste demonstra que excluir
um nome alternativo não altera o executável selecionado; isso não autoriza editar fontes por links.
Falta de target MSVC ou de ferramenta continua BLOCKED, sem instalação implícita. Clippy exige
o componente nativo já instalado; ausência é BLOCKED. A fila JSONL da CLI é limitada: leitor
travado não bloqueia o monitor de tempo/recursos; saturação cancela e falha de entrega preserva
o resultado auditável, sem aguardar stdout indefinidamente.
O teste CI em processo elevado confirma a recusa, mas identifica execução positiva como
BLOCKED_ELEVATED. CI verde não representa validação visual nem aprovação operacional.

Os relatórios de QA diferenciam IMPLEMENTADO, TESTADO, PENDENTE e BLOQUEADO. Nenhum pacote
atualiza a instalação operacional. Nenhum teste inicia Gemma, WSL, OpenClaw ou serviços reais.
Antes de qualquer teste com Gemma: exigir autorização específica, verificar servidor existente,
modelo/parâmetros, recursos e slot ocioso; usar apenas perfil/fixtures sintéticos, uma chamada
limitada por vez e aprovação de cada job. Não alterar contexto, MTP, visão ou sampler. Preparar
plano supervisionado separado; a presente fase não o executa.

O transporte de saída limitado também cobre jobs solicitados por chamada de ferramenta na CLI de conversa. O cancelamento externo por ID é observado durante a aprovação nesse caminho. Estados timed_out/interrupted são falhas no resultado e histórico da conversa; não são sucesso. Os testes de agente usam backend HTTP sintético, conferem o papel tool e os campos confiáveis/não confiáveis, sem modelo real. A integração com memória reutiliza a projeção operacional do diário; logs de compiladores não viram fatos confirmados automaticamente.
