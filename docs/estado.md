# Estado do projeto

Sobrescrito a cada fechamento de sessão (`/encerrar`). Nunca acrescentado; histórico é `git log`.
Máximo de ~30 linhas: se passar disso, algo aqui deveria estar num commit ou num doc.

## Última sessão

2026-10-14, Gabriel com Claude Code, branch `ccr-03e0166a-1f50u2`. Estudo do Orca e montagem da
documentação de desenvolvimento (`CLAUDE.md`, skills `encerrar` e `pesquisa`, hook de retomada).

## Onde parou

Estudo do Orca em `docs/pesquisa/orca.md`, com a análise de orquestração na §6. Decidido não
fundir os projetos (D05) e manter a cooperação opcional (D06). Plano da orquestração em
`docs/backlog.md`, etapa O. Nenhum código mudou nesta sessão.

## Próximo passo

Etapa O1 + O2: ask assíncrono com `wait` e asks duráveis em SQLite. Começar por `ask_one` em
`src-tauri/src/ipc.rs` e pelo `ask_waiters` em `src-tauri/src/pty.rs`.

## Decisões pendentes

- Onde fica o banco dos asks: por cena (junto do JSON em `~/.config/narrater/scenes/`) ou um só
  em `~/.config/narrater/`.

## Lembrar

- O Orca foi só lido, não executado. Conferir a saída `--json` antes da etapa P.
