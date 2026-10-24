# Lucille — Fase 3.2: mapa e confiabilidade

## Integração autorizada

PR #3: merge cb4d10f08a37fbf6f18873304071c93bbd09eb9f. PR #4: base atualizada para main e merge 34058ed1776948c702ab64b1e4429210b8699fa7. PR #5: base atualizada para main e merge 066f3d3f711d336150ac90f5476185696374356b. Antes de cada merge: checks GNU/MSVC/credenciais aprovados, diff esperado, caminhos privados protegidos e estado mergeable clean. Main conferida após cada integração. Não houve force push, bypass ou escrita no upstream.

## Mapa anterior às alterações (main 066f3d3)

| Etapa real | Implementação existente | Lacuna observada |
|---|---|---|
| Registro | agente::responder salva JSON, depois memory::record; só user/assistant sem segredos e até 64 KiB são espelhadas | Disputa com manutenção pode omitir o espelho; erro visível, sem diagnóstico estruturado |
| Extração | Store::record + classify, prefixos explícitos como Prefiro/Decisão/Tarefa | Frase com vocativo Lucille e projeto fictício não reconhecida; atualização de editor sem chave semântica |
| Revisão | memory_action save/list/delete + painel Memória | Aprovar substituição exige editar estados separadamente |
| Consolidação | action consolidate, até 20 lotes de 20 mensagens, checkpoint por conversa | Somente manual; commits intermediários antes do checkpoint |
| Recuperação | Store::retrieve, active apenas, FTS5 + preferências/restrições duradouras, escopo global/projeto e orçamento | Motivo de resultado vazio e custo da recuperação pouco visíveis |
| Contexto | agente::rodar recupera antes da geração; dados delimitados em role user, IDs no histórico | TTFT começa no HTTP, sem medida separada da preparação de memória |
| Deduplicação | SHA-256 normalizado + tipo + projeto; fontes preservadas | Reprocessamento sem contagem explicativa |
| Contradições | fact_key em preferred-language/computer ou chave manual; itens conflict excluídos | Editor e nome de projeto dos cenários não possuem chave automática |
| Exclusão | tombstone privado + invalidação de resumos derivados + IDs monotônicos | Necessário ampliar testes de atualização, reexecução e recuperação |
| Projetos | tabela projects, documento contextual ativo, escopo fixado na conversa | Fluxo existente deve ser exercitado após reinício |
| Identidade | identity_profile SQLite + fallback AGENTS.local.md; nunca escrita pela extração | Manter comportamento e testar independência do backend |
| Privacidade | raiz LOCALAPPDATA/Lucille, WAL, guard de caminhos/gitleaks, pacote allowlist | Migrações existentes não criam snapshot prévio automaticamente; relatório sanitizado ausente |

## Direção da implementação

Preservar o pipeline determinístico e aprovação humana. Acrescentar consolidação automática opcional em lotes curtos somente fora de gerações ativas, checkpoints transacionais, rejeições persistentes e diagnósticos de códigos/contagens/tempos sem textos. Nenhum modelo extra, shell, download de pesos, API de memória ou alteração dos servidores externos.


## Fluxo implementado

Configuração auto_consolidation desligada por padrão. Quando ligada com extração habilitada, um timer local de 2 segundos processa no máximo cinco mensagens por passagem, somente sem geração ativa. Identidade, backend e ferramentas não são modificados. A CLI executa uma passagem ociosa após terminar um turno; o aplicativo usa um worker de baixo custo. Não há extração com LLM, downloads ou chamadas externas de memória.

Na consolidação automática, somente declarações reconhecidas como potencialmente duráveis podem gerar candidatos/resumo. Frases passageiras avançam o checkpoint sem criar memórias. A consolidação manual continua podendo propor resumos extrativos de trechos do usuário, por solicitação explícita. Cada lote une candidatos, resumo e checkpoint em savepoint transacional. Falha desfaz o lote; a próxima passagem retoma o mesmo checkpoint, sem saltar fontes. Encerrar o processo durante um lote deixa o SQLite concluir ou reverter a transação. Interromper pausa o modo automático até salvar opções ou reiniciar. Uma nova geração interrompe o avanço entre lotes; um lote já iniciado termina. A extração permanece supervisionada: nada é promovido automaticamente a active.

