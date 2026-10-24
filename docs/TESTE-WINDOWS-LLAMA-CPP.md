# Teste manual: ORDIS e llama.cpp no Windows 10

Este procedimento valida o pacote `ORDIS-Windows-Test.zip` do artifact **ORDIS-Windows-Test-Fase-1.6**, substituto do artifact de teste anterior. O pacote incorpora a identificação do modelo ativo e a separação das configurações do TabbyAPI. O ORDIS é somente cliente HTTP do llama.cpp. Ele não inicia, para, reinicia, mata nem altera o servidor, o GGUF ou o contexto configurado.

## O que vem no pacote

- `app/ORDIS.exe`, `app/ordis-cli.exe` e `app/WebView2Loader.dll` compilados no target Windows GNU original.
- `app/ui/ordis.png`, configurações de referência, scripts públicos do ORDIS, prompt base versionado e este roteiro.
- `Iniciar-ORDIS-Teste.ps1`, que prepara somente os arquivos ausentes em `%LOCALAPPDATA%\Lucille\ORDIS-Teste` e inicia o app com `ORDIS_PROJECT` apontado para esse diretório.

O ZIP não contém TabbyAPI, ComfyUI, Python, modelos, chaves ou dados de conversas. Para o teste de llama.cpp, TabbyAPI e ComfyUI podem permanecer desligados. Iniciar o ORDIS não deve encerrar o aplicativo; as ações de ligar/desligar os serviços TabbyAPI e ComfyUI não devem ser usadas neste roteiro.

O estado local deste pacote fica em `%LOCALAPPDATA%\Lucille\ORDIS-Teste\runtime`. O lançador não sobrescreve arquivos existentes. O diretório completo pode conter conversas e credenciais protegidas; não o envie para o GitHub nem anexe logs brutos a um issue. O caminho atual do runtime do projeto original continua sendo o definido por `config/paths.json`; esta fase não migra nem move dados existentes. A convenção planejada para novos dados Lucille é `%LOCALAPPDATA%\Lucille\`.

## Pré-requisitos e extração

1. Windows 10 Pro 22H2 x64 atualizado, com o Microsoft Edge WebView2 Evergreen Runtime instalado. Se a janela não abrir, confirme o runtime WebView2 em **Aplicativos instalados**.
2. Descompacte o ZIP inteiro em uma pasta gravável, por exemplo `C:\Lucille\ORDIS-Teste`. Não execute o app de dentro do visualizador de ZIP.
3. Clique com o botão direito em `Iniciar-ORDIS-Teste.ps1` e escolha **Executar com PowerShell**. Se a política local bloquear o script, abra PowerShell, navegue até a pasta extraída e execute `powershell -ExecutionPolicy Bypass -File .\Iniciar-ORDIS-Teste.ps1` apenas para esse comando.
4. O app de teste usa dados isolados em `%LOCALAPPDATA%\Lucille\ORDIS-Teste`. Para voltar ao projeto já instalado com seus dados, inicie o binário definindo `ORDIS_PROJECT` para a raiz desse projeto; o lançador do ZIP não muda esse caminho por conta própria.

## Checklist funcional

Faça as etapas com o servidor llama.cpp já iniciado pelo usuário e configurado no endereço `http://127.0.0.1:8080`.

1. Abra o ORDIS. Confirme que a janela aparece mesmo com TabbyAPI ausente e ComfyUI parado.
2. Abra **Configurações > Modelos**, selecione llama.cpp e salve o endpoint `http://127.0.0.1:8080`.
3. Use **Testar conexão**. Confirme que o modelo informado corresponde ao GGUF carregado no servidor.
   Confirme que contexto e limite por resposta aparecem como desconhecidos quando o servidor não os anuncia. A ferramenta `session_status` deve informar `Lucille`, backend `llama.cpp`, conexão ativa e limites nulos, sem reaproveitar contexto ou opções do TabbyAPI.
   Confirme que a tela de modelos oculta as opções TabbyAPI enquanto llama.cpp está selecionado e que, ao voltar ao TabbyAPI, os valores anteriores continuam salvos.
   No rodapé do chat, confirme o formato `Lucille · llama.cpp · conectado`; pare o servidor manualmente para verificar que a interface mostre desconectado e volte a conectado após reiniciá-lo manualmente.
4. Envie uma pergunta curta, sem informação privada. Confirme que o texto aparece em partes durante o streaming.
5. Envie outra pergunta e use **Interromper**. Confirme que o ORDIS para de receber a resposta e que o servidor continua ativo.
6. Se o modelo e o template suportarem ferramentas, peça uma consulta simples de ferramenta que não altere arquivos ou serviços. Registre se a chamada foi reconhecida e concluída.
7. Se o servidor separar reasoning, confirme se a interface o apresenta separado. A ausência desse campo pode ser normal para o template usado.
   Texto comum dentro de tags como `<think>` continua como texto da resposta. Só o campo SSE explícito `reasoning_content` deve aparecer no painel de raciocínio.
