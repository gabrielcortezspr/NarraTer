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

---

## 6. Foco: orquestração de agentes

Fontes no Orca: `skill-guides/orchestration.md` e `references/*.md` (~600
linhas de contrato que o agente precisa seguir), `src/main/runtime/orchestration/`
(`preamble.ts`, `mailbox-pointer-*`, `db/` em SQLite, schema na v39). No
NarraTer: `ipc.rs` (`ask_one`), `pty.rs` (fila, `ask_waiters`, ledger) e
`assets/SKILL-manager.md`.

### 6.1 Os dois modelos

**NarraTer: conversa observada.** `ask` = enfileira → digita quando o alvo
está ocioso → espera a tela assentar → devolve a tela. Um único verbo
síncrono. Tudo em memória: `ask_waiters`, fila (`inbox`) e ledger (limitado
por `LEDGER_CAP`).

**Orca: contrato de trabalho.** Run (inbox do coordenador) → Task (com
dependências) → Dispatch (uma tentativa). O worker recebe um preâmbulo com
IDs e comandos e precisa: mandar `heartbeat` a cada N minutos, `check` nos
checkpoints, `ask` para perguntas e `worker_done --outcome` exatamente uma
vez. O coordenador cria os workers e fica em `check --wait`, que acorda no
primeiro `worker_done`/`question`/`escalation`. Depois faz `reply` e `ack` e
decide reusar, reter ou liberar cada worker. Tudo fica em SQLite, com
`--retry-request` para operações idempotentes.

### 6.2 Onde o NarraTer ganha

| Ponto | Por quê |
|---|---|
| **Zero cooperação** | Qualquer TUI vira worker. No Orca, um modelo que esquece `worker_done` ou o heartbeat deixa o coordenador esperando (a própria skill gasta páginas com "ausência não é prova") |
| **Simplicidade para o líder** | Um `ask` devolve o resultado. No Orca o coordenador roda um loop de `check --wait` → processar → `reply` → `ack` → `release` e precisa carregar ~600 linhas de skill |
| **Peer-to-peer** | Qualquer par conectado conversa (e o ask-back cobre respostas longas). O Orca é hierárquico: coordenador ↔ worker |
| **Topologia como permissão** | As arestas limitam quem alcança quem, e o usuário vê e desenha isso. No Orca qualquer mailbox é endereçável |
| **Entrega que o alvo vê** | O prompt é digitado no terminal do alvo. No Orca o follow-up fica no mailbox até o worker rodar `check` (o "pointer" no PTY é só best-effort) |
| **Attention** | O `ask` devolve "o alvo está num menu de permissão" sem o worker fazer nada |
| **Notas compartilhadas** | Um quadro comum ligado por arestas. O Orca usa arquivos e `--report-path` |
| **Modo Maestro explícito** | Quem pode recrutar é escolha do usuário (coroa). O Orca usa um limite de profundidade |

### 6.3 Onde o NarraTer perde

| Ponto | No NarraTer | No Orca |
|---|---|---|
| **Durabilidade** | Fecha o app e perde asks pendentes, fila e histórico | SQLite, entrega FIFO com ack e replay |
| **Resultado estruturado** | A resposta é "a tela", sem sucesso/falha; o líder interpreta texto | `worker_done` com `outcome`, resumo de 3 frases, `files-modified` e `report-path` |
| **Líder bloqueado** | `ask` prende o shell do líder (até 1 h); `--batch` só volta quando **todos** terminam | Dispara todos e acorda no **primeiro** evento |
| **Pergunta vs resposta** | O ask-back **resolve** o ask do líder: pergunta e resposta se confundem | Mensagens tipadas (`question` ≠ `worker_done`), `ask --options`, `--resume <id>` após timeout |
| **Steering no meio** | Só como novo prompt, que entra depois do turno | `send --to dispatch:<id>`, lido nos checkpoints |
| **Idempotência** | Timeout = retirar da fila; "não reenvie" fica só na skill | `--retry-request`, `request-show`, `--resume` |
| **Saída maior que a tela** | Volta só a tela visível (o vt100 guarda 2000 linhas de scrollback, mas não usamos) | `worker-read` com cursor, inclusive do transcript do provedor |
| **Plano externo** | O plano vive no contexto do líder (some na compactação) | `task-create --deps` + `task-list --ready` funcionam como memória externa |
| **Ciclo de vida** | Recrutas se acumulam; nada força a decisão de dispensar | Contabilidade obrigatória: reusar, reter ou liberar |
| **Liveness** | running/idle/attention pela tela | `live`/`unverifiable`/`exited`, separando o agente morto do PTY vivo |
| **Endereços em grupo** | Nomes explícitos no `--batch` | `@all`, `@idle`, `@claude`, `@worktree:<id>` |

