# Fase 2 — configurações, hardware e serviços

## Base e escopo

`feature/hardware-settings` parte de `e36f382` em `feature/llama-cpp-backend` e depende do PR #1. O PR #1 está aberto, sem conflitos e com GNU/MSVC aprovados; o usuário relatou funcionamento real no Windows 10. Está tecnicamente pronto para decisão humana de merge. Nenhum merge é feito nesta fase. O PR da Fase 2 aponta inicialmente para a branch da Fase 1; depois de #1 ser mesclado, poderá ser redirecionado para `main`.

Esta auditoria combina inspeção dos caminhos de leitura/gravação, testes sintéticos e CI Windows. Ela não representa execução de todos os controles sobre os dados ou runtimes reais do usuário. Não há instalação, migração, exclusão de dados, memória SQLite ou executor de programação.

## Inventário das configurações auditadas

Os caminhos abaixo são relativos ao projeto efetivo definido por `ORDIS_PROJECT` e `config/paths.json`. O lançador usa `%LOCALAPPDATA%\Lucille\ORDIS-Teste`. Nenhum destes arquivos privados deve ser versionado.

| Seção / opção | Persistência | Efeito e avaliação |
| --- | --- | --- |
| Aparência: sistema/claro/escuro | WebView localStorage `ordis.tema` | Altera CSS e tema da janela; não controla serviços. Preservado. |
| Barra de conversas e preferências visuais | localStorage da WebView | Preferências de apresentação; não muda o backend. Preservadas. |
| Backend selecionado e URLs Tabby/llama | `runtime/inference/backend.json` | Alterna somente o cliente HTTP. Campos `tabby_url` e `llama_url` independentes; corrigido o envio incorreto dos nomes pelo JavaScript. |
| Chave llama.cpp | `runtime/inference/llama-api-key.dpapi` | DPAPI do usuário Windows. Branco mantém; remover apaga só a chave selecionada. Não aparece no diagnóstico. |
| Credencial TabbyAPI | `runtime/tabbyAPI/api_tokens.yml` | Credencial preexistente do runtime; nunca incluída no ZIP. |
| Modelo EXL3, contexto, template, raciocínio, visão e MTP | `runtime/tabbyAPI/config.yml` | Aplicáveis ao Tabby gerenciado. Ocultos para llama.cpp e Tabby externo; preservados ao alternar. Alteração do Tabby gerenciado pode exigir recarga explícita pelo fluxo existente. |
| Limite por resposta Tabby | `runtime/modelos.json` | Parâmetro do cliente Tabby. Não é enviado ao llama.cpp como limite herdado. |
| Contexto / limite llama.cpp | Não são gravados pelo ORDIS | Leitura de `/props`; desconhecidos quando ausentes. `n_ctx` é capacidade configurada por slot, não tokens usados nem capacidade de treinamento. Limite positivo de geração é apresentado como padrão anunciado, não garantia absoluta. |
| Streaming, ferramentas, raciocínio e visão | Estado retornado pelo servidor / cliente | Cartões distinguem protocolo, configuração, anúncio e teste. Anúncio de suporte não significa validação do modelo/template. Testes automatizados sintéticos não marcam o modelo real como validado. |
| Flash Attention / KV cache | Leitura opcional do servidor | Só aparecem como conhecidos se campos explícitos forem retornados. Não se inferem de GPU, quantização GGUF ou metas do usuário. |
| ComfyUI: endereço e gerenciamento | `runtime/settings.json` | Gerenciamento restrito à instalação ORDIS em `http://127.0.0.1:8188`. Outros endereços são externos; não são iniciados nem encerrados. Mudança de serviço exige encerrar antes o ComfyUI gerenciado em execução. |
| Checkpoint SDXL | `runtime/modelos.json` | Listagem efetivamente encontrada no serviço (`object_info`) ou, para instalação gerenciada offline, no diretório local. Entrada ausente fica explícita e não é oferecida como modelo instalado. Nenhum download automático. |
| Workflow importado | `runtime/fluxos/*.json` | Importação API e marcas existentes preservadas. Modelos/custom nodes do workflow dependem da instalação do ComfyUI; a existência do arquivo não garante execução. |
| Passos, CFG, sampler, scheduler, prompt negativo, resolução e seed | Configuração em `modelos.json`; pedido/metadados da imagem locais | Mantidos os limites existentes de resolução, passos e parâmetros. Prompts são enviados ao endereço ComfyUI que o usuário configurou. |
| Imagens e anexos | `agente/workspace/imagens`, `agente/workspace/anexos` | Privados. Serviço externo devolve imagem pela API `/view`; cópia recebida fica no workspace local, com tamanho/tipo limitados. |
| Proxmox: URL, token ID, segredo e CA | `runtime/proxmox/` | Consultas somente leitura, CA própria e segredo DPAPI. Diagnóstico mostra apenas configurado/CA e não consulta nós automaticamente. Teste de conexão permanece no controle Proxmox. |
| Painel: VRAM | `nvidia-smi`, sem gravar inventário | Consulta limitada a 3 segundos; retorna desconhecido se indisponível. Todas as GPUs NVIDIA retornadas aparecem no diagnóstico; o cartão compacto usa a primeira. GPUs de outros fabricantes não são detectadas por esta implementação. |
| Painel: velocidade | Tabby: arquivo local de métricas; llama: valores filtrados de `/metrics` | Corrigida a exibição de métricas antigas do Tabby sob llama.cpp. Velocidade llama é a média informada pelo servidor, não garantia por requisição nem prova de atingir a meta. |
| Processos gerenciados | `runtime/state.json`, `control.lock`, marcador de cancelamento | Apenas processos rastreados pelo ORDIS e seus descendentes. Servidores externos não ativam o botão de encerramento. llama.cpp nunca é registrado nem controlado. |
| Benchmark / CLI | CLI e relatórios locais existentes | Benchmark continua protegido contra execução com llama.cpp. Acrescentado `ordis-cli diagnostico`, somente leitura. Nenhum teste de carga automático. |
| Ferramentas do agente | Definições Rust | `read` continua restrito ao workspace atual; status, web pública, Proxmox e imagens preservados. Nenhuma ferramenta de terminal, escrita ou Git nova. A descrição de imagens agora informa a preservação dos servidores externos. |
| Histórico e armazenamento | `runtime/agente/sessoes/*.json` e workspace atual | Operações existentes preservadas. Lançador cria somente arquivos ausentes; testes sintéticos verificam preservação. Não há migração nem pipeline de memória. |
| Rede | URLs de serviços e credenciais existentes | URLs HTTP(S) sem usuário/senha, query ou fragmento. Clientes de diagnóstico não seguem redirects. Consultas de descoberta são GET. Erros mostram mensagem sanitizada, não corpo bruto do servidor. |
| Logs | `runtime/logs` | Botão abre pasta existente; nenhuma publicação/exportação automática. Logs antigos de serviços podem conter dados privados e não devem ser anexados ao GitHub. |
| Diagnóstico de hardware | Somente memória da aplicação | CPU, arquitetura, núcleos/threads, SSE4.2/AVX/AVX2/FMA, RAM, GPU/VRAM, Windows e WebView2 por APIs locais. Não lê números de série, hostname, conversas ou credenciais. |
| Modo Desenvolvedor | `runtime/settings.json` | Preferências de workspace separado, repositórios, shell, caminhos Git/Python/Node/Cargo, tempos e logs. Nenhum caminho configurado é aberto ou executado pelo agente nesta fase. |
| Política do futuro Desenvolvedor | Validada em Rust e apresentada na UI | Modo desativado; leitura/escrita/execução/commit/push falsos; confirmação sempre; workspace isolado, backups e reversão obrigatórios. Tentativa de ativar permissão é recusada. |