Mensagens disputando o mutex com manutenção geram referências duráveis na pasta privada pending-records: somente ID da conversa e ordinal, apontando para o JSON já salvo. A ociosidade relê no máximo quatro referências por passagem, sem copiar conversas inteiras para RAM e sem bloquear o stream. Reiniciar recupera as referências pendentes mesmo com consolidação automática desligada. A gravação é idempotente; se houver crash após inserir no SQLite e antes de retirar a referência, a próxima passagem deduplica. Um JSON ausente/corrompido exige intervenção; a referência passa a .failed e o diagnóstico não contém o texto. Históricos originais não são modificados. O controle de geração é por processo; não se recomenda executar várias janelas/CLIs de conversa na mesma pasta privada simultaneamente.

Frases explícitas reconhecidas incluem o cenário Girassol com vocativo Lucille, preferências de editor e suas atualizações, objetivos, tarefas, decisões, soluções e restrições autorizadas. Declarações especulativas não são extraídas. Conteúdo produzido pela assistente não gera candidatos. Possíveis credenciais e alguns padrões de informação sensível (CPF, e-mail, diagnóstico etc.) não são espelhados automaticamente; a heurística é conservadora e incompleta. Históricos JSON mantêm a política original e podem conter a mensagem; não os publique.

Contradições usam chaves determinísticas por domínio e projeto. O botão Usar esta versão confirma atomicamente a nova versão e torna as anteriores obsoletas. Sem autorização explícita de substituição, aprovar um conflito falha. A resolução não infere a verdade sozinha. Novas formulações semanticamente equivalentes fora das regras podem exigir revisão/chave manual. A exclusão mantém tombstones e desativa resumos derivados; não apaga conversas ou backups. IDs preservam o maior valor já alocado inclusive ao restaurar um snapshot anterior da mesma base.

## Migração e dados existentes

Schema 4 adiciona opção automática, estados de processamento e diagnósticos. Antes de migrar qualquer base existente suportada, cria snapshot consistente via API SQLite backup, valida integridade, chaves e schema e só depois migra. Se backup/validação falhar, a atualização não prossegue. Bases novas não têm dados a copiar. Backups schemas 1–4 podem ser validados e migrados na restauração; a restauração continua exigindo confirmação explícita e cópia de segurança da base atual. Nunca execute uma versão antiga sobre a base nova. Nesta fase os testes só abrem fixtures; nenhum banco real do usuário é migrado por comandos de desenvolvimento.

## Observabilidade e privacidade

O painel mostra estados, última consolidação, fila pendente, opções automáticas, obsoletas e recuperações recentes. Não encontrar informação durável é sucesso normal, explicado à parte de falhas. As últimas 200 ocorrências técnicas guardam apenas códigos internos, quantidades, duração e data. Relatório sanitizado agrega códigos/contagens/tempos e flags operacionais, sem caminhos, IDs, nomes de projeto, perfil, textos ou consultas. Códigos desconhecidos são substituídos por unknown_code, inclusive em banco modificado externamente. Contadores de erros de abertura/SQLite/migração/disputa existem também em RAM e são reiniciados com o processo. Se a base não abrir, a interface apresenta erro sanitizado; não exporta seu conteúdo.

O relatório técnico é salvo somente em pasta privada technical-reports, protegida pelo guard de caminhos e .gitignore. Os dados permanecem em LOCALAPPDATA/Lucille. O endpoint configurado recebe somente o contexto aprovado e selecionado como parte da conversa; não há API separada de memória. Controle por permissões do Windows, sem criptografia SQLite própria. Exportações completas e backups continuam privados.

## Medições e testes