### 6.4 O que incorporar

Regra: **cooperação opcional que melhora o resultado, nunca exigida**. A tela
continua sendo a base; se o agente cooperar, a resposta fica mais rica.

1. **Ask assíncrono + `wait`** (maior ganho, mudança pequena: `ask_one` já tem
   as fases separadas).
   - `narrater ask "Dev" "…" --no-wait` → devolve `ask-7f3a`.
   - `narrater wait [ids…] [--any|--all] [--timeout S]` → devolve eventos
     `{id, agent, kind: done|attention|question|exited|timeout, text}` assim
     que acontecem.
   - O `ask` síncrono vira `ask --no-wait` + `wait <id>`.
2. **Asks duráveis.** Tabela SQLite (`rusqlite`) por cena: id, de, para,
   prompt, estado (`queued`/`delivered`/`done`/`attention`/`timeout`/`exited`/`unknown`),
   resultado e timestamps. No restart, o que estava pendente vira `unknown`
   (lição do Orca: nunca inferir o desfecho). O painel de histórico da aresta
   passa a ler daqui.
3. **Mensagens tipadas opcionais** (generaliza o ask-back):
   - `narrater done "resumo" [--failed] [--files a,b]` resolve o ask com
     resultado estruturado. Sem ele, a tela continua valendo.
   - `narrater question "Líder" "…" [--options a,b]` vira um evento
     `question` no `wait` do líder, **sem** encerrar o ask original.
   - `narrater reply <id> "…"` responde.
4. **`ask --resume <id>`** para reanexar a um ask depois de um timeout do
   shell do líder, em vez de reenviar.
5. **Resultado = saída desde a entrega**, não só a tela: guardar a posição no
   scrollback no momento da entrega e devolver tudo que veio depois (com
   limite e cursor).
6. **`narrater tell "Dev" "…"`**: mensagem sem espera (steering), entregue no
   próximo idle.
7. **Contabilidade na skill do Maestro**: depois de cada `done`, decidir
   reusar ou `dismiss`. `narrater list` mostra quem está ocioso e sem tarefa.
8. **Depois:** tarefas com dependências como um quadro no canvas (nós de
   tarefa ligados aos agentes), em vez de uma tabela escondida.

**Não trazer:** heartbeat obrigatório, `worker_done` "exatamente uma vez"
como requisito, a tríade Run/Task/Dispatch, consumer fences e federação.
É complexidade que só se paga com o modelo cooperativo deles.

## 7. Ler a saída do worker além da tela (2026-10-15)

> Atualização. Pergunta: o Orca resolve o problema que o benchmark mediu
> ([`comparacao-versoes.md`](comparacao-versoes.md)), a resposta do `ask` cortada na tela? Leitura do
> código no commit `02b4ea28` (2026-10-15) e testes nossos com o Claude Code 2.1.300.

### 7.1 Como o Orca faz

`[citado]` O comando é `orca orchestration worker-read --dispatch <id> [--source auto|transcript|terminal]
[--cursor <c>] [--limit <n>]` (`src/cli/specs/orchestration-worker-specs.ts:62`). Ele tem duas fontes, nesta
ordem:

1. **Transcript do provedor** (Claude, Codex, Grok, OMP). O Orca instala hooks na configuração do
   agente, e o hook informa `session_id` e `transcript_path`. Com isso o Orca lê o `.jsonl` a partir de
   um offset em bytes (`worker-transcript-local-read.ts`). Sem hook, ou com um provedor sem
   leitor (Gemini, OpenCode…), cai para a fonte 2 com `fallbackReason` tipado.