8. Confirme que a sessão aparece no histórico e continua disponível após fechar e reabrir o ORDIS.
9. Feche o ORDIS e confirme no gerenciador de tarefas que o processo do llama.cpp continua ativo. O ORDIS não deve encerrar esse processo.
10. Com o llama.cpp ainda sob controle manual, teste a indicação de servidor indisponível e recupere a conexão depois. Não use a ação de desligar serviços do ORDIS para este teste.
11. Se houver runtime ComfyUI previamente instalado e configurado no projeto real, valide a coexistência com uma operação curta iniciada pelo fluxo ComfyUI normal do ORDIS. Não execute teste de imagem durante a validação isolada do ZIP, que não inclui ComfyUI.
    Com llama.cpp selecionado, o ORDIS deixa o servidor externo em execução durante a geração do ComfyUI. A VRAM do llama.cpp continua ocupada; ajuste ou pare o servidor manualmente se o ComfyUI não tiver memória suficiente. O ORDIS não altera os parâmetros do llama.cpp.

Para testar com o runtime já existente (TabbyAPI/ComfyUI), defina `ORDIS_PROJECT` para a raiz do checkout ou instalação que contém `config/paths.json`. Isso usa os dados que já estão naquele projeto; não copie, mova ou apague arquivos para esta validação.

## Diagnóstico sem dados privados

- Anote versão/build do llama.cpp, nome público do modelo, endpoint local, resultado de conexão, comportamento do streaming e o horário aproximado. Não registre prompt, resposta, chave, caminho pessoal ou conteúdo da conversa.
- A saída de `/v1/models` pode ser consultada pelo botão de conexão. Evite publicar a resposta completa se ela contiver caminhos locais; registre somente o identificador do modelo.
- Não compartilhe `runtime/`, arquivos DPAPI, sessões, banco de dados, arquivos `.env` ou logs brutos. Se for necessário relatar falha, copie apenas a mensagem sanitizada e confira que não contém prompt, resposta, token ou caminho de usuário.

## Procedimento de desempenho

Não faça essas medições automaticamente. Execute manualmente em uma sessão sem tarefas concorrentes e mantenha os parâmetros do llama.cpp sob seu controle.

1. Para cada tentativa, registre o modelo, quantização, contexto solicitado, tamanho aproximado do prompt, temperatura, modo GPU, versão do servidor e estado do ComfyUI.
2. Meça TTFT do envio até o primeiro fragmento visível, com cronômetro. Meça tokens/s pela métrica `/metrics` se o servidor expuser a métrica; anote quando ela estiver indisponível.
3. Observe RAM no Gerenciador de Tarefas e VRAM pelo Gerenciador de Tarefas ou `nvidia-smi`. Registre pico e valor após a resposta.
4. Repita com prompts curtos e contextos gradualmente maiores, parando se houver paginação intensa, falta de memória, travamento ou instabilidade. 65.536 tokens é uma meta de contexto, não uma garantia de memória ou velocidade.
5. Só teste concorrência com ComfyUI quando a configuração real estiver pronta. Registre se a resposta continua estável e os picos de RAM/VRAM.
6. Para cada cenário, faça ao menos três medições curtas e compare mediana de TTFT e tokens/s. Pare antes de qualquer teste longo ou que altere a configuração do servidor.

O ORDIS/Rust não embute llama.cpp e o código desta aplicação não exige AVX2 para conectar à API. O binário `llama-server.exe` é externo: em um i7-3770 sem AVX2/FMA, confirme que a build desse servidor foi compilada para instruções compatíveis com a CPU. A RTX 3060 pode acelerar a inferência, mas não elimina limites de largura de banda, memória KV, contexto ou suporte do modelo. 20 tokens/s e contexto de 65K precisam ser medidos no equipamento real.

## Compatibilidade ainda pendente

O CI compila em Windows Server 2022; ele não comprova funcionamento em Windows 10. A janela usa WebView2 e efeitos visuais acrylic que podem se apresentar de forma diferente no Windows 10. O GNU precisa de `WebView2Loader.dll` ao lado do executável; o runtime Evergreen do WebView2 deve estar instalado. Este pacote não inclui runtime Python/CUDA do TabbyAPI ou ComfyUI, nem servidor llama.cpp, GGUF ou drivers NVIDIA. A validação final requer o Windows 10 22H2 e o hardware descrito pelo usuário.

Se o llama.cpp não informar contexto e limite de resposta, esses valores ficam desconhecidos. Nesse caso o ORDIS envia o histórico da conversa sem aplicar o corte baseado no contexto configurado para TabbyAPI e omite `max_tokens`; o próprio servidor pode rejeitar uma conversa que ultrapasse o contexto que configurou.
