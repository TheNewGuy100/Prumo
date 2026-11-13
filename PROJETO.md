# Prumo

Monitor de recursos para Windows em que os números fecham: CPU, GPU, memória,
núcleos e threads na mesma linha do tempo, com controle sobre os processos.

## Por que existe

As ferramentas atuais respondem uma pergunta cada e nenhuma cruza as respostas:

| Ferramenta | Faz bem | Falta |
|---|---|---|
| RivaTuner / Afterburner | Frametime e overlay em jogo | Não sabe nada de processos |
| btop4win | Visão geral em tempo real | Só leitura, GPU rasa, layout quebrado, memória contada em dobro |
| Gerenciador de Tarefas | Controle (prioridade, afinidade) | Sem histórico, esconde ~30% da memória em uso |
| RAMMap / Process Explorer | Dado completo | Tabela crua, exige saber o que é "Metafile" |

Medição que motivou o projeto (PC de desenvolvimento, 32 GB):

- Memória em uso: 21,6 GB
- Soma da coluna "Memória" do Gerenciador: 15,1 GB (faltam 6,5 GB)
- Soma do working set por processo, como no btop: 26,0 GB (sobram 4,3 GB)
- O que fecha a conta (RAMMap): 15,0 privado + 3,6 compartilhado/mapeado
  + 2,6 kernel + 0,4 driver

## O que o Prumo entrega

1. **Memória com a conta fechada.** Toda a RAM atribuída a uma categoria
   legível: programas (privado), Windows (kernel), compartilhado e outros,
   cache liberável, livre. A soma bate com o total. Por processo, privado e
   total com compartilhado aparecem lado a lado. O detalhe do bloco
   "compartilhado e outros" é recurso opcional, sob demanda.
2. **Linha do tempo cruzada.** CPU por núcleo, GPU, memória, disco e frametime
   no mesmo eixo, por processo. Responde "o que causou esse engasgo?".
3. **Controle no mesmo lugar.** Prioridade, afinidade, modo de eficiência e
   limpeza de working set, direto na lista.
4. **Custo próprio à mostra.** O Prumo exibe o próprio uso de CPU e memória.
5. **Gravação de sessão.** Grava uma sessão de jogo ou de trabalho e mostra
   depois, na janela, tudo que o overlay mostraria e mais o que causou cada
   engasgo. Funciona em qualquer modo de tela, inclusive tela cheia exclusiva.

## Arquitetura

Três partes, uma fonte de dados:

```
  Coletor (segundo plano) ──► histórico em buffer circular
        │
        ├──► Janela  (fixa no 2º monitor  |  drawer por atalho)
        ├──► Overlay próprio (janela transparente sobre o jogo)
        └──► Gravação de sessão (disco) ──► relatório na janela
```

Princípio: **o Prumo não depende de software de terceiros para funcionar.**
Medição e exibição são próprias, para que atualização alheia não quebre o uso.

### Coletor

Processo em segundo plano que mede e guarda o histórico recente. Roda sempre,
para que a linha do tempo já exista quando a janela abrir.

| Dado | Fonte no Windows | Default | Mínimo |
|---|---|---|---|
| CPU total e por núcleo, memória total, GPU total | Contadores de desempenho / chamadas de sistema | 500 ms | 100 ms |
| Processos e threads (CPU, memória) | `NtQuerySystemInformation`, ETW | 1 s | 250 ms |
| GPU por processo (uso, VRAM) | Contadores "GPU Engine", NVML (NVIDIA) | 1 s | 500 ms |
| Memória por categoria (conta fechada por diferença) | Contadores de desempenho documentados, sem privilégio | 1 s | 250 ms |
| Detalhe do compartilhado (opcional) | API de sistema não documentada, exige administrador | só sob demanda | — |
| Frametime | Captura própria via ETW (mesmos eventos que o PresentMon lê; o código dele é MIT e serve de referência) | por evento | — |

Os intervalos são estimativa e precisam ser medidos no protótipo antes de
virarem limite.

Regras:

- **Intervalo por tipo de dado**, configurável dentro de mínimo e máximo próprios.
- **Taxa adaptativa:** modo lento com a janela fechada, rápido com ela aberta.
- **Buffer circular de tamanho fixo:** a memória do coletor não cresce.
  Referência: 250 processos × 6 métricas × 600 amostras ≈ 4 MB.
- **Orçamento:** menos de 1% de CPU e menos de 50 MB no modo padrão.

### Janela

Uma interface só, com dois modos:

- **Fixa:** fica aberta no segundo monitor.
- **Drawer:** sobe de baixo para cima no centro da tela por atalho global.
  - *Espiar:* abre sem pegar foco nem mouse, some ao soltar a tecla.
  - *Abrir:* interativo, para mexer em prioridade e afinidade.

Limitação conhecida: janela comum não aparece sobre tela cheia exclusiva.
Funciona em janela sem borda; para tela cheia exclusiva, usar o modo gravação.

### Overlay próprio

