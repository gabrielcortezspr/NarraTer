# MCP → Canvas: fundação e roadmap

Agentes rodando em terminais do NarraTer podem manipular o canvas através de
tools MCP do servidor `narrater-mcp`. Este documento descreve o contrato da
ponte e o roadmap das próximas tools.

## Fluxo

```
agente (claude) ── tool call canvas_* ──► narrater-mcp (stdio, ~/.local/bin)
  │ JSON {from: $NARRATER_ID, mode: "canvas", action, params}
  ▼ Unix socket /tmp/narrater-<pid>.sock
ipc.rs handle_canvas():
  valida a sessão do `from`
  req_id = uuid; registra oneshot::Sender em PtyStateInner.canvas_waiters[req_id]
  app.emit("canvas_request", { req_id, from, from_label, action, params })
  await oneshot (timeout 10s) ──► escreve o resultado no socket ──► tool retorna
                    ▲
frontend (src/lib/canvasBridge.ts, listener global no App):
  aplica a action no useCanvasStore (única fonte de verdade; o auto-save persiste)
  invoke("canvas_respond", { reqId, result })
```

Pontos do contrato:

- **`canvas_request`** (evento Tauri): `{ req_id, from, from_label, action, params }`.
  `params` chega como veio do agente (`serde_json::Value`, pode ser `null`).
- **`canvas_respond`** (comando Tauri, `src-tauri/src/canvas_bridge.rs`): resolve o
  waiter pelo `req_id`. O frontend **sempre** responde, inclusive com `Erro: ...`,
  para não deixar o agente esperando o timeout.
- **Timeout**: 10s (`CANVAS_TIMEOUT` em `ipc.rs`). Se o frontend não responder,
  o waiter é removido e a tool retorna `Erro: timeout aguardando o canvas`.
- **Convenção de resultado**: texto livre; prefixo `Erro:` marca falha
  (o narrater-mcp converte em `isError: true`).

## ACL (v1)

Qualquer agente com **sessão PTY válida** pode usar as tools de canvas — ele
manipula o próprio canvas em que vive. As **edges continuam governando apenas
a comunicação agente↔agente** (send/ask). Evolução futura: permissão por nó
(ex.: nota "trancada"), capability por role, e modo somente-leitura.

## Tools implementadas

| Tool | Params | Resultado |
|---|---|---|
| `canvas_list_nodes` | — | JSON `[{id, type, label, x, y}]` |
| `canvas_create_note` | `content` (req), `label?`, `x?`, `y?` | `ok: nota criada (id: …)`. Sem x/y, nasce à direita do terminal do agente. |
| `canvas_update_note` | `id` (id **ou** label), `content` (req), `mode?: append\|replace` | `ok: nota atualizada (id: …)`; erro legível se o label for ambíguo. |

## Roadmap (rascunho de schemas)

- `canvas_read_note { id }` → conteúdo da nota (deixa o agente retomar contexto).
- `canvas_create_text { text, x?, y? }` → bloco de texto leve.
- `canvas_move_node { id, x, y }` → reorganização espacial pelo agente.
- `canvas_connect_nodes { source, target }` → cria edge; **precisa** passar pela
  classificação de tipo (agent-pipe/agent-note) e por `syncConnections` para a
  rota valer no backend.
- `canvas_create_terminal { agent_type, role?, instructions? }` → spawn de agente
  por agente; exigirá política de aprovação do usuário (prompt/toast) antes de
  executar.
- `canvas_delete_node { id }` → destrutiva; exigirá confirmação do usuário.

## Gotchas

- O `narrater-mcp` é **regravado a cada boot** do app (`ipc.rs`); mudanças na
  lista de tools só valem após reiniciar o NarraTer **e** os agentes claude
  (o servidor MCP é processo filho do claude).
- `--allowedTools mcp__narrater` (em `src/lib/tauri.ts`) pré-aprova o servidor
  inteiro — tools novas não precisam de mudança ali.
- A ponte depende do refactor "store como única fonte de verdade": o nó criado
  pelo backend entra no zustand store sem resetar posições, e o auto-save
  persiste sem interação do usuário.