## Problemas encontrados e corrigidos

1. O formulário do backend enviava `tabbyapi`/`llama_cpp` como campos de URL; Rust esperava `tabby_url`/`llama_url`. Endereços personalizados podiam voltar ao padrão.
2. Salvar imagem exigia um EXL3 local mesmo com llama.cpp selecionado. As configurações de conversa de backends externos agora são preservadas e não exigem instalação Tabby.
3. A UI do backend juntava tudo em uma linha; foi substituída por cartões de conexão, modelo, limites, capacidades, métricas e gerenciamento.
4. Contexto llama não era consultado em `/props`. Agora a descoberta usa campos explícitos e ignora valores de treinamento/uso e limites negativos sem teto fixo.
5. O painel podia apresentar métricas antigas do Tabby com outro backend ativo. Cada fonte agora está identificada.
6. ComfyUI tinha URL fixa, listagem apenas local e avisos pouco claros para ausência de checkpoint. Endereço, estado, descoberta e gerenciamento são independentes.
7. O controle global podia sugerir encerramento de serviços externos. Serviços informam se são gerenciados; botões e mensagens respeitam essa informação.
8. O lançador copiado para a raiz do ZIP procurava o executável no diretório pai. A descoberta agora aceita a raiz do pacote e a localização original em `scripts/`.
9. A promessa fixa de ganho MTP foi retirada da interface: ganho e custo de VRAM dependem do modelo e do ambiente.
10. Consultas demoradas do painel podiam se sobrepor. Há uma consulta por vez e timeout para a consulta NVIDIA.

## O que é conhecido e o que continua dependendo do ambiente