Janela transparente, sempre no topo, que não recebe clique nem foco, desenhada
por cima do jogo. Não injeta código no processo do jogo, então não há o que um
anti-cheat detectar.

- Funciona em janela e em janela sem borda.
- Não funciona em tela cheia exclusiva; nesse caso, o modo gravação cobre.
- Configurável: o que aparece, onde e em que tamanho, incluindo gráfico de
  frametime e não só o número.
- Perfis por jogo: overlay e gravação configurados por executável.
- **Fora do escopo: overlay por injeção.** É o que o RTSS e o Special K fazem.
  Um injetor novo e desconhecido tem risco de banimento maior que o do RTSS,
  que é tolerado por reputação, e exigiria manter hooks para DX9/11/12, Vulkan
  e OpenGL.

### Modo gravação

O buffer circular guarda só os últimos minutos. A gravação persiste uma sessão
inteira em disco, em resolução total, para análise posterior.

- **Início e fim:** atalho global, ou automático ao detectar um jogo em
  primeiro plano (regra a definir).
- **O que grava:** frametime por quadro, CPU por núcleo, GPU, VRAM, memória e
  os processos mais ativos, todos no mesmo relógio.
- **Relatório da sessão:**
  - FPS médio, 1% low e 0,1% low.
  - Gráfico de frametime com os engasgos marcados.
  - Para cada engasgo, o que mudou no sistema naquele instante: processo que
    subiu de CPU, VRAM no limite, paginação em disco.
  - Comparação entre duas sessões (antes e depois de uma mudança de
    configuração ou driver).
- **Custo:** a gravação escreve em disco em blocos, fora do caminho da coleta,
  para não causar o engasgo que está medindo. Tamanho por hora a medir.

Não depende de overlay nem de modo de tela: é a resposta do Prumo para tela
cheia exclusiva sem injeção.

### Fora do escopo

- **Overlay por injeção** (ver acima).
- **Ponte com o RTSS.** Cobriria só overlay ao vivo em tela cheia exclusiva,
  caso já atendido pelo modo gravação, e traria dependência de terceiro e
  atrito de licença (SDK proprietário em projeto GPLv3). Quem usa RTSS pode
  mantê-lo ao lado do Prumo. Reavaliar apenas se houver pedido.

## Telas

### Janela principal (direção aprovada em 12/11/2026)

De cima para baixo:

1. **Barra do topo:** nome, janela de tempo em uso ("média de 1 s"), custo do
   próprio Prumo (CPU e memória) e botão "Gravar sessão".
2. **Resumo em quatro blocos:** CPU, memória em uso, GPU e VRAM. Cada rótulo
   traz a definição do glossário: "CPU · tempo" com o turbo à parte,
   "GPU · motor 3D" em vez de "uso de GPU".
3. **Barra de memória:** uma barra só, em que programas, Windows,
   compartilhado e outros, cache liberável e livre somam o total. O bloco
   calculado por diferença é hachurado e rotulado "por diferença".
4. **Núcleos:** lógicos agrupados em pares por núcleo físico, com o aviso de
   que 50% não é metade da capacidade.
5. **Linha do tempo:** CPU e frametime no mesmo eixo. Engasgo marcado com o
   que aconteceu junto; o texto diz "junto", nunca "causa".
6. **Processos:** colunas CPU, memória privada, memória com compartilhado,
   GPU (com o nome do motor) e VRAM. Processos do mesmo programa agrupados.
   Linha "vida curta e não atribuído" para a CPU sem dono. Linha de soma que
   fecha com o total; a coluna "com compartilhado" aparece como "não somável".

Acrescentado em 13/11/2026, a pedido:

- **Lista dinâmica:** mostra todos os programas e ocupa a altura que sobrar
  da janela, com rolagem própria. As linhas de soma ficam fixas embaixo.
- **Filtro por nome,** com a soma passando a ser a soma do filtro.
- **Ordenação** por qualquer coluna, clicando no cabeçalho; segundo clique
  inverte. "Ordenar por peso" volta ao padrão (a maior entre a fatia de CPU e
  a fatia de memória).
- **Controle pelo botão direito,** primeira parte: classe de prioridade
  (ver "Como roda hoje").

Acrescentado em 13/11/2026, segunda rodada, a pedido:

- **Disco e rede:** seção nova. Disco mostra atividade e taxa de leitura e
  escrita por disco físico. Rede mostra recebido e enviado por interface.
- **Coluna E/S** na lista: leitura mais escrita por segundo do programa. Soma
  disco, rede e dispositivos, porque o Windows não separa neste contador.
  **Rede por processo não existe sem captura elevada.**
- **Detalhes do programa em gaveta:** clicar numa linha abre os detalhes logo
  abaixo dela, na própria lista, e clicar de novo fecha. Várias podem ficar
  abertas ao mesmo tempo. Mostra o caminho do executável, os totais do grupo
  e cada processo (PID, CPU, memórias, leitura, escrita, VRAM, prioridade),
  até 12, com o resto resumido numa linha. Dali dá para copiar o caminho ou
  filtrar a lista pela pasta. Uma marca à esquerda do nome indica se a
  gaveta está aberta. (Era uma janela separada; trocado a pedido.)
