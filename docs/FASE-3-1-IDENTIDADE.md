# Lucille — Fase 3.1: identidade e memória supervisionada

## Dependências e identidade

Esta alteração depende dos PRs #3 (memória) e #4 (métricas) do fork Ygnio/ORDIS. Sua branch parte de feature/response-metrics. Não altera processos TabbyAPI, llama.cpp ou ComfyUI, nem aprova merges.

A identidade anterior estava na primeira frase de agente/workspace/AGENTS.md. O prompt era montado em app/src/agente.rs e incluía também AGENTS.local.md. Agora o arquivo público descreve regras do aplicativo ORDIS; o perfil privado fornece o nome Lucille por padrão. A frase padrão antiga é normalizada somente em memória durante a montagem do contexto, inclusive em instalações que já copiaram o template antigo. Nomes do aplicativo, binários, ferramentas e backends permanecem ORDIS quando apropriado.

Configurações > Lucille permite editar nome, descrição, personalidade e instruções permanentes. O perfil é salvo explicitamente na tabela identity_profile do banco privado. Cada solicitação, inclusive uma nova conversa ou após reinicialização, carrega o perfil persistente. Falhas de leitura não substituem silenciosamente um perfil existente pelo padrão.

Sem perfil salvo, AGENTS.local.md é preservado integralmente como instruções legadas; formas simples de nome ("Você é Aurora, ...", "Seu nome é ...", "Você se chama ...", "Nome: ...") são reconhecidas. Formatos complexos não são interpretados: revise os campos antes de salvar. Nenhum arquivo personalizado é reescrito, removido ou automaticamente importado como perfil explícito. O perfil salvo passa a ser a fonte permanente; o arquivo legado continua intocado. Credenciais reconhecíveis são recusadas no formulário, sem ecoar o conteúdo no erro. Essa heurística não é um detector universal de segredos.

## Contexto e confiança

O perfil editado pelo usuário integra as instruções de sistema. Memórias confirmadas são inseridas separadamente como mensagem de dados delimitados lucille_memory_data. Candidatos, conflitos e itens obsoletos não entram na recuperação. A memória não possui operação automática que escreva a identidade. Alterações de perfil exigem a API explícita de configuração da interface.

Preferências e restrições aprovadas globais ou do projeto atual são consideradas mesmo sem coincidência lexical. Demais fatos usam FTS5; seleção e invólucro compartilham o orçamento configurado (estimativa conservadora de tokens, não tokenização exata do modelo). O contexto continua sujeito ao limite geral do chat. Instruções dizem expressamente que dados recuperados não autorizam ferramentas nem alteram identidade; um modelo ainda pode desobedecer instruções, portanto isso não é uma garantia contra todo prompt injection.

A resposta guarda somente IDs das memórias selecionadas. O painel expansível consulta o conteúdo atual; memórias excluídas aparecem como removidas. A interface não afirma recuperar o texto histórico de versões antigas. O painel Memória apresenta confirmadas, candidatos, conflitos, recuperações recentes, projetos e estado real da consolidação, incluindo fila pendente e interrupções.

## Ciclo supervisionado

Mensagens do usuário são registradas e classificadas por regras locais para gerar candidatos; não há modelo adicional ou consumo de VRAM. A extração é deliberadamente conservadora, não entende toda preferência expressa em linguagem natural. Revise, edite e aprove no painel Memória. A aprovação usa o estado active; conflitos com a mesma chave de fato exigem resolução explícita. Deduplicação usa conteúdo normalizado, tipo e projeto.

A consolidação percorre os próximos 20 trechos por lote, sem saltar mensagens antigas. Uma execução processa até 20 lotes e pode ser interrompida entre lotes; execute novamente se ainda houver fila. Resumos extrativos e candidatos continuam aguardando aprovação. O estado e o checkpoint persistem após reinício; nenhuma consolidação promove hipóteses automaticamente a fatos.

Exclusões e edições registram impressões digitais privadas para impedir a recriação automática do conteúdo rejeitado na extração ou consolidação. Resumos derivados de uma fonte excluída tornam-se obsoletos. IDs nunca são reutilizados. Uma exclusão de memória não apaga a conversa original ou backups anteriores: gerencie-os separadamente. Uma recriação manual é permitida; a exclusão não é apagamento forense do disco.

## Dados locais e migração

