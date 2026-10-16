# Estado do projeto

Sobrescrito a cada fechamento de sessão (`/encerrar`). Nunca acrescentado; histórico é `git log`.
Máximo de ~30 linhas: se passar disso, algo aqui deveria estar num commit ou num doc.

## Última sessão

2026-10-15, Gabriel com Claude Code, branch `ccr-03e0166a-1f50u2`. Benchmark de orquestração (D19) e,
a partir dele, o `ask` lendo o transcript do Claude (D20): versões E, F e G medidas.

## Onde parou

- O `ask` devolve a resposta final inteira do transcript do Claude e não dá o turno por encerrado
  antes de ele terminar de fato (`src-tauri/src/transcript.rs`, D20).
- Na versão G (`c6ba63e`), as falhas do protocolo foram a zero (0 de 24 asks antes da hora, 0 respostas
  cortadas), com custo de US$ 0,74 contra 1,02 da D.
- Relatório com a seção 7 (E, F, G) em `docs/relatorios/orquestracao-versoes.pdf`; estudos novos em
  `pesquisa/agent-teams.md` e `pesquisa/orca.md` §7.

- Painel Activity (Ctrl+J, D21): histórico de estados, asks e mensagens gravado pelo backend em
  `~/.config/narrater/activity.jsonl`; verificado no app real (`docs/pesquisa/img/painel-atividade*.png`).

## Próximo passo

O8: tarefas com dependências e pega automática (inspirada no Agent Teams), medida no benchmark. Depois:
leitor de transcript do Codex; variantes O9 (dev ↔ dev, statusline, tarefa sem teto).

## Decisões pendentes

- O3: quando a tela ociosa e o `narrater done` chegarem juntos, qual vale.
- Tela alternativa do Claude: manter (decidido por ora); desligar seria preferência opcional.
- Gerador de equipes (`pesquisa/orquestracao.md` §4, backlog O11): formato proposto, aguardando o Gabriel. Antes,
  o braço "sozinho" do benchmark (O9) decide se equipe vale; precisa de cota para rodar.

## Lembrar

- Benchmark: versões compiladas e rodadas em `/home/user/bench` (efêmero); `$BENCH/build.sh X`. Precisa de
  `Xvfb :77` e `IS_SANDBOX=1`. Cópia das métricas em `bench/orquestracao/resultados/`.
- O `turn_eval` vaza marcadores de sessão para o Claude medido (backlog E).
- O vt100 mostra o desenho de linha DEC como `qqqq` no texto das respostas (backlog V).
- Não usar `pkill -f` com padrão que aparece na própria linha de comando.
