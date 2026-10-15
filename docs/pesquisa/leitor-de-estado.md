# Leitor de estado: como saber que um agente terminou

> Estudo feito em 2026-10-14. Fontes primárias: código do **CLI Agent Orchestrator** da AWS
> (`awslabs/cli-agent-orchestrator`, commit `5eb58fa` de 2026-10-13, Apache-2.0) e do **Orca**
> (`stablyai/orca` 1.4.214, MIT), lidos no scratchpad, sem rodar nenhum dos dois. Medições feitas
> no NarraTer com o avaliador `turn_eval` (seção 4), com shells reais e o **Claude Code 2.1.299
> real** (modelo `haiku`). Codex, Gemini CLI e OpenCode não estão instalados neste container e
> **não foram medidos**.

## TL;DR

Orquestrar agentes de fornecedores diferentes depende de uma pergunta: **o agente terminou?** O
`ask` só devolve uma resposta útil se acertar o fim do turno. Errar para cedo devolve uma tela pela
metade; errar para tarde prende o líder.

Existem três formas de responder, e os projetos estudados combinam as três:

| Fonte | Quem usa | Vantagem | Custo |
|---|---|---|---|
| **Tela** (emulador headless + padrões) | NarraTer, CAO, Orca | Funciona com qualquer TUI, sem cooperação | Heurística por CLI; quebra quando a TUI muda |
| **Título da janela** (OSC 0/2) | Orca | Sinal curto e estável que várias CLIs já emitem | Nem toda CLI usa; precisa de catálogo de glifos |
| **Hooks / eventos nativos** | Orca (hooks em ~25 CLIs), CAO via herdr | Exato quando existe | Exige configurar cada CLI; é cooperação (D01) |

O Claude coordenar bem os próprios subagentes não resolve o nosso caso: dentro do Claude Code, o
subagente é uma chamada de ferramenta que **retorna** (o fim é um evento do próprio programa). Entre
CLIs de fornecedores diferentes não há esse retorno; o fim do turno tem de ser **observado**. Por
isso o leitor de estado é o problema central de uma orquestração multi-fornecedor.

## 1. Como os outros fazem

### 1.1 CLI Agent Orchestrator (AWS)

`[citado]` `src/cli_agent_orchestrator/providers/base.py`, `providers/claude_code.py`,
`services/status_monitor.py`, `constants.py`.

- Um **provider por CLI** (Claude Code, Codex, Kiro, Cursor, Gemini via Antigravity, OpenCode,
  Copilot, Grok, Kimi…), cada um com seu conjunto de regex sobre a tela. Estados:
  `IDLE`, `PROCESSING`, `COMPLETED`, `WAITING_USER_ANSWER`, `ERROR`, `UNKNOWN`.
- `IDLE` (antes da primeira tarefa) é separado de `COMPLETED` (depois de uma tarefa): o mesmo
  prompt vazio significa coisas diferentes conforme já houve input.
- Para o Claude Code: spinner `[✶✢✽✻✳·*]…ing…` = processando; `✻ Cultivated for 12s` = resumo de
  conclusão; `↑/↓ to navigate | Enter to confirm` = esperando resposta do usuário; padrões
  separados para os diálogos de confiança, bypass e upgrade de modelo; e um
  `BACKGROUND_WAIT_PATTERN` para quando o Claude espera tarefas em segundo plano com cara de ocioso.
- **Quiescência**: a tela renderizada (pyte, 400×200) só é avaliada em duas bordas, quando a saída
  recomeça e depois de 200 ms sem saída. O comentário em `constants.py` registra que a avaliação
  a cada pedaço de saída "foi medida pior que o caminho cru" por causa de oscilações.
- **Estado pronto pegajoso**: depois de `IDLE`/`COMPLETED`/`WAITING_USER_ANSWER`, o monitor não
  volta a `PROCESSING` até um novo input, porque redesenhos da TUI apagam os marcadores e faziam o
  estado oscilar.
- Depois de mandar input, se o fim da tela não mudou desde o envio, continua `PROCESSING` (não lê
  a tela anterior ao prompt como resposta).
- Backend alternativo **herdr** (experimental): um runtime de terminal que publica `agent_status`
  por painel como evento, em vez de o CAO fazer polling e regex.
- Padrões de coordenação: *handoff* (síncrono), *assign* (assíncrono) e *send message*.

### 1.2 Orca

`[citado]` `src/main/runtime/agent-state-rules/*.json`, `src/shared/agent-title-glyphs.ts`,
`src/shared/agent-title-core.ts`, `src/main/agent-hooks/`.

