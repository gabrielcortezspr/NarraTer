# NarraTer × Maestri: onde erramos e como o Maestri acerta

> Estudo feito em 2026-10-01, rodando **dentro** do Maestri 0.19.4 para Linux
> (`/opt/Maestri`, build `77a06e36`). As afirmações sobre o Maestri vêm da
> inspeção da instalação local: o bundle Electron (`app.asar` → `out/main/index.js`),
> as strings do binário `maestri` (core em Swift, linkado estaticamente), os
> arquivos em `~/.maestri`, as skills que ele instala e a saída de `maestri help`
> e `maestri debug`. Nada foi tirado de documentação pública. Quando um ponto é
> inferência, o texto avisa.

---

## TL;DR: por que o Maestri conversa com IAs de qualquer empresa

O Maestri trata **o terminal como API universal** e não pede **nada** ao agente
além do que todo agente de código já tem, que é ler arquivos de instrução e
rodar comandos de shell.

| Camada | Maestri (portável) | NarraTer (preso ao Claude) |
|---|---|---|
| **Como o agente aprende o protocolo** | *Skills* (`SKILL.md`) instaladas em `~/.claude/skills` **e** `~/.agents/skills`, o padrão aberto que Codex, Gemini CLI, OpenCode, Antigravity etc. leem. Roles viram `CLAUDE.md` + `AGENTS.md` | `--append-system-prompt` (flag exclusiva do Claude Code). O Codex não recebe instrução nenhuma. O tipo custom não recebe nada |
| **Como o agente fala** | Uma CLI (`maestri`) no PATH. Todo agente tem uma ferramenta de shell | MCP via `--mcp-config` / `-c mcp_servers…` (sintaxe por fornecedor), com a CLI como alternativa que nenhum agente fora o Claude conhece |
| **Como se sabe que o alvo terminou** | Observando a **tela renderizada** (xterm headless + máquina de estados `idle/working/needsAttention/interrupted`) | Silêncio de bytes por 1,5 s, ou *Stop hook* (que só existe no Claude) |
| **Como volta a resposta do `ask`** | O próprio `ask` retorna o texto da tela quando o alvo fica ocioso. O alvo **não precisa cooperar** | O alvo **precisa** chamar `reply_message` com o `#id`. Quem não conhece o protocolo deixa o `ask` travado até o timeout |
| **Como entra o prompt** | Um plano de entrega calculado pelo core, usando o modo *bracketed paste* **observado** no terminal | Escolhido por `agent_type` (`claude`/`codex` → bracketed; outros → texto + `\r` 300 ms depois) |

Resumindo: **o NarraTer depende de o agente colaborar, e o Maestri só observa
o terminal.** Para colaborar, o agente precisa de flags, MCP, hooks e prompts
específicos de cada fornecedor, o que na prática deixa o NarraTer preso ao
Claude Code. Observar o terminal é igual para qualquer TUI.

---

## 1. Como o Maestri funciona por dentro

### 1.1 Processos

```
┌──────────────────────── Electron (maestri-app) ─────────────────────────┐
│ renderer: canvas React 19 + xterm.js (só exibe)                         │
│ main:     node-pty  ──bytes──►  @xterm/headless  (1 por terminal)       │
│                                   │ serializa tela, bracketedPasteMode,  │
│                                   │ contentHash                          │
│           ◄── deliverPrompt / ptyWrite ──┐      ── ptyOutput{b64,hash} ─►│
└──────────────────────────────────────────┼──────────────────────────────┘
                                           │ stdio (frames JSON)
                          ┌────────────────┴────────────────┐
                          │ core: `maestri --serve <token>` │  (o mesmo binário da CLI,
                          │  registro de workspaces/ropes   │   Swift estático)
                          │  máquina de estados por agente  │
                          │  plano de entrega de prompt     │
                          │  HTTP em Unix socket            │
                          └────────────────┬────────────────┘
                                           │ $MAESTRI_SOCKET  (/tmp/maestri-*/maestri.sock, 0700)
              agente no terminal ── `maestri ask "X" "..."` (HTTP /cli, headers X-Terminal-ID…)
```

Evidências:
- `main` instancia um terminal **headless** por PTY (`feed()`, `whenIdle()`,
  `computeHash()`, `serializeLive()`) e encaminha cada chunk ao core como
  `{type:"ptyOutput", payload:{terminalId, dataBase64, contentHash}}`.