- **`prumo --abrir NOME`** abre o Prumo já com a gaveta daquele programa
  aberta; **`prumo --filtro TEXTO`** abre já filtrado.
- **Filtro que acha o programa pelo que ele tem aberto** (13/11/2026): além
  do nome e do caminho do executável, o filtro procura no título das janelas
  e na linha de comando de cada processo. Caso que motivou: procurar
  o nome de um projeto e achar o Unity que está com ele aberto. Testado: o
  filtro devolveu o Unity (pelo título da janela, que começa com o nome do
  projeto) e o Blender.
  A gaveta mostra as janelas e as linhas de comando do programa.
  - Linha de comando: lida uma vez por processo, com a mesma permissão
    limitada do caminho, sem ler a memória do processo. Usa uma classe de
    consulta do Windows que não está na documentação oficial; se falhar, o
    campo fica vazio e o resto funciona.
  - Títulos das janelas: API documentada, lidos a cada amostra.
  - Isto não é a lista de arquivos abertos. Um programa que abre um arquivo
    sem pôr o nome no título nem na linha de comando não é achado. A lista
    de arquivos abertos continua fora (ver os limites abaixo).
- **Pipe restrito ao usuário atual** (13/11/2026): como as amostras passaram
  a levar linhas de comando, que podem conter dados sensíveis, o pipe agora
  só aceita o usuário que roda o coletor e o sistema. Se a restrição não
  puder ser montada, o coletor avisa e usa a permissão padrão. Testado só o
  caminho normal (mesmo usuário conecta); a recusa a outro usuário não foi
  testada.
- **Filtro por nome ou caminho** do executável.
- **Dicas ao passar o mouse** em blocos, títulos, legenda da memória e
  cabeçalhos, com o texto do glossário em versão curta.
- **Setas de ordenação desenhadas,** porque a fonte padrão não tem o
  caractere e mostrava um quadrado.
- **Configuração** (botão no topo): mostrar ou esconder cada seção, mudar a
  ordem delas, escolher as colunas da lista e restaurar o padrão. A
  ordenação escolhida também é lembrada. Salvo em
  `%APPDATA%\Prumo\config.txt`.
- **Altura mínima da lista:** oito linhas. Se as seções e esse mínimo não
  couberem, a página inteira rola.

- **Disco e rede à direita da lista de processos** (13/11/2026, a pedido):
  a lista fica à esquerda e, à direita dela, uma coluna estreita com Disco em
  cima e Rede embaixo. Cada disco ocupa duas linhas curtas (atividade, depois
  leitura e escrita). Com a janela abaixo de 940 de largura, ou com a lista
  de processos escondida, disco e rede voltam a ocupar a largura toda, lado a
  lado. Na configuração, "Disco e rede" deixou de ter posição na ordem: só
  liga e desliga. A janela passou a abrir mais larga para caber.

Limites desta rodada:

- A lista de processos fica sempre por último, com disco e rede ao lado; as
  outras seções mudam de ordem livremente.
- Não há espaço em disco (só atividade e taxa), nem arquivos abertos por
  processo, que exigiria API não documentada.
- O intervalo de coleta ainda não é configurável pela janela.

## Como roda hoje

Um executável, dois processos:

```
prumo.exe --coletor   mede, guarda 60 s de histórico, publica no pipe
prumo.exe             janela: lê do pipe e desenha
```

- **Pipe nomeado local** (`\\.\pipe\prumo-coletor`), só de saída: o coletor
  escreve e não aceita nenhum pedido de quem conecta. Conexões de rede são
  recusadas, e a leitura é restrita ao usuário atual e ao sistema.
- **A janela inicia o coletor** se ele não estiver rodando. Iniciado assim, o
  coletor sai sozinho 15 s depois que a última janela fecha. Iniciado à mão
  com `--coletor`, fica até receber `prumo --parar`.
- **Só um coletor por vez;** um segundo percebe e sai.
- **Ao conectar, a janela recebe o histórico** que o coletor já tinha, então a
  linha do tempo não começa vazia.
- **Cliente lento perde amostras** em vez de atrasar o coletor.

Controle (`src/controle.rs`): prioridade e, desde 13/11/2026, encerramento.

Três níveis de proteção, valendo para as duas ações:

| Nível | Quem é | O que o Prumo permite |
|---|---|---|
| Livre | Programa comum | Prioridade e encerramento |
| Windows | Executável dentro da pasta do Windows, ou processo cujo caminho o Windows não deixa consultar | Bloqueado por padrão. Liberado pela opção "Liberar processos do Windows", em Configurar |
| Crítico | System, Registry, Secure System, Memory Compression, smss, csrss, wininit, winlogon, services, lsass, lsaiso | Nunca, nem com a opção ligada |

