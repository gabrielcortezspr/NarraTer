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

## Fase 3 — Protocolo e MCP mais ricos

- **broadcast**: enviar para todos os peers de uma vez (padrão
  orquestrador → workers).
- **check_messages/inbox**: tool para o agente puxar mensagens pendentes em
  vez de depender só da injeção — cobre o caso do agente ocupado por muito
  tempo.
- **Progress notifications no MCP** durante ask longo (o protocolo MCP suporta
  `notifications/progress`), para o agente chamador não parecer travado por
  120s.
- **Codex e custom com paridade**: codex aceita MCP via `-c mcp_servers...`;
  gerar a config equivalente em `getSpawnSpec` e estender o framing/prompt por
  tipo de agente.

## Fase 4 — Observabilidade no canvas

- **Ledger de mensagens** no backend (ring buffer por par origem→destino)
  emitido ao front por evento.
- Clicar numa edge agent-pipe abre o **histórico daquela conversa**; a edge
  anima/pulsa quando uma mensagem passa.
- No tile: mostrar o **conteúdo da fila pendente** (não só o contador) com
  opção de cancelar mensagem enfileirada.

## Fase 5 — Segurança e testes

- **Token por terminal**: `NARRATER_TOKEN` aleatório injetado no spawn e
  validado no IPC junto ao `from`, fechando o spoof de identidade via socket.
- **Testes Rust** para `resolve_route`, `strip_injected_echo` (casos com
  wrap), dedup de labels e a máquina de fila/idle — hoje nada disso tem teste
  e são exatamente as partes com mais regressão potencial.

## Ordem sugerida

Fase 1 primeiro e isolada — é o que transforma o `ask` de "funciona às vezes"
em confiável, e as fases seguintes se apoiam no id de mensagem que ela
introduz. Depois 2 (entrega), 3 (protocolo), 4 (UX) e 5 (hardening), cada uma
entregável de forma independente.