- **Duas fontes num store só**: hooks gerenciados em ~25 CLIs e regras de tela declarativas.
- As regras são **dados**, não código: 14 arquivos JSON (claude, codex, gemini, opencode, cursor,
  pi…), cada regra com região (`title`, `screen`, `text`), padrão, prioridade e resposta
  (`idle`/`working`/`blocked`, força `strong`/`weak`, `requiresQuiet`).
- **Glifos de título** (`agent-title-glyphs.ts`): Claude `✳` ocioso; Gemini `✦` trabalhando, `⏲`
  trabalhando em silêncio, `◇` ocioso, `✋` pedindo permissão; spinners genéricos em braile
  (U+2800–U+28FF) e quarto de círculo (U+25D0–U+25D3).
- Regra de processo (`AGENTS.md` do Orca): toda regra que lê a tela de uma CLI é escrita contra
  uma **transcrição capturada**, nunca contra uma tela lembrada.

### 1.3 Agent teams do Claude Code e subagentes

`[citado, fontes secundárias]` Blogs e guias (não há leitura de código): colegas com lista de
tarefas compartilhada e caixa de mensagens. Funcionam porque são todos Claude Code, no mesmo
programa: o fim de um colega é um evento interno. Não se aplica a CLIs de outros fornecedores.

## 2. Como o NarraTer fazia até 2026-10-14

`[citado]` `src-tauri/src/screen.rs`, `src-tauri/src/pty.rs`.

- vt100 headless por PTY; hash do conteúdo da tela a cada 200 ms.
- Agentes: `working` se o rodapé tem `esc to interrupt` (até 20 s sem mudar) ou a tela mudou há
  menos de 1,2 s; `attention` para menus de permissão e `(y/n)`; senão `idle`.
- Shells: `working` se há job em primeiro plano (grupo de processos do PTY ≠ shell) ou a tela
  mudou há menos de 0,6 s. **Dentro do tmux** o primeiro plano do PTY é o cliente tmux, então só a
  tela contava.
- O título da janela era descartado.

## 3. O que já temos de equivalente e o que falta

| Mecanismo | CAO | Orca | NarraTer |
|---|---|---|---|
| Emulador headless | pyte | xterm headless | vt100 |
| Quiescência antes de declarar ocioso | 200 ms | `requiresQuiet` | 1,2 s (agentes), 0,6 s (shells) |
| Não ler a tela pré-prompt como resposta | sim | sim | `MIN_TURN` + `last_change` na entrega |
| Primeiro plano de shells | — | — | sim; **no tmux, a partir desta sessão** |
| Título da janela | não | sim | **capturado a partir desta sessão** |
| Regras como dados | não (código) | sim (JSON) | não (código) |
| Medição de acerto | testes e2e | transcrições capturadas | **`turn_eval`, a partir desta sessão** |

## 4. Método de medição

`[citado]` `src-tauri/examples/turn_eval.rs`, `examples/turn_replay.rs`,
`tests/turn_transcripts.rs`.

- **Detector**: o próprio `ScreenModel` + `classify` do app, no tick de 200 ms do monitor, e a
  regra do `ask` (devolve no primeiro estado não-`working` pelo menos `MIN_TURN` = 800 ms depois
  da entrega). O prompt é digitado com o `plan_delivery` do app.
- **Oráculo** (a verdade, independente da tela):
  - shell: o próprio comando grava um timestamp ao terminar (`cmd; date +%s%N >> arquivo`);
  - Claude Code: hooks `Stop` (fim do turno) e `PermissionRequest` (diálogo de permissão),
    configurados por `--settings` **só no avaliador**. O app continua sem hooks (D01).
- **Métricas por execução**: estado devolvido, e latência = detecção − oráculo. Latência negativa
  é **prematura**: o ask devolveu antes do fim, com a resposta pela metade.
- **Cenários**: shell silencioso (`sleep 4`), com saída contínua, rajada seguida de silêncio; os
  três diretamente e dentro do tmux (como os terminais persistentes do app). Claude Code: resposta
  curta, ferramenta silenciosa (`sleep 8` pelo Bash), resposta longa, pedido de permissão.
- **Ambiente**: container Linux, Xeon 2,8 GHz, 4 vCPUs; Claude Code 2.1.299 com o modelo `haiku`;
  tmux do Ubuntu 24.04. Comando: `cargo run --release --example turn_eval -- --reps N [--claude]
  [--tmux-fg none|pane]`. Os resumos estão em `docs/pesquisa/dados/leitor-de-estado/`.
- **Gravações**: cada execução grava a saída bruta (asciicast) e a linha do tempo (estado, título,
  rodapé). `turn_replay` reproduz uma gravação no emulador para desenvolver regras offline.

## 5. Resultados

`[medido]` Linha de base = leitor como estava; depois = com as mudanças das decisões D10 e D11.

