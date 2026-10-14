# CLAUDE.md

Contexto para qualquer agente que abrir este repositório. Aqui fica só o que **não muda**;
o estado vivo está em `docs/estado.md` e o índice da documentação em `docs/README.md`.

## O projeto

NarraTer: um canvas infinito para terminais e agentes de IA no Linux (Tauri 2). Cada terminal é
um nó; as arestas dizem quem pode falar com quem; os agentes conversam pela CLI `narrater`.
Nasceu como o equivalente Linux do [Maestri](https://www.themaestri.app/en) (macOS) e hoje
estuda também o [Orca](https://github.com/stablyai/orca) para a orquestração de agentes.

## Stack e comandos

Tauri 2 + Rust no backend (`src-tauri/`), React 18 + TypeScript + Vite no frontend (`src/`),
React Flow para o canvas, xterm.js para os terminais, Zustand para o estado.

```sh
npm install && npm run tauri dev          # app em desenvolvimento
cd src-tauri && cargo test                # testes do backend
npx tsc --noEmit                          # typecheck do frontend
cd src-tauri && cargo run --example turn_probe -- 'Reply with PONG' claude   # heurística de turno contra um programa real
cd src-tauri && cargo run --release --example turn_eval -- --reps 5 --claude # leitor de estado medido contra um oráculo
cd src-tauri && cargo run --example turn_replay -- FILE.cast --at 8          # reproduz uma gravação no emulador
```

A CLI que os agentes usam é o binário `narrater-cli` (`src-tauri/src/bin/narrater-cli/`), que o
app instala em `~/.local/bin/narrater`. As skills que o app instala nos agentes ficam em
`src-tauri/assets/SKILL*.md`. Não confunda com as skills **deste repositório** (`.claude/skills/`),
que servem para desenvolver o NarraTer.

## Princípios que não podem ser quebrados

**O terminal é a API universal (D01).** Nada no protocolo entre agentes pode *exigir* que o agente
coopere: nada de flags de fornecedor (`--append-system-prompt`, `--mcp-config`), hooks
obrigatórios ou resposta por ID que o alvo precise mandar. O estado do turno vem da tela
(`screen.rs`). Já tentamos o modelo cooperativo em julho e ele nos prendeu ao Claude Code.
Cooperação só entra como **opcional que melhora o resultado** (D05): se o agente não cooperar,
tudo continua funcionando pela tela.

**Instruções chegam por arquivo, não por flag.** Skills em `~/.claude/skills` e `~/.agents/skills`;
roles como `CLAUDE.md` + `AGENTS.md` em `.narrater/roles/<id>/`, onde o agente é iniciado.

**Aresta é permissão.** Todo pedido vindo da CLI passa por `resolve_target` no backend, que valida
a conexão. Nunca confie no nome que a CLI mandou.

**Nunca reenviar prompt.** Timeout de `ask` tira o prompt da fila e diz para usar `check`. Reenviar
duplica trabalho num agente que talvez já esteja fazendo.

**Quem é dono de quê.** O backend é dono de PTYs, estado de turno, fila e IPC. O store do
frontend é a fonte da verdade do grafo (nós, arestas, notas); comandos de canvas da CLI vão ao
frontend por `canvas_request`.

**Regra que lê a tela de um agente se testa contra o programa real (D12).** Meça com o
`turn_eval` (oráculo independente: timestamp do comando, hooks do Claude só no avaliador) e
acrescente a transcrição em `src-tauri/tests/transcripts/`, que o teste `turn_transcripts`
reproduz. Nunca escreva a regra contra uma tela lembrada.

**Toda decisão é justificada (D09).** O projeto precisa de margem para explicação científica.
Decisão com alternativa real vai para `docs/decisoes.md` com as alternativas recusadas e por quê,
evidência rotulada (`[medido]` com método reproduzível, `[observado]`, `[citado]`, `[inferido]`) e
a condição que a derrubaria. Ao propor algo ao usuário, separe o que foi medido do que é
suposição.

## Armadilhas conhecidas

- O `invoke` do Tauri 2 espera argumentos em camelCase; `old_name` chega como `undefined` sem erro.
- Swap de agente (`recruit --replace`, `role assign`) respawna sob o mesmo id; o leitor antigo sai
  em silêncio, sem `pty_exit`.
- Marcadores de sessão do processo pai (`CLAUDE_CODE_*`, `MAESTRI_*`) não podem vazar para os
  terminais filhos.

## Convenções

- Commits em português, no formato `tipo: descrição` (`feat`, `fix`, `refactor`, `docs`, `perf`),
  com corpo explicando o porquê.
- Documentação para o usuário do app em inglês (`README.md`, `docs/narrater-cli.md`). Documentação
  interna (pesquisa, decisões, estado, backlog) em português.

## Estado, decisões e pesquisa

Cada fato é escrito num lugar só:

| O quê | Onde |
|---|---|
| Onde parou, próximo passo, pendências | `docs/estado.md` (reescrito a cada `/encerrar`) |
| Por que é assim (decisões com alternativa real) | `docs/decisoes.md` (só cresce) |
| O que vamos fazer, por etapa | `docs/backlog.md` |
| Estudos de outros projetos | `docs/pesquisa/` (skill `pesquisa`) |
| Regras permanentes | este arquivo |

O hook `SessionStart` (`.claude/settings.json` → `scripts/retomar.sh`) imprime o estado, os
últimos commits e o `git status` no início de toda sessão. Feche cada sessão com `/encerrar`
antes do commit.
