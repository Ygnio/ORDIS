# Fase 3 — memória privada

## Instalação e localização

Use o artifact `ORDIS-Windows-Test-Fase-3` do GitHub Actions. Extraia em uma pasta nova e execute `Iniciar-ORDIS-Teste.ps1`. O lançador e as configurações existentes são preservados. WebView2 continua sendo requisito da interface.

Banco de teste: `%LOCALAPPDATA%\Lucille\ORDIS-Teste\runtime\memory\lucille.sqlite3`.

O aplicativo reutiliza `Caminhos.runtime` quando projeto e runtime estão dentro da árvore privada `%LOCALAPPDATA%\Lucille`. Ao executar uma cópia a partir de um checkout ou outra pasta, usa `%LOCALAPPDATA%\Lucille\ORDIS\runtime\memory\lucille.sqlite3`. A seção Memória mostra o caminho efetivo. Sem LOCALAPPDATA ou com pasta privada inválida, a memória falha de forma explícita e o chat continua com JSON. Não se cria banco no repositório.

Backups ficam em `memory\backups`, exportações em `memory\exports`, junto do banco. Não copie essas pastas para Git, artifacts ou serviços externos. SQLite não é criptografado: privacidade depende das permissões da conta Windows e da proteção do disco. As chaves de backend continuam no armazenamento DPAPI existente.

## Arquitetura e migrações

`app/src/memory.rs` não depende do backend nem executa processos, ferramentas, SQL fornecido pela interface ou requisições de rede. SQLite é compilado com `rusqlite/bundled` e FTS5; não requer instalação SQLite no computador do usuário. Não há embeddings, downloads de modelos, uso de GPU ou dependência AVX2 específica.

- Migração 001: `conversations`, `messages`, `memories`, `projects`, `sources`, `change_history`, `retrieval_audit`, `settings`, `consolidation`, `schema_migrations`.
- Preferências, decisões, resumos e tarefas interrompidas são categorias de memória, também expostas por views SQLite: `preferences`, `decisions`, `summaries`, `interrupted_tasks`.
- Migração 002: índice FTS5 com normalização de acentos e triggers de inserção, atualização e exclusão.
- Migrações usam transação e `PRAGMA user_version`; versões futuras são recusadas. Fontes são preservadas; o histórico de alterações registra identificadores, ação e data, sem copiar o conteúdo excluído.
- WAL, sincronização FULL, chaves estrangeiras, timeout de contenção de 100 ms e cache de 2 MiB. A conexão é curta e as operações locais são serializadas. Falhas de memória não interrompem a resposta do backend.

O histórico JSON permanece no local existente, sem importação, migração ou exclusão automática. Apenas novas mensagens de usuário e texto final da assistente são espelhados no SQLite. Raciocínio, imagens e resultados de ferramentas não são copiados. Mensagens com possíveis segredos ou acima de 64 KiB são omitidas do novo espelho. O JSON original segue sua política anterior, portanto pode conter conteúdo sensível digitado pelo usuário.

## Pipeline

Entrada → análise local → classificação conservadora → candidato → deduplicação normalizada por conteúdo/categoria/projeto → revisão → FTS5 → seleção por orçamento → contexto delimitado do backend ativo.

A extração reconhece prefixos como `Prefiro`, `Meu computador`, `Decidimos`, `Solução:`, `Objetivo:`, `Tarefa:`, `Restrição:`, `Projeto:` e `Lembre que`. Conversa passageira não vira memória durável automaticamente. Todos os itens extraídos começam como candidatos; a interface permite aprová-los, editar ou excluir. Hipóteses possuem categoria e origem próprias; não são promovidas por consolidação.

Chaves de fato iguais no mesmo projeto com conteúdo diferente geram contradição. Algumas chaves simples são detectadas automaticamente (idioma preferido/computador); outras devem ser informadas na revisão. Para resolver, marque a versão antiga como obsoleta e aprove a atual. Detecção semântica geral não está implementada.

A recuperação considera texto (BM25), recência, importância, categoria, projeto e origem. Recupera apenas itens ativos gerais e do projeto selecionado; sem projeto ativo, não mistura memórias de outros projetos. A interface permite filtrar todos os projetos para revisão. Auditoria privada guarda IDs recuperados, custo estimado e data, com retenção de 1000 consultas, sem repetir perguntas ou conteúdo.

O orçamento configurável é de 128 a 4096 tokens **estimados**. A estimativa conservadora usa bytes/2 mais overhead e inclui o envelope completo; não é contagem pelo tokenizer do modelo. Itens grandes que excedem o orçamento são omitidos. O contexto é uma mensagem de dados de usuário, codificada como JSON com delimitadores escapados, antes do pedido real. Nunca é uma mensagem de sistema, ferramenta ou chamada executável. Os validadores das ferramentas existentes continuam sendo a fronteira de permissão. Prompts não garantem imunidade do modelo a toda injeção; revisar fatos e não guardar instruções continua necessário.

