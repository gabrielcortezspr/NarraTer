# Estado do projeto

Sobrescrito a cada fechamento de sessão (`/encerrar`). Nunca acrescentado; histórico é `git log`.
Máximo de ~30 linhas: se passar disso, algo aqui deveria estar num commit ou num doc.

## Última sessão

2026-10-14, Gabriel com Claude Code, branch `ccr-03e0166a-1f50u2`. Estudo visual e perfil de design
"Palco" (D13–D16), com o primeiro passo do rework visual (etapa V: V1–V3).

## Onde parou

Tokens novos em `src/index.css`/`tailwind.config.js`; 440 classes com hex trocadas por tokens;
terminal, arestas, palco, toolbar, barra lateral e demais tiles redesenhados. Verificado: `tsc`,
`cargo test`, app real sob Xvfb (capturas em `docs/pesquisa/img/`). Leitor de estado e asks
(D07–D12) seguem como estavam.

## Próximo passo

V4 (uso dos agentes): ler a porcentagem do Claude (rota a decidir: endpoint OAuth ou statusline,
`docs/pesquisa/visual.md` §5) e mostrar no tile; depois revisar chat, composer, role manager e
paleta pelo `docs/design.md`.

## Decisões pendentes

- Rota do uso do Claude: endpoint OAuth (sem configurar nada, pode dar 429) ou statusline (sem
  custo, mexe na statusline do usuário).
- O3: quando a tela ociosa e o `narrater done` chegarem juntos, qual vale.

## Lembrar

- No modo dev, o recarregamento a quente do front (Vite) deixa os nomes fora do backend: `ask`
  responde "no agent named"; reiniciar o app resolve.
- No boot com tmux, a resposta DA do terminal às vezes aparece no prompt (backlog V).
- Teste no app real: `xvfb-run -a npx tauri dev` (ou `Xvfb :77` + `DISPLAY=:77`), captura com
  `import -window root`; identidade de um terminal em `/proc/<pid>/environ` do cliente tmux.
- Não usar `pkill -f` com padrão que aparece na própria linha de comando.
