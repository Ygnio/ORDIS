# Lucille — Fase 3.4

Base: Fase 3.3, sobre os PRs internos #6, #7 e #8. Nenhum merge automático.

## Visão

Em Configurações → Visão, habilite imagens. PNG, JPEG e WebP são decodificados
com limites nativos, orientação corrigida, redimensionados e regravados como PNG.
EXIF e outros metadados são removidos sempre. Até quatro anexos por mensagem,
1–20 MiB por arquivo, no máximo 32 milhões de pixels na origem, lado de até
16.384 pixels e alocação de decodificação limitada a 128 MiB. Lado de saída
configurável de 256–4096 pixels. O processamento verifica cancelamento entre
etapas; a biblioteca de decodificação não é interrompida no meio de uma etapa.

Seleção de arquivo, arrastar/soltar e colar usam o mesmo importador. A miniatura
é removível antes do envio. O servidor precisa anunciar imagens. Os anexos só
podem ser enviados a endereço loopback literal; o transporte não usa proxies.
Não se presume igualdade de capacidades entre TabbyAPI e llama.cpp. Em TabbyAPI,
habilite também Visão na configuração específica do modelo.

Captura abre o seletor nativo `ms-screenclip:` do Windows, após confirmação.
Selecione uma região/janela, cole com Ctrl+V, revise a miniatura e envie.
Esc cancela no seletor; remover miniatura cancela o anexo. Não há monitoramento,
gravação contínua nem leitura automática da área de transferência. A disponibilidade
da Ferramenta de Captura deve ser verificada na instalação do Windows 10.

Anexos ficam junto ao perfil privado, em `runtime/memory/vision`, nunca no
workspace de código. JSON de conversa guarda referências, sem Base64. Retenção
padrão 0 apaga anexos no fechamento normal e na próxima abertura do aplicativo,
inclusive após falha.
Pode-se escolher 1–30 dias. Expiração é aplicada na leitura, importação, fechamento e abertura.
Referências históricas expiradas ficam indisponíveis; não se inventa seu conteúdo.
Mensagens anteriores com anexos retidos podem voltar ao contexto, com limite
configurável de 0–8 mensagens anteriores, além da mensagem atual. Imagens geradas
pelo ComfyUI preservam seu fluxo anterior; não são automaticamente reanexadas.

O orçamento textual continua estimado; foi removido o custo fixo de 1200 tokens
por imagem. Uso real é o retornado pelo backend. O servidor pode recusar contexto
excessivo: 65.536 tokens configurados não certificam capacidade multimodal plena.

O diagnóstico diferencia capacidade anunciada de teste sintético confirmado.
“Testar com imagem sintética” envia somente formas geométricas conhecidas; exige
resposta com cores/formas corretas e salva apenas metadados/tempos privados.
Somente essa requisição sintética usa temperatura zero e enable_thinking=false;
conversas normais preservam suas preferências de raciocínio.
Não estima VRAM, projetor ou tempo exclusivo do encoder sem telemetria confiável.
Troca de pesos sob mesmo alias exige novo teste. A auditoria externa do servidor
complementa esse diagnóstico.

## Memória semântica

Provedor opcional `local_http`, compatível com `/v1/embeddings`. Configuração
persistente no SQLite privado: endpoint HTTP loopback literal, ID/versionamento
do modelo, caminho informativo, dimensão e prefixo de consulta. O caminho **não
executa** arquivos. O aplicativo não inicia servidores, não baixa pesos e não
reserva GPU. O usuário mantém o provedor separado ligado quando quiser usar
semântica. Falha, dimensão incorreta ou provedor desligado mantém o FTS5.

Exemplo manual com pesos já disponíveis, usando um build compatível com a CPU:

```powershell
& 'C:\Ferramentas\llama.cpp\llama-server.exe' `
  --model 'C:\Modelos\Qwen3-Embedding-0.6B-Q8_0.gguf' `
  --alias lucille-embedding-qwen3-0.6b-q8-v1 `
  --embeddings --pooling last --gpu-layers 0 --threads 4 `
  --ctx-size 4096 --batch-size 512 --ubatch-size 512 --parallel 1 `
  --host 127.0.0.1 --port 8081