Máquina de referência relatada: Windows 10 Pro 22H2, i7-3770 sem AVX2/FMA, 16 GB RAM e RTX 3060 12 GB. O diagnóstico consulta a máquina em que o executável está rodando; não fixa esses dados no programa.

65.536 tokens, KV Q8_0, Flash Attention e 20 tokens/s são referências desejadas. O ORDIS não configura nem confirma esses valores por suposição. `/props` varia entre versões; limites ausentes e modos não expostos ficam desconhecidos. O indicador de ferramentas anunciadas não comprova a chamada real no GGUF/template atual. O painel de raciocínio continua aceitando apenas o canal explícito do backend.

O aviso de menos de 4 GiB livres é uma heurística de risco, não uma medição da memória exigida por um workflow. O usuário decide como lidar com concorrência; ORDIS não descarrega GGUF, altera KV cache ou encerra llama.cpp. Se ComfyUI usar HTTPS, a geração e o histórico usam HTTP seguro; sem suporte TLS no cliente WebSocket atual, o detalhe por passo não está disponível e o acompanhamento cai para polling.

## Atualização sem perder dados

1. Feche apenas o app ORDIS. Mantenha o servidor llama.cpp sob controle manual.
2. Extraia o novo artifact **ORDIS-Windows-Test-Fase-2** por completo em uma pasta nova. O pacote anterior da Fase 1.6 foi substituído.
3. Use `Iniciar-ORDIS-Teste.ps1` da nova pasta. Ele reutiliza `%LOCALAPPDATA%\Lucille\ORDIS-Teste`, cria somente arquivos ausentes e preserva histórico, preferências, configuração e DPAPI existentes.
4. Não copie `runtime`, conversas, arquivos DPAPI ou personalidade privada para dentro de um checkout Git. Não apague a pasta de dados para atualizar.
5. Para voltar ao binário anterior, feche o novo app e use o lançador anterior apontando para a mesma pasta de dados. Novas preferências ficam em arquivo separado e as configurações antigas continuam compatíveis. Não rode duas versões ao mesmo tempo.
6. A instalação `C:\AI\llama.cpp` permanece independente. O pacote não contém nem atualiza modelos, Python/CUDA, drivers ou runtimes.

## Reteste no Windows 10 real

- Confirmar CPU, 4 núcleos/8 threads, AVX, ausência de AVX2/FMA e RAM/VRAM medidas. Nenhum valor do runner de CI deve aparecer como se fosse o PC do usuário.
- Conferir Windows e versão WebView2; simular ausência de `nvidia-smi` somente se já houver um ambiente de teste separado, sem alterar drivers.
- Consultar llama.cpp em 8080 e confirmar modelo Lucille. Comparar contexto mostrado com `/props` da versão instalada sem compartilhar o JSON bruto. Se o campo não existir, aceitar desconhecido.
- Confirmar que a média de `/metrics`, quando habilitada, é identificada como média do servidor. Medir os 20 tokens/s na máquina real, sem promessa pelo CI.
- Salvar um endpoint de teste, fechar/reabrir e confirmar persistência. Voltar ao endpoint real manualmente. Alternar backend e verificar preservação do Tabby.
- Confirmar que ausência de RealVisXL/checkpoint é explícita, sem instalação automática. Se houver serviço externo existente, configurá-lo e consultar seus checkpoints.
- Gerar uma imagem sintética curta somente com modelo/workflow já instalado e recursos adequados; observar VRAM, cancelamento e preservação do PID llama.cpp. ComfyUI externo também deve permanecer ativo ao final.
- Testar tema, chat, SSE, ferramenta segura `session_status`, raciocínio explícito, cancelamento e reabertura do histórico de teste.
- Salvar preferências do Desenvolvedor e reiniciar o app. Nenhum comando, arquivo de repositório ou commit deve ser criado por essas preferências.
- Testar Proxmox somente se já configurado, pelo botão específico e com PVEAuditor. Não publicar inventário privado.
- Confirmar no uso do novo ZIP que histórico e configurações anteriores continuam presentes. Não anexar a pasta privada aos resultados.

## Próximas fases

Memória persistente privada, programação e automodificação continuam aguardando autorização futura. A arquitetura pretendida mantém um executor separado das preferências, permissões por workspace, confirmação de ações, limites de tempo, logs locais, testes e rollback antes de aplicar qualquer alteração à instalação ativa. Nada disso é habilitado por salvar este formulário.

## Fontes de protocolo

- [llama.cpp: propriedades e métricas do servidor](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)
- [llama.cpp: capacidades do template](https://github.com/ggml-org/llama.cpp/blob/master/common/jinja/caps.h)
- [ComfyUI: endpoints de consulta, modelos e imagens](https://github.com/Comfy-Org/ComfyUI/blob/master/server.py)
