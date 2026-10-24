# Auditoria inicial — ORDIS / Projeto Lucille

**Data:** 24/10/2026  
**Snapshot:** `d40c279` (`ORDIS: versão inicial pública`), branch `main`.  
**Escopo:** auditoria do repositório original para orientar a implementação no fork. O relatório não inclui configurações específicas do equipamento do usuário.

## Arquitetura observada

ORDIS é uma aplicação Windows em Rust 2021 + Tauri 2. A interface em HTML/CSS/JavaScript puro é servida pelo WebView2; o binário CLI reutiliza o crate Rust.

```text
UI (app/ui) ── comandos/eventos Tauri ── Rust (app/src)
                                         ├─ agente ─ HTTP/SSE ─ TabbyAPI
                                         ├─ imagens ─ HTTP/WebSocket ─ ComfyUI
                                         ├─ controle de processos locais
                                         ├─ sessões JSON em runtime
                                         └─ ferramentas, Proxmox e benchmark
```

Não há camada de abstração de inferência, cliente llama.cpp, SQLite ou memória de longo prazo. A configuração e descoberta de modelos de conversa dependem do TabbyAPI e do formato EXL.

## Componentes e achados

- **Agente:** `app/src/agente.rs` monta o contexto, chama diretamente `/v1/chat/completions` do Tabby, consome SSE, separa `reasoning_content`, acumula tool calls e executa até 12 rodadas. O HTTP usa `ureq` síncrono, timeout de conexão de 5 s e leitura de até 600 s. Cancelamento fecha o stream quando o loop observa a flag atômica; ainda não há teste contra servidor simulado.
- **Comandos e interface:** `app/src/main.rs` expõe comandos Tauri e eventos do agente. A UI sem build fica em `app/ui/`; os eventos de streaming são tratados por `agente.js`. A janela pede efeito Acrylic e transparência; Windows 10/WebView2 requer validação visual.
- **Modelos:** `app/src/modelos.rs` lê e reescreve chaves do YAML do Tabby, enumera modelos EXL e opções de ComfyUI. `app/src/benchmark.rs` também chama o Tabby diretamente.
- **Processos:** `app/src/controle.rs` gerencia Tabby e ComfyUI, registra processos iniciados pelo app em `runtime/state.json`, valida PID/tempo de criação/command line e recusa encerrar serviços externos. Um lock coordena painel, CLI, imagens e benchmark. A geração ComfyUI descarrega e restaura o modelo de conversa.
- **llama.cpp externo:** integração deve ser somente cliente HTTP. O lifecycle de inferência do ORDIS não deve iniciar, parar, reiniciar, registrar ou matar esse processo, inclusive durante a troca de GPU para ComfyUI.
- **ComfyUI:** `app/src/imagens.rs` envia workflows por HTTP, acompanha progresso por WebSocket e consulta histórico. Essa integração é separada do chat.
- **Ferramentas:** `app/src/ferramentas.rs` implementa leitura restrita ao workspace, busca Python, fetch web com validação de destino, consulta Proxmox somente leitura e geração de imagem. `proxmox.rs` usa DPAPI e CA configurada.
- **Conversas e identidade:** sessões em `runtime/agente/sessoes/*.json` guardam mensagens, reasoning, imagens e tool calls, gravadas via arquivo temporário. O prompt vem de `agente/workspace/AGENTS.md` e complemento local ignorado pelo Git.
- **Métricas:** VRAM é lida de `nvidia-smi`; tokens/s vem de monitor específico do Tabby. Não é uma API de métricas genérica.
- **Requisitos publicados:** README indica Windows 11, GPU de 16 GB VRAM, 32 GB RAM e runtime Python/CUDA instalado manualmente. Isso não valida a configuração-alvo fornecida em separado. Contexto elevado, memória KV e desempenho precisam ser medidos no hardware real.
- **Testes atuais:** testes Rust de transformação de configuração e benchmark; testes Python do monitor e launcher. Não há servidores simulados para chat, testes SSE/cancelamento, integração com llama.cpp ou ciclo de vida.

## Arquivos previstos

**Backend:** `app/src/agente.rs`; novos módulos `app/src/backends/{mod,tabby,llama_cpp}.rs`; `app/src/main.rs`; `app/src/controle.rs`; `app/src/modelos.rs`; possivelmente `app/src/benchmark.rs`; `app/ui/{app.js,agente.js,modelos.js,config.js,index.html,app.css}`; `app/Cargo.toml` e lockfile só se novas dependências forem necessárias; testes simulados; README, docs e CHANGELOG.

**Memória (fase posterior):** novos módulos SQLite em `app/src/memoria/`; agente e comandos Tauri; UI de gerenciamento; migração idempotente dos JSON; testes e documentação. Preservar arquivos antigos durante importação.

## Riscos e decisões

1. APIs compatíveis com OpenAI ainda divergem em SSE, autenticação, reasoning, ferramentas, multimodalidade, modelo e métricas. Uma estrutura de capabilities deve habilitar somente recursos observados.
2. Tool calling depende do template do modelo GGUF e da configuração do llama.cpp; deve ser validado, não inferido pela presença do endpoint.
3. Cancelamento deve fechar apenas a requisição ativa e funcionar mesmo com stream parado; definir timeout de conexão, leitura ociosa e resposta incompleta.
4. Saúde do backend externo é distinta do lifecycle do Tabby/ComfyUI. Trocar backend não deve interromper processos externos.
5. O estimador atual de tokens é aproximado. Contexto e desempenho só podem ser confirmados com os parâmetros reais e medições no equipamento.
6. Migração de sessões deve manter imagens e tool calls; não apagar os JSON na importação. Memórias recuperadas precisam ter origem e ser tratadas como dados não confiáveis, nunca como instruções.
7. Preservar licença MIT, histórico e créditos do ORDIS; nunca versionar tokens, pesos, logs privados ou dados pessoais.

## Estratégia de validação

- Servidores HTTP simulados para ambos backends: descoberta, autenticação opcional, eventos SSE fragmentados, tools, cancelamento, timeout, falhas HTTP e streams incompletos.
- Testes de capabilities, seleção, persistência de sessão e regressão do fluxo Tabby.
- Testes de processo com servidor falso confirmando que o lifecycle jamais atinge o llama.cpp externo.
- Executar a suíte existente; manter hardware real fora dos resultados simulados.
- No Windows 10, validar WebView2, conexão com o servidor existente, tool calling/raciocínio/imagem conforme capabilities, contexto, tokens/s e VRAM sem alterar a instalação.

## Plano de implementação

1. Branch `feature/llama-cpp-backend`: capabilities e interface comum; cliente llama.cpp e teste de conexão; streaming/cancelamento; seleção e status na UI; testes simulados e regressão Tabby; documentação.
2. Após validação da fase 1, branch `feature/persistent-memory`: schema/migração SQLite, importação JSON, busca e recuperação orçamentada, controles de privacidade e testes.
3. Manter `main` sincronizável com upstream e sem commits de funcionalidade. Nenhum PR para o repositório original nem merge/release sem autorização.
