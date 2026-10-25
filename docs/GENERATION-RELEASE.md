# Proveniência e escopo — 2026.10.25-generation-guard.2

O ID identifica a primeira distribuição oficial instalada. Ela partiu do commit
`90e3e1acf574174e621ba5ef426d976cf3c99b72`, acrescido de código não commitado no momento
da compilação. Não há commit ou tag que, sozinho, reproduza exatamente essa entrega.
Manifestos locais registram hashes de fontes e binários, sem versionar dados pessoais.

## Mudanças compartilhadas preparadas para o fork

Política de limites, transporte SSE, validação terminal e de ferramentas, persistência
de respostas parciais/estados, indicadores e configuração de perfis, diagnóstico
visual conservador e roteiros sintéticos. Os módulos compartilhados são os aprovados
no QA da distribuição. Ver `GENERATION-SAFETY.md` para comportamento e limitações.

Esta política contém geração por tokens e tempo; o aviso de repetição não é uma
garantia semântica de detecção de loop. Não há reinício ou reconfiguração de servidor.
Não foi implementado fechamento forçado de pensamento ou orçamento nativo de raciocínio.

## Ajustes de instalação separados

A distribuição instalada usa nome/identificador próprios, perfil estável por usuário,
mutex de instância e diretório WebView explícito. Seu inicializador verifica hashes,
seleciona releases e abre apenas a GUI. O bootstrap de QA não faz parte dela.

Esses ajustes não são copiados incondicionalmente para o main da edição genérica:
isso rejeitaria perfis ORDIS_PROJECT existentes e os pacotes/launchers de teste.
A proposta do fork mantém a descoberta de perfil original e não contém bootstrap
de isolamento de QA, perfil pessoal, estado de instalação ou caminho privado real.

Uma distribuição oficial reproduzível pelo fork requer uma modalidade de build
explícita e testes de empacotamento próprios, em alteração separada. Até lá, não se
deve afirmar que o build genérico do CI tem o mesmo hash ou comportamento de
inicialização do binário instalado, mesmo compartilhando a proteção de geração.

## Histórico Git e publicação

A base depende de APIs de memória/visão já presentes na branch semântica. Preparar
uma branch dedicada a partir dessa base preserva o histórico; partir diretamente
de main sem as dependências não é equivalente. Nenhum rebase ou squash dos commits
anteriores é necessário. A publicação, tag e PR exigem autorização posterior.

Nunca versionar bancos, histórico pessoal, perfis, chaves, modelos, backups, logs ou
manifestos privados da instalação. A distribuição do binário tampouco depende de
copiar esses dados para o GitHub.