- Um grupo vale pelo nível mais restritivo entre seus processos.
- É uma trava contra engano, não uma barreira de segurança. Quem decide de
  fato é o Windows: sem elevação, ele recusa encerrar processos do sistema
  mesmo com a opção ligada.

Prioridade:

- Cinco classes, de Baixa a Alta. "Tempo real" não é oferecida.
- Vale para todos os processos do grupo e dura até o programa fechar.
- Uma recusa do Windows é mostrada ("recusou N") e não há nova tentativa.
- Também por linha de comando: `prumo --prioridade PID baixa|abaixo|normal|acima|alta`.

Encerramento:

- Pelo menu do botão direito (o grupo todo) ou pelo botão na gaveta (um
  processo só).
- Sempre pede confirmação, dizendo quantos processos fecham e que o que não
  foi salvo se perde.
- Fecha na hora, sem pedir ao programa que se feche sozinho.
- O Prumo não encerra o próprio processo.
- Também por linha de comando: `prumo --encerrar PID`, com as mesmas travas.

Testado em 13/11/2026, em processos criados para o teste: programa comum
encerra; processo da pasta do Windows fica bloqueado com a opção desligada
(encerramento e prioridade) e encerra com ela ligada; o processo System
(pid 4) é recusado pelo próprio Windows mesmo com a opção ligada. Não testado:
o menu, o botão da gaveta e a janela de confirmação, que dependem de clique.

Serviços do Windows (13/11/2026). Caso que motivou: o `AnyDesk.exe` aparecia
com CPU zero e não aceitava prioridade nem encerramento. Era o serviço do
AnyDesk, rodando na conta LocalSystem, na sessão dos serviços; o Windows não
deixa um usuário sem elevação mexer nele, nem consultar o caminho. O Prumo
chamava isso de "processo do Windows", o que estava errado.

- O coletor lista os serviços em execução e o processo de cada um (API
  documentada, sem privilégio) e lê uma vez a configuração de cada serviço:
  tipo de início, conta e executável.
- Quando o Windows não deixa consultar o processo, o executável vem da
  configuração do serviço. Com isso o AnyDesk deixou de cair no nível
  "Windows" por falta de caminho.
- A gaveta mostra os serviços do programa: nome, tipo de início e conta.
- O filtro também procura no nome dos serviços.
- Antes de oferecer prioridade ou encerramento, o Prumo testa se o Windows
  deixa (abre e fecha o processo, sem mudar nada). Se não deixa, o menu
  explica o motivo em vez de oferecer uma ação que seria recusada: para
  serviço, diz o nome, a conta e aponta o `services.msc`; para os demais, diz
  que pertence a outra conta ou é protegido.
- O Prumo ainda não para nem desativa serviços. Exige administrador.

Conferido na tela: o AnyDesk aparece com o caminho em Program Files e a linha
"AnyDesk Service (AnyDesk) · início automático · conta LocalSystem". O texto
do menu não foi conferido, porque depende de clique.

Limite conhecido: pela linha de comando, um processo crítico cujo caminho não
pode ser consultado aparece como "Windows", não como "Crítico", porque o nome
não é conhecido ali. Na janela o nome vem do coletor e a lista de críticos
vale.

Custo depois de acrescentar disco, rede, E/S e caminho por processo: coletor
0,18 a 0,21% de CPU e 18,6 MB; janela 0,01 a 0,44% de CPU e 73 MB.

Testado em 13/11/2026: separação dos processos, saída automática, parada,
proteção contra segundo coletor e troca de prioridade num processo de teste
(Normal, Abaixo do normal e de volta). Não testado: os cliques na janela
(ordenação, filtro e menu), que dependem de alguém usando.

Fora do controle por enquanto: afinidade de núcleos, modo de eficiência,
regras automáticas, e parar ou desativar serviços do Windows pelo
gerenciador de serviços (hoje o Prumo só encerra processos).

### Relatório de gravação

Direção aprovada em 12/11/2026, com ajustes:

1. **Cabeçalho:** aplicativo, data, duração gravada, tempo com quadros e
   tempo de atividade do computador no início da gravação.
2. **Resumo:** FPS médio, 1% low e 0,1% low com o método ao lado, pior quadro
   e número de engasgos.
3. **Gráfico de frametime** da sessão inteira, com engasgos marcados e pausas
   sombreadas e fora do cálculo.
4. **Lista de engasgos:** instante, duração e o que aconteceu junto, ou
   "nada externo encontrado".
5. **Clicar num engasgo** abre a linha do tempo completa daquele instante,
   com CPU, GPU, memória e processos.

Decisões:

- **Engasgo não é limite fixo.** Frametime acima de 16 ms pode ser só o jogo
  pesado para a CPU ou a GPU, de forma sustentada. Isso é carga, não engasgo.
  O critério de engasgo fica em aberto até testes com jogos reais.
- **Comparação entre sessões fica para o futuro.**

### Overlay

Direção aprovada em 12/11/2026. Painel pequeno num canto, sem clique e sem
foco:

