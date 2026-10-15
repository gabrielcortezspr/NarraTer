# Estado do projeto

Sobrescrito a cada fechamento de sessão (`/encerrar`). Nunca acrescentado; histórico é `git log`.
Máximo de ~30 linhas: se passar disso, algo aqui deveria estar num commit ou num doc.

## Última sessão

2026-10-15, Gabriel com Claude Code, branch `ccr-03e0166a-1f50u2`. V4 (uso do Claude pela
statusline, D17) e conexões como cordas com física (D18). PR #5 (Palco) já na main.

## Onde parou

`usage.rs` instala ou embrulha a statusline do Claude, e `narrater usage-report` leva o payload ao
backend. A barra lateral mostra as janelas de 5 h e semanal; o tile mostra um anel com o contexto.
As arestas viraram cordas (`src/lib/rope.ts`): alças nos quatro lados, desvio de nós, sono depois
de assentar. Verificado: `tsc`, `cargo test` (85 ok), `scripts/rope-check.ts`, `turn_eval` com
statusline (12/12) e o app real sob Xvfb (`docs/pesquisa/img/cordas.gif`).

## Próximo passo

Uso de Codex e Gemini (backlog V4). Depois, V5 (cordas com muitos nós) e a revisão do chat,
composer, role manager e paleta pelo `docs/design.md`.

## Decisões pendentes

- O3: quando a tela ociosa e o `narrater done` chegarem juntos, qual vale.

## Lembrar

- O app instala a statusline no `~/.claude/settings.json` de quem roda. Em teste, faça backup
  antes e restaure depois; apague `~/.local/share/narrater/claude-statusline.installed`.
- O vt100 mostra o desenho de linha DEC como `qqqq` no texto das respostas (backlog V).
- No modo dev, o recarregamento a quente do front deixa os nomes fora do backend; reiniciar o
  app resolve.
- No boot com tmux, a resposta DA do terminal às vezes aparece no prompt (backlog V).
- Teste no app real: `Xvfb :77` + `DISPLAY=:77`, captura com `import -window root`.
- Não usar `pkill -f` com padrão que aparece na própria linha de comando.
