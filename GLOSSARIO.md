# Glossário dos números

O que cada número exibido pelo Prumo significa, de onde vem e por que foi
escolhido. Nenhum número entra na interface sem estar definido aqui.

Medições de referência feitas em 12/11/2026 no PC de desenvolvimento
(Ryzen 7 5700X, 16 processadores lógicos, RTX 3060 12 GB, 32 GB de RAM),
com `experimentos/numeros.ps1`.

## Regras gerais

1. **Todo número tem unidade, denominador e janela.** "CPU 60%" é incompleto;
   "60% do tempo dos 16 processadores lógicos, média de 1 s" é completo.
   A interface mostra o curto e explica o completo ao passar o mouse.
2. **A mesma palavra nunca nomeia duas medidas.** "Memória" sozinha não
   aparece; aparece "privada", "total com compartilhado" e assim por diante.
3. **As partes somam o todo.** Quando não dá para atribuir uma parte, ela
   aparece como bloco próprio com nome honesto, nunca some.
4. **O que é estimativa ou diferença é marcado como tal.**

## CPU

O Windows oferece duas medidas, e elas divergem:

| Cenário medido | Tempo | Utility | Desempenho |
|---|---|---|---|
| Máquina em uso leve | 9 a 17% | 13 a 19% | ~120% |
| Mais 8 threads em laço | 60 a 66% | 75 a 80% | ~125% |

- **Tempo** (`% Processor Time`): fração do tempo em que os processadores
  lógicos não estavam ociosos.
- **Utility** (`% Processor Utility`): o tempo ponderado pela frequência.
  Com turbo, um núcleo ocupado conta mais que 100%. Na medição,
  Utility ≈ Tempo × Desempenho.
- **Desempenho** (`% Processor Performance`): frequência atual em relação à
  nominal. Acima de 100% significa turbo.

Com 8 threads ocupadas em 16, as duas medidas diferem em 15 pontos. Uma
ferramenta que mostra uma e outra que mostra a outra nunca vão concordar.

### Definições adotadas

| Número | Definição | Fonte |
|---|---|---|
| **Uso de CPU** (total) | Tempo: % do tempo dos processadores lógicos ocupados, na janela | `% Processor Time`, `_Total` |
| **Uso por núcleo** | Tempo, por processador lógico | idem, por instância |
| **Uso por processo** | Tempo do processo ÷ tempo total disponível (0 a 100% da máquina) | contadores de processo |
| **Em núcleos** | A mesma medida em núcleos equivalentes (ex.: "3,2 núcleos") | derivado |
| **Frequência** | Desempenho, mostrado à parte como "turbo 125%" | `% Processor Performance` |

Por que Tempo e não Utility:

- A soma dos processos fecha com o total (medido: 59 a 64% contra 60 a 66%).
  Com Utility não fecha.
- Vai de 0 a 100% sem estourar.
- A frequência é informação útil, mas é outra informação. Misturada no mesmo
  número, esconde as duas.

### Avisos que a interface precisa dar

- **50% não é metade da capacidade.** Este processador tem 8 núcleos físicos
  e 16 lógicos. Com 8 threads, uma por núcleo físico, o Tempo marca ~60%, mas
  a maior parte da capacidade real já está em uso: o segundo lógico de cada
  núcleo rende bem menos que o primeiro. O Prumo agrupa os lógicos por núcleo
  físico para isso ficar visível.
- **A divergência do print original (btop 96%, Gerenciador 52%) continua sem
  explicação completa.** A diferença entre Tempo e Utility explica uns 15
  pontos, não 44. O mais provável é que as duas ferramentas tenham amostrado
  fases diferentes de uma compilação. Não foi reproduzido.

## Memória (RAM)

Definições resultantes dos testes registrados no PROJETO.md, risco 3.

| Número | Definição | Fonte |
|---|---|---|
| **Em uso** | Total − cache liberável − livre − modificada | contadores de memória |
| **Programas** | Soma da memória privada de todos os processos | working set privado |
| **Windows** | Pools do kernel, cache, código e drivers do sistema residentes | contadores de memória |
| **Compartilhado e outros** | Em uso − Programas − Windows. **Calculado por diferença** | derivado |
| **Cache liberável** | Standby: conteúdo de arquivos mantido na RAM, descartado na hora se alguém precisar | contadores de standby |
| **Livre** | Páginas livres e zeradas | contador |
| **Modificada** | Páginas aguardando escrita em disco | contador |

Por processo:

| Número | Definição |
|---|---|
| **Privada** | Memória na RAM que só esse processo usa. É o que some se ele fechar |
| **Total com compartilhado** | Working set: privada mais as páginas compartilhadas que ele toca. **Não é somável** entre processos |

**Comprometida** (commit) aparece separada da RAM, com o próprio limite
(RAM + arquivo de paginação). Nunca ao lado do total de RAM: na medição
original eram 31,3 GB comprometidos contra 31,9 GB de RAM, o que parecia
memória esgotada sem ser.

## GPU

O uso de GPU não é um número só. A placa tem vários motores independentes, e
o Windows mede cada um por processo. Na medição havia 31 motores e 447
combinações de processo e motor.

| Medida | Valor medido |
|---|---|
| Motor mais ocupado (3D) | 8,8% |
| Soma de todos os motores | 9,1% |
| `nvidia-smi` (driver) | 12% |

