# Lucille 4.1 — ferramentas de desenvolvimento locais

## Base e arquitetura

Esta branch depende de `feature/memory-reliability` (PR #6). Não integra esse PR,
não modifica upstream e não faz merge automático. Identidade, memória, métricas,
ComfyUI, TabbyAPI e llama.cpp continuam nos módulos existentes.

`developer.rs` implementa permissões, validação de caminhos, propostas, aprovação
humana e diário privado. `developer_runner.rs` controla os processos filhos.
`agente.rs` reutiliza schemas OpenAI/SSE de ambos os backends e o ciclo sequencial
de ferramentas. `developer.js` fornece configuração, cartões e histórico.
Nenhum modelo adicional, serviço externo ou biblioteca pesada foi adicionado.

## Configuração e uso no Windows 10

1. Extraia o pacote de validação e execute `Iniciar-ORDIS-Teste.ps1` conforme o
   roteiro de teste anterior. Preserve a instalação normal e seus dados privados.
2. Em Configurações → Modo Desenvolvedor, selecione um diretório de projeto
   existente, separado do aplicativo e dos dados privados. Ative leitura e,
   conscientemente, propostas de escrita/execução. Todas começam desativadas.
3. Para execução, informe o caminho **absoluto** do Python instalado (3.9+)
   e/ou de `powershell.exe` instalado no Windows. Nada é instalado automaticamente.
4. Peça uma tarefa no chat. Inspecione cada cartão: motivo, caminho, diff,
   executável, argumentos, cwd e hashes. Permitir vale somente para aquela
   operação. Recusar bloqueia novas mutações nesse turno. Cancelar interrompe
   a tarefa e encerra somente o Job Object criado por ela.
5. Histórico → Abrir conversa permite retomar por um novo pedido humano.
   Reiniciar nunca reaplica operações nem reaproveita aprovações pendentes.

## Ferramentas e limites

* `workspace_list/read/search`: leitura UTF-8 com linhas, busca literal por texto,
  nome ou símbolo; conteúdo sempre não confiável. Leitura parcial até 32 MiB
  percorridos, linhas de até 128 KB; hash somente dentro do limite de edição.
* `workspace_write`: cria; não sobrescreve. `workspace_patch`: substitui uma única
  ocorrência exata, exige SHA-256 atual, valida novamente após aprovação.
* `workspace_mkdir/move/delete`: diretórios vazios ou arquivos. Sem remoção recursiva.
* `developer_execute`: um script na raiz; cwd dentro do workspace; runtime absoluto
  e manifesto de fontes visíveis na aprovação. Retorna stdout/stderr, exit code,
  duração, truncamento e estado; saída incremental e cancelamento.
* `developer_environment`: detecta Git, Python, Cargo, Rust, Node/npm, CMake, GCC,
  MSVC e PowerShell no PATH, sem executá-los ou instalá-los.
* `developer_processes`: filhos aprovados (PID e criação) e processos com executável/cwd no workspace.
  Caminhos indisponíveis são omitidos; não consulta cmdline ou ambiente de outros
  programas. `developer_diagnostics`: política e diagnósticos existentes.

Limites configuráveis: edição 1 KB–8 MiB, leitura 512–128000 bytes,
saída 512–256000 bytes, timeout 1–600 s, 1–64 chamadas por turno. Padrões:
512 KiB, 24000 bytes, 64000 bytes, 120 s, 16 chamadas. Máximo de 12 rodadas
e três chamadas idênticas por turno; execução é sequencial.

## Modelo de execução restrito

Esta versão **não oferece shell geral nem sandbox de sistema operacional**.
O Python admite funções puras, controle de fluxo, math e unittest, com imports
locais a partir de snapshots aprovados. AST, validação de atributos pela identidade real dos objetos, builtins reduzidos e proxies de
math/unittest impedem imports dinâmicos, arquivos, rede, reflexão e subprocessos.
`unittest.main()` usa apenas o módulo aprovado, sem nomes de testes/módulos externos.
PowerShell admite somente Write-Output, Start-Sleep, throw e exit literais,
validando a AST real; variáveis, interpolação, pipelines e chamadas externas
são recusadas. Não é compatível com todo projeto Python ou script PowerShell.

Git, Cargo, Node, CMake e compiladores são detectados, **ainda não executados**.
Builds arbitrários e instalação de dependências precisam de uma futura política
de isolamento. Execução elevada é recusada mesmo após aprovação. Nenhuma opção
de administração, Registro, drivers, BIOS/boot ou alteração das políticas pelo
modelo é exposta. Não se deve usar este recurso como fronteira contra um
processo hostil já executando com o mesmo usuário.

O processo inicia apenas o bootstrap confiável. Fontes são liberadas por stdin
após associação a um [Windows Job Object](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
com kill-on-close, um processo e limite de memória de 512 MiB. Timeout,
cancelamento e fechamento do aplicativo encerram esse job; servidores externos
llama.cpp, TabbyAPI e ComfyUI nunca são associados a ele.

## Arquivos, privacidade e conflitos

Traversal, ADS, dispositivos, caminhos externos, reparse points/junctions,
hard links, dados privados e arquivos com possíveis segredos são recusados.
Handles dos diretórios sem compartilhamento de exclusão protegem ancestrais
contra troca de junction durante operações. Escrita usa arquivo temporário,
sync e MoveFileExW; backup privado antes de alteração de arquivo existente.
Hash verifica conflitos após aprovação; um escritor concorrente hostil do mesmo
usuário ainda pode disputar o pequeno intervalo da substituição atômica.

Configuração, tarefas, propostas, saída, backups e SQLite/WAL/SHM ficam em
`%LOCALAPPDATA%\Lucille\...\runtime\memory\developer-data`, fora do projeto.
Backups não são restauração automática. Política, permissões e aprovação são
comandos IPC humanos; não são ferramentas anunciadas ao modelo. Memórias,
conteúdo de arquivos, logs ou web não concedem autorização. Apenas preferências
confirmadas recuperadas pela infraestrutura existente entram como dados, nunca
como sistema confiável. Histórico de tarefas é local e não vira memória confirmada
automaticamente. Segredos não são exportados; fixtures são sintéticas.

## Roteiro de aceitação

Crie `calc.py` com `def add(a,b): return a-b` e `test_calc.py`:

```python
import unittest
from calc import add
class Calc(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(2,3),5)
unittest.main()
```

Peça: “Lucille, examine este projeto, encontre o erro, corrija o código e execute
os testes para verificar se a correção funcionou.” Verifique leitura, plano,
patch com hash/diff, aprovação, execução aprovada, resultado e resumo factual.
Repita com o arquivo defeituoso e recuse a proposta: hash deve permanecer igual,
sem nova mutação no turno. Reinicie; confira diário e retomada somente por pedido.
Teste cancelar durante Start-Sleep e expirar timeout sem afetar o servidor.

CI valida GNU original e MSVC, Rust, Python, JavaScript, formatação e privacidade.
Runner CI pode ser elevado: testes de baixo nível exercitam Job Objects; a API
de produção continua recusando execução elevada. Aceitação real exige Windows
10 com token normal, runtime instalado e modelo com tool calling funcional.
Nenhuma instrução AVX2/FMA foi adicionada. Contexto 65536 não é certificado por
esta fase. A experiência completa de interface e runtimes diversos exige teste
na máquina de destino; esta fundação não substitui completamente o Codex.