1. FPS em destaque, com o frametime em ms ao lado.
2. Gráfico curto do frametime recente.
3. CPU e GPU (com o nome do motor) numa linha; VRAM em outra.
4. Indicador "gravando" com o tempo, quando houver gravação.

Posição, tamanho e quais linhas aparecem são configuráveis por jogo.

### Drawer

Direção aprovada em 12/11/2026. Sobe do centro da parte de baixo, com cerca
de 80% da largura, deixando o resto da tela visível:

1. Os quatro blocos de resumo da janela principal, compactos.
2. A barra de memória com a conta fechada, em versão fina.
3. Os três grupos de processos mais ativos no momento.
4. Rodapé com os atalhos: segurar para espiar (soltar fecha), uma tecla para
   abrir e interagir, outra para fechar.

O drawer não tem linha do tempo nem núcleos: é resumo. Quem quer o detalhe
abre a janela principal. O atalho padrão está em aberto.

## Distribuição

- **Repositório:** https://github.com/TheNewGuy100/Prumo (público, desde
  13/11/2026).
- **CI** (`.github/workflows/ci.yml`): a cada envio para `main` e a cada pull
  request, compila no Windows com as dependências travadas e guarda o
  executável como artefato.
- **Releases** (`.github/workflows/release.yml`): enviar uma tag `vX.Y.Z`
  compila e publica a release com o `.zip` (executável, licença, README e
  glossário) e o arquivo de verificação SHA-256. A tag precisa bater com a
  versão do `Cargo.toml`, senão o workflow falha de propósito. As releases
  saem marcadas como pré-lançamento enquanto o executável não for assinado.
- **Para publicar uma versão nova:** mudar a versão no `Cargo.toml`,
  fazer o commit, criar a tag `vX.Y.Z` e enviá-la.
- **Gratuito e de código aberto.** Nada é cobrado.
- **Licença: GPLv3.** Derivados têm que continuar abertos.
  - Código MIT e Apache-2.0 pode ser incorporado (crates Rust, PresentMon).
- **Assinatura de código** só quando houver versão para distribuir. Por ser
  aberto, as opções de menor custo ficam disponíveis (SignPath Foundation,
  Certum Open Source); conferir regras e preços na época.
- **Sem driver de kernel**, para não exigir assinatura de driver e para não
  entrar em conflito com anti-cheat.
- **Regras de convivência com anti-cheat:**
  - Nunca abrir o processo de um jogo com leitura de memória.
  - Não esconder o overlay da captura de tela.
  - Controle (prioridade, afinidade) desligado ou tolerante a recusa em jogos
    protegidos.
  - Overlay desligável por jogo; a gravação funciona sem ele.

## Ordem de construção

1. **Coletor** com CPU, processos e memória total, mais medição do próprio custo.
2. **Memória com a conta fechada** por diferença, só com contadores.
3. **Janela** no modo fixo, com a linha do tempo.
4. **Modo drawer** e atalho global.
5. **GPU por processo e frametime** (captura própria via ETW).
6. **Modo gravação** e relatório da sessão.
7. **Overlay próprio** em janela transparente.
8. **Controle** de processos.

## Primeira versão (proposta, não aprovada)

O menor recorte que já é útil e que roda inteiro sem privilégio:

| Entra | Fica para depois |
|---|---|
| Coletor: CPU total, por núcleo e por processo | Frametime e auxiliar elevado |
| Coletor: memória com a conta fechada por diferença | Modo gravação e relatório |
| Coletor: GPU por motor e por processo, VRAM | Overlay |
| Janela principal no modo fixo, sem a curva de frametime | Drawer e atalho global |
| Custo do próprio Prumo à mostra | Controle de processos |
| Linha "vida curta e não atribuído" | Detalhe do compartilhado |

Critério de pronto: a janela aberta no segundo monitor durante uma compilação
e uma sessão de Unity, com as somas fechando e o custo do Prumo abaixo do
orçamento, medido.

Por que este recorte: prova a tese central (números que fecham), mede o
orçamento em código nativo e não depende de nenhum dos riscos ainda sem teste
(overlay, anti-cheat, Vulkan e OpenGL).

## Riscos e buracos conhecidos

Em ordem de gravidade. Os três primeiros mudam a arquitetura e devem ser
resolvidos antes de escrever o coletor de verdade.

### 1. Os números ainda não têm definição (resolvido em GLOSSARIO.md)

**Resolvido em 12/11/2026:** as definições de CPU, memória, GPU, VRAM, disco e
frametime estão em [GLOSSARIO.md](GLOSSARIO.md), com as medições que as
sustentam. Ficam em aberto o limite de engasgo e o método do 1% low, que
dependem do teste de ETW. O texto abaixo é o registro original do problema.

- **CPU:** o Windows tem duas medidas, tempo e "utility" (ponderada pela
  frequência), com valores diferentes. Falta escolher qual o Prumo mostra.
  A divergência que originou o projeto (96% no btop contra 52% no Gerenciador)
  nunca foi explicada.
