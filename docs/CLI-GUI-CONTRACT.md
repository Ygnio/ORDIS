# Contrato CLI/GUI — Fase 8.1

## Entrada humana e compatibilidade

`ordis-cli conversar` recebe primeiro um objeto JSON, compacto ou multilinha:

```json
{"text":"Pedido sintético","attachments":[]}
```

`id` opcional reabre uma sessão existente. O primeiro objeto termina ao fechar suas chaves;
não é necessário fechar stdin. Consumidores anteriores que enviam o objeto e EOF continuam
funcionando. EOF **nunca** aprova ferramentas. A saída existente de texto e linhas
`[developer] {evento}` / `[sessão ID]` é preservada; não foi convertida silenciosamente em JSONL.

Quando o evento nativo `awaiting_approval` apresentar um ID, o controlador humano pode enviar
uma linha JSON no mesmo stdin:

```json
{"type":"approval","id":"ID EXATO DO EVENTO","decision":"allow"}
```

`deny` recusa. Campos extras, outras decisões e IDs de operações encerradas são rejeitados.
Uma decisão recebida antes da abertura ou depois do prazo não vale para outra operação.
Texto do modelo, raciocínio, resultados de ferramentas e stdout jamais são lidos como consentimento.
O ID é criado nativamente; `developer_decide` não é ferramenta oferecida ao modelo.

`perguntar "texto"` mantém o modo legado: `permitir` digitado **durante a solicitação vigente**
aprova somente essa operação; outro texto recusa. Também aceita os quadros JSON com ID.
Esse modo legado é exclusivo de `perguntar`; `conversar` exige o protocolo com identificador.

A configuração compartilhada `approval_timeout_seconds` admite 1–600 segundos (padrão 600),
editável em Configurações/Desenvolvedor. A GUI e a CLI usam o mesmo prazo nativo, iniciado
antes de publicar o evento. ID expirado ou repetido é recusado pelo núcleo mesmo que a
interface ainda mostre o cartão. Timeout, EOF ou recusa bloqueiam novas mutações nesse turno.

## Cancelamento

Ctrl+C/Ctrl+Break em uma conversa da CLI sinaliza o mesmo cancelamento usado pela GUI.
`conversar` também aceita `{"type":"cancel"}` em stdin. O sinal não encerra o processo
abruptamente: o núcleo preserva resposta parcial, estado e métricas disponíveis e impede
ferramentas pendentes. O handler não mata processos, não envia sinais ao servidor e não chama
controladores de serviços. A leitura inicial do objeto também responde ao cancelamento.

O teste de Ctrl+C cria um console separado e oculto exclusivamente para sua CLI sintética.
Um auxiliar confirma que apenas ele e essa CLI pertencem ao console antes de enviar o evento.
Nunca transmitir Ctrl+C ao console do Codex ou a processos externos.

## Dados e configuração

| Recurso | Contrato comum |
| --- | --- |
| Backend | `runtime/inference/backend.json`, validação nativa `backends`; chaves continuam DPAPI |
| Geração | `runtime/inference/generation.json`, mesmos perfis e validação `generation_policy` |
| Identidade | ações `identity_get`/`identity_save` de `memory`, mesmo banco e precedência |
| Memória | mesmo SQLite, escopo de projeto, aprovação e recuperação; sem fusão Python/Rust |
| Histórico/métricas | sessões JSON do mesmo núcleo `agente`, incluindo parciais e cancelamento |
| Anexos | mesmo módulo `vision`, limites e remoção de metadados |
| Desenvolvedor | permissões e diário privados compartilhados, aprovação por operação, sem Git autônomo |

Na CLI, `configuracao generation load|save` e `configuracao backend load|save` usam as
mesmas funções nativas da interface. Para salvar backend, enviar
`{"value":{"selected":"llama_cpp","llama_url":"http://127.0.0.1:8080"}}`;
`api_key` é opcional e nunca é impresso. `configuracao settings load` consulta preferências.
Gerenciamento de serviços continua sujeito aos controles existentes da GUI: este comando não
oferece alteração de gerenciamento. Identidade/memória usam `memoria <ação>`; desenvolvedor
usa `desenvolvedor load|save|history`; visão usa `visao load|save|diagnostic`.
Consultas/salvamentos recebem JSON até EOF, pois não mantêm uma conversa interativa aberta.

