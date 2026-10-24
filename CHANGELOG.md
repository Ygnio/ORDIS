# Changelog

Alterações específicas do fork Lucille. O histórico e os créditos originais do ORDIS permanecem preservados.

## Unreleased

- Fase 2: diagnóstico somente leitura de hardware/serviços, descoberta de limites llama.cpp via GET `/props`, métricas filtradas e cartões de capacidades.
- Corrigidas persistência de URLs, dependência indevida de modelo Tabby ao salvar imagem e métricas herdadas entre backends.
- Adicionados endereço independente do ComfyUI, descoberta de checkpoints, proteção de serviços externos e preferências inativas do futuro Desenvolvedor.
- Corrigida a raiz do lançador de teste no ZIP; teste sintético verifica preservação de dados existentes. Inventário em `docs/FASE-2-CONFIGURACOES.md`.

- Registrada a auditoria inicial em `docs/INITIAL-ARCHITECTURE-AUDIT.md`.
- Iniciada a implementação da abstração de backends de inferência.
- Adicionado o seletor de TabbyAPI/llama.cpp, URLs independentes, descoberta do modelo, streaming SSE, cancelamento, capacidades e chave llama.cpp protegida por DPAPI.
- Isolado o ciclo de vida do llama.cpp externo; o benchmark de contexto permanece restrito ao TabbyAPI.
- Documentada a configuração GGUF em `docs/LLAMA-CPP-GGUF.md`.
- A validação Rust local inicial foi bloqueada pela ausência de `dlltool.exe`; a Fase 1.6 foi validada posteriormente no GitHub Actions GNU/MSVC. A Fase 2 mantém a validação no CI e amplia os testes sintéticos, sem substituir o reteste do pacote no Windows 10 real.