Cada resposta guarda memoria_recuperacao_ms: tempo observado da chamada de recuperação, incluindo abertura/seleção/auditoria, antes do HTTP. O TTFT existente continua medido desde o envio HTTP e não inclui essa preparação, identidade, descoberta do backend ou início do processo. O painel de memórias usadas mostra a medida separada. Rodadas de ferramentas repetem a referência ao mesmo custo inicial; não some esse valor por rodada.

scripts/test-release-identity.py valida identidade, arquivo legado, métricas e persistência usando a API real de memória da CLI, via JSON em stdin (sem SQL de aprovação). scripts/test-release-reliability.py executa cenários A–F em processos novos, ambos backends sintéticos e seis amostras de latência (três com recuperação desligada e três ligada), relatando medianas observadas de recuperação, TTFT HTTP e duração CLI. Não estabelece metas nem conclui causalidade a partir dessa amostra pequena; exclui o custo de GGUF, GPU e interface. O fixture é novo e privado; seu conteúdo nunca entra no ZIP. O teste só afirma que o contexto enviado inclui a memória; a resposta semântica de um modelo real exige aceitação manual.

## Roteiro manual — Windows 10 real

1. Preserve cópia privada da instalação anterior. Use Iniciar-ORDIS-Teste.ps1 para pasta isolada. Confirme o snapshot pré-migração ao abrir uma cópia sintética schema 3; restaure o snapshot sintético e verifique integridade antes de testar dados próprios.
2. Configure Lucille em Identidade e o endpoint externo existente, sem mudar contexto/parâmetros nem parar o servidor.
3. Ative extração e recuperação. Deixe consolidação automática desligada inicialmente. Diga: Lucille, o nome do meu projeto fictício é Girassol Azul. Confira candidato e fonte; aprove.
4. Feche/reabra, inicie nova conversa e pergunte qual é o projeto fictício. Confira nome na resposta e memórias utilizadas. Verifique que a identidade permanece Lucille.
5. Diga: Lucille, o nome do meu projeto fictício é Girassol Verde. Confira conflito e ausência de ambos como fatos confirmados até revisar. Use esta versão para Verde; gere outra resposta e confira somente Verde.
6. Exclua Verde, consolide e reabra nova conversa. O item não deve voltar. O histórico original permanece, conforme aviso de exclusão.
7. Faça também o ciclo Prefiro o editor Alfa / Passei a preferir o editor Beta. Resolva para Beta; confirme que Alfa fica obsoleto.
8. Cadastre projeto fictício, objetivo, decisão técnica e próxima tarefa, selecione-o antes de nova conversa. Reinicie; consulte tarefa e teste outro projeto para verificar isolamento.
9. Alterne para TabbyAPI e de volta ao llama.cpp. O banco não depende do modelo antigo. Não inicie um Tabby gerenciado se isso interferir no servidor externo ou na VRAM; use endpoints externos independentes.
10. Ative consolidação automática, converse e acompanhe fila/estado após a geração. Cancele um lote, salve opções para retomar; feche/reabra e confirme checkpoint. Teste desativação da extração e da recuperação independentemente.
11. Escreva frase passageira; confira sem informação durável como resultado normal. Use somente CPF/e-mail/credenciais fictícios para testar omissão. Exporte diagnóstico sanitizado e confira ausência dos textos.
12. Valide streaming longo, cancelamento, ferramentas, ComfyUI e latência com o GGUF/hardware reais. Revise o pacote, git ls-files e o guard de privacidade. Nunca envie bases, conversas, snapshots, relatórios privados ou o prompt pessoal ao GitHub.

## Preparação da Fase 4 (documentação apenas)

Futuro agente poderá consumir as APIs tipadas de memória: record/retrieve, projeto e próxima tarefa, aprovação/substituição e IDs rastreáveis. Qualquer execução ou edição futura exigirá política de permissões própria, isolamento de workspace, auditoria sanitizada, confirmação de operações destrutivas e distinção de dados recuperados de instruções confiáveis. Memórias não concedem autorização de shell/Git. Esta fase não adiciona ferramentas de shell, edição autônoma de arquivos ou automodificação.