- **GPU:** o uso é por motor (3D, decodificação de vídeo, cópia). Falta definir
  qual vira "o" número, e se a VRAM mostrada é dedicada, compartilhada ou ambas.
- **Memória compartilhada por processo:** uma página usada por dez processos
  não tem dono único. Falta escolher a regra (divisão proporcional, por exemplo).

Como resolver: escrever um glossário com a definição de cada número exibido.

### 2. Privilégio do coletor

Captura por ETW exige administrador (ou grupo específico do Windows) e é
obrigatória para o frametime. "Elevar só quando preciso" provavelmente não se
sustenta. Desenho realista: coletor como serviço do Windows com privilégio
alto, janela sem privilégio conversando com ele. Isso traz instalador,
assinatura de código e superfície de segurança a cuidar.

**Teste de privilégio feito em 12/11/2026:** iniciar uma sessão ETW sem
elevação falha com "Acesso negado" (0x80070005), mesmo com o usuário sendo
membro de Administradores. Os provedores necessários existem na máquina
(Microsoft-Windows-DXGI, DxgKrnl, Dwm-Core, D3D9).

Conclusão: o frametime exige que a captura rode elevada. Caminhos possíveis:

1. Coletor como serviço do Windows, instalado uma vez com elevação.
2. Usuário no grupo "Usuários de Log de Desempenho", que a documentação diz
   permitir sessões de trace sem administrador. Não testado.
3. Coletor comum sem frametime, com um auxiliar elevado só para a captura.

**Teste de captura feito em 12/11/2026** (`experimentos/frametime-etw.ps1`,
PowerShell elevado, 10 s, provedor Microsoft-Windows-DXGI):

| Medida | Resultado |
|---|---|
| Eventos de Present (ID 42 início, ID 43 fim) | 2.420 pares; os IDs estavam certos |
| App Claude (janela animando) | 2.417 quadros, 240,0 FPS, frametime médio 4,17 ms, P99 5,16 ms, máx. 6,07 ms |
| Unity | 3 quadros em 10 s: amostra inválida, o editor não estava redesenhando continuamente |
| Blender | Nenhum evento |
| Volume total | 2.916 eventos/s, 3,05 MB em 10 s |
| Eventos que não são Present | 83% (IDs 138, 139, 144, 145) |

Conclusões:

- **A captura funciona e a resolução basta.** 4,17 ms de média é exatamente
  240 Hz, com variação de quadro a quadro visível.
- **Precisa filtrar na origem.** Só 17% dos eventos interessam. Filtrando por
  ID ou palavra-chave, o volume cai na mesma proporção. Guardando só o
  instante de cada quadro, uma hora a 240 FPS dá cerca de 14 MB.
- **O provedor DXGI não cobre tudo.** Jogos em Vulkan e OpenGL não passam por
  ele. Para esses é preciso o provedor do kernel gráfico (DxgKrnl), como o
  PresentMon faz. Não testado.
- **Mede o intervalo entre chamadas de Present,** não o instante em que o
  quadro chega à tela. Suficiente para a primeira versão.
**Segunda captura, mesmo dia** (20 s, Unity em Play):

| Medida | Resultado |
|---|---|
| Unity | 1.368 quadros, 254,3 FPS médio, frametime médio 3,93 ms |
| P99 do frametime | 29,79 ms (equivale a 34 FPS) |
| Pior quadro | 297,58 ms |
| Tempo com quadros | cerca de 5,4 s dos 20 s capturados |
| Volume | 642 eventos/s, 1,18 MB em 20 s |

- **Validado com conteúdo renderizando de verdade.** A média diz 254 FPS e o
  P99 diz 34 FPS: é exatamente a diferença que o modo gravação existe para
  mostrar.
- **Pausa não é engasgo.** O Unity apresentou quadros em só 5,4 s dos 20 s.
  O coletor precisa distinguir "o aplicativo parou de desenhar" de "um quadro
  demorou", senão toda pausa vira um engasgo enorme. O pior quadro de 297 ms
  pode ser uma coisa ou outra; este teste não distingue.
- **Aplicativo sem quadros não tem FPS.** A interface mostra "sem quadros",
  não "0 FPS".
- Ainda não testado: jogo em Vulkan ou OpenGL, e jogo em tela cheia.

### 3. Memória detalhada depende de API não documentada (resolvido para a primeira versão)

A varredura estilo RAMMap usa estruturas que a Microsoft pode mudar a qualquer
versão, o que contraria o princípio de não quebrar com atualização alheia.
Os totais por categoria (privado, pools do kernel, cache, standby, livre) saem
de contadores documentados e baratos; a varredura só é necessária para detalhar
o compartilhado.

**Teste feito em 12/11/2026** (contadores de desempenho documentados, 235
processos, PC de desenvolvimento):

