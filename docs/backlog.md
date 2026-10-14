# Backlog

O que vamos fazer, por etapa. O plano de cada etapa é o que foi combinado; o andamento entra como
uma linha de citação sob o título (`/encerrar` atualiza), sem reescrever o plano. Etapas novas
entram no fim.

---

## O · Orquestração: durabilidade e eventos sem exigir cooperação

Origem: [`pesquisa/orca.md`](pesquisa/orca.md) §6.4 · decisão D06.

> Em andamento. 2026-10-14: O1 e O2 concluídas (D07, D08), testadas ponta a ponta no app real sob
> Xvfb; O4 coberta em parte (`wait <id>` retoma um ask depois do timeout do shell, sem reenviar).
> O painel de histórico da aresta ainda lê o ledger em memória, não o `asks.jsonl`.

**O1 · Ask assíncrono + `wait`.** `narrater ask "Dev" "…" --no-wait` devolve um id na hora.
`narrater wait [ids…] [--any|--all] [--timeout S]` devolve eventos
`{id, agent, kind: done|attention|question|exited|timeout, text}` assim que acontecem. O `ask`
síncrono atual passa a ser `--no-wait` + `wait <id>`, sem mudar de comportamento. Começa em
`ask_one` (`src-tauri/src/ipc.rs`), que já tem as fases de entrega e de espera separadas.

**O2 · Asks duráveis.** SQLite (`rusqlite`): id, de, para, prompt, estado
(`queued`/`delivered`/`done`/`attention`/`timeout`/`exited`/`unknown`), resultado e timestamps.
Depois de reiniciar o app, o que estava pendente vira `unknown`; nunca inferir o desfecho. O
painel de histórico da aresta passa a ler daqui (hoje é um `VecDeque` em memória limitado por
`LEDGER_CAP`).

**O3 · Mensagens tipadas opcionais.** `narrater done "resumo" [--failed] [--files a,b]` resolve o
ask com resultado estruturado. `narrater question "Líder" "…" [--options a,b]` vira evento
`question` no `wait` do líder sem encerrar o ask original. `narrater reply <id> "…"` responde.
Generaliza o ask-back atual.

**O4 · `ask --resume <id>`** para reanexar a um ask depois de um timeout do shell do líder, em vez
de reenviar.

**O5 · Resultado = saída desde a entrega.** Guardar a posição no scrollback do vt100 no momento
da entrega e devolver tudo que veio depois, com limite e cursor, em vez de só a tela visível.

**O6 · `narrater tell "Dev" "…"`.** Mensagem sem espera (steering), entregue no próximo idle.

**O7 · Contabilidade no modo Maestro.** A skill `narrater-manager` passa a pedir, depois de cada
`done`, a decisão entre reusar e `dismiss`; `narrater list` mostra quem está ocioso e sem tarefa.

**O8 · Tarefas com dependências no canvas.** Nós de tarefa ligados aos agentes, como quadro
visível, em vez de uma tabela escondida. Depois de O1–O3.

**Fora do escopo** (D06): heartbeat obrigatório, `worker_done` exigido, Run/Task/Dispatch,
federação entre máquinas.

---

## E · Detecção de estado declarativa

Origem: [`pesquisa/orca.md`](pesquisa/orca.md) §4 B1.

> Não iniciada. Achado de 2026-10-14: um **shell dentro do tmux** rodando um comando sem saída
> (`sleep 6`) é dado como ocioso em ~1 s, porque com tmux o backend não enxerga o processo em
> primeiro plano (`compute_turn` em `pty.rs`) e a tela parada parece fim de turno. Um ask para
> esse shell volta antes da hora. Corrigir aqui (ex.: perguntar ao tmux `#{pane_current_command}`).
>
> Em andamento. 2026-10-14: shell no tmux corrigido (D10); título da janela capturado e usado,
> com o catálogo do Orca (D11); avaliador `turn_eval` com oráculo, `turn_replay` e teste de
> transcrições reais (D12). Resultados em [`pesquisa/leitor-de-estado.md`](pesquisa/leitor-de-estado.md) §5.
> Falta: regras como dados (JSON), transcrições de Codex/Gemini/OpenCode, onboarding e diálogo de
> confiança como estado próprio, espera de tarefas em segundo plano do Claude.

Trocar as listas fixas de `screen.rs` (`BUSY`, padrões de attention) por regras JSON por agente
(região, regex, prioridade, `requiresQuiet`), partindo dos arquivos do Orca
(`src/main/runtime/agent-state-rules/*.json`, MIT, com atribuição). Capturar o título da janela
(OSC 0/2), que hoje não guardamos e é o sinal mais confiável do Claude Code. Validar cada regra
com o `turn_probe`.

---

## P · Ponte com o Orca (experimento)

Origem: [`pesquisa/orca.md`](pesquisa/orca.md) §4 C · decisão D05.

> Não iniciada. Antes: instalar o Orca numa máquina Linux e conferir a saída `--json`.

Nó "Orca" no canvas, só leitura primeiro (`orca worktree ps --json`,
`orca orchestration worker-list --json`); depois `narrater ask` atravessando para terminais do
Orca via `terminal send --enter --wait-submit` + `terminal wait --for tui-idle` + `terminal read`.
Sempre opcional.

---

## V · Visual e uso dos agentes

Origem: [`pesquisa/visual.md`](pesquisa/visual.md).

> Em andamento. 2026-10-14: perfil "Palco" definido em [`design.md`](design.md) (D13–D16); V1
> (tokens), V2 (tiles e arestas) e V3 (moldura em vidro, atalhos fora da barra lateral) aplicados;
> capturas antes/depois em `pesquisa/img/`. Falta: V4 (uso), modo claro, revisar painéis
> secundários (chat, composer, role manager, paleta) peça por peça. Achado: no boot com tmux, a
> resposta do terminal à consulta DA (`1;2c0;276;0c`) às vezes aparece digitada no prompt.

**V1 · Tokens.** Refazer `src/index.css` em pares superfície/texto, com níveis base × elevado
(canvas, painel, tile, popover), bordas em branco translúcido, texto em quatro níveis e o acento
restrito a estado (seleção, foco, agente trabalhando).

**V2 · Tiles e arestas.** Raio de ~10 px, cabeçalho integrado ao corpo, scrollbar discreta, texto
do cabeçalho ≥ 11 px; arestas discretas que só ganham cor e movimento quando há conversa.

**V3 · Barra lateral e moldura.** Barra lateral e toolbar como camada de controle translúcida
(o equivalente web do material "regular": `backdrop-filter` com blur e luminosidade); atalhos
saem da barra lateral.

**V4 · Uso dos agentes.** Backend lê a porcentagem por CLI (Claude, Codex, Gemini primeiro) no
modelo `usedPercent`/`windowMinutes`/`resetsAt`; o front mostra uma barra fina por janela (5 h,
7 dias) no tile do agente e um resumo na moldura, com cinza < 60%, amarelo < 80%, vermelho acima.
