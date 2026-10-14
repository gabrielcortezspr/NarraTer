# NarraTer × Orca: o que cada um faz e onde dá para juntar

> Estudo feito em 2026-10-14 lendo o código-fonte do Orca
> ([stablyai/orca](https://github.com/stablyai/orca), versão 1.4.214, licença
> MIT, © Lovecast Inc.) e do NarraTer (branch atual). O Orca **não foi
> executado** neste estudo, só lido: tudo sobre comportamento em runtime é
> leitura de código e das skills que ele distribui.

---

## TL;DR

Os dois resolvem o mesmo problema (vários agentes de IA em terminais, num app
desktop, com worktrees e uma CLI que os próprios agentes usam), mas por
caminhos opostos:

| | **NarraTer** | **Orca** |
|---|---|---|
| Metáfora | Canvas infinito: terminais são nós, arestas dizem quem fala com quem | IDE: repo → worktree → abas/splits de terminal, com sidebar de status |
| Como sabe o estado do agente | **Observa a tela** (vt100 headless + heurística de rodapé) | **Hooks** instalados em ~25 CLIs de agente (POST para um hook server local) **+ regras de tela declarativas** em JSON |
| Como os agentes se falam | `narrater ask` síncrono: digita no alvo, espera ficar ocioso, devolve a tela. O alvo não precisa cooperar | `orca terminal send/read/wait` (equivalente ao nosso) **+** `orca orchestration`: Runs, Tasks em DAG, Dispatches, mailbox durável em SQLite, `ask/reply` por ID, `worker_done`, gates de decisão. O worker **coopera** seguindo uma skill |
| Quem pode falar com quem | Só quem está ligado por aresta | Qualquer mailbox endereçável (não há topologia) |
| Stack | Tauri 2 + Rust (~10,7 mil linhas) + React 18 (~9 mil) | Electron 43 + React 19 + node-pty + `@xterm/headless` (~2 milhões de linhas de TS) |
| Linux | AppImage/deb planejados | AppImage, deb e rpm já distribuídos; piso glibc 2.31; clipboard Wayland nativo; computer-use para Linux |
| Extras | Portais (Chrome via CDP), notas, sketch, rotinas, chat, modo Maestro | App mobile + relay na nuvem, GitHub/Linear/Jira, diff review, editor de código, busca em sessões, uso/contas, browser embutido, `orca serve` headless |

**Conclusão:** juntar os códigos não é viável (stacks incompatíveis e uma
diferença de ~100× em tamanho). Mas há uma correlação forte de conceito, e
dá para aproveitar o Orca de duas formas: **trazer para o NarraTer as ideias
que o Orca resolveu melhor** e, como experimento, **fazer o NarraTer enxergar
e conversar com uma instância do Orca pela CLI dele**.

---

## 1. Como o Orca funciona no Linux

### 1.1 Processos

- **Electron main** (`src/main`): gerencia PTYs (node-pty), um daemon de
  terminais que sobrevive a updates (`src/main/daemon`), git/worktrees, SSH,
  WSL, browser embutido, hooks, orquestração e o runtime RPC.
- **Renderer** (`src/renderer`): React 19 + xterm.js com WebGL, splits,
  editor, diff, sidebar de worktrees.
- **Runtime RPC**: socket Unix em `userData/o-<pid>-<sufixo>.sock`
  (`src/main/runtime/runtime-rpc/runtime-rpc-socket-metadata.ts`). A CLI
  `orca` (`src/cli`) fala com ele; quase todo comando aceita `--json`.
- **`orca serve`**: o mesmo runtime sem janela (servidor headless), que pode
  ser pareado com outro desktop ou com o celular.
- **Relay na nuvem** (`cloud/`): WebSocket com E2EE ligando o app mobile ao
  desktop, além de um gateway de push.

### 1.2 Estado dos agentes: hooks + regras de tela

O Orca tem **duas fontes** que alimentam um único store (o do hook server, no
host de execução):

1. **Hooks gerenciados.** Para cada CLI conhecida (Claude, Codex, Gemini,
   Cursor, Copilot, Amp, Droid, Grok, Kimi, Qwen…; a lista está em
   `src/main/agent-hooks/managed-agent-hook-registry.ts`) o Orca escreve hooks
   no arquivo de config do agente (ex.: `~/.claude/settings.json`). O script
   do hook manda eventos para um servidor local identificado por
   `ORCA_AGENT_HOOK_PORT` / `ORCA_AGENT_HOOK_TOKEN`.
2. **Regras de tela declarativas** (`src/main/runtime/agent-state-rules/*.json`,
   14 arquivos: claude, codex, gemini, opencode, cursor, pi…). Cada regra diz
   uma região (título da janela, rodapé…), um regex, uma prioridade e uma
   resposta (`idle`/`working`…, com força e `requiresQuiet`). Exemplo: o
   Claude Code põe `✳` no título quando está ocioso.

`orca terminal wait --for tui-idle` usa isso para esperar o fim de um turno,
que é o mesmo papel do nosso `screen.rs`.

### 1.3 Comunicação entre agentes

Há duas camadas:

- **Direta (igual à nossa):** `terminal read`, `terminal send --enter`,
  `terminal wait --for tui-idle`. A diferença é que o Orca devolve recibos em
  etapas (`input_accepted` → `turn_started`) com um request ID para fazer
  replay sem reenviar o prompt.
- **Orquestração estruturada** (`skill-guides/orchestration.md`):
  - **Run**: namespace + inbox do coordenador.
  - **Task**: unidade de trabalho, com dependências (DAG).
  - **Dispatch**: uma tentativa autoritativa de uma Task num terminal.
  - **Mensagens tipadas** em SQLite (`question`, `worker_done`, `escalation`,
    heartbeat), entregues em FIFO e com `ack`.
  - `worker-start --spec` cria a Task, sobe o agente e injeta um preâmbulo com
    os IDs. O worker responde com `orca orchestration ask/reply` e termina com
    `worker_done --outcome succeeded|failed`.
  - O coordenador espera com `check --wait --types worker_done,escalation,question`.
  - Liveness em três estados (`live` / `unverifiable` / `exited`), com a regra
    de que perder contato não prova que o processo morreu.
  - Federação: workers podem rodar em servidores Orca remotos (SSH ou pareados).

Essa camada é **cooperativa**: só funciona se o agente seguir a skill.

---

## 2. Como o NarraTer funciona hoje (resumo)

- Cada PTY é espelhado num `vt100` no Rust (`screen.rs`), que deriva
  `running`/`idle`/`attention` só da tela (hash de conteúdo, rodapé
  `esc to interrupt`, settle de 1,2 s; shells pelo process group).
- A CLI `narrater` (Rust) fala JSON com o app por um socket Unix privado,
  autenticado por `NARRATER_ID` + `NARRATER_TOKEN`.
- `ask` enfileira o prompt até o alvo ficar ocioso, digita
  (bracketed paste se a TUI ativou), espera o turno acabar e devolve a tela.
  Tem o padrão *ask-back*, `check`, `--raw` e `--batch`.
- As arestas do canvas controlam quem pode falar com quem.
- Modo Maestro (`recruit`/`dismiss`/roles), floors (git worktree), workspaces
  SSH/Docker, rotinas, chat, notas e portais CDP.

---

## 3. Correlação ponto a ponto

| Conceito | NarraTer | Orca | Observação |
|---|---|---|---|
| Worktree por tarefa | `floor create` | `worktree create` | Equivalentes; o Orca tem lineage pai/filho e links de PR/issue |
| Subir um agente | `recruit --preset` | `agent launch` / `worktree create --agent` | O Orca aceita `--model`/`--effort` e tem recibo de lançamento com replay |
| Ler a tela | `check` | `terminal read` (com cursor/paginação) | |
| Mandar prompt | `ask` (bloqueia e devolve resposta) | `terminal send` + `terminal wait` | Nosso `ask` é mais simples para o agente |
| Fim de turno | heurística em código Rust | regras JSON + hooks | **Ponto onde o Orca está à frente** |
| Pergunta/resposta | `ask` / ask-back (efêmero) | `orchestration ask/reply` (durável, por ID) | |
| Status de andar/worktree | `floor status --state --progress` | `worktree set --comment --workspace-status` | Quase idênticos |
| Agendamento | `routine` | `automations` | |
| Instruções para o agente | skills em `~/.claude/skills` + `~/.agents/skills`, roles como `CLAUDE.md`/`AGENTS.md` | skills (`orca-cli`, `orchestration`…) | Mesma abordagem |
| Remoto | workspaces SSH/Docker | SSH, WSL, `orca serve`, federação | |
| Browser para agentes | portal (Chrome headless via CDP) | browser embutido + Design Mode | Ambos com snapshot e refs |
| Topologia de conversa | arestas no canvas | não existe | **Nosso diferencial** |
| Canvas espacial | sim | não | **Nosso diferencial** |

---

## 4. Opções de integração

### A. Fundir os códigos (fork do Orca ou NarraTer dentro dele): **não recomendado**

Electron/TypeScript vs Tauri/Rust e ~2 M linhas vs ~20 mil. O Orca publica
versão quase todo dia e tem regras internas pesadas (ratchets de lint,
compatibilidade de wire). Manter um fork custaria mais que o próprio NarraTer.

### B. Portar as ideias certas para o NarraTer: **recomendado, começar aqui**

O Orca é MIT, então podemos reaproveitar material com atribuição.

1. **Regras de estado declarativas.** Trocar as listas fixas de `screen.rs`
   (`BUSY`, padrões de attention) por arquivos JSON por agente, no formato do
   Orca (região, regex, prioridade, `requiresQuiet`). Dá para partir dos 14
   arquivos deles (mantendo o aviso MIT). Isso pede capturar o **título da
   janela** (OSC 0/2), que hoje não guardamos e que é o sinal mais confiável
   do Claude Code. Continua sem precisar da cooperação do agente.
2. **Hooks como sinal opcional.** Se o agente tiver hook instalado, usar o
   evento; se não, a tela. A tela continua sendo a base, então não perdemos o
   "funciona com qualquer TUI".
3. **Camada durável por cima do `ask`.** Um `narrater task` simples:
   tarefa → tentativa → `done --outcome`, guardado em SQLite (`rusqlite`).
   No canvas isso vira coisa visível: a aresta mostra a tarefa em andamento e
   o nó mostra o resultado. É a parte do Orca que mais combina com a nossa
   lógica de comunicação, e o canvas a deixa mais clara do que uma sidebar.
4. **Recibos de entrega.** Separar "o texto entrou" de "o turno começou", com
   um request ID para fazer replay sem reenviar.

### C. Ponte NarraTer ↔ Orca pela CLI: **experimento interessante**

A superfície estável do Orca é a CLI com `--json` (o RPC interno muda rápido;
o próprio código trata clientes e servidores com versões diferentes). Com ela:

- **Nó "Orca" no canvas**: um tile que chama `orca worktree ps --json` e
  `orca orchestration worker-list --json` e desenha os worktrees e workers do
  Orca como nós, com status ao vivo. O NarraTer vira um "mapa" da frota do
  Orca.
- **Arestas que atravessam os apps**: um agente do NarraTer faz
  `narrater ask "orca:fix-login" "…"` e o backend traduz para
  `orca terminal send --enter --wait-submit` + `orca terminal wait --for tui-idle`
  + `orca terminal read`. O agente do lado do Orca não precisa saber do
  NarraTer, que é o mesmo princípio do nosso `ask`.
- **O caminho inverso** é mais fácil ainda: um agente rodando no Orca já roda
  comandos de shell, então pode usar `narrater ask` se estiver num terminal
  com `NARRATER_*` no ambiente (por exemplo, um terminal do Orca aberto a partir
  do NarraTer).

Riscos: depender de um app de terceiros que muda todo dia, e precisar do Orca
instalado e rodando (ou `orca serve`). Por isso deve ser um tipo de nó
opcional, não parte do núcleo.

### D. Contribuir um canvas para o Orca

Possível em teoria (MIT, aceitam PRs), mas seria uma feature enorme num
código com regras rígidas, e o NarraTer perderia a identidade. Não vale agora.

---

## 5. Proposta de ordem

1. **B1**: regras de estado em JSON + captura do título OSC. É pequeno,
   melhora a detecção de turno para todos os agentes e não muda o protocolo.
2. **B3**: `narrater task` com SQLite e visualização no canvas.
3. **C**: protótipo do nó "Orca" (só leitura: `worktree ps` + `worker-list`),
   depois o `ask` atravessando para terminais do Orca.
4. **B2/B4** conforme a necessidade.

Antes do passo C, vale instalar o Orca (AppImage ou deb) numa máquina Linux de
verdade e conferir a saída `--json` dos comandos acima, porque este estudo só
leu o código.