- O core é iniciado com `en(path, ["--serve", token])` e responde com frames
  `deliverPrompt`, `ptyWrite`, `query`, `promptDeliveryPlan`…
- A CLI chama o core por HTTP no socket (a string
  `curl … -H "X-Terminal-ID:$MAESTRI_TERMINAL_ID" … "http://$MAESTRI_HOST/cli"`
  aparece no binário, e `maestri debug` testa `HTTP /list`).

### 1.2 O `ask` é síncrono e agnóstico

1. `maestri ask "Dev" "prompt"` → o core resolve "Dev" entre os nós ligados por
   *rope* ao chamador.
2. O core pede ao `main` um `deliverPrompt`. O `main` consulta
   `planPromptDelivery({stage:"full", text, bracketed: headless.bracketedPasteMode})`
   e escreve os passos com os atrasos planejados (`replayPromptWrites`).
   **O bracketed paste só é usado se a TUI realmente ativou o modo (DECSET 2004).**
3. O core acompanha a tela do alvo (hash + heurísticas de rodapé; no binário
   aparecem padrões como `esc to`, `continue?`, `proceed?`, `shift+tab`, `tab to`)
   e classifica o terminal como `working`, `idle`, `needsAttention` ou `interrupted`.
4. Quando o alvo volta a `idle`, o `ask` **retorna o que está na tela**. O alvo
   não precisou saber de nada.
5. **Ask-back**: se, durante a espera, o alvo rodar `maestri ask "Chamador" "…"`,
   essa mensagem **resolve o `ask` pendente**. Isso está no binário:
   *"responded by messaging you directly instead of finishing quietly… treat it
   as the reply to this ask"*. Esse caminho é opcional e só serve para respostas
   longas que não cabem numa tela.
6. Se o tempo estourar, a orientação é `maestri check "Dev"` (lê a tela atual),
   **nunca reenviar**. Há também `--raw "\e[A\n"` para menus interativos e
   `--batch` para fan-out paralelo.

### 1.3 Instruções portáveis em vez de flags

- **Skills.** O Maestri publica as skills `maestri`, `maestri-manager`,
  `maestri-portal` etc. em várias bases (`skillBases`): `.claude/skills`
  (Claude Code), `.agents/skills` (padrão aberto *Agent Skills*, lido por
  Codex/Gemini/OpenCode/Antigravity) e `.hermes/skills`. Nesta máquina estão em
  `~/.claude/skills/maestri*` e `~/.agents/skills/maestri*`. O texto da skill
  ensina o protocolo inteiro, e o agente só o carrega quando precisa.
- **Roles.** `installRoleStrict` cria `<projeto>/.maestri/roles/<id>/` com
  `CLAUDE.md` **e** `AGENTS.md` (o mesmo conteúdo:
  `<your_assigned_role>…</your_assigned_role>` + um bloco `<working_directory>`
  apontando para o projeto real), além de `.gemini/settings.json` /
  `.qwen/settings.json` com `context.fileName: ["AGENTS.md"]` e
  `includeDirectories: [projeto]` quando o comando contém `gemini`/`qwen`. O
  recruta é **iniciado com cwd nesse diretório**, e cada CLI carrega o role pela
  convenção nativa dela.
- **Presets** (`~/.maestri/preferences.json`): Claude Code (`claude`), Codex
  (`codex --no-daemon`), Antigravity (`agy`), OpenCode (`opencode`), Shell. A
  identidade do agente é inferida do comando (`claude`→anthropic,
  `codex`→openai, `opencode`, `gemini`, `antigravity`). Isso serve para ícone,
  uso e ajustes de entrega, nunca para mudar o protocolo.
- **Nenhum** `--append-system-prompt`, `--mcp-config`, `--settings` ou hook. O
  terminal desta sessão foi iniciado com o comando `claude`, sem nenhuma flag
  (`workspace.json` → `"command": "claude"`).

### 1.4 Outras decisões que ajudam

- **Ropes não direcionadas.** `maestri connect A B` permite que os dois lados
  façam `ask`. O sentido do pedido vem do chamador, não da aresta.
- **Diretório de trabalho por terminal** (`workingDirectory`) e por workspace,
  além de *floors* com clone git isolado.