2. **Terminal**: um anel de até 2000 linhas montado do fluxo do PTY, sem ANSI e com os redesenhos
   colapsados. Spinner e linha reescrita não se acumulam (`terminal-tail-buffer.ts`). Em tela
   alternativa, devolve só a tela renderizada por `@xterm/headless`.

**O cursor** é opaco e preso à fonte: `owr1_` + JSON com dispatch, fonte, hash de identidade, posição e
os 64 bytes antes do offset como checkpoint (`worker-output-cursor.ts`). Arquivo reescrito, troca de
processo ou outro dispatch geram `source_changed`.

**Limites que pesam para nós:**
- `[citado]` **Não ancora "desde o dispatch".** A primeira leitura sem cursor devolve as últimas 40
  mensagens (ou 120 linhas), e o cursor aponta para o fim. A data do dispatch só serve para escolher a
  sessão certa do hook (`worker-provider-session.ts`).
- `[citado]` **Cada bloco de texto é cortado em 1.200 caracteres** (`worker-transcript-payload.ts`), sem
  paginação dentro do bloco. Um relatório final longo chega cortado no transcript também.
- `[citado]` Por isso o resultado normal no Orca **não** vem do `worker-read`. Vem do `worker_done`,
  que o worker é obrigado a mandar (`preamble.ts:97`: corpo de 3 frases, `--outcome`,
  `--files-modified`, `--report-path`). O `worker-read` é ferramenta de inspeção e recuperação.
  O `report-path` é só um texto: o Orca nunca lê o arquivo.

### 7.2 O que medimos aqui

- `[medido]` **O Claude Code 2.1.300 roda em tela alternativa.** Testamos `claude` em tmux 80×13 e
  pedimos 60 linhas. O resultado: `#{alternate_on}=1` e `#{history_size}=0`, ou seja, só as 13 linhas
  visíveis existem. Ler o scrollback, como a O5 dizia, **não funcionaria** com o Claude.
- `[medido]` Com `CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1` (variável encontrada no binário), o mesmo
  teste deu `alternate_on=0`, `history_size=64` e as 60 linhas inteiras no histórico do tmux.
- `[inferido]` O nosso vt100 lê a saída do **cliente** tmux, que também usa tela alternativa. O histórico
  tem que vir do servidor tmux (`capture-pane -S <linha>`), não do scrollback do vt100.
- `[observado]` O JSON que o Claude manda à statusline (D17) traz `transcript_path` e `session_id`
  (`target/turn-eval-statusline/work/*.statusline.json`). A statusline que já instalamos pode informar
  o transcript de cada terminal sem hook novo.

### 7.3 O que levar para a O5

| Peça | Do Orca | Nosso ajuste |
|---|---|---|
| Fonte preferida | Transcript via hook | Transcript via **statusline** (já instalada, opcional, D06). Se não houver, procurar por pasta (`~/.claude/projects/<cwd>`) e horário |
| Âncora | Fim do arquivo (tail) | **Offset no momento da entrega**: devolve o turno inteiro, não um tail |
| Corte | 1.200 caracteres por bloco | Sem corte na última mensagem do assistente; limite total com aviso |
| Cursor | Opaco, fonte + identidade + checkpoint | Copiar a ideia (o hash de 64 bytes detecta arquivo reescrito) |
| Fallback 1 | Anel de linhas do PTY | Histórico do servidor tmux desde a entrega (com `DISABLE_ALTERNATE_SCREEN` opcional para o Claude) |
| Fallback 2 | Tela do xterm headless | A tela de hoje (`screen.text()`), marcada como parcial |
| Relatório do worker | `worker_done` obrigatório | Continua opcional (`narrater ask "Líder" "…"` / reply), D01 |

Resumo: o Orca tem as peças (transcript com cursor, fallback rotulado), mas não resolve o nosso caso
direito. Ele não ancora na entrega e corta o texto. Por isso empurra o resultado para uma mensagem
obrigatória do worker, o que a D01 não permite.
