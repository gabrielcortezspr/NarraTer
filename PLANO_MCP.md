# Plano de Melhoria — MCP e comunicação entre agentes

Diagnóstico sobre o protocolo narrater atual (socket Unix + narrater-mcp +
injeção idle-gated no PTY) e plano de evolução em 5 fases.

## Diagnóstico — fragilidades encontradas

1. **`ask` raspa output cru do PTY** (`ipc.rs`, handle_ask). A resposta volta
   cheia de escapes ANSI, frames de spinner e redraws do TUI do Claude Code —
   o chamador recebe ruído, não texto. O strip de ANSI existe no front
   (`ansi.ts`) mas não é usado aqui.
2. **`strip_injected_echo` é heurístico e quebra com line-wrap**: mensagens
   longas quebram em várias linhas no eco do TUI, o marcador `[narrater de X]`
   não casa, e o eco vaza para a resposta.
3. **Asks concorrentes ao mesmo alvo se misturam**: todo `ResponseListener`
   com o mesmo `target_id` recebe todo o output (`pty.rs`), então duas
   perguntas simultâneas ao mesmo agente recebem as respostas embaralhadas.
4. **Detecção de idle é só "1,5s de silêncio"** e o `MAX_QUEUE_WAIT` de 30s
   injeta texto num terminal ocupado — pode cair no meio de um redraw ou de
   input meio digitado do usuário.
5. **O protocolo exige resposta, mas a rota de volta pode não existir**: o
   system prompt manda "SEMPRE reporte o resultado de volta com send_message",
   mas se a edge A→B for unidirecional o reply falha com "sem conexão".
6. **`from` é spoofável**: qualquer processo local do usuário pode conectar no
   socket e alegar qualquer `NARRATER_ID`.
7. **Zero observabilidade**: nenhuma mensagem entre agentes fica registrada; a
   edge no canvas não mostra o que passou por ela. E zero testes no projeto.
8. **Só o claude recebe MCP**: codex e agentes custom ficam só com o CLI
   `narrater` e sem system prompt de protocolo.

## Fase 1 — Resposta explícita no ask ✅ (implementada)

Maior ganho; resolve 1, 2 e 3 de uma vez. Trocar "raspar o PTY" por "o agente
responde de propósito":

- Cada ask entregue ganha um id curto: `[narrater de planner #a3f2]: ...`.
- Novo modo `reply` no IPC (`narrater reply <id> <texto>` + tool MCP
  `reply_message`), que resolve o oneshot do ask pendente diretamente — texto
  limpo, sem eco, sem ANSI, sem adivinhação de idle.
- O system prompt em `agentPrompt.ts` passa a instruir: "responda com
  reply_message usando o id da mensagem".
- O scraping atual vira fallback apenas para alvos shell (onde faz sentido
  capturar stdout do comando), agora passando por um strip de ANSI no lado
  Rust antes de devolver.
- Como cada reply carrega o id da requisição, asks concorrentes deixam de se
  misturar naturalmente.

## Fase 2 — Entrega e idle mais robustos ✅ (implementada)

- **Idle real via hooks do Claude Code**: settings gerado em
  `~/.local/share/narrater/claude-hooks.json` com hook `Stop` →
  `narrater notify-idle`, passado ao claude via `--settings` no spawn. A
  primeira notificação marca a sessão como `hook_idle`: o timer de silêncio
  deixa de valer para ela (só o hook a torna Idle) e o force-inject sobe de
  30s (`MAX_QUEUE_WAIT`) para 120s (`HOOKED_QUEUE_WAIT`), que vira só um
  backstop para hook quebrado. Shells/TUIs sem hook mantêm o timer.
- **Bracketed paste na injeção** para claude/codex: o frame vai como paste
  explícito (`ESC[200~ … ESC[201~`) com `\r` em seguida — determinístico, sem
  o sleep de 300ms. Agentes custom (TUI desconhecido) mantêm o burst em duas
  fases como fallback.
- **Rota de resposta implícita**: a entrega de uma mensagem de A para B grava
  um grant B→A válido por 10 min (`REPLY_GRANT_TTL`); `resolve_route` aceita
  edge OU grant. Elimina o conflito com o "SEMPRE reporte de volta" do prompt
  para o send_message (o reply do ask já não passava por rota).

## Fase 3 — Protocolo e MCP mais ricos ✅ (implementada)

- **broadcast**: `narrater broadcast <msg>` / tool `broadcast_message` envia
  para todos os peers de saída de uma vez (padrão orquestrador → workers).
- **check_messages/inbox**: `narrater inbox` / tool `check_messages` puxa e
  drena as mensagens pendentes (concedendo os reply grants e liberando asks
  como na entrega normal) — cobre o agente ocupado por muito tempo.
- **Progress notifications no MCP**: o narrater-mcp agora roda cada tools/call
  numa thread (um ask bloqueante não trava as demais tools) e, quando o
  cliente manda `progressToken`, emite `notifications/progress` a cada 10s
  durante asks.
- **Codex com paridade de MCP**: spawnado com
  `-c mcp_servers.narrater.command=narrater-mcp`. ⚠️ validar manualmente a
  sintaxe do override na versão instalada do codex. Prompt de protocolo para
  codex/custom segue pendente (vem com o framing por tipo de agente).

## Fase 4 — Observabilidade no canvas ✅ (implementada)

- **Ledger de mensagens** no backend: ring buffer global (500 registros) com
  from/to/labels/kind/msg/#id/ts para todo send, ask, reply e broadcast; cada
  registro é emitido ao front via evento `narrater_msg`, e o comando
  `narrater_ledger(a, b)` devolve a conversa de um par (ambas as direções).
- Clicar numa edge agent-pipe abre o **histórico daquela conversa**
  (EdgeHistoryPanel, estilo chat, ao vivo via lastActivity); a edge **pulsa em
  verde** quando uma mensagem passa pela rota.
- No tile: o badge da fila virou popover com o **conteúdo pendente**
  (remetente, #id, texto) e botão de **cancelar** por mensagem
  (`pty_queue_cancel`; cancelar um ask acorda o chamador com erro de entrega).
  O evento `pty_queue` agora carrega os itens, não só o contador.

## Fase 5 — Segurança e testes ✅ (implementada)

- **Token por terminal**: `NARRATER_TOKEN` aleatório injetado no spawn,
  enviado pelo CLI e pelo narrater-mcp em toda requisição e validado no IPC
  antes do dispatch — um processo local que alegue um `NARRATER_ID` alheio
  sem o token recebe `Erro: NARRATER_TOKEN inválido`.
- **Testes Rust** (9): strip_ansi (CSI/OSC/escapes), strip_injected_echo
  (incluindo o caso de line-wrap), `route_allowed` (edge direcionada, grant
  válido e grant expirado), dedup de labels e o framing com/sem #id. A lógica
  de rota e o dedup foram extraídos para funções puras para serem testáveis.
- Pendente de fases futuras: testes da máquina de fila/idle (exige mock de
  PtySession) e permissões por nó no canvas (ACL v2).

## Ordem sugerida

Fase 1 primeiro e isolada — é o que transforma o `ask` de "funciona às vezes"
em confiável, e as fases seguintes se apoiam no id de mensagem que ela
introduz. Depois 2 (entrega), 3 (protocolo), 4 (UX) e 5 (hardening), cada uma
entregável de forma independente.
