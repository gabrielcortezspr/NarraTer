# Agent Teams (Claude Code): o que dá para reaproveitar

> Estudo de 2026-10-15 lendo a documentação oficial: <https://code.claude.com/docs/en/agent-teams>,
> `agents`, `sub-agents`, `costs`, `hooks` e o CHANGELOG do Claude Code. Não executamos o recurso:
> tudo aqui é `[citado]` da documentação, salvo onde está marcado `[inferido]`.

## O que é

Recurso experimental do Claude Code (desligado por padrão; `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`;
"research preview" desde a 2.1.32). Uma sessão é o **líder**, e os **companheiros** são sessões
completas e independentes do Claude Code, cada uma com o próprio contexto. Eles rodam dentro do
terminal do líder (`in-process`) ou em painéis do tmux ou do iTerm2. Só funciona com Claude e em
sessão interativa.

| Peça | Como funciona |
|---|---|
| **Caixa de correio** | Um JSON por agente em `~/.claude/teams/<time>/inboxes/<nome>.json`. Entrega automática: "o líder não precisa consultar" |
| **Mensagem direta** | Qualquer companheiro fala com qualquer outro pelo nome (`SendMessage`). Não há broadcast: uma mensagem por destinatário |
| **Lista de tarefas** | Em `~/.claude/tasks/<time>/`, com três estados (pendente, em andamento, concluída) e **dependências**: tarefa bloqueada não pode ser pega, e concluir uma destrava as dependentes. Pegar tarefa usa trava de arquivo. Quem termina pega sozinho a próxima livre |
| **Fim do turno** | "Quando um companheiro termina e para, ele avisa o líder **com a resposta final inteira** na notificação." Erro de API também é avisado |
| **Hooks de qualidade** | `TeammateIdle` (exit 2 devolve feedback e o mantém trabalhando), `TaskCreated` e `TaskCompleted` (exit 2 recusa criar ou concluir) |
| **Confiança** | Mensagem de outro agente nunca vale como do usuário: não aprova permissão nem dá consentimento |

Limitações declaradas:
- um time por sessão;
- sem times aninhados;
- o líder é fixo;
- `/resume` não restaura companheiros;
- o estado da tarefa às vezes atrasa;
- o encerramento pode ser lento.

Custo: "significativamente mais tokens que uma sessão". A página de custos cita cerca de 7× quando os
companheiros rodam em modo de plano.

Orientação oficial:
- Agent Teams quando os agentes precisam trocar achados e coordenar entre si; subagentes para tarefas
  focadas que só reportam;
- nada de times para trabalho sequencial, para o mesmo arquivo ou cheio de dependências;
- de 3 a 5 companheiros, com 5 ou 6 tarefas cada;
- tarefas autocontidas com entrega clara;
- Sonnet para os companheiros.

## Comparação com o NarraTer

| | Agent Teams | NarraTer hoje (D) |
|---|---|---|
| Fornecedores | Só Claude | Qualquer agente de terminal (D01) |
| Resultado ao líder | **Empurrado**: a resposta final inteira chega sozinha | **Puxado**: o líder roda `ask`/`wait` e recebe a tela (cortada) |
| Quem fala com quem | Todos com todos, por nome | Só quem tem aresta; no benchmark, só líder ↔ dev |
| Divisão do trabalho | Lista de tarefas com dependências, pega automática | O líder distribui pedido a pedido |
| Portão de qualidade | Hooks que recusam "concluído" | Nenhum |
| Visual | Painel de agentes ou painéis do tmux | Canvas com arestas e notas |

## O que reaproveitar (sem quebrar a D01)

1. **Resposta final inteira, empurrada.** É o que a O5 faz: ler a última mensagem do agente no
   transcript ao fim do turno, sem o agente cooperar. No Agent Teams isso já vem pronto, porque é o
   mesmo produto. Aqui lemos o arquivo que o Claude já escreve. `[inferido]` Também podemos empurrar,
   e não só devolver no `ask`: quando um dev que recebeu um pedido termina, a resposta já fica
   disponível ao líder sem um `check`.
2. **Lista de tarefas com dependências e pega automática** (backlog O8). `[inferido]` É a ideia que
   mais ataca o custo medido. No benchmark, cada fase nova passava pelo Narrador, com um turno do líder
   e o contexto relido. Se os devs pegam a próxima tarefa livre sozinhos, o líder sai do caminho
   crítico. Dá para fazer agnóstico: tarefas como nós do canvas, com `narrater task next/done`, e quem
   não cooperar recebe a tarefa por mensagem.
3. **Dev fala com dev.** `[inferido]` No benchmark, o contrato da interface entre DevA e DevB passava
   pelo Narrador. Uma aresta DevA ↔ DevB é configuração, não código. Dá para medir como variante do
   benchmark (topologia em triângulo contra estrela).
4. **Portão de qualidade opcional.** Um "concluído" que roda um comando (os testes, por exemplo) antes
   de aceitar, no espírito do `TaskCompleted`. `[inferido]` Pode cortar os pedidos separados de
   "verificação final" que o benchmark contou.
5. **Regra de dimensionamento** (3 a 5 agentes, tarefas autocontidas, Sonnet nos companheiros) vai
   para a skill do líder, como orientação e não como regra.

O que **não** reaproveitar:
- a dependência de arquivos internos do Claude (`~/.claude/teams`), que muda sem aviso e não existe
  para outros agentes;
- a aprovação automática de planos;
- o líder fixo, porque no canvas qualquer nó pode coordenar.