| Parcela | GB | Fonte |
|---|---|---|
| Em uso (total − standby − livre − modificada) | 21,37 | Contadores de memória |
| Privado dos processos | 15,04 | Soma do working set privado |
| Kernel (pools, cache, código e drivers do sistema) | 1,72 | Contadores de memória |
| **Resto não atribuído** | **4,60** | — |

O caminho documentado e barato fecha **78,5%** da conta. O resto bate com o
que o RAMMap mostrou antes: cerca de 3,6 GB de arquivo mapeado e memória
compartilhada, mais cerca de 1 GB de estruturas do kernel sem contador
(tabelas de página, PTEs do sistema, memória travada por driver).

**Segundo teste, mesmo dia: `QueryWorkingSetEx`** (API documentada que
informa, por página de cada processo, se ela é compartilhada e por quantos).
Script em `experimentos/memoria-compartilhada.ps1`. Resultado: **não serve
para fechar a conta.**

| Medida | Resultado |
|---|---|
| Tempo para varrer tudo | 118 s (18,3 milhões de páginas, protótipo em C# via PowerShell) |
| Processos abertos sem administrador | 154 de 250; os 96 negados somam 3,95 GB de working set |
| Privado na RAM | 14,86 GB (confere com os 15,04 dos contadores) |
| Compartilhado bruto (contado em cada processo) | 9,08 GB: 5,66 imagem + 3,42 arquivo mapeado |
| Contagem de donos por página | 7 em todas as páginas, o valor máximo do campo |

Três problemas:

- **A contagem de donos não deduplica.** O campo tem 3 bits e satura em 7.
  Veio 7 em todas as páginas compartilhadas, inclusive num teste só com o
  `explorer`, onde o executável próprio deveria ter 1 dono. Ou o campo não é
  confiável nesta versão do Windows (build 26200), ou há erro de leitura não
  encontrado. Nos dois casos, não dá para transformar 9,08 GB brutos nos
  ~3,6 GB reais.
- **Lento.** Cerca de 6 a 10 µs por página. Código nativo deve melhorar, não
  medido, mas a ordem de grandeza é de segundos a minutos.
- **Acesso negado em 38% dos processos** sem administrador.

**Decisão adotada em 12/11/2026:**

1. Primeira versão usa só contadores: programas (privado), Windows (kernel) e
   um bloco "compartilhado e outros" calculado por diferença. A soma fecha em
   100% por construção, com um bloco de cerca de 20% sem detalhe interno.
2. Por processo, mostrar privado e total lado a lado (ex.: Unity 3,0 GB
   privado, 4,5 GB com compartilhado), explicando a diferença.
3. O detalhamento do bloco (estilo RAMMap) fica como recurso opcional, sob
   demanda e com administrador, usando a API não documentada e falhando com
   aviso se o Windows mudar as estruturas.

A leitura por WMI levou 75 ms (memória) e 126 ms (processos). É o caminho mais
lento disponível; não serve de referência para o custo do coletor nativo.

### 4. "O que causou o engasgo" é correlação, não causa

O Prumo sabe o que aconteceu ao mesmo tempo, não o que causou. Muitos engasgos
vêm de dentro do jogo (compilação de shader, carregamento de asset) e não
deixam sinal no sistema. A interface deve dizer "aconteceu junto" e admitir
"nada externo encontrado", nunca afirmar causa.

### 5. Anti-cheat: risco menor, não zero

Sem injeção o risco cai muito, mas janela transparente sobre o jogo é a mesma
técnica de cheats externos, e alterar prioridade ou afinidade exige abrir o
processo do jogo. O tamanho real do risco por jogo é desconhecido. As regras de
convivência estão na seção Distribuição.

### 6. Orçamento nunca medido

Menos de 1% de CPU e 50 MB é meta, sem número por trás. Eventos por quadro via
ETW e GPU por processo são as coletas com mais chance de estourar. O overlay em
janela pode acrescentar latência enquanto visível; medir.

### 7. O controle pode trair a proposta

"Limpar working set" quase sempre é placebo: o número cai e o desempenho piora.
Não oferecer sem aviso claro. Afinidade em processo do sistema pode
desestabilizar a máquina; proteger ou avisar.

### 8. Escopo sem primeira versão definida

Oito etapas para uma pessoa, sem um ponto que diga "já é útil". Falta definir
a versão mínima. Só há um hardware de teste (Ryzen 7 5700X, RTX 3060): CPU
híbrida com núcleos P/E, GPU AMD/Intel e notebook ficam sem validação.

### 9. A tela ainda não foi desenhada

A motivação original era "falta integração e visualização", e até aqui só a
coleta foi projetada e testada. Não existe esboço da janela, do drawer, do
overlay nem do relatório de gravação. É a maior parte ainda em branco.

### 10. Processos de vida curta escapam da amostragem

Com coleta a cada 1 s, um processo que nasce e morre em menos tempo nunca é
visto. Numa compilação (dezenas de `rustc` curtos), a soma dos processos fica
abaixo do total de CPU. Sem ETW não há como atribuir esse uso. Saída coerente
com o resto do projeto: mostrar a diferença como bloco "processos de vida
curta e não atribuído", e atribuir de verdade quando o módulo elevado estiver
ativo.

### 11. O auxiliar elevado é superfície de ataque

Um processo elevado que aceita pedidos de um processo comum pode virar caminho
de escalada de privilégio. Regra: o auxiliar só aceita "iniciar" e "parar",
só emite dados de frametime e não executa nada que venha de fora.

### 12. Nada foi medido em código nativo (parcialmente resolvido)

**Medido em 12/11/2026 com o coletor v0 em Rust** (`src/main.rs`, sem
dependências externas, lendo contadores PDH, 235 processos, intervalo de 1 s):

| Medida | Resultado | Orçamento |
|---|---|---|
| CPU do coletor | 0,00 a 0,19% da máquina | menos de 1% |
| Memória do coletor | 13,4 MB | menos de 50 MB |
| Tempo de cada coleta | 8,5 a 23,6 ms | — |
| Executável | 196 KB | — |

O coletor sozinho cabe no orçamento com folga. Ainda fora da medida: a janela,
o histórico em buffer e a captura de frametime. A sincronização de relógio
entre as fontes da linha do tempo continua sem projeto.

**Medido no mesmo dia com a janela principal aberta** (`src/janela.rs`, egui
0.37 via eframe com OpenGL, redesenho só quando chega amostra):

| Medida | Resultado | Orçamento |
|---|---|---|
| CPU em regime, janela aberta | 0,17 a 0,19% da máquina | menos de 1% |
| Memória, janela aberta | 89 a 146 MB de working set | menos de 50 MB |
| Executável | 4,4 MB | — |

A CPU cabe com folga. **A memória estoura o orçamento com a janela aberta.**
O coletor sozinho usa 13,4 MB, então o excedente é da janela: contexto
OpenGL, driver gráfico e fontes. Caminhos a avaliar, nenhum testado:

1. Manter coletor e janela em processos separados, como no desenho original.
   O orçamento de 50 MB vale para o coletor, que fica sempre ligado; a janela
   só custa enquanto está aberta.
2. Trocar o backend gráfico (wgpu com DirectX no lugar de OpenGL) e medir.
3. Aceitar um orçamento próprio para a janela aberta.

**Medido em 13/11/2026 com coletor e janela em processos separados**
(caminho 1, adotado):

| Processo | CPU em regime | Working set | Privada |
|---|---|---|---|
| Coletor (sempre ligado) | 0,12% da máquina | 17,6 MB | 9,9 MB |
| Janela (só enquanto aberta) | 0,04% da máquina | 76,3 MB | 82,3 MB |

O coletor cabe no orçamento de 50 MB com folga. A janela custa cerca de 76 MB
enquanto está aberta; trocar o backend gráfico para reduzir isso continua sem
teste.

Observações da primeira execução:

- As somas fecham: memória por categoria soma o total, e CPU dos processos
  mais "vida curta e não atribuído" soma o total.
- "Vida curta e não atribuído" ficou em 0,1 a 2,8% com a máquina em uso leve.
  Parte disso é defasagem entre contadores, não só processo de vida curta.
  Falta medir durante uma compilação.
- Os lógicos estão agrupados por núcleo físico, pela topologia que o Windows
  informa (8 núcleos de 2 lógicos nesta máquina).
- A janela e o coletor rodam em processos separados desde 13/11/2026.

### Limitações conhecidas e aceitas

- **Temperatura da CPU fora da primeira versão.** Ler os sensores exige driver
  de kernel, que o projeto não usa. Possível no futuro como integração
  opcional com HWiNFO ou LibreHardwareMonitor já abertos pelo usuário.
- **GPU sem privilégio (medido):** uso por motor, por processo e VRAM vêm dos
  contadores do Windows sem elevação. Temperatura e consumo da GPU dependem
  da biblioteca do fabricante; NVIDIA não testado, AMD e Intel sem hardware.

### Menores

- Detecção de "o que é um jogo" para a gravação automática.
- Tamanho da gravação em disco por hora.
- Conflito de atalho global com jogos e outros programas.
- Overlay em "tela cheia" com otimizações do Windows: esperado que funcione,
  não testado.

## Decisões em aberto

| Decisão | Opções | Inclinação |
|---|---|---|
| Linguagem do coletor | **Decidido em 12/11/2026: Rust** | Baixo custo, bom acesso a Win32. Risco restante: ETW em Rust, que só entra com o frametime, depois da primeira versão |
| Tecnologia da janela | **Decidido em 12/11/2026: egui** (eframe 0.37) | Medido: 89 a 146 MB com a janela aberta, acima do orçamento; ver risco 12 |
| Controle | Manual na hora, ou regras automáticas por programa | Manual primeiro; regras depois |
| Privilégio | Sempre administrador, ou elevar só para a memória detalhada | Elevar só quando preciso |
| Outras GPUs | Só NVIDIA no início, ou AMD/Intel junto | Contadores do Windows cobrem as três; NVML como extra |
