---
name: benchmark
description: Roda o benchmark de orquestração do NarraTer, que compara versões do projeto (commits) na mesma tarefa de programação com três agentes (Narrador líder + DevA + DevB) e mede tokens, contexto, eficiência e trabalho feito contra testes escondidos. Use quando o usuário pedir para comparar versões, medir gasto de tokens ou eficiência da orquestração, testar se uma mudança melhorou a orquestração, rodar "o teste de orquestração", ou repetir/estender o estudo de docs/pesquisa/comparacao-versoes.md.
---

# Benchmark de orquestração

Tudo fica em `bench/orquestracao/`. O resultado do estudo vai para `docs/relatorios/` (`.tex`) e o
resumo para `docs/pesquisa/comparacao-versoes.md`. Antes de mudar qualquer peça, leia o estudo
anterior: a comparação só vale se a tarefa, os testes escondidos e o método forem os mesmos.

| Peça | O que é |
|---|---|
| `fixture/pedidos/` | Repositório que os agentes recebem: código, `REGRAS.md`, `docs/PEDIDO-MULTIMOEDA.md` (funcionalidade) e `docs/RECLAMACOES.md` (dois defeitos plantados) |
| `oculto/test_oculto.py` | 31 testes escondidos em quatro níveis (Basico, Bordas, Defeitos, Regressao). **Os agentes nunca podem ver este arquivo**; nunca copie para a pasta de trabalho |
| `oculto/placar.py` | Roda os testes escondidos e os da própria pasta e devolve JSON |
| `solucao/pedidos/` | Solução de referência: tem que passar 31/31 (confira depois de mexer nos testes) |
| `rodar.py VERSAO REP` | Uma rodada, do zero até o placar |
| `rodar_todas.sh` | As rodadas em ordem contrabalançada |
| `analisar.py RUN...` | Métricas das transcrições (`metricas.json` por rodada) |
| `agregar.py` | Tabelas e gráficos de todas as rodadas para o relatório |

## Preparar as versões

1. Cada versão é uma worktree em `$BENCH/vX` (padrão `BENCH=/home/user/bench`):
   `git worktree add --detach $BENCH/vX <commit>`.
2. Compile com `$BENCH/build.sh X` (ou à mão: `npm ci && CARGO_TARGET_DIR=$BENCH/target npx tauri build --no-bundle`)
   e copie `narrater` e `narrater-cli` para `$BENCH/bin/vX/`. O `narrater-cli` tem que ficar ao
   lado do `narrater`: o app o instala a partir da própria pasta.
3. Versão nova: confira em `rodar.py` (`cena`, `papeis`) se o formato da cena, os campos dos nós
   e os textos dos papéis padrão continuam onde o script procura. Uma versão que mude o jeito de
   subir agentes pode precisar de um caso próprio, como a A (sem `cwd`, arestas dirigidas).

## Regras do método (não quebre sem registrar em decisoes.md)

- **Toda rodada começa zerada**: HOME próprio (config do Claude, skills, sessões, memória,
  `~/.config/narrater`), servidor tmux próprio (`TMUX_TMPDIR`), pasta de trabalho copiada do
  fixture, ambiente mínimo (só login e proxy; nada de `CLAUDE_CODE_*` da sessão que dispara).
  Nenhum agente retoma sessão (`--session-id` novo a cada início).
- **O mesmo para todas as versões**: o wrapper `~/.local/bin/claude` fixa o modelo por agente
  (Narrador = Opus 5.5, DevA = Sonnet 5.5, DevB = Haiku 5.5) e entrega o pedido ao Narrador
  como prompt inicial, 25 s depois de ele subir. O texto do pedido está em `rodar.py` (`PEDIDO`).
  Os papéis são os padrão de cada versão (leader/coder), como o seletor de agentes os aplicaria.
- **`IS_SANDBOX=1`**: o container roda como root e o Claude recusa `--dangerously-skip-permissions`
  sem isso.
- **Ordem contrabalançada** e a métrica principal de tokens é **processados** (entrada + cache
  lido + cache criado): o cache do servidor dura até 1 h e baratearia quem roda depois.
- **Fim**: o Narrador escreve `TAREFA CONCLUIDA`, ou 40 min (`BENCH_LIMITE_S`).
- Precisa de um X: `Xvfb :77 -screen 0 1600x1000x24 &` antes (o app é Tauri).

## Rodar

```sh
cd bench/orquestracao
python3 rodar.py D 0                 # piloto: confira final.png, agentes.log e o placar
./rodar_todas.sh                     # 12 rodadas, ~3 h; ou passe a ordem: ./rodar_todas.sh A1 B1
python3 analisar.py $BENCH/runs/v*-r[1-9]
python3 agregar.py                   # tabelas e figuras em docs/relatorios/
```

Cada rodada em `$BENCH/runs/vX-rN/`: `agentes.log` (rótulo, modelo, sessão, pasta), `inicio`,
`rodada.json`, `placar.json`, `diff.patch`, `final.png`, `tela-*.txt` (tmux, versões B+),
`app.log`, `home/.claude/projects/*/<sessão>.jsonl` (as transcrições).

## Relatar

- Separe `[medido]` de `[observado]` e `[inferido]` (D09). Com 3 rodadas por versão, só chame de
  diferença o que não se sobrepõe entre as rodadas; o resto é observação.
- Custo em US$ é preço de API equivalente (tabela em `analisar.py`): a conta é de assinatura.
- Atualize `docs/relatorios/orquestracao-versoes.tex`, compile com `tectonic` (ou `pdflatex`) e
  resuma em `docs/pesquisa/comparacao-versoes.md`.
