# Fase 8.1F — Fechamento auditável do QA

Base: 08b9b1aa75e76250f50c394bd151c38a9a1c0521. Instalação operacional excluída dos testes.
O pacote anterior não foi sobrescrito. O relatório local contém passos, hashes e evidências;
perfis, bancos e inventário privado não são versionados.

## Correção encontrada

A expiração nativa já recusava IDs tardios, mas era registrada como recusa e o cartão não
mostrava prazo. A versão parity.2 publica o vencimento para apresentação, mostra contagem
regressiva, desabilita os botões vencidos e registra `timeout` no diário. A decisão confiável
continua no núcleo, com relógio monotônico; o relógio da interface não concede autorização.
O teste de UI também invoca o handler depois do vencimento e comprova ausência de IPC.

## Cobertura e limites

| Grupo manual | Estado de controle inicial | Evidência/ação requerida |
| --- | --- | --- |
| Identidade e CRUD de memória | BLOCKED na automação visual | Núcleo/pacotes testados; requer confirmação humana do ciclo completo na GUI sintética |
| PNG/JPEG/WebP, inválido/ausente | BLOCKED na automação visual | Seleção e preview reais requerem confirmação humana; usar somente imagens geradas para teste |
| Troca de backend e falhas HTTP | BLOCKED na automação visual | Mocks e transporte testados; faltam ações correspondentes pela GUI |
| Expiração visual | BLOCKED até confirmação humana | Correção nativa/DOM automatizada; observar contador, término e ausência de aprovação tardia |

BLOCKED não significa PASS. A captura Windows falhou na fase anterior e os cliques retornaram
geometria indisponível. Será tentada acessibilidade/validação humana; screenshots não serão inventados.
O PR #11 permanece em rascunho enquanto existirem bloqueios relevantes. O relatório local de QA
é atualizado com os resultados efetivamente observados e registra explicitamente os bloqueios.

## Compatibilidade e reversão

Uma versão antiga com `deny_unknown_fields` pode recusar a configuração que contém
`approval_timeout_seconds` e usar defaults restritivos. Isso não deve ser confundido com
apagamento dos arquivos. Testar apenas cópias sintéticas: preservar bytes/hash do original,
ler com a versão antiga, recuperar uma cópia compatível retirando apenas o campo conhecido,
e confirmar permissões/identidade. Na atualização real futura: backup datado e verificado;
não sobrescrever bancos novos com backups antigos. Não há migração pessoal nesta fase.
