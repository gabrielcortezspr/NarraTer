# Testes de comunicação entre agentes

O objetivo do NarraTer é a **comunicação entre agentes de IA rodando em
terminais no PC**. Esta pasta guarda os testes que mostram, com agentes
reais, se essa comunicação funciona: o que foi pedido, o que cada terminal
mostrou e o que deu errado.

Cada sessão de testes tem um relatório próprio, com data no nome. Os
cenários se repetem entre sessões, para dar para comparar versões.

## Relatórios

| Data | Relatório | Versão testada | Resumo |
|---|---|---|---|
| 2026-10-12 | [Agentes Claude Code reais](2026-10-12-claude-code-reais.md) | `9d23932` → `28b05d0` | 14 cenários; 3 bugs achados e corrigidos |

## Método

**Instância isolada.** O app roda com `XDG_CONFIG_HOME` e `XDG_DATA_HOME`
apontando para uma pasta temporária, com uma cena montada só para o teste.
As cenas e configurações do usuário não são tocadas.

**Agentes reais.** Os terminais rodam o Claude Code de verdade, sem flags
extras e sem `--dangerously-skip-permissions`. Eles aprendem o protocolo
pela skill `narrater` que o próprio app instala, como aconteceria com um
usuário. Nenhum prompt diz *como* usar o NarraTer além do que o cenário
pede.

**Operador.** Um terminal extra ("Operator") roda um shell e é ligado aos
agentes. O pesquisador age por ele com a CLI, exatamente como um agente
faria:
- `narrater ask "Agente" "…"` dá a tarefa e devolve a tela quando o turno
  acaba;
- `narrater check "Agente"` lê a tela de qualquer agente ligado, para ver
  o lado de quem recebeu a mensagem.

O operador **não** participa da comunicação que está sendo testada: ele só
inicia o cenário e observa. A topologia (quem está ligado a quem) é
definida na cena.

**O que se registra em cada cenário:** o pedido, a topologia, o tempo total
do `ask` do operador, a resposta final e trechos da tela de cada agente
envolvido. Um cenário passa quando a informação chega correta ao destino
pelo caminho que a topologia permite.

## Limites

- O operador vê as telas pelo `check`, que mostra só o que cabe na tela do
  terminal; respostas longas aparecem cortadas no início.
- O modelo e o modo de permissão vêm da configuração do Claude Code do
  usuário (nos testes de 2026-10-12: Opus 5.5, *auto mode*). Os tempos
  dependem disso.