Schema 3 adiciona perfil, rejeições, estado de manutenção e sequência de IDs. Migrações são transacionais. Backups válidos dos schemas 2 e 3 são aceitos, com verificação de integridade, estrutura e cópia de segurança anterior à restauração; backups 2 são migrados durante restauração. A restauração substitui a base inteira, inclusive o perfil; confirme o conteúdo do backup escolhido. Não abra um banco schema 3 com a versão antiga do aplicativo; mantenha uma cópia privada anterior à atualização para eventual retorno.

Armazenamento de produção: %LOCALAPPDATA%\Lucille\ORDIS\runtime\memory\lucille.sqlite3. O launcher de teste usa uma pasta privada isolada sob %LOCALAPPDATA%\Lucille\ORDIS-Teste. SQLite/WAL/SHM, perfis, exportações, históricos, prompts privados e credenciais não fazem parte do pacote nem do Git. As memórias são locais em texto no SQLite, sem criptografia de disco própria; use a proteção do usuário/disco do Windows. Dados selecionados são enviados ao endpoint de inferência configurado como parte do chat; um backend remoto recebe esse contexto.

.gitignore, scripts/privacy_guard.py, testes de caminhos privados e gitleaks no CI reduzem o risco de publicação. O ZIP usa uma lista explícita de arquivos públicos. Nenhum teste usa conversas reais. Os scanners não garantem detectar conteúdo privado arbitrário em qualquer nome de arquivo: revise o diff antes de publicar.

## Validação automatizada

- Rust GNU e MSVC: migrações, restauração v2, perfil persistente, preferência sem sobreposição lexical, exclusão sem ressurreição, consolidação incremental, conflitos, projetos, orçamento, memória maliciosa como dados, streaming, cancelamento, métricas e backends existentes.
- Python: testes existentes e proteção de caminhos privados.
- JavaScript: sintaxe, backends, memória, métricas e formulário de identidade.
- scripts/test-release-identity.py PACOTE.zip: executa o CLI real em fixture privada descartável e dois backends HTTP sintéticos; verifica Lucille padrão, preservação do prompt legado, perfil personalizado após novos processos, aprovação/recuperação, IDs e métricas persistidos, integridade SQLite e proteção do histórico. A aprovação e edição do perfil neste smoke test são SQL sintético; a interação humana da interface segue o roteiro abaixo.
- CI publica o ZIP somente depois de compilar e executar o smoke test. Sem dados reais, modelo ou GPU.

## Roteiro real de aceitação (Windows 10)

Use o launcher isolado e dados fictícios. Não copie sua pasta privada para GitHub ou para o pacote.

1. Abra Configurações > Lucille, defina nome Lucille e instruções fictícias, salve.
2. Abra uma nova conversa, pergunte o nome; confirme Lucille. Confira também uma instalação com perfil personalizado anterior: ele não deve ser sobrescrito.
3. Escreva "Prefiro respostas curtas sobre o projeto fictício Atlas".
4. Abra Memória > Candidatos; confira fonte, tipo e conteúdo. Se necessário, use Consolidar e observe fila/estado.
5. Edite o candidato e aprove-o (estado active). Resolva conflitos explicitamente; não aprove instruções maliciosas como identidade.
6. Feche e reabra o aplicativo. Verifique os campos da identidade e a memória confirmada.
7. Abra outra conversa, pergunte algo relacionado ao Atlas e também cumprimente sem repetir palavras. Confira preferências recuperadas e expanda Memórias utilizadas na resposta. Alterne entre llama.cpp e TabbyAPI externos sem reiniciar processos.
8. Edite a memória confirmada. Gere outra resposta e confira a versão atual. Exclua a memória.
9. Abra outra conversa e execute a consolidação; confirme que o item excluído não volta. Confira que referências antigas informam remoção. Teste orçamento, projetos distintos e uma contradição fictícia.
10. Execute python scripts/privacy_guard.py e revise git diff --cached --name-only antes de publicar código. Nunca envie o banco, backups ou exportações. Confirme que o ZIP não contém esses arquivos.

Também testar na interface: interrupção da geração, métricas por resposta, desligar recuperação/extração, restaurar um backup sintético, ComfyUI e ferramentas. Um servidor HTTP simulado não comprova a obediência de um modelo real, a interação WebView2 ou uso de GPU. Permanecem validações manuais necessárias com os modelos e hardware do usuário.