Os três são "uso de GPU" e nenhum está errado. O driver mede a fração do
tempo em que qualquer trabalho rodava, com janela própria.

### Definições adotadas

| Número | Definição | Fonte |
|---|---|---|
| **Uso de GPU** (total) | O motor mais ocupado, sempre com o nome dele: "3D 62%" | contadores `GPU Engine` |
| **Por tipo de motor** | 3D, decodificação de vídeo, codificação de vídeo, cópia, computação, mostrados quando acima de zero | idem, somado por motor |
| **Por processo** | Uso do processo em cada tipo de motor | idem, por processo |

Por que o motor mais ocupado: é o gargalo real e é o critério do Gerenciador
de Tarefas. A soma dos motores pode passar de 100% e não significa nada.

O número do driver (`nvidia-smi`, NVML) pode aparecer como extra para NVIDIA,
rotulado como "driver", nunca substituindo o principal.

## VRAM

| Medida | Valor medido |
|---|---|
| Dedicada, pelo adaptador | 8,16 GB |
| Dedicada, somando os processos | 8,63 GB |
| `nvidia-smi` | 8,15 GB (8343 MiB) |
| Compartilhada | 0,82 GB |

O total do adaptador confere com o driver. A soma dos processos passa do total
em 0,47 GB: é a mesma contagem em dobro da RAM, com recursos compartilhados
entre processos.

### Definições adotadas

| Número | Definição | Fonte |
|---|---|---|
| **VRAM em uso** | Dedicada do adaptador, sobre o total da placa | `GPU Adapter Memory` |
| **VRAM por processo** | Dedicada do processo. **Não é somável** com exatidão; a diferença para o total é mostrada | `GPU Process Memory` |
| **Memória de GPU compartilhada** | RAM do sistema emprestada à GPU. Mostrada à parte, com o aviso de que já está contada na RAM | idem |

## Disco

"Disco" sozinho não aparece. São três medidas com nomes próprios:

| Número | Definição |
|---|---|
| **Atividade** | % do tempo em que o disco estava atendendo pedidos |
| **Taxa** | Leitura e escrita em MB/s, total e por processo |
| **Espaço** | Ocupado sobre a capacidade |

Implementado: atividade (100% menos o tempo ocioso do disco) e taxa, por
disco físico. Espaço ainda não.

## E/S por processo

| Número | Definição |
|---|---|
| **E/S** | Bytes lidos mais bytes escritos por segundo pelo processo |

Não é só disco: o contador do Windows soma arquivos, rede e dispositivos. A
interface diz "E/S", nunca "disco", para esse número.

## Rede

| Número | Definição |
|---|---|
| **Recebido / enviado** | Bytes por segundo, por interface de rede |

Rede por processo não aparece: o Windows só informa isso por captura de
eventos, que exige elevação.

## Frametime

Captura por ETW validada em 12/11/2026 (ver PROJETO.md, risco 2): Unity em
Play com 254 FPS médio e P99 de 29,79 ms. O frametime medido é o intervalo
entre chamadas de apresentação, não a chegada do quadro à tela.

Regras que saíram do teste:

- **Sem quadros não é 0 FPS.** Aplicativo que não está desenhando aparece
  como "sem quadros".
- **Pausa não é engasgo.** Um intervalo longo em que o aplicativo parou de
  desenhar (menu, alt-tab, fim do Play) encerra um trecho e começa outro.
  Não entra no cálculo de frametime nem de 1% low. O limite que separa pausa
  de engasgo está em aberto.

| Número | Definição |
|---|---|
| **Frametime** | Tempo entre a apresentação de dois quadros consecutivos, em ms |
| **FPS médio** | Quadros apresentados ÷ tempo decorrido. Não é a média dos FPS instantâneos |
| **1% low** | FPS correspondente ao percentil 99 do frametime. Outras ferramentas usam a média dos 1% piores quadros e dão valor diferente; o Prumo mostra o método ao lado |
| **0,1% low** | O mesmo, com o percentil 99,9 |
| **Engasgo** | Quadro com frametime muito acima dos vizinhos. Nunca um limite fixo em ms. O critério exato está em aberto, a validar com jogos reais |
| **Carga sustentada** | Frametime alto de forma contínua (jogo pesado para a CPU ou a GPU). Não é engasgo e não entra na lista de engasgos |

## Tempo de atividade

| Número | Definição |
|---|---|
| **Tempo de atividade** | Tempo desde a última inicialização completa do Windows |

Aviso que a interface precisa dar: com a Inicialização Rápida do Windows
ligada, "Desligar" não zera esse contador; só "Reiniciar" zera. Um PC
desligado toda noite pode mostrar dias de atividade. A verificar na máquina
antes de fixar o texto do aviso.

No relatório de gravação, o valor registrado é o do início da sessão.

## Janela de tempo

Todo valor instantâneo é a média do último intervalo de coleta, e a interface
mostra qual é. Dois monitores com intervalos diferentes mostram números
diferentes para a mesma máquina; é a causa mais provável da divergência do
print original.

## Em aberto

- Critério de engasgo (relativo aos quadros vizinhos) e o que separa engasgo
  de carga sustentada.
- Método do 1% low: confirmar percentil ou oferecer os dois.
- Uso de CPU por processo em CPUs híbridas (núcleos P e E), sem hardware
  para testar.
- GPUs AMD e Intel: os contadores do Windows devem valer igual; não testado.
