# Prumo

Monitor de recursos para Windows em que os números fecham.

O Gerenciador de Tarefas diz que há 22 GB de memória em uso, mas a lista de
processos soma 15. Outra ferramenta mostra os mesmos processos somando 26.
Nenhuma das duas explica a diferença. O Prumo nasceu dessa pergunta: mostra
CPU, memória, GPU, disco e rede de um jeito em que as partes somam o todo, e
diz com clareza o que cada número significa.

> **Estado: versão inicial.** Funciona no dia a dia, mas foi testado em uma
> máquina só (Windows 11, CPU AMD, GPU NVIDIA). O executável ainda não é
> assinado, então o Windows pode avisar na primeira execução.

## O que ele mostra

- **Memória com a conta fechada.** Uma barra em que programas, Windows,
  compartilhado, cache e livre somam o total. O que é calculado por diferença
  aparece marcado como tal, em vez de sumir.
- **CPU por tempo, com o turbo à parte.** Os processadores lógicos aparecem
  agrupados por núcleo físico, porque 50% de uso não é metade da capacidade.
- **GPU pelo motor mais ocupado,** com o nome do motor (3D, vídeo, cópia), e
  VRAM dedicada.
- **Disco e rede:** atividade e taxa por disco, tráfego por interface.
- **Processos agrupados por programa,** com memória privada e memória com
  compartilhado lado a lado, E/S, GPU e VRAM. A soma da lista fecha com o
  total, incluindo uma linha para o que não pôde ser atribuído.
- **Custo próprio à mostra.** O Prumo exibe quanto ele mesmo consome.

Cada número tem uma explicação ao passar o mouse. As definições completas
estão no [GLOSSARIO.md](GLOSSARIO.md).

## O que dá para fazer

- **Filtrar** por nome, caminho do executável, título de janela, linha de
  comando ou serviço. Procurar o nome de um projeto encontra o editor que
  está com ele aberto.
- **Ordenar** por qualquer coluna.
- **Abrir os detalhes** de um programa na própria linha: executável, janelas,
  linha de comando, serviços do Windows e cada processo.
- **Mudar a prioridade** e **encerrar** programas, pelo botão direito.
- **Organizar a tela:** escolher as seções, a ordem delas e as colunas.

### Travas do controle

| Nível | Quem é | O que o Prumo permite |
|---|---|---|
| Programa comum | Roda fora da pasta do Windows | Prioridade e encerramento |
| Processo do Windows | Roda de dentro da pasta do Windows | Bloqueado, até liberar em Configurar |
| Essencial | System, csrss, lsass, winlogon e outros | Bloqueado sempre |

Encerrar sempre pede confirmação. As travas protegem contra engano; quem
decide de fato é o Windows, que recusa mexer em processos do sistema sem
administrador. Quando isso acontece, o Prumo explica o motivo.

## Baixar

Pegue o arquivo `.zip` mais recente em
[Releases](https://github.com/TheNewGuy100/Prumo/releases), extraia e rode
`prumo.exe`. Não tem instalador e não precisa de administrador.

Requisitos: Windows 10 ou 11, 64 bits.

## Uso

```
prumo                      abre a janela
prumo --filtro TEXTO       abre já com a lista filtrada
prumo --abrir NOME         abre já com os detalhes de um programa
prumo --terminal           imprime as amostras no terminal
prumo --amostras N         o mesmo, limitado a N amostras
prumo --coletor            roda só o coletor, em segundo plano
prumo --parar              encerra o coletor
prumo --prioridade PID P   muda a prioridade (baixa, abaixo, normal, acima, alta)
prumo --encerrar PID       encerra um processo
prumo --intervalo-ms N     intervalo de coleta (padrão 1000, mínimo 100)
```

A configuração da tela fica em `%APPDATA%\Prumo\config.txt`.

## Como funciona

Um executável, dois processos:

- **O coletor** mede em segundo plano e guarda o último minuto de histórico.
  Custa cerca de 0,2% de CPU e 20 MB.
- **A janela** só desenha o que o coletor publica. Custa cerca de 80 MB
  enquanto está aberta. Ela inicia o coletor sozinha, e ele sai pouco depois
  que a última janela fecha.

Princípios:

- **Sem privilégio.** Tudo que a versão atual mostra vem de contadores de
  desempenho documentados do Windows, sem administrador.
- **Sem driver de kernel e sem injeção** em outros processos.
- **Não lê a memória de nenhum processo.** Só consulta informações básicas.
- **Nada sai da máquina.** O Prumo não usa a rede. Coletor e janela conversam
  por um canal local, restrito ao usuário que os executa.

## Limites conhecidos

- Rede por processo não aparece: o Windows só informa isso por captura
  elevada. A coluna E/S soma disco, rede e dispositivos.
- Não lista os arquivos abertos por um processo.
- Não para nem desativa serviços do Windows; só identifica.
- Sem temperatura de CPU, que exigiria driver de kernel.
- Frametime, gravação de sessão, overlay em jogo e gaveta por atalho estão
  projetados, mas ainda não implementados.
- Testado em uma configuração de hardware. CPUs híbridas e GPUs AMD ou Intel
  devem funcionar, mas não foram verificadas.

## Compilar

Precisa do [Rust](https://rustup.rs) com o toolchain MSVC.

```
cargo build --release
```

O executável sai em `target\release\prumo.exe`.

## Documentos

- [PROJETO.md](PROJETO.md): o projeto, as decisões, os testes e os riscos.
- [GLOSSARIO.md](GLOSSARIO.md): o que cada número significa e por quê.
- [experimentos/](experimentos): os scripts usados para medir antes de decidir.

## Licença

[GPLv3](LICENSE). Gratuito e de código aberto.
