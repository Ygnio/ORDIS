# Fase 3.3 — Memória ativa

Base: `feature/lucille-developer-agent` (PR #7), dependente de `feature/memory-reliability` (PR #6). Ambos estavam abertos em 24/10/2026. PR próprio, sem merge automático ou alterações no upstream.

## Arquitetura incremental

SQLite/FTS5, identidade, histórico JSON, recuperação, consolidação e diário Desenvolvedor permanecem. Migração aditiva 005 cria classificação, versões, políticas, auditoria, solicitações, bloqueios de origem/chave e referências nativas. Backup privado antes da migração. Nenhum modelo, embedding, dependência pesada ou serviço novo.

As onze categorias funcionais mapeiam os tipos existentes: identidade/relacionamentos/temporárias → fact; preferências → preference; perfil técnico → hardware; projetos → project; decisões → decision; tarefas → task; conhecimento/experiências → solution; instruções → constraint. Tipos legados goal, summary e hypothesis continuam válidos. Etiquetas múltiplas são metadados; busca continua FTS5 lexical. Similaridade lexical conservadora complementa fingerprints e origem: mudanças de nomes, números ou negação não são mescladas. Não se afirma equivalência semântica universal. Classificação por finalidade pode ser revista pela pessoa. Temporárias precisam de validade explícita e deixam a recuperação quando expiram. A extração não adivinha datas: candidatos temporários sem validade não podem ser aprovados. Trechos de origens esquecidas/rejeitadas ficam fora de novos resumos. Os bloqueios são conservadores e podem abranger outros fatos da mesma mensagem; restauração deliberada exige criação/revisão manual.

## Ferramentas

`memory_search`, `memory_propose`, `memory_update`, `memory_forget`, `memory_inspect` são integradas ao mesmo ciclo de ferramentas llama.cpp/TabbyAPI, com orçamento e cancelamento. Não expõem IPC humano. Propostas usam trecho literal da última mensagem real do usuário ou conteúdo exato de evidência nativa. `source` e `confidence` não concedem aprovação. Conteúdo de arquivos não é evidência de declaração do usuário. Classificação continua conservadora, sem alegação de compreensão semântica universal.

Atualização cria candidato vinculado à versão original; aprovação verifica se a original mudou, preserva histórico e torna a anterior obsoleta. Esquecimento exige pedido explícito na última mensagem real do usuário e cria solicitação persistente de um único ID; interface mostra conteúdo e exige confirmação específica. Exclusão remove versões daquele ID, mantém tombstones e bloqueios contra extração da mesma origem/chave. Não apaga conversas JSON, mensagens SQLite originais ou backups. Rejeição bloqueia a origem para evitar repetição da consolidação.

Memória operacional consulta o diário original, sem duplicar transcrições. Uma tabela auxiliar vincula tarefas novas ao workspace existente; tarefas legadas sem escopo não são automaticamente atribuídas a projetos. Recuperam-se estados das três tarefas recentes do mesmo workspace, sem títulos, prompts, saídas ou permissões. Evidências de developer_execute registram apenas resultado real do processo (sucesso, falha ou desconhecido), referência, data e código de saída; saída integral e arquivos não são copiados. Operações de escrita concluídas podem gerar referência mínima ao tipo e caminho relativo, com ligação ao diário existente. Código de saída zero não prova por si só que todos os testes passaram ou que o projeto está correto.

## Aprendizado da Lucille

Padrão manual: todas as propostas exigem revisão. Assistido: mesma aprovação, com organização/classificação dos candidatos. Automático controlado: precisa ser habilitado pela pessoa, por categoria; restrito a declarações literais inequívocas de perfil técnico, projetos, decisões ou tarefas. Segredos/dados sensíveis detectáveis, instruções, relações, identidade, experiências e atualizações conflitantes não são confirmados automaticamente. Auditoria local e botão para desfazer confirmação automática. O modelo não pode editar políticas.

Política de aprovação e perfis não entram no Git. A detecção de dados sensíveis é conservadora, não garante detectar todo segredo. Revisão humana continua necessária. Contexto de memórias permanece dado sem privilégios, separado da identidade permanente.

## Validação real com perfil sintético

1. Extraia pacote e inicie por Iniciar-ORDIS-Teste.ps1, mantendo seu servidor externo existente e seu perfil real separados.
2. Em Memória, confira modo manual. Diga: “Lucille, meu projeto fictício se chama Aurora.” Revise origem/categoria, aprove e abra outra conversa para recuperar Aurora.
3. Diga “Renomeei Aurora para Horizonte”; peça memory_update do ID conhecido. Confira conteúdo antigo/novo e aprove a substituição. Consulte histórico.
4. Peça esquecer o ID. Confirme apenas esse conteúdo na página Memória. Reinicie e confirme ausência na busca. Mensagens e backups continuam existentes.
5. Execute tarefa sintética pelo modo Desenvolvedor. Peça memory_inspect e proposta da evidência nativa. Confira resultado, data e aprovação. Não confunda execução bem sucedida com validação universal.
6. Teste recusa, cancelamento, categorias, etiquetas, expiração e reversão automática. Verifique TabbyAPI/ComfyUI em seu hardware.

GNU/MSVC são validados no CI; o pacote GNU preserva Windows 10/WebView2 e não exige AVX2/FMA por mudança desta fase. Testes reais de interface, GPU e todos os caminhos de dependências devem ser relatados separadamente, sem presumir sucesso por mocks. Nenhum processo externo llama.cpp é alterado.
