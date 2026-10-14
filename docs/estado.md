# Estado do projeto

Sobrescrito a cada fechamento de sessão (`/encerrar`). Nunca acrescentado; histórico é `git log`.
Máximo de ~30 linhas: se passar disso, algo aqui deveria estar num commit ou num doc.

## Última sessão

2026-10-14, Gabriel com Claude Code, branch `ccr-03e0166a-1f50u2`. Estudo do Orca, memória de
desenvolvimento (CLAUDE.md, skills `encerrar` e `pesquisa`) e etapas O1 + O2 da orquestração.

## Onde parou

`narrater ask --no-wait`, `narrater wait` e `narrater asks` funcionando, com asks duráveis em
`~/.config/narrater/asks.jsonl` (D07, D08). Verificado: `cargo test` (79 + 6), `tsc`, e ponta a
ponta no app real sob Xvfb (ordem de chegada, espera parcial, `--all`, retomada por id, restart →
`unknown`). Skills dos agentes já ensinam o fluxo.

## Próximo passo

Etapa O3, mensagens tipadas opcionais: `narrater done` / `question` / `reply`, começando pelo
ask-back em `run_ask` (`src-tauri/src/ipc.rs`), que hoje encerra o ask do líder com a pergunta.

## Decisões pendentes

- Quando chegarem os dois sinais (tela ociosa e `narrater done`), qual vale para encerrar o ask.

## Lembrar

- Shell dentro do tmux com comando silencioso parece ocioso (backlog, etapa E). Em teste de ponta a
  ponta use comandos que imprimem enquanto rodam.
- Teste ponta a ponta neste container: `apt install libwebkit2gtk-4.1-dev xvfb tmux`, cena em
  `~/.config/narrater/scenes/default.json`, `xvfb-run -a npx tauri dev`, e a identidade de um
  terminal lida de `/proc/<pid>/environ` do cliente tmux.
- O Orca foi só lido, não executado. Conferir a saída `--json` antes da etapa P.