- **Sessão independente da UI.** Terminais "unloaded" existem no registro, e o
  core responde por eles (a mensagem *"exists but its process is not running…
  ask the user to open it"* é explícita).
- **Diagnóstico.** `maestri debug` mostra o socket, as permissões, o env e o
  teste HTTP. `$MAESTRI_CLI` funciona como alternativa se o PATH estiver errado.
- **Binário único.** A CLI é o core. Não há script Python gerado em tempo de
  execução.

---

## 2. Como o NarraTer funciona hoje (resumo)

- `pty.rs`: `portable_pty`, um reader thread por sessão, que emite `pty_output`
  para o front e marca `last_output`. O monitor (`start_status_monitor`, tick de
  500 ms) passa a sessão para `Idle` após 1,5 s de silêncio, ou via Stop hook
  (`notify-idle`), e drena uma mensagem da fila por tick.
- `ipc.rs`: socket `/tmp/narrater-<pid>.sock` com JSON. Na inicialização
  escreve `~/.local/bin/narrater` (Python), `~/.local/bin/narrater-mcp` (Python,
  servidor MCP) e `~/.local/share/narrater/claude-hooks.json`.
- `ask` para um agente de IA = `ask_by_reply`: injeta `[narrater from X #id]: msg`
  e espera alguém chamar `reply #id`. Para shell = `ask_by_scraping`
  (bytes + `strip_ansi`).
- O protocolo chega ao agente por `buildAgentSystemPrompt` →
  `--append-system-prompt` (**só Claude**). As ferramentas chegam por
  `--mcp-config` (Claude) ou `-c mcp_servers.narrater.command=…` (Codex).
- O spawn é decidido no **frontend** (`terminalManager.spawnInto`), na primeira
  vez que o tile é anexado ao DOM.

---

## 3. Onde erramos

Cada item segue o formato: **o que fazemos → por que quebra → como o Maestri
resolve**.

### E1. O `ask` depende da cooperação do alvo (é o erro principal)

- **O que fazemos.** `handle_ask` manda todo alvo que não é shell para
  `ask_by_reply` (`src-tauri/src/ipc.rs:265`). A resposta só chega se o alvo
  chamar `narrater reply <id>` / `reply_message` (`ipc.rs:379-469`).
- **Por que quebra.** Um Codex, Gemini, OpenCode ou agente custom que não
  recebeu o protocolo (ver E2) responde normalmente **na tela dele** e nunca
  chama `reply`. Quem perguntou fica bloqueado 120 s e recebe
  `"the agent did not reply"`. Até o Claude falha quando esquece o `#id`, quando
  é interrompido ou quando responde com `send_message`. A comunicação entre
  fornecedores diferentes fica impossível por construção.
- **Como o Maestri resolve.** O `ask` termina quando o alvo fica `idle` e
  devolve a tela. A resposta explícita (ask-back) é **opcional** e resolve o
  `ask` pendente sem precisar de `#id`.

### E2. O protocolo chega por mecanismos exclusivos do Claude

- **O que fazemos.** `getSpawnSpec` (`src/lib/tauri.ts`) e `spawnInto`
  (`src/lib/terminalManager.ts:131-136`) montam um system prompt **apenas para
  `claude`**. O Codex recebe só o servidor MCP, sem nenhuma explicação sobre o
  frame `[narrater from X #id]` nem sobre a cadeia de comando. O tipo `custom`
  não recebe nada: não ganha MCP nem instrução, e a CLI `narrater` está no PATH
  sem que ninguém conte que ela existe.
- **Por que quebra.** Cada fornecedor tem a própria flag de system prompt (ou
  não tem nenhuma). Cada CLI nova exigiria um caso novo no `switch`, e várias
  simplesmente não oferecem esse recurso.
- **Como o Maestri resolve.** Usa convenções de **arquivo** que todos já leem:
  `SKILL.md` em `~/.claude/skills` e `~/.agents/skills`, e
  `AGENTS.md`/`CLAUDE.md` (+ `settings.json` do Gemini/Qwen) no cwd do role.

### E3. Instruções de não-Claude são digitadas no composer na hora do boot

- **O que fazemos.** `ptyWrite(id, instructions + "\n")` logo depois do spawn
  (`terminalManager.ts:156-158`).
- **Por que quebra.**
  1. **Corrida com o boot da TUI**: o texto chega antes de a interface estar
     pronta e pode ser perdido ou embaralhado.
  2. `\n` **não submete** numa TUI em raw mode (o próprio `pty.rs:722` comenta
     que precisa de `\r`).
  3. Mesmo quando funciona, a instrução vira uma **mensagem de usuário** que o
     agente pode compactar ou esquecer, e não contexto permanente.
- **Como o Maestri resolve.** O role é arquivo de contexto, carregado pelo
  próprio agente no boot e recarregado depois de cada compactação.

### E4. A detecção de "terminou" se baseia em silêncio de bytes e num hook do Claude

- **O que fazemos.** `IDLE_THRESHOLD = 1,5 s` sem output marca `Idle`
  (`pty.rs:11`, `pty.rs:647-665`). Se a TUI nunca fica em silêncio (spinner,
  relógio, redraw), a mensagem é **injetada à força** depois de 30 s com o
  agente no meio do turno (`MAX_QUEUE_WAIT`, `pty.rs:15` e `683-690`). O Claude
  usa o Stop hook, que **só existe no Claude**. Pela documentação do Claude
  Code, o hook também não dispara quando o turno é interrompido pelo usuário, e
  aí a sessão fica `Running` até o backstop de 120 s.
- **Por que quebra.** Para qualquer agente que não é o Claude, ou o estado fica
  "ocioso" falso durante uma pausa de raciocínio (e a mensagem é injetada no
  meio), ou fica "ocupado" eterno com spinner (e a mensagem é forçada depois de
  30 s, também no meio do turno). As duas situações corrompem o turno.
- **Como o Maestri resolve.** Avalia o **estado da tela**, não o fluxo de bytes.
  Um redraw que não muda o conteúdo não conta (`contentHash`), e o rodapé da TUI
  diz se ela está trabalhando (`esc to interrupt`), pedindo permissão
  (`proceed?`, `continue?`) ou ociosa. Isso funciona igual para qualquer TUI
  sem hooks.

### E5. O backend não tem modelo de tela, só bytes crus

- **O que fazemos.** O único xterm é o do frontend. O backend guarda apenas o
  stream. O `ask_by_scraping` concatena bytes e roda `strip_ansi`
  (`ipc.rs:555-694`), e o `strip_injected_echo` admite no próprio teste que
  quebra com line-wrap (`ipc.rs:1270-1275`). A aresta agent-note espelha bytes
  sem ANSI e grava ruído de redraw na nota (`Canvas/index.tsx:186-193`).
- **Por que quebra.** TUIs (Ink, ratatui, bubbletea) redesenham com movimento de
  cursor. Tirar o ANSI de um redraw produz texto duplicado e fragmentado. Sem
  modelo de tela também não dá para implementar `check` (ver a tela agora) nem
  saber o modo de paste.
- **Como o Maestri resolve.** Mantém um `@xterm/headless` por PTY no processo
  principal: `serializeLive()` devolve a tela renderizada, e
  `bracketedPasteMode` vem do próprio emulador.

### E6. O bracketed paste é presumido pelo tipo, não observado

- **O que fazemos.** `claude | codex` → `ESC[200~…ESC[201~\r`. Os outros tipos
  recebem texto e `\r` 300 ms depois (`pty.rs:726-750`).
- **Por que quebra.** Um agente custom que suporta paste (Gemini, OpenCode) cai
  no caminho frágil de 300 ms. Um shell com bracketed paste ativo (bash ≥ 5.1
  ativa por padrão) recebe `^U` + texto puro. E se o Claude/Codex desativar o
  modo em algum estado (modal, menu), o paste quebra.
- **Como o Maestri resolve.** Usa
  `planPromptDelivery({…, bracketed: r.headless.bracketedPasteMode})`, ou seja,
  pergunta ao emulador o que a TUI pediu de fato.

### E7. Roteamento direcionado com "reply grants" e arestas reversas

- **O que fazemos.** As arestas são direcionadas. Para responder sem aresta
  reversa existe um grant temporário de 10 min (`REPLY_GRANT_TTL`). O líder é
  instruído a criar a aresta reversa via MCP antes de delegar
  (`agentPrompt.ts:31`, `roles.rs:47`).
- **Por que quebra.** É estado escondido que expira. A regra de topologia vira
  instrução para o LLM ("crie a aresta reversa"), e um esquecimento derruba o
  fluxo. Gasta tokens e gera bugs que dependem de tempo.
- **Como o Maestri resolve.** A rope é não direcionada. O `ask` bloqueante já
  carrega a resposta, e o ask-back resolve o pedido pendente sem precisar de
  rota nova.

### E8. A sessão existe só se o tile estiver montado

- **O que fazemos.** O spawn acontece no primeiro `attachTerminal`
  (`terminalManager.ts:217-227`), chamado pelo `useEffect` do `TerminalTile`.
  O canvas usa `onlyRenderVisibleElements` (`Canvas/index.tsx:356`), e com zoom
  abaixo do LOD o corpo do terminal fica escondido.
- **Por que quebra.** Ao carregar uma cena, os terminais fora da viewport **não
  existem no backend**. `narrater ask "Dev"` responde `no agent named 'Dev'`
  até o usuário rolar até o tile. A orquestração passa a depender da posição da
  câmera.
- **Como o Maestri resolve.** O registro de terminais vive no main/core. A UI só
  exibe. Um terminal "unloaded" tem resposta explícita e acionável.

### E9. Sem diretório de trabalho por terminal

- **O que fazemos.** O `CommandBuilder` nunca recebe `cwd` (`pty.rs:268-298`),
  então todo terminal herda o cwd do processo NarraTer. O `pre_trust_claude_cwd`
  confia em `std::env::current_dir()` **do app** (`pty.rs:191`) e reescreve
  `~/.claude.json` inteiro, o que gera corrida com o Claude, que escreve o mesmo
  arquivo.
- **Por que quebra.** Não dá para ter agentes em projetos ou worktrees
  diferentes. Também não dá para usar o truque de role por diretório do Maestri.
  Editar `~/.claude.json` concorrentemente pode apagar dados do usuário.
- **Como o Maestri resolve.** Usa `workingDirectory` por terminal, workspace com
  raiz própria, floors com clone git e cwd de role em `.maestri/roles/<id>`.

### E10. Infraestrutura frágil de CLI e MCP

- Scripts Python são regravados em `~/.local/bin` a cada boot (`ipc.rs:820-1207`):
  dependem de `python3` e sobrescrevem arquivos do usuário. O MCP só pega
  mudanças depois de reiniciar o app **e** o agente (`docs/mcp-canvas-tools.md`).
- O socket fica em `/tmp/narrater-<pid>.sock`, e não existe comando de
  diagnóstico.
- O MCP é a interface principal, mas cada CLI configura MCP de um jeito (e
  algumas exigem aprovação interativa). A CLI, que funciona com qualquer agente,
  ficou em segundo plano.
- **Como o Maestri resolve.** Tem um binário estático, que é ao mesmo tempo a
  CLI e o daemon, mais `$MAESTRI_CLI` como alternativa, `maestri debug` e
  `maestri help <cmd>`. O canal é a CLI, e o MCP nem existe.

### E11. Lógica de agente espalhada pelo frontend

- O frontend decide flags de spawn, system prompt, injeção de instruções e
  guards de idle (`terminalManager.ts`, `tauri.ts`, `Canvas/index.tsx`). O
  backend decide o framing e a entrega (`pty.rs`). As regras sobre "o que é um
  agente" ficam divididas em dois lugares e duas linguagens.
- **Como o Maestri resolve.** Concentra tudo no core: identidade do agente,
  plano de entrega, estado do turno e roteamento. O renderer só exibe.

---

## 4. O que o NarraTer já faz bem e vale manter

- Token por terminal contra spoof de identidade (`NARRATER_TOKEN`). O Maestri
  faz algo parecido com headers e token.
- Ledger de mensagens por aresta e o painel de histórico, que dão uma boa
  observabilidade.
- Fila visível e cancelável por terminal (`pty_queue_cancel`).
- Colapso de envios duplicados (`enqueue_message`).
- Ferramentas de canvas para agentes (notas, mover, conectar), equivalentes às
  `maestri note …`.
- Roles com modo "delegate-only". O Maestri não tem bloqueio de ferramentas por
  role: esse é um diferencial do NarraTer, mas hoje só funciona no Claude
  (`--disallowedTools`).

---

## 5. Plano de correção (por prioridade)

### P0: tornar o protocolo agnóstico ao fornecedor

1. **Terminal headless no backend.** Um emulador VT por sessão em Rust
   (`alacritty_terminal`, `wezterm-term` ou `vt100`), alimentado no reader
   thread do `pty.rs`. Ele expõe:
   - `screen_text()` (tela renderizada, sem ANSI);
   - `bracketed_paste_enabled()` (DECSET 2004);
   - `content_hash()` atualizado a cada write.
2. **Máquina de estados de turno pela tela**: `idle | working | needs_attention | exited`.
   - `working`: o hash mudou nos últimos N ms **e/ou** o rodapé casa com
     padrões de "ocupado" (`esc to interrupt`, spinner).
   - `needs_attention`: padrões de confirmação (`proceed?`, `continue?`,
     `(y/n)`, menus `❯ 1.`).
   - `idle`: a tela está estável por X ms **e** sem padrão de ocupado.
   - Padrões por agente ficam numa tabela de dados (TOML/JSON) e não espalhados
     no código. O Stop hook do Claude pode continuar como **sinal extra**, sem
     ser obrigatório.
3. **`ask` síncrono por tela.** Entrega o prompt, espera `working`, depois
   `idle`, e retorna o delta da tela (ou a tela inteira). Também implementar:
   - **ask-back**: se o alvo enviar uma mensagem ao chamador que está esperando,
     essa mensagem resolve o `ask`;
   - `reply #id` continua aceito, mas deixa de ser obrigatório;
   - `narrater check <alvo>` (tela atual) e `narrater ask <alvo> --raw "…"`.
4. **Instruções por arquivo, não por flag.**
   - Instalar `~/.claude/skills/narrater/SKILL.md` **e**
     `~/.agents/skills/narrater/SKILL.md` com o protocolo inteiro (hoje ele está
     em `agentPrompt.ts`).
   - Roles: gerar `<projeto>/.narrater/roles/<id>/{CLAUDE.md,AGENTS.md}`
     (+ `.gemini/settings.json`/`.qwen/settings.json`), iniciar o agente com cwd
     ali e incluir o bloco `<working_directory>` apontando para o projeto.
   - Manter o `--append-system-prompt` só como reforço opcional para o Claude.
5. **Entrega de prompt guiada pelo emulador**: usar bracketed paste se o modo
   estiver ativo. Se não estiver, enviar texto + `\r` com atraso por preset.

### P1: robustez e paridade

6. **Spawn no backend, independente do tile.** Ao carregar a cena, o backend
   sobe (ou registra como "dormente") todos os terminais. Um `ask` para um
   dormente devolve um erro acionável ou acorda o terminal.
7. **`cwd` por terminal e por cena.** Adicionar o campo em `SceneNode`, passar
   `cmd.cwd(...)` no spawn e remover a escrita global em `~/.claude.json` (ou
   gravar de forma atômica e só para o cwd do terminal).
8. **Presets de agentes** como dados: `claude`, `codex`, `gemini`, `opencode`,
   `qwen`, `agy`, `shell` e custom, com a identidade detectada pelo comando.
9. **Conexões não direcionadas (ropes)**, ou pelo menos tratar a aresta como
   bidirecional para `ask`. Remover os reply grants e a instrução de "criar
   aresta reversa".

### P2: ergonomia

10. Trocar os scripts Python por um binário Rust (subcomando do próprio app,
    ou um crate `narrater-cli` estático) e adicionar `narrater debug` e
    `$NARRATER_CLI`.
11. Deixar o MCP **opcional**. Deixar o "delegate-only" funcionar também por
    convenção (instrução no role) quando o agente não aceitar `--disallowedTools`.
12. `narrater ask --batch` (fan-out paralelo com retorno agregado), no lugar do
    `broadcast` fire-and-forget.

---

## 5.1 Status da implementação (branch `feat/maestri-parity`, 2026-10-01)

| Item | Status | Onde |
|---|---|---|
| E1. `ask` sem cooperação do alvo | ✅ `ask` síncrono pela tela + ask-back | `src-tauri/src/ipc.rs` (`ask_one`) |
| E2. Protocolo por flags do Claude | ✅ skill em `~/.claude/skills` e `~/.agents/skills`; sem system prompt e sem MCP | `src-tauri/assets/SKILL.md`, `src/lib/agents.ts` |
| E3. Instruções digitadas no composer | ✅ roles como `CLAUDE.md` + `AGENTS.md` (+ Gemini/Qwen) no cwd do agente | `src-tauri/src/roles.rs` |
| E4. Idle por silêncio ou Stop hook | ✅ máquina de estados pela tela (`working/idle/attention`) e process group para shells | `src-tauri/src/screen.rs` |
| E5. Sem modelo de tela | ✅ `vt100` headless por PTY, com `check` e `--raw` | `screen.rs`, `pty.rs` |
| E6. Bracketed paste presumido | ✅ observado no emulador (`plan_delivery`) | `screen.rs` |
| E7. Arestas direcionadas e reply grants | ✅ conexões não direcionadas; grants e `#id` removidos | `pty.rs` (`connected`) |
| E8. Sessão presa ao tile | ✅ a cena inicia todos os terminais; o xterm só abre no primeiro attach; o fit ignora tiles ocultos (antes encolhia o PTY para 2×1) | `src/stores/canvas.ts`, `src/lib/terminalManager.ts` |
| E9. Sem cwd por terminal | ✅ campo `cwd` na cena e no picker; pre-trust atômico só para o cwd real | `pty.rs`, `AgentPicker` |
| E10. CLI e MCP frágeis | ✅ MCP removido; CLI em Rust (`narrater-cli`, só std + serde_json, ~2 ms por chamada contra ~58 ms do Python), copiado para `~/.local/bin/narrater` no boot; paridade verificada com 103 linhas de comando contra o CLI Python antigo | `src-tauri/src/bin/narrater-cli/` |
| E11. Lógica espalhada | 🟡 presets viraram dados (`agents.ts`), e protocolo/estado/entrega ficam no backend | — |
| Extra | ✅ marcadores de sessão do processo pai (`CLAUDE_CODE_*`, `MAESTRI_*`) não vazam para os terminais | `pty.rs` |

Validação: além dos testes unitários, o probe (`cargo run --example turn_probe`)
foi rodado contra bash e contra o Claude Code real (turno simples, turno de 17 s
com ferramenta, e menu de permissão → `attention`). Também houve um teste ponta
a ponta no app real (cena isolada via `XDG_CONFIG_HOME`), cobrindo `list`, `ask`
shell → Claude, ask-back, `note create/read/edit`, a permissão de notas,
`--raw`, `--batch` e a recusa para quem não está conectado.

## 5.2 Roadmap: as outras skills do Maestri

A base acima (identidade, conexões, socket, tela e `ask` agnóstico) é o que as
demais skills reaproveitam. Ordem sugerida:

1. ✅ **maestri-manager → `narrater recruit/dismiss/role`** (feito: modo
   Maestro por terminal, `recruit` com `--preset/--role/--command/--dir`,
   `--replace` no mesmo nó, `dismiss`, `preset list`, `role …`, `notify`,
   skill `narrater-manager`; validado com um Claude Maestro montando um time
   sozinho a partir de um pedido em linguagem natural). Fora do escopo por
   ora: `--floor`/`--workspace` (dependem do item 4).
2. ✅ **maestri-portal → `narrater portal …`** (feito: o `<iframe>` deu lugar a
   uma aba real de Chrome headless via CDP — screencast no tile com mouse,
   teclado e colar repassados; `create/edit/close`, `snapshot` com refs
   `@eN`, `click/fill/type/key/hover/focus/select/check/scroll/drag`,
   `text/html/evaluate/wait`, `screenshot`, `logs`, `resize`, `ua`; skill
   `narrater-portal`). Pendente: popups (`window.open` → portal "#2"),
   iframes cross-origin no snapshot e o modo de design do Maestri.
3. ✅ **maestri-routines → `narrater routine …`** (feito: agendador no
   backend, `--every/--daily/--weekly/--once`, `--terminal`/`--reminder`,
   `--count`, `--until`, `--pre-run` com `{{output}}`, skip-if-busy,
   `--no-notify`, `--disabled`, `--id` para duplicatas; painel na sidebar;
   o "scheduled prompt" antigo migra para rotina; skill `narrater-routines`).
4. ✅ **maestri-workspace → workspaces e floors** (feito: cena com diretório
   e grupos/pastas na sidebar; `workspace create/list/rename/move` com
   `--from`; andares por nó, todos os terminais rodando; `git worktree` por
   andar (`--branch`, `--existing-branch`, `--no-git`, `--copy-ground`);
   `recruit --floor`; barra de andares; skill `narrater-workspace`).
   Depois (2026-10-08): `floor status` para qualquer agente (ponto de cor e
   progresso no chip, notificação ao entrar em `blocked/review/done`,
   `--floor` só para o Maestro), `floor land` com `--into/--delete-branch/
   --keep-floor` (conflito detectado por `merge-tree` antes de mudar algo;
   também no menu *Land…* do chip) e `floor delete` com `--keep-branch/
   --force`, ambos só a partir do térreo. Diferença que fica: o NarraTer
   roda uma cena por vez, então `recruit --workspace` ainda não existe, e
   não há `--copy` nem `--pull-request`.
5. ✅ **maestri-portal-devices → portais Android** (feito: `portal devices`,
   `portal create --simulator` com boot headless do AVD e desligamento quando
   nenhum portal usa; tela no tile por `screencap` com toque, swipe, teclas e
   texto; `snapshot` do `uiautomator` com refs; `click/type/key/scroll/swipe/
   drag/button/launch/terminate/navigate/screenshot/info`; verbos de browser
   recusados; skill `narrater-portal-devices`). Limite: a tela vem por
   polling (~4 fps), sem scrcpy.
6. ✅ **maestri-chat → NarraTer Chat** (feito: painel por agente com 7
   threads por cor, cabeçalho de chat entregue no terminal, `narrater say`
   com `--progress/--file/--attach`, imagens inline e deduplicação,
   `narrater recall` com `--turns/--last/--from/--to/--all/--json`, `list` e
   `attachment`; indicador de trabalho e badge de não lidas; skill
   `narrater-chat`). Validado com um Claude real, que carregou a skill, fez
   `recall` da thread nova, mandou progresso e respondeu via `say`.

Com isso, todas as skills do Maestri têm equivalente no NarraTer. O que
segue diferente: uma cena aberta por vez (sem `recruit --workspace`), tela
Android por polling e acesso remoto/celular do Maestri Chat, que ficou de
fora.

---

## 6. Apêndice: evidências colhidas

| Item | Onde |
|---|---|
| Env do terminal Maestri | `MAESTRI_SOCKET=/tmp/maestri-32ab405f/maestri.sock`, `MAESTRI_TERMINAL_ID`, `MAESTRI_WORKSPACE_ID`, `MAESTRI_CLI`, `TERM_PROGRAM=Maestri` |
| Core = CLI | `out/main/index.js`: `spawn(path, ["--serve", token])`. `file`: ELF estático com runtime Swift (`FoundationNetworking`, `MaestriCoreKit/StripAnsi.swift`) |
| Tela headless | `index.js`: `this.headless.whenIdle()`, `stickyHash=this.computeHash()`, `serializeLive()`, `headless.bracketedPasteMode` |
| PTY → core | `{type:"ptyOutput",payload:{terminalId,dataBase64,contentHash}}` |
| Plano de entrega | `planPromptDelivery({stage:"full",text:n,bracketed:r.headless.bracketedPasteMode})` → `replayPromptWrites` com `delayMs` |
| Estados de turno | strings do core: `idle`, `working`, `needsAttention`, `interrupted`, `armSettle`, `cancelSettle` |
| Heurísticas de rodapé | strings do core: `continue?`, `proceed?`, `shift+tab`, `esc to`, `tab to` |
| Ask-back | core: *"responded by messaging you directly instead of finishing quietly… treat it as the reply to this ask."* |
| Bases de skills | `index.js`: `.claude/skills`, `.agents/skills`, `.hermes/skills`. Disco: `~/.claude/skills/maestri*`, `~/.agents/skills/maestri*` |
| Roles por arquivo | `installRoleStrict`: `.maestri/roles/<id>/{CLAUDE.md,AGENTS.md,role.json}` + `.gemini`/`.qwen` `settings.json`. `buildContent` → `<your_assigned_role>` + `<working_directory>` |
| Presets | `~/.maestri/preferences.json`: `claude`, `codex --no-daemon`, `agy`, `opencode`, shell |
| Terminal desta sessão | `~/.maestri/workspaces/<id>/workspace.json`: `"command": "claude"`, `"agentType": "claude_code"`, `workingDirectory` próprio, sem flags |
