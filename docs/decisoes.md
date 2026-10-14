# Registro de decisões

Por que o NarraTer é como é. Uma entrada por decisão que teve **alternativa real**. Se não havia
escolha, não é decisão.

**Este arquivo só cresce.** Nunca reescreva uma entrada: decisão revista ganha entrada nova que
cita a antiga. O histórico de mudança de ideia é informação. É o que permite responder "por que
não fizemos do jeito óbvio?" sem depender de memória.

Cada entrada tem quatro campos. **Contexto** é o problema, sem a solução. **Decisão** é o que
fizemos. **Alternativa recusada** é o caminho que alguém provavelmente vai propor de novo, e é o
campo mais importante. **Consequência** é o que isso custou ou travou.

As entradas D01 a D04 foram reconstruídas em 2026-10-14 a partir das mensagens de commit; as
seguintes são registradas na sessão em que a decisão foi tomada.

## Índice por área

| Área | Decisões |
|---|---|
| Comunicação entre agentes | D01, D02, D06 |
| CLI e distribuição | D03 |
| Escopo do produto | D04, D05 |

---

## D01 · O terminal é a API universal; o agente não precisa cooperar

`2026-10-01` · comunicação · commit `dcf3aaf` · mecânica em `src-tauri/src/screen.rs` e `ipc.rs`; estudo em [`pesquisa/maestri.md`](pesquisa/maestri.md)

**Contexto.** Entre 2026-07-05 e 2026-07-13 o NarraTer adotou um protocolo cooperativo: o ask
entregava `[narrater de X #id]` e esperava o alvo responder com `narrater reply <id>` ou a tool MCP
`reply_message` (`bb49171`), o idle vinha de um *Stop hook* (`e13adc9`), e as instruções iam por
`--append-system-prompt` e `--mcp-config`. Na prática só o Claude Code seguia o protocolo; um alvo
que não conhecia o `#id` deixava o ask travado até o timeout, e cada fornecedor pedia flags próprias.

**Decisão.** Ler o estado do turno da tela renderizada (vt100 headless por PTY), entregar o prompt
quando o alvo está ocioso e devolver a tela no fim do turno. Instruções por skills e por
`CLAUDE.md`/`AGENTS.md`. MCP e hooks removidos.

**Alternativa recusada.** Manter o protocolo cooperativo (resposta por ID, hooks, MCP), que é o
modelo que o Orca usa na orquestração dele.

**Consequência.** Qualquer agente que roda shell participa, inclusive de fornecedores diferentes
na mesma conversa. Em troca, a resposta é "a tela" (sem estrutura, limitada ao que cabe nela) e a
detecção de turno depende de heurística que precisa ser testada contra cada CLI (`turn_probe`).

## D02 · Criar uma aresta não notifica os agentes

`2026-07-13` · comunicação · commit `18b4163` · `resolve_target` em `src-tauri/src/ipc.rs`

**Contexto.** Cada aresta nova enfileirava uma notificação de sistema nos dois agentes, e cada uma
auto-submetia um turno: gastava tokens mesmo sem nenhuma mensagem trafegar.

**Decisão.** A aresta só atualiza a tabela de rotas do backend. O agente descobre os colegas na
hora de mandar (`narrater list`) e o backend valida cada envio.

**Alternativa recusada.** Avisar os agentes ao conectar, para que "saibam" da topologia.

**Consequência.** Conectar é gratuito. Os agentes não têm a topologia na memória e precisam
consultar `list`; as skills dizem isso explicitamente.

## D03 · A CLI `narrater` é um binário Rust, não um script Python

`2026-10-08` · CLI · commit `ea446c6` · `src-tauri/src/bin/narrater-cli/`

**Contexto.** A CLI é chamada pelos agentes a cada ask, check e note. O script Python levava
~58 ms por chamada e dependia de um Python no sistema e em cada workspace remoto.

**Decisão.** Reescrever em Rust só com `std` + `serde_json` (~2 ms por chamada), com paridade
conferida em 103 linhas de comando contra o script antigo.

**Alternativa recusada.** Manter o Python, mais fácil de editar.

**Consequência.** Um binário a mais no build (`beforeDevCommand` compila a CLI antes do app) e o
`.deb` já inclui `/usr/bin/narrater-cli`. Mudança na CLI exige recompilar.

## D04 · Sem portais Android; o foco é agentes em terminais no PC

`2026-10-12` · escopo · commit `0812446`

**Contexto.** Os portais de dispositivo (adb, emulador, tile de toque) puxavam dependências e
manutenção para uma área fora do núcleo do produto.

**Decisão.** Remover os portais Android, a skill `narrater-portal-devices` e as dependências
`image` e `roxmltree`. Cenas antigas abrem o nó como portal de browser vazio.

**Alternativa recusada.** Manter os portais como recurso secundário.

**Consequência.** Menos superfície. O app apaga a skill antiga onde a encontrar
(`RETIRED_SKILLS` em `ipc.rs`).

## D05 · Não fundir o NarraTer com o Orca; trazer as ideias

`2026-10-14` · escopo · commits `4c144b5`, `4a14591` · estudo em [`pesquisa/orca.md`](pesquisa/orca.md)

**Contexto.** O Orca (MIT) resolve o mesmo problema (agentes em terminais, worktrees, CLI para os
agentes) e tem uma camada de orquestração madura. A ideia era juntar os dois projetos.

**Decisão.** Manter os códigos separados. Portar ideias para o NarraTer e, como experimento
opcional, falar com uma instância do Orca pela CLI `--json` dele.

**Alternativa recusada.** Fork do Orca com o canvas dentro, ou o NarraTer embutido nele.
Electron/TypeScript contra Tauri/Rust, ~2 milhões de linhas contra ~20 mil, e um projeto que
publica versão quase todo dia: manter o fork custaria mais que o próprio NarraTer.

**Consequência.** O NarraTer mantém a identidade (canvas, topologia, zero cooperação). Uma ponte
com o Orca, se vier, depende de uma CLI de terceiros que muda rápido e precisa ser opcional.

## D06 · Na orquestração, cooperação é opcional e melhora o resultado; nunca é exigida

`2026-10-14` · comunicação · estudo em [`pesquisa/orca.md`](pesquisa/orca.md) §6 · plano em [`backlog.md`](backlog.md) etapa O

**Contexto.** O estudo do Orca mostrou o que perdemos com o ask puramente observado: nada é
durável, a resposta não tem estrutura nem sucesso/falha, o líder fica bloqueado até todos
terminarem e pergunta do worker se confunde com resposta. O Orca resolve isso com um contrato que
o worker precisa seguir (heartbeat, `worker_done` exatamente uma vez, `check` nos checkpoints).

**Decisão.** Trazer a durabilidade e o modelo de eventos (ask assíncrono, `wait`, asks em
SQLite) sem pedir nada ao agente, e oferecer mensagens tipadas (`done`, `question`, `reply`) como
caminho **opcional**: se o agente usar, o resultado fica estruturado; se não usar, a tela continua
valendo.

**Alternativa recusada.** Adotar o contrato do Orca (Run/Task/Dispatch, heartbeat e `worker_done`
obrigatórios). É o mesmo modelo cooperativo que abandonamos em D01, e o próprio Orca gasta boa
parte da skill tratando o caso do agente que não segue o contrato.

**Consequência.** Duas fontes para o fim de um ask (tela ou mensagem tipada), e o backend precisa
decidir qual vale quando chegam as duas. A skill dos agentes ganha comandos novos, mas nenhum é
obrigatório.