Memórias selecionadas são enviadas ao **backend de conversa configurado** como parte da inferência. Para operação inteiramente local, configure TabbyAPI/llama.cpp local. Não há serviço externo novo nem API de embeddings. Não se alteram parâmetros, limites anunciados ou processos do llama.cpp. A interface `LocalEmbeddingIndex` reserva uma extensão local futura, ainda sem implementação.

## Interface e projetos

Abra Memória (Ctrl+5). Busca, filtros, origem, datas, CRUD, aprovação e estatísticas são funcionais. Recuperação e extração têm controles independentes. Selecione o projeto antes de iniciar uma nova conversa. A primeira mensagem registrada fixa sua origem; reabrir uma conversa mantém esse projeto, mesmo após mudar a seleção padrão. Projetos guardam objetivo, repositório, branch, arquivos, arquitetura, problemas, soluções, testes, próximas tarefas e estado de retomada como texto, sem executar Git, terminal ou modificar código.

Consolidação é manual, em segundo plano, limitada a 20 conversas por lote e interrompível entre conversas. Gera resumos **extrativos** dos últimos 20 trechos de usuário (até 3000 caracteres), candidatos com referências à fonte. Reaplica a classificação nesses trechos, identificando decisões e outros fatos explícitos mesmo quando a extração estava desabilitada, deduplicando sem sobrescrever aprovações existentes. Não é resumo semântico por LLM. A repetição sem mensagens novas não duplica o resumo. Não apaga fontes nem lê históricos JSON antigos. Revise decisões extraídas, associe projetos e marque fatos obsoletos manualmente.

## Exportação, backup e restauração

Exportação JSON inclui todas as tabelas de dados da memória, em um snapshot transacional local. Não exporta FTS duplicado nem arquivos JSON legados. A interface limita consultas a 200 resultados, mas exportação e backup são completos.

Backup usa a API online SQLite, consistente mesmo com WAL; não copie somente o arquivo principal enquanto o app estiver aberto. A restauração exige selecionar um snapshot local, validar integridade/schema/chaves estrangeiras/versão e hash, confirmar explicitamente e criar backup de segurança da situação atual. Arquivo modificado após validação é recusado. Restauração não altera históricos JSON, chaves ou parâmetros de backend. A primeira versão restaura snapshots compatíveis do diretório privado, não importa JSON nem bancos de terceiros. Limite de validação: 512 MiB.

Excluir uma memória elimina seu registro/indexação/fontes da memória; fontes de conversa e backups anteriores permanecem. Exclusão não promete apagamento forense de WAL, backups, SSD ou histórico JSON. Sem retenção automática de conversas: o usuário controla as cópias locais.

## Validação Windows 10 real

1. Confirmar que launcher preserva dados/configuração e llama.cpp externo continua ligado.
2. Abrir Memória e conferir caminho privado de teste, schema 2 e estatísticas.
3. Criar memória sintética, buscar sem acentos, editar e conferir o índice; reiniciar e confirmar persistência.
4. Dizer `Prefiro respostas curtas`; revisar candidato e aprovar. Reabrir outra conversa sobre respostas curtas e conferir uso contextual. Desabilitar recuperação e repetir.
5. Desabilitar extração mantendo recuperação; conferir que a opção é independente.
6. Criar dois projetos sintéticos e confirmar isolamento, campos de retomada e conflitos por chave.
7. Validar streaming, tool calling e cancelamento com TabbyAPI e llama.cpp. Confirmar ComfyUI e proteção dos processos externos.
8. Criar backup, exportar JSON, editar um fato, cancelar restauração e depois confirmar restauração. Conferir preservação dos JSON antigos e backup de segurança.
9. Consolidar e interromper lote; revisar resumo/candidatos/fontes. Não usar dados reais em screenshots ou relatórios públicos.
10. Medir latência/RAM no i7-3770 e testar carga com o GGUF real. Contexto 65.536 é pretendido, não presumido ou reconfigurado.

## Recursos e limites

Sem RAM/VRAM de segundo modelo. Estimativa: cache SQLite de 2 MiB por conexão, overhead da biblioteca e resultados limitados; WAL/FTS e tamanho em disco crescem com os dados. Exportação escreve linhas incrementalmente. Backups e restauração são ações explícitas e podem aguardar acesso ao armazenamento. Medições de consumo/latência no Windows 10 e comportamento factual do modelo real continuam pendentes. CI usa Windows Server 2022 GNU/MSVC, não substitui validação de CPU sem AVX2, WebView2 e drivers reais.

Testes sintéticos cobrem criação/migração/reabertura/FTS/classificação/dedup/conflitos/CRUD/orçamento/projetos/troca de backend/SSE/cancelamento/backup/restauração/rollback/privacidade/segredos/JSON legado/contexto não privilegiado. O workflow executa Rust nos dois toolchains, Python, Node, formatação, launcher e varredura de credenciais antes de gerar o ZIP.

## Trabalho de interface independente

Adendo autorizado: métricas por resposta (tokens de geração, tempo total, TTFT/tempo de geração separados, taxa, dados reais de usage/timings com estimativas identificadas, progresso discreto, cancelamento, persistência JSON e controle de exibição). Será implementado em branch/PR separados da memória, sem merge automático ou alteração dos processos de inferência. Não faz parte do diff funcional desta Fase 3.
