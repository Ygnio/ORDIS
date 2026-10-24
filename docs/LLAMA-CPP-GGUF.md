# llama.cpp e modelos GGUF

O ORDIS pode conversar pelo TabbyAPI ou por um servidor llama.cpp já iniciado. Selecione o backend em **Configurações > Modelos**. O endereço padrão do llama.cpp é `http://127.0.0.1:8080`; o TabbyAPI permanece em `http://127.0.0.1:5000` e continua gerenciado pelo ORDIS quando usa esse endereço padrão.

Para llama.cpp, inicie e configure o `llama-server` por conta própria, carregando o GGUF desejado. O ORDIS apenas consulta `/v1/models`, envia requisições compatíveis com OpenAI para `/v1/chat/completions` e tenta consultar `/metrics`. Ele nunca inicia, encerra, reinicia ou altera parâmetros do processo llama.cpp. A chave API é opcional e, quando fornecida, fica protegida com DPAPI para o usuário Windows atual.

O teste de conexão identifica o modelo carregado e os recursos que o servidor anuncia. Streaming, saída separada de raciocínio, chamadas de ferramenta e entrada de imagem podem variar com a versão do servidor, o modelo e o template. Ferramentas são disponibilizadas pela API, mas precisam ser validadas com o template escolhido. Imagens só são enviadas quando o modelo anuncia a modalidade de imagem. Métricas aparecem apenas se `/metrics` estiver acessível.

Um endereço TabbyAPI diferente do padrão também é tratado como endpoint externo: o ORDIS conecta, mas não tenta iniciar o processo local. Para recuperar o gerenciamento padrão do TabbyAPI, volte o endereço para `http://127.0.0.1:5000`.

O benchmark atual altera a configuração e carrega contextos do TabbyAPI; por isso, ele informa que exige TabbyAPI quando llama.cpp está selecionado. Contexto, velocidade, consumo de VRAM, tool calling, raciocínio e visão do llama.cpp ainda precisam ser verificados no servidor e hardware reais.
