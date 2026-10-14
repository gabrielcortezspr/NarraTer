# Estado do projeto

Sobrescrito a cada fechamento de sessão (`/encerrar`). Nunca acrescentado; histórico é `git log`.
Máximo de ~30 linhas: se passar disso, algo aqui deveria estar num commit ou num doc.

## Última sessão

2026-10-14, Gabriel com Claude Code, branch `ccr-03e0166a-1f50u2`. Leitor de estado: estudo do
CAO (AWS) e do Orca, avaliador com oráculo e três correções medidas (D10–D12).

## Onde parou

Leitor medido com `turn_eval`: shell no tmux 0/20 prematuros (antes 6/6); Claude 2.1.299 com
latência 466–670 ms (antes 1,3–2,1 s) e permissão detectada (antes nunca). Transcrições reais em
`src-tauri/tests/transcripts/` viram teste. Verificado: `cargo test` (82 + 6 + 1), `tsc`, app real.

## Próximo passo

Etapa O3 (mensagens tipadas opcionais), começando pelo ask-back em `run_ask`
(`src-tauri/src/ipc.rs`); ou, se houver Codex/Gemini instalados, gravar transcrições deles com
`turn_eval` e conferir os glifos de título citados do Orca (`screen.rs`, `title_signal`).

## Decisões pendentes

- Quando chegarem os dois sinais (tela ociosa e `narrater done`), qual vale para encerrar o ask.

## Lembrar

- A piscada de 0,6 s do Claude no diálogo de permissão apareceu em 3 execuções e sumiu em 11;
  causa não identificada (`pesquisa/leitor-de-estado.md` §5.1).
- Claude recém-instalado mostra onboarding, que o leitor vê como ocioso (backlog E).
- `turn_eval --claude` precisa de `claude` logado; ele marca o onboarding como feito em `~/.claude.json`.
- Não usar `pkill -f` com um padrão que aparece na própria linha de comando: mata o shell que roda.
- Teste no app real: `apt install libwebkit2gtk-4.1-dev xvfb tmux`, cena em
  `~/.config/narrater/scenes/default.json`, `xvfb-run -a npx tauri dev`, identidade em `/proc/<pid>/environ`.
