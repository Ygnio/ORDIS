# Changelog

Alterações específicas do fork Lucille. O histórico e os créditos originais do ORDIS permanecem preservados.

## Unreleased

- Registrada a auditoria inicial em `docs/INITIAL-ARCHITECTURE-AUDIT.md`.
- Iniciada a implementação da abstração de backends de inferência.
- Adicionado o seletor de TabbyAPI/llama.cpp, URLs independentes, descoberta do modelo, streaming SSE, cancelamento, capacidades e chave llama.cpp protegida por DPAPI.
- Isolado o ciclo de vida do llama.cpp externo; o benchmark de contexto permanece restrito ao TabbyAPI.
- Documentada a configuração GGUF em `docs/LLAMA-CPP-GGUF.md`.
- Validação nesta máquina: os 30 testes Python existentes, `node --check app/ui/modelos.js`, `rustfmt --edition 2021 --check app/src/backends.rs` e `git diff --check` passaram. `cargo fmt --check` encontra formatação preexistente fora do escopo. `cargo test --manifest-path app/Cargo.toml` foi tentado, mas parou porque falta `dlltool.exe` (MinGW) no toolchain Windows GNU; os testes Rust e simuladores HTTP não foram executados.
