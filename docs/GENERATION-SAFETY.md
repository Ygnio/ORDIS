# Contenção de geração no cliente Lucille

Implementação local sujeita a revisão. Não reconfigura nem reinicia llama.cpp, TabbyAPI,
ComfyUI ou seus processos. Limitar geração não corrige percepção visual nem garante uma
resposta final correta.

## Caminho da requisição

`agente::transmitir → backends::chat → generation_stream::request → SSE`.
`generation_policy` seleciona limites; `generation_protocol` valida frames, encerramento
e lotes de ferramentas; `response_metrics` preserva metadados no histórico JSON existente.
`geracao_fim` e `geracao_aviso` são avisos da aplicação, separados do texto do modelo.
O agente só executa um lote validado e terminado normalmente; cada chamada também
confere cancelamento antes de executar. Não há repetição automática da requisição.

## Configuração local

Configurações → Limites de geração. O arquivo privado é
`runtime/inference/generation.json`. Carregar padrões não cria arquivo. Salvar exige
ação explícita; um arquivo inválido causa erro e é preservado para revisão.

| Perfil | Tokens de geração | Total | Primeiro token | Inatividade |
|---|---:|---:|---:|---:|
| Conversa | 4096 | 300 s | 120 s | 90 s |
| Visão | 4096 | 300 s | 120 s | 90 s |
| Programação/respostas longas | 16384 | 900 s | 300 s | 180 s |
| Ferramentas/agentes | 8192 | 600 s | 180 s | 120 s |

4096 acompanha o fallback existente do cliente; os perfis extensos ampliam esse
orçamento em vez de impor o teto experimental de 1024 a toda tarefa. São valores
iniciais conservadores, sem alegação de calibração para todos os modelos/hardwares.
Tokens: 256–65536; total: 10–3600 s; primeiro token/inatividade: 5 s até o total.
O usuário pode escolher qualquer perfil explicitamente. Automático prioriza modo de
desenvolvimento/continuação de ferramentas, depois imagens, depois conversa. Não há
classificador de linguagem para detectar programação: selecione o perfil longo.

Limites explícitos de chamadores especializados prevalecem. O diagnóstico visual de
160 tokens e os diagnósticos de conversa/ferramentas mantêm seus limites próprios.
Os diagnósticos não streaming conservam o teto de 300 s, limitado pelo prazo local
se menor; não têm os três relógios independentes do streaming. TabbyAPI mantém seu
limite anterior e 900 s, com o tratamento seguro de encerramento compartilhado.
Nenhum campo de contexto, MTP, batch, ubatch, KV, projetor ou sampler global é enviado
pela política. `max_tokens` limita saída, não configura o contexto.

## Encerramento e ferramentas

- `stop`/`tool_calls`: conclusão só com protocolo terminal válido e conteúdo final ou
  lote completo. É necessário receber `[DONE]`.
- `length`: incompleta, mesmo se o JSON de uma chamada parecer completo.
- `no_final`: término sem resposta final e sem ferramentas válidas. Não inventa texto.
- `cancelled`: interrupção voluntária, com prioridade sobre encerramento coincidente.
- `timeout`, `first_token_timeout`, `idle_timeout`: relógios independentes. Raciocínio,
  texto e fragmentos de ferramentas contam como progresso; heartbeats não contam.
- `transport_error`, `backend_error`, `protocol_error`: falha distinta de conclusão.

Texto final parcial continua no histórico, identificado como incompleto. Nenhum lote
interrompido é executado. Exigem-se IDs únicos, nomes completos e argumentos JSON
objeto válidos; não se preenchem IDs ou argumentos ausentes. O HTTP não repete pedidos.
Resultados desconhecidos de ferramentas interrompidas continuam projetados como
interrompidos no contexto, sem reexecutar ações. Respostas incompletas não entram na
extração de aprendizado da memória; seu histórico continua preservado.

O transporte abandona a future/body ao cancelar. O runtime fecha a conexão
assincronamente; o aplicativo não mata o servidor. Os testes verificam EOF remoto
antes dos headers e após inatividade. Um servidor remoto que ignore desconexões não
pode ser forçado pelo cliente a liberar seu slot; valide a reação da build utilizada.

## Repetição e privacidade

Somente observação: até 16 KiB de cauda do raciocínio, verificação a cada 512 bytes,
blocos de 256/512/1024 bytes e pelo menos oito recorrências. O aviso não cancela.
Conteúdo com indícios de código, JSON, logs, listas, tabelas ou citações é excluído
conservadoramente. Não é detecção semântica: pode omitir loops e advertir sobre prosa
legitimamente repetida. Texto final e chamadas de ferramentas não ativam o detector.

Metadados novos: backend, perfil, limite aplicado, prazo total, `finish_reason`, estado,
presença de final e agregados de repetição, além de tokens/tempos já existentes.
Não há exportação de telemetria, log novo de prompts, raciocínio, imagens, chaves ou
argumentos. Corpos de erros de backend são omitidos. O recurso anterior de histórico
local de raciocínio não é expandido por esta mudança. Configuração e histórico são
privados, ignorados pelo Git. Fixtures usam apenas dados sintéticos.

## Orçamento nativo de raciocínio

A auditoria da build `a894dae93` identificou `reasoning_budget_tokens` e
`thinking_budget_tokens` em `tools/server/server-common.cpp`, com suporte do parser
Gemma 4. Isso não comprova que forçar o fechamento seja seguro para ferramentas ou
que produza final correto. Esta etapa não envia esses parâmetros, não desativa
thinking e não altera templates. Investigar sua interação com tool calling exige
experimento separado antes de torná-los configuração de produto.

## Validação

```powershell
cargo test --offline --locked --manifest-path app/Cargo.toml
cargo test --offline --locked --features transport --manifest-path tests/generation-harness/Cargo.toml
node --test scripts/test-*.cjs
python -B -m unittest discover -s tests -p 'test_*.py'
python -B scripts/privacy_guard.py
git diff --check
```

O harness compila os mesmos módulos de produção por `#[path]`; não simula o
transporte implementado. Seus mocks HTTP usam somente portas efêmeras em loopback.
O exemplo `local_validation` exige opt-in explícito, um pedido por execução, teto de
1024 tokens/45 s, e retorna somente metadados. Não o execute sem autorização,
servidor identificado e slot ocioso. Testes automatizados não iniciam esse exemplo.

Roteiro manual em instalação de teste isolada, após aprovação:

1. Conferir defaults/perfis, salvar um limite e reiniciar a instalação de teste.
2. Gerar resposta normal, longa e visual com imagem sintética, respeitando o perfil.
3. Forçar `length` e conferir aviso com/sem final; reabrir o histórico.
4. Cancelar, conferir botão/indicador liberados e iniciar outra solicitação.
5. Testar indisponibilidade, inatividade e prazo total; verificar estados distintos.
6. Simular lote truncado e lote válido; confirmar zero ações do truncado e uma
   execução por chamada válida, inclusive após cancelamento/reabertura.
7. Conferir TabbyAPI, ComfyUI e ferramentas reais em ambiente de teste apropriado.
8. Confirmar processos e configurações de operação intactos; varrer privacidade.

A implementação não foi integrada à instalação operacional. QA visual no WebView2,
ferramentas com efeitos reais e comportamento de modelos/backend diferentes devem
ser registrados separadamente dos testes de protocolo e dos mocks.
