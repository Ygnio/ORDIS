# Changelog

Alterações específicas do fork Lucille. O histórico e os créditos originais do ORDIS permanecem preservados.

## 2026.10.30-cli-gui-parity.1 — staging

- Toolchain fixo e executor offline com cache/build isolados, preflight sem instalação e rastreabilidade.
- `conversar` preserva stdin após o JSON inicial; aprovação humana por ID, decisão e prazo.
- Recusa segura em EOF, timeout e ID inválido/repetido/expirado; texto do modelo não concede consentimento.
- Ctrl+C e cancelamento JSON da CLI preservam conteúdo parcial e métricas pelo mesmo núcleo da GUI.
- Configuração compartilhada, prazo de aprovação editável e distribuição Lucille com perfil estável nos dois binários.
- GUI/CLI informam versão, commit e target; pacote pareado exige metadados iguais e manifesto SHA-256.
- Staging sintético explícito, sem importações pessoais, migrações operacionais ou substituição de guard.2.

## 2026.10.25-generation-guard.2 — 2026-10-25

### Proteção de geração

- Perfis de conversa, visão, respostas longas e agentes, com limites de tokens,
  prazo total, primeiro progresso e inatividade independentes no streaming.
- Tratamento explícito de conclusão, truncamento, cancelamento, ausência de resposta
  final e erros de transporte/backend/protocolo. Conteúdo parcial e estado são
  preservados no histórico, sem inventar resposta final ou repetir a requisição.
- Validação de lotes de ferramentas antes da execução, incluindo tipos, tamanhos,
  IDs únicos e argumentos JSON. Lotes inválidos ou interrompidos não executam ações.
- Aviso conservador de repetição de raciocínio, sem cancelamento automático por
  repetição e sem alterar thinking, sampler, contexto, MTP ou projetor do servidor.
- Indicadores de estado em português e configuração local dos perfis. TabbyAPI e
  processos externos continuam preservados.

### Correções e validação

- Diagnóstico visual só confirma capacidade com resposta concluída.
- Roteiros de integração isolam também LOCALAPPDATA/APPDATA; limpeza de fixtures
  tolera bloqueios transitórios do Windows por prazo limitado.
- Testes de protocolo, transporte, persistência e ferramentas, com dados sintéticos;
  CI passa a incluir o teste JavaScript de geração e a integração CLI correspondente.
- Proteções de privacidade ampliadas para artefatos instalados, modelos e arquivos
  privados renomeados; varredura local pode incluir arquivos ainda não rastreados.

### Proveniência da distribuição instalada

- A instalação Windows identificada por este ID foi compilada sobre o commit-base
  `90e3e1acf574174e621ba5ef426d976cf3c99b72` com alterações locais. Esse commit-base
  sozinho **não representa** o conteúdo completo da distribuição instalada.
- A distribuição oficial também possui inicialização de perfil e armazenamento
  WebView específicos. Esses ajustes de instalação não são impostos à edição
  genérica do repositório, que conserva suporte a ORDIS_PROJECT e perfis de teste.
- Este registro não significa tag, publicação, novo pacote de CI ou PR criado.
  O mapeamento de fonte e as limitações estão em docs/GENERATION-RELEASE.md.

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
