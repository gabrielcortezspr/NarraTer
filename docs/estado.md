# Estado do projeto

Sobrescrito a cada fechamento de sessão (`/encerrar`). Nunca acrescentado; histórico é `git log`.
Máximo de ~30 linhas: se passar disso, algo aqui deveria estar num commit ou num doc.

## Última sessão

2026-10-15, Gabriel com Claude Code, branch `ccr-03e0166a-1f50u2`. Benchmark de orquestração entre
versões (D19): 4 versões × 3 rodadas, relatório em `docs/relatorios/orquestracao-versoes.pdf`.

## Onde parou

`bench/orquestracao/` (skill `benchmark`) roda a mesma tarefa com 3 agentes em qualquer commit e mede
tokens, contexto, tempo e trabalho feito contra 31 testes escondidos. Resultado
(`pesquisa/comparacao-versoes.md`):
- todas as versões tiraram 31/31;
- A (MCP) foi a mais barata e D a mais cara: 2,2 contra 4,6 min e 1,8 contra 3,4 milhões de tokens;
- o mecanismo identificado é a resposta do `ask` lida da tela, que chega cortada e força pedidos
  repetidos.

Dados brutos em `bench/orquestracao/resultados/2026-10-15/`.

## Próximo passo

O5 (`ask` devolve toda a saída desde a entrega, não só a tela) e rodar o benchmark de novo como
aceite. Depois: tarefa mais difícil (sem teto) e agentes de outros fornecedores no benchmark; uso de
Codex e Gemini (V4).

## Decisões pendentes

- O3: quando a tela ociosa e o `narrater done` chegarem juntos, qual vale.

## Lembrar

- Benchmark: as versões compiladas e as rodadas ficam em `/home/user/bench` (fora do repositório e
  efêmeras); `$BENCH/build.sh X` recompila. Precisa de `Xvfb :77` e `IS_SANDBOX=1`.
- O app instala a statusline no `~/.claude/settings.json` de quem roda: em teste, use HOME próprio.
- O vt100 mostra o desenho de linha DEC como `qqqq` no texto das respostas (backlog V).
- No modo dev, o recarregamento a quente do front deixa os nomes fora do backend; reiniciar resolve.
- Não usar `pkill -f` com padrão que aparece na própria linha de comando.
