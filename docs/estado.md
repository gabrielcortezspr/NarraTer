# Estado do projeto

Sobrescrito a cada fechamento de sessão (`/encerrar`). Nunca acrescentado; histórico é `git log`.
Máximo de ~30 linhas: se passar disso, algo aqui deveria estar num commit ou num doc.

## Última sessão

2026-10-16, Gabriel com Claude Code, branch `ccr-03e0166a-1f50u2`. Pesquisa de orquestração, equipe contra
agente sozinho medida, correção das falhas do método e gerador de equipes.

## Onde parou

- Um agente sozinho venceu a equipe nas duas tarefas do benchmark, com a mesma nota (D22):
  - `pedidos`: 1,6 contra 3,9 min (`relatorios/quando-orquestrar.pdf`);
  - `armazem`, quatro módulos independentes: 1,8 contra 2,8 min (`relatorios/equipe-ou-sozinho.pdf`).
- Feito a partir disso (D23, D24, `pesquisa/orquestracao.md` §8), verificado no app real com
  `bench/e2e/gerador-equipes.sh`:
  - `recruit --model`, com o modelo salvo na cena e mostrado no terminal;
  - `ask --check`: aceite rodado pelo backend; falha vira `failed`;
  - aviso de pedido repetido;
  - `narrater team create/templates`, com a chave Automatic teams na barra.

## Próximo passo

Medir a montagem "gerada" no benchmark: o Narrador com Automatic teams decide sozinho. Depois: O8 (tirar o líder
entre as fases), `team dismiss`, leitor de transcript do Codex.

## Decisões pendentes

- Chave Automatic teams: liga/desliga por ora; "propor e confirmar" se agentes montarem equipe sem motivo.
- O3: quando a tela ociosa e o `narrater done` chegarem juntos, qual vale.

## Lembrar

- Benchmark: `/home/user/bench` (efêmero; as rodadas antigas foram apagadas para liberar disco, e as métricas estão
  em `bench/orquestracao/resultados/`). Precisa de `Xvfb :77` e `IS_SANDBOX=1`.
- O disco do contêiner enche com `target/` e rodadas: apague `target/debug/incremental` e `$BENCH/runs` antigos.
- O socket do tmux não aceita caminhos longos: rode testes de ponta a ponta em `/tmp/<curto>`.
- Não usar `pkill -f` com padrão que aparece na própria linha de comando.