| Cenário | Antes: corretos / prematuros (n) | Antes: latência | Depois: corretos / prematuros (n) | Depois: latência |
|---|---|---|---|---|
| shell direto (3 cenários) | 9 / 0 (9) | 603–801 ms | 30 / 0 (30) | 602–802 ms |
| tmux, saída contínua | 3 / 0 (3) | 763–769 ms | 10 / 0 (10) | 749–778 ms |
| **tmux, comando silencioso** | **0 / 3 (3)** | **−3,2 s** | **10 / 0 (10)** | 603–802 ms |
| **tmux, rajada + silêncio** | **0 / 3 (3)** | **−2,2 s** | **10 / 0 (10)** | 796–802 ms |
| Claude, resposta curta | 3 / 0 (3) | 1.294–1.355 ms | 5 / 0 (5) | 481–670 ms |
| Claude, ferramenta silenciosa | 3 / 0 (3) | 1.332–1.454 ms | 5 / 0 (5) | 466–644 ms |
| Claude, resposta longa | 3 / 0 (3) | 1.440–2.112 ms | 5 / 0 (5) | 470–643 ms |
| **Claude, permissão** | **0 / 0 (3), nunca detectado em 180 s** | — | **3 / 0 (3)** | 103–222 ms |

Leitura estatística, com a ressalva de amostras pequenas e uma única máquina:

- Claude, turnos normais agrupados: antes n=9, mediana 1.355 ms; depois n=15, mediana 551 ms. As
  faixas não se sobrepõem (teste exato de Mann–Whitney, p ≈ 1,5×10⁻⁶ bicaudal).
- tmux com comando silencioso (os dois cenários): antes 6/6 prematuros, depois 0/20 (teste exato
  de Fisher, p ≈ 4,3×10⁻⁶).
- Shell direto não mudou (o primeiro plano já era lido pelo grupo de processos do PTY).

### 5.1 Achados durante a medição

1. **Shell no tmux** (`[observado]`): dentro do tmux o primeiro plano do PTY é o cliente tmux;
   sem esse sinal, a tela parada durante um `sleep` parecia fim de turno. Mesmo lendo o primeiro
   plano certo, uma primeira correção ainda deu 4/15 "prematuros" de 2–3 ms: a leitura caía no
   instante entre o fim do job e o redesenho do prompt. Tratar "job em primeiro plano" como
   atividade da tela eliminou o caso (D10).
2. **Diálogo de permissão do Claude** (`[observado]`): nas 3 execuções da linha de base o
   Claude Code piscou um `●` a cada 0,6 s enquanto o diálogo esperava (289 piscadas em 180 s), e
   a tela nunca ficou parada pelos 1,2 s exigidos: o ask ficaria preso até o timeout. Nas 11
   execuções seguintes **não houve piscada** (a saída simplesmente para). A causa da diferença não
   foi identificada; os dois comportamentos estão nas gravações de teste.
3. **Título da janela** (`[observado]`, Claude Code 2.1.299): `✳ <tarefa>` em repouso,
   `◐`/`◑ <tarefa>` durante o turno, e `✳` também com o diálogo de permissão aberto.
4. **Escolha do oráculo** (`[medido]`): o hook `Notification` dispara ~5,8–6,0 s depois de o
   diálogo aparecer (ele existe para avisar um usuário distraído); medido contra ele, o leitor
   parecia "prematuro" em 5,9 s nas 5 execuções. O `PermissionRequest` dispara quando o diálogo é
   mostrado; contra ele a latência é 103–222 ms. O oráculo certo é o `PermissionRequest`.
5. **Primeiro uso do Claude** (`[observado]`): numa instalação nova, a tela de tema e o
   "press enter to continue" são lidos como ociosos; um ask nessa hora seria digitado no
   onboarding. Não tratado nesta sessão (ver backlog, etapa E).

### 5.2 Limitações

- Só o Claude Code foi medido entre os agentes. Codex, Gemini CLI e OpenCode não estão
  instalados aqui; as regras de título para eles vêm do catálogo do Orca (`[citado]`) e **não
  foram verificadas**.
- Uma versão do Claude Code (2.1.299) e um modelo (`haiku`); TUIs mudam entre versões.
- A latência inclui a granularidade do tick de 200 ms do avaliador.
- Shells remotos (SSH/Docker) continuam só com a tela: o `/proc` é da máquina local.

## 6. O que fica para depois

- Regras como dados (JSON por CLI, como o Orca) e transcrições de Codex, Gemini e OpenCode.
- Onboarding e diálogos de confiança como estado próprio, para o ask não digitar neles.
- `BACKGROUND_WAIT_PATTERN` do CAO: o Claude esperando tarefas em segundo plano com cara de ocioso.
- Estado "pronto" pegajoso (CAO) para estabilizar o que a interface mostra.