```

Para esse modelo: dimensão 1024; prefixo de consulta
`Instruct: Given a search query, retrieve relevant personal memories.\nQuery: `
(quebra de linha real na caixa de configuração). Documentos não recebem prefixo.
O GGUF Q8 existente tem cerca de 610 MiB; execução CPU requer RAM adicional
para buffers/KV. Não implica autorização para baixar outros pesos. FTS5 sem
embeddings não requer modelo adicional. Modelos menores multilíngues com endpoint
compatível são alternativas, mas exigem escolher o prefixo/dimensão corretos e
medir desempenho no hardware real; não foram certificados por nome.

O servidor principal de conversa não deve ser reconfigurado só para embeddings.
Seu projetor visual não é um indexador textual. A ausência de `/v1/embeddings`
nesse servidor não constitui defeito da implementação de memória.

## Índice e confiança

Migração aditiva 006, backup automático antes da migração. IDs e textos existentes
preservados. Vetores normalizados são armazenados no próprio SQLite, com ID,
revisão, modelo, dimensão e data. Fila persistente mantém estado, tentativas,
erro genérico e próxima tentativa. Triggers invalidam imediatamente vetores ao
alterar texto, aprovação, estado, escopo ou classificação. Exclusão faz cascade.
Trocar endpoint, ID, dimensão ou caminho invalida o índice; alterar pesos no
mesmo caminho exige **mudar ID** ou usar Reindexar. O endpoint é de confiança local:
o aplicativo não prova a identidade criptográfica de pesos servidos externamente.

Worker processa uma memória por ciclo ocioso de 2s, sem reprocessar entradas
prontas. Inference ocorre fora da trava de SQLite, com deadline de 3s. Commit
condicional confere configuração, revisão, estado e texto; resultados antigos
são descartados após alterações/cancelamento. Falhas retornam à fila após 30s.
Durante geração, worker pausa; cancelamento descarta o resultado antes do commit.

Consulta usa cosseno exato, até 10.000 memórias, sem extensão nativa/ANN. Combina
top 100 lexical e top 100 vetorial por RRF (k=60), limiar de cosseno 0,25, IDs
deduplicados, fontes `lexical`, `vector` ou `both`. Preferências/instruções
duradouras incluídas pela prioridade anterior usam `contextual` quando não
correspondem à busca. A origem persiste no histórico de cada resposta. Categorias/projetos, estado
confirmado e validade são filtrados antes da fusão. O orçamento e o invólucro
de dados não confiáveis são preservados. A busca atual faz uma chamada local
limitada a 3s durante a recuperação e pode aumentar TTFT; não confundir essa
latência com velocidade de geração. Coleções acima do limite precisam avaliação
de índice dedicado. Ajuste de limiar/qualidade exige corpus representativo.

`memory_search` usa o mesmo ranqueador. Aprovações, bloqueios de recriação,
esquecimento, políticas e identidade da Fase 3.3 permanecem nativos. Embeddings
não concedem permissões. Propostas `source: visual` exigem registro nativo de
anexo na última mensagem e categorias knowledge/experiences; recebem prefixo de
hipótese, nunca aprovação automática nem alteração direta de identidade.
Texto encontrado em imagem não é confirmação humana. Evidência de correção real
deve vir dos resultados nativos do Desenvolvedor, quando disponível.

## Auditoria e validação

Auditoria local inicial: arquitetura GGUF `gemma4`, projetor `clip` com encoder
visual e nome compatível, modelo auxiliar `gemma4-assistant`. Servidor
`0.4.1-dev`, build `11064`, commit `a894dae93`, Clang 20.1.8 x86_64.
Parâmetros observados: contexto 65536, projetor offload, GPU layers 999,
flash attention on, KV q8_0/q8_0, batch 512/ubatch 128, parallel 1,
Jinja, draft-mtp com até 2 tokens. Servidor permaneceu ligado e inalterado.
Teste sintético inicial reconheceu formas e texto; timings mostraram tokens
draft e aceitos. `/v1/embeddings` do servidor principal respondeu 501.

Roteiro real em perfil sintético isolado:

1. Testar visão sintética e medir tempo, usage e MTP.
2. Habilitar anexos; enviar PNG/JPEG, múltiplos anexos e texto. Testar arquivo
   malformado, cancelar, remover miniatura e backend sem suporte.
3. Capturar só janela sintética pelo seletor, colar/revisar/enviar; testar Esc.
4. Reter anexo por um dia, reabrir e referenciá-lo; repetir com retenção 0.
5. Aprovar preferência fictícia sobre criação de personagem/classes; comparar
   consulta “Que tipo de RPG costuma combinar comigo?” com FTS5 e híbrida.
6. Editar/reclassificar/trocar modelo, confirmar fila e invalidação; excluir e
   confirmar ausência lexical/vetorial. Desligar provedor, confirmar fallback.
7. Testar memória ativa e agente Desenvolvedor, incluindo recusa, sem permitir
   que screenshot autorize ferramentas. TabbyAPI/ComfyUI reais requerem seus
   servidores e modelos disponíveis; mocks não certificam esses servidores.
8. Medir RAM/VRAM, latência de indexação/recuperação, processamento visual e TTFT.

CI executa GNU original, MSVC separado, Python, JavaScript, Gitleaks e guardas de
privacidade, incluindo aceitação em binários empacotados. Somente fixtures sintéticos.
Resultados completos, medidas e pendências ficam no relatório de entrega/PR.

Referências primárias: [llama.cpp servidor na revisão auditada](https://github.com/ggml-org/llama.cpp/blob/a894dae93/tools/server/README.md),
[multimodal](https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md).

Especificação do prefixo e dimensões: [Qwen3-Embedding-0.6B](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B).
