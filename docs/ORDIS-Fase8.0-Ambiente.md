# Fase 8.0 — Ambiente reproduzível

## Incidente e prevenção

Na auditoria da Fase 7, `rustup target list --installed --toolchain stable-x86_64-pc-windows-gnu`
instalou implicitamente o toolchain ausente no RUSTUP_HOME selecionado. Não era uma consulta
somente de leitura. A instalação foi registrada e permanece preservada; esta fase não remove ferramentas.

Não usar proxies `cargo`/`rustc`, consultas seletoras do rustup nem `cargo +stable` na estratégia
offline. O arquivo de toolchain agora fixa Rust 1.99.1 GNU. Ele declara a versão; **não instala**
nada por si só, mas invocar proxies pode fazê-lo. O executor evita os proxies inteiramente.

## Ferramentas encontradas em 30/10/2026

* Rust GNU 1.99.1, commit 58cf8f9c3, Cargo 1.99.1, LLVM 23.1.1: instalação preexistente do workspace.
* GNU GCC 16.2.0 e binutils 2.47.20260726: instalação MinGW completa em outro checkout local.
* Visual Studio Build Tools 18, MSVC 14.52.36815 e outras versões: presentes fora do PATH.
  A inspeção inicial não encontrou biblioteca padrão Rust para o target MSVC.
* Python 3.14.7 e Node 24.22.1: executáveis existentes.
* O primeiro cache Cargo do workspace não continha `image`; a tentativa offline falhou sem rede.
  O cache existente do usuário contém a dependência e foi copiado para um diretório isolado.

Os caminhos privados, hashes dos executáveis e datas dos dois toolchains foram registrados no
relatório local de evidências. Não pertencem ao repositório público.

## Executor offline

`scripts/offline_build.py` recebe um JSON **local**, não versionado, com caminhos absolutos:

```json
{
  "cargo": "D:/toolchains/rust/bin/cargo.exe",
  "rustc": "D:/toolchains/rust/bin/rustc.exe",
  "rustdoc": "D:/toolchains/rust/bin/rustdoc.exe",
  "cc": "D:/toolchains/mingw/bin/gcc.exe",
  "ar": "D:/toolchains/mingw/bin/ar.exe",
  "linker": "D:/toolchains/mingw/bin/gcc.exe",
  "target": "x86_64-pc-windows-gnu",
  "cache_source": "D:/caches/cargo",
  "jobs": 2
}
```

Exemplo, com Python já instalado:

```powershell
python -B scripts/offline_build.py --config D:/local/gnu.json --output D:/staging/build-001 -- test
python -B scripts/offline_build.py --config D:/local/gnu.json --output D:/staging/build-001 --reuse -- build
```

Pré-condições: todos os executáveis nativos e a biblioteca padrão do target devem existir.
Proxies com o hash de `rustup.exe` são recusados antes de execução. O cache é copiado,
não compartilhado por links, e os artefatos permanecem no staging. `--reuse` exige os mesmos hashes
de ferramentas e target. PATH, RUSTC, RUSTDOC, CC, AR e variáveis Cargo são definidos somente no
processo filho; não há alteração de PATH global. A pasta do compilador C precede a do Rust:
foi reproduzida uma incompatibilidade de DLL do GCC quando a ordem era inversa
(mesmo arquivo C: saída 1 sem diagnóstico com Rust primeiro, saída 0 com MinGW primeiro).
O executor fixa `--offline --locked`,
RUSTUP_AUTO_INSTALL=0 e um RUSTUP_HOME vazio isolado. Registra commit, Cargo.lock, target,
argumentos e hashes em `environment.json`, além de `build.log`.

Para MSVC, fornecer ferramentas nativas e ambiente INCLUDE/LIB previamente instalado por meio
da chave `environment`. A presença de cl.exe, sozinha, não constitui um target Rust MSVC completo.
Não usar `target-cpu=native` nem AVX2: o target x86-64 padrão deve atender Windows 10/i7-3770.

## Testes e CI

`tests/test_offline_build.py` demonstra que ferramenta ausente e proxy são recusados antes de
lançar qualquer processo ou criar cache. A tentativa real com cache incompleto confirmou falha
offline sem instalar dependências. Testes Python: 50 aprovados; JavaScript: 38 aprovados.
A compilação Rust isolada é registrada separadamente, sem antecipar seu resultado.

O CI usa runners descartáveis e provisionamento explícito autorizado, com Rust 1.99.1 GNU/MSVC.
Depois do provisionamento, chama executáveis nativos por caminho, desativa auto-install e registra
versão/hash/commit. Esse provisionamento de CI não faz parte do executor offline local.
Pushes e PRs em `build/**` também acionam as verificações GNU/MSVC/credenciais.

## Limites

Fixar Rust e Cargo.lock melhora a rastreabilidade, mas não promete ZIPs bit a bit idênticos:
Windows SDK, linker e imagem do runner também influenciam a saída. Os hashes do manifesto
identificam os artefatos efetivamente testados. O empacotamento pareado e o contrato de perfil
pertencem à Fase 8.1. A instalação oficial permanece fora desses diretórios e não é substituída.