A GUI mantém seu worker ocioso; a CLI executa a mesma manutenção após uma conversa concluída,
e a omite após cancelamento. Consolidação automática e embeddings só funcionam se explicitamente
habilitados no perfil. Esta fase não os habilita nem migra bancos. Evitar alterações concorrentes
de configuração por duas interfaces durante uma tarefa; persistência compartilhada não fornece
controle transacional de múltiplos processos sobre arquivos JSON.

## Distribuições e rastreabilidade

* **ORDIS genérico:** configuração ancestral `config/paths.json` ou `ORDIS_PROJECT`; interface
  e identificação ORDIS preservadas. Não se instala sobre Lucille.
* **Lucille oficial:** feature `official-interface`; ambos descobrem
  `%LOCALAPPDATA%/Lucille/Interface`, com runtime relativo, sem redirecionamento por junctions
  nem importação automática. GUI protege instância única e cache WebView no perfil.
* **Lucille staging:** feature `staging-interface` inclui a oficial, mas exige
  `LUCILLE_STAGING_ROOT` igual ao LOCALAPPDATA sintético e marcador `synthetic-staging.txt`
  contendo `ORDIS synthetic staging` mais LF. Sem isso, recusa abrir perfil. Não existe
  bootstrap que crie perfil, copie conversas ou altere a instalação operacional.

O perfil de staging deve ser preparado explicitamente fora dos binários. O inicializador
`Start-Lucille-Staging.ps1 -SyntheticLocalAppData <raiz>` valida hashes e usa ambiente somente
do processo, sem iniciar serviços. `-Cli <comando>` seleciona a CLI. Ele não cria nem importa dados.
Não escolher o LOCALAPPDATA verdadeiro para staging. O marcador não é uma barreira contra
um operador malicioso: serve para impedir abertura acidental sobre dados reais.

GUI e CLI aceitam `--version` sem acessar perfil. Configurações da GUI também mostram versão,
SHA e target. O build registra Git HEAD e estado dirty, target e distribuição. O empacotador
`scripts/package-paired.py` exige checkout limpo e metadados idênticos em **ambos os executáveis**;
recusa diferenças e copia somente uma lista explícita de arquivos públicos. Manifesto registra
SHA-256, tamanhos, Cargo.lock e commit; `SHA256SUMS` inclui o manifesto.

Exemplo após compilar ambos com as mesmas features e ferramentas:

```powershell
python -B scripts/package-paired.py --bin-dir D:/build/target/x86_64-pc-windows-gnu/debug --loader D:/build/WebView2Loader.dll --output D:/staging/lucille-paired --official-staging
```

A versão de produto é `2026.10.30-cli-gui-parity.1`; a versão Cargo/Tauri 0.1.0 é mantida para
compatibilidade. O alvo x86-64 padrão não exige AVX2. A presença do WebView2 Runtime é requisito
prévio; nenhum instalador é disparado pelo staging. Testes em Windows 10/i7-3770 e CI
Windows Server 2022 complementam-se; não são equivalentes a testar o Gemma real.

## Matriz de validação

`test-cli-control.py` cobre aprovação/recusa, EOF, timeout, ID inválido/repetido/expirado,
quadros adulterados, consentimento falso do modelo, cancelamento pendente e de streaming,
Ctrl+C real e persistência parcial. Executar com `--official-staging` para o par Lucille.
Os testes existentes de geração e pacotes cobrem os dois backends, ferramentas inválidas,
identidade, memória, anexos e métricas. Os testes Rust do núcleo são compilados para cada binário.

QA visual deve usar somente perfil sintético: conferir Configurações/versão/perfis, enviar
conversa, parcial e streaming longo; cancelar enquanto ativo; aprovar e recusar ferramenta
sintética; testar timeout; reabrir histórico e memória; anexar imagem sintética e observar falha
de comunicação controlada. Registrar separadamente o que foi observado na GUI e o que foi
comprovado por teste automatizado. Uma falha de captura não equivale a aprovação visual.

A instalação `2026.10.25-generation-guard.2` permanece intacta. Este pacote é staging;
ativação operacional, importações pessoais e Fase 8.2 exigem outra etapa autorizada.
