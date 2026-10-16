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

**O5 · Resultado = saída desde a entrega.** ✅ 2026-10-15 para o Claude (D20). O `ask` lê no transcript
a resposta final inteira e usa o transcript também para não dar o turno por encerrado antes da hora.
Medido no benchmark: versões E, F e G em `pesquisa/comparacao-versoes.md`. Falta:
- leitor de transcript do Codex (`~/.codex/sessions/**/rollout-*.jsonl`);
- histórico do servidor tmux como segunda fonte, para agentes sem transcript (com
  `CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN` opcional, `pesquisa/orca.md` §7).

**O6 · `narrater tell "Dev" "…"`.** Mensagem sem espera (steering), entregue no próximo idle.

**O7 · Contabilidade no modo Maestro.** A skill `narrater-manager` passa a pedir, depois de cada
`done`, a decisão entre reusar e `dismiss`; `narrater list` mostra quem está ocioso e sem tarefa.

**O8 · Tarefas com dependências no canvas.** Nós de tarefa ligados aos agentes, como quadro
visível, em vez de uma tabela escondida. Depois de O1–O3. Reforçada pelo estudo do Agent Teams
(`pesquisa/agent-teams.md`): quem termina pega sozinho a próxima tarefa livre (`narrater task
next/done`), e quem não coopera recebe a tarefa por mensagem. É o próximo alvo de custo: depois da
O5, o que sobra de custo são as voltas ao Narrador entre fases.

**O9 · Variantes do benchmark.**
- Topologia em triângulo (DevA ↔ DevB) contra a estrela de hoje.
- Statusline ligada contra desligada com mais rodadas: na E, as rodadas sem statusline foram mais
  baratas, mas p = 0,2.
- Uma tarefa mais difícil, sem teto de qualidade.
- **Braço "sozinho"** (só o Narrador) e braço "dev + revisor" contra a equipe G, com métrica de custo por teste
  escondido aprovado. É a pergunta que o estudo A–G não fez: vale montar equipe? (`pesquisa/orquestracao.md` §3 F1–F2, §4.3).
  Primeiro da fila, porque decide o padrão do gerador (O11).

**O10 · Comando de aceite.** `narrater ask … --check "CMD"` (e o mesmo campo na tarefa da O8): no fim do turno o
backend roda o comando no cwd do alvo; se falhar, o ask termina em `attention` com a saída, em vez de `done`. Não exige
cooperação (D01). Ataca a família de falhas de verificação (`pesquisa/orquestracao.md` §3 F3).

**O11 · Gerador de equipes.** Especificação de equipe como dados (membros com preset, **modelo**, role, worktree e se
escrevem; arestas; notas com a tarefa e o critério de aceite; limite de agentes), aplicada de uma vez por
`narrater team create spec.json` e desfeita por `team dismiss <id>`, com layout em grupo. Modelos de partida: sozinho,
dev + revisor, dev + QA, leque de leitura, devs em paralelo com worktrees. Entradas: tela do app (sem IA), pedido a um
agente, e automático pelo Narrador quando a preferência "O Narrador pode montar equipes" (em `preferences.rs`) estiver
ligada, passando pelo portão da skill (≥ 2 subtarefas independentes com arquivos disjuntos, verificador com contexto
limpo, ou contexto que não cabe). A decisão e o motivo vão para uma nota (`pesquisa/orquestracao.md` §4).

**O12 · Laço e estagnação.** Perceber o líder repetindo o mesmo pedido ou um ask sem progresso (MAST: repetição é 15,7%
das falhas; Magentic-One replaneja depois de 2 passos parados) e avisar no painel Activity (`pesquisa/orquestracao.md` §3 F7).

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
> capturas antes/depois em `pesquisa/img/`. 2026-10-15: V4 feito para o Claude via statusline
> (D17) e conexões viraram cordas com física (D18). Falta: uso de Codex e Gemini, modo claro, revisar painéis
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
Feito para o Claude (D17). Pendente: Codex (`codex app-server`, `account/rateLimits/read`) e
Gemini (`retrieveUserQuota`).

**E · turn_eval vaza marcadores de sessão.** O `turn_eval` remove só 3 variáveis do ambiente do
filho. O Claude medido herdou o id de sessão da sessão que o disparou (o `session_id` da statusline
era o da sessão pai). Usar a mesma lista de `pty.rs` (`is_inherited_session_var`) e medir de novo.

**V5 · Cordas, próximos passos.** Medir a física com dezenas de nós; evitar que uma corda fique
presa entre dois nós próximos (escolher o lado do contorno pelo caminho mais curto); cordas
paralelas entre o mesmo par não podem se sobrepor.

**Achado (V4).** O texto que o vt100 extrai mostra os caracteres de desenho de linha DEC como
`qqqq` nas respostas de `ask`; traduzir o conjunto G0 especial.
