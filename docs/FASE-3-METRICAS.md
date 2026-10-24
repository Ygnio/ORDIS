# Métricas por resposta — adendo independente

Branch `feature/response-metrics`, PR separado tendo `feature/persistent-memory` como base. O diff de revisão contém somente métricas e seus testes/documentação. O ZIP de validação inclui memória e métricas. Nenhum dos PRs autoriza merge automático.

## Uso

Em Configurações → Aparência, use **Métricas por resposta** para mostrar/ocultar os indicadores. A preferência é persistida localmente como as demais preferências de aparência. A coleta é leve e permanece no histórico local mesmo com a exibição oculta. Não há novo modelo, biblioteca pesada, download, chamada de tokenização externa ou reinício do backend.

Cada geração da assistente, incluindo rodadas de ferramentas, possui seus próprios indicadores abaixo do conteúdo. O nome é o identificador real do modelo informado pelo backend (ou o solicitado caso o stream não informe outro), sem assumir que todos os modelos se chamam Lucille.

- **Tokens gerados**: exclusivamente `usage.completion_tokens`, com fallback para `timings.predicted_n`. Nunca se usa `prompt_tokens` nem `total_tokens`. Podem incluir raciocínio e tool calling contabilizados pelo servidor.
- **Total**: tempo local monotônico desde o início da solicitação HTTP até fim/cancelamento. Inclui espera, prefill e rede, mas não inicialização anterior do serviço, recuperação de memória ou execução de ferramentas entre solicitações.
- **TTFT**: tempo até o primeiro fragmento recebido de texto, raciocínio ou ferramenta. É observado no cliente, não um contador interno do servidor.
- **Geração (backend)**: `timings.predicted_ms`, se disponível, separada do total.
- **Após primeiro token (local)**: intervalo observado do primeiro fragmento até o término da solicitação, quando o tempo de geração do backend não está disponível. Não se afirma que seja tempo puro de decodificação.
- **tok/s**: `timings.predicted_per_second` real, ou derivado de tokens/tempo de geração do backend. Sem timings, a taxa local é explicitamente estimada com `(tokens−1)/(intervalo após primeiro fragmento)`, a partir de pelo menos 250 ms. Não se divide pelo tempo total incluindo prefill para apresentar velocidade de decodificação.
- **≈N tokens (estimativa local)**: quando não há contagem real, usa bytes da saída/4 arredondados. É uma heurística sem tokenizer, não contagem de caracteres/palavras apresentada como tokens reais. Pode ser imprecisa para português, raciocínio ou ferramentas. Se não houve saída, não se inventa contagem/taxa.

O stream solicita `stream_options.include_usage=true` pela API compatível com OpenAI. Ambos os backends são atendidos pelo mesmo parser. A disponibilidade efetiva de usage e timings depende da versão do servidor. Não se enviam parâmetros de carga/contexto nem se altera o processo. Versões antigas que recusarem essa opção precisam de validação de compatibilidade; não há repetição automática de uma geração recusada, para evitar duplicação.

O indicador discreto atualiza aproximadamente a cada 250 ms, inclusive durante espera pelos headers e entre chunks, sem repintar o texto da resposta. Os contadores globais do monitor de inferência não são usados para medir respostas individuais, pois podem incluir outros clientes.

## Persistência e cancelamento

`Mensagem.metricas` é um campo opcional no histórico JSON existente; inclui apenas nome do modelo, números, origem de cada medida e estado. Conversas antigas sem esse campo continuam abrindo e não recebem números calculados retroativamente. Reiniciar o ORDIS preserva os valores gravados, sem recalculá-los a partir do texto.

Cancelamento devolve estado `cancelled`, tempo local observado e TTFT se houver, sem fabricar contagem final de tokens ou taxa estimada. Uma contagem/timing parcial enviado pelo servidor permanece identificado como observado, não como total final. Cancelar durante espera pelos headers também interrompe o pedido HTTP; não encerra o processo llama.cpp/TabbyAPI. Cancelar antes de qualquer solicitação ao backend não cria artificialmente uma resposta gerada.

Um erro HTTP ou stream truncado continua sendo erro; não é convertido em conclusão bem-sucedida. Métricas já emitidas durante um stream interrompido podem aparecer na interface, mas um resultado que retornou erro antes de salvar a mensagem não vira resposta concluída no histórico.

## Validação

Testes sintéticos: separação entre prompt/geração/total, usage final sem choices, timings, estimativas explícitas, ausência de estatísticas, raciocínio/ferramentas, progresso SSE, conclusão, cancelamento antes da conexão/dos headers/durante SSE, persistência JSON e leitura do formato legado. Node testa apresentação e estados; Rust testa parser/tempos/persistência e mocks HTTP. CI executa GNU e MSVC, testes Python, JS, launcher e varredura de credenciais.

No Windows 10 real:

1. Usar o ZIP novo em pasta separada, mantendo armazenamento privado do lançador e llama.cpp externo ligado.
2. Testar com llama.cpp e TabbyAPI: resposta curta e longa, prompt longo, raciocínio e ferramentas. Conferir nome do modelo e distinção total/TTFT/geração.
3. Comparar completion_tokens/timings com a resposta da API ou a interface do próprio servidor, usando somente uma solicitação para evitar comparar gerações diferentes.
4. Cancelar antes do primeiro token e durante a resposta; verificar estado, ausência de tokens inventados e que o servidor continua em execução.
5. Reiniciar, reabrir conversa e conferir preservação de métricas. Abrir histórico antigo e verificar que não recebeu valores fictícios.
6. Ocultar/mostrar indicadores em Aparência e conferir persistência da preferência.

CI usa Windows Server 2022. Precisão do tokenizer, desempenho/latência e compatibilidade com a versão real do llama.cpp/Gemma/TabbyAPI e CPU sem AVX2 continuam exigindo validação local. Nenhuma conversa real, chave, banco ou backup é incluído no código/ZIP.

Referências da API: [llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md), [schema de streaming](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/server-schema.cpp), [TabbyAPI Usage](https://github.com/theroyallab/tabbyAPI/wiki/03.-Usage).
