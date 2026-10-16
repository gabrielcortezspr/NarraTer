# Documentação do NarraTer

Leia X quando Y. Cada fato mora num lugar só; regras permanentes estão no
[`CLAUDE.md`](../CLAUDE.md) da raiz.

## Para quem usa o app

| Arquivo | Leia quando |
|---|---|
| [`../README.md`](../README.md) | Quer saber o que é o NarraTer, como instalar e rodar |
| [`narrater-cli.md`](narrater-cli.md) | Vai usar ou mudar a CLI `narrater`, ou entender o fluxo do `ask` |

## Desenvolvimento

| Arquivo | Leia quando |
|---|---|
| [`estado.md`](estado.md) | Vai começar a trabalhar: onde parou e o próximo passo |
| [`backlog.md`](backlog.md) | Quer saber o que vem depois e o plano de cada etapa |
| [`decisoes.md`](decisoes.md) | Vai mudar algo e quer saber por que está assim |
| [`design.md`](design.md) | Vai mexer na interface: perfil "Palco", tokens, grafo, tiles e moldura |

## Pesquisa

Estudos de outros projetos (skill `pesquisa`). Um arquivo por assunto.

| Arquivo | Assunto | Data |
|---|---|---|
| [`pesquisa/maestri.md`](pesquisa/maestri.md) | Como o Maestri faz agentes de qualquer fornecedor conversarem; origem do protocolo atual | 2026-10-01 |
| [`pesquisa/orca.md`](pesquisa/orca.md) | Arquitetura do Orca, correlação com o NarraTer; §6 orquestração; §7 como ler a saída do worker além da tela (transcript, cursor) | 2026-10-15 |
| [`pesquisa/leitor-de-estado.md`](pesquisa/leitor-de-estado.md) | Como saber que um agente terminou: CAO (AWS), Orca, o método de medição (`turn_eval`) e os resultados antes/depois | 2026-10-14 |
| [`pesquisa/agent-teams.md`](pesquisa/agent-teams.md) | Agent Teams do Claude Code: caixa de correio, lista de tarefas com dependências, resposta final empurrada; o que reaproveitar sem quebrar a D01 | 2026-10-15 |
| [`pesquisa/comparacao-versoes.md`](pesquisa/comparacao-versoes.md) | Benchmark de orquestração entre versões (A–D): tokens, contexto, tempo e trabalho feito; relatório em `relatorios/orquestracao-versoes.pdf` | 2026-10-15 |
| [`relatorios/equipe-ou-sozinho.pdf`](relatorios/equipe-ou-sozinho.pdf) | Teste desenhado para a equipe ganhar (tarefa `armazem`, 4 módulos independentes): sozinho × equipe paralela × dev + revisor; previsões da literatura contra o medido | 2026-10-16 |
| [`relatorios/quando-orquestrar.pdf`](relatorios/quando-orquestrar.pdf) | Quando orquestrar agentes não compensa: literatura (Kim et al., Vals.ai, Cognition, MAST) e o braço "sozinho" do benchmark (1 terminal: 2,5× mais rápido, 7× menos tokens, mesma nota) | 2026-10-16 |
| [`pesquisa/orquestracao.md`](pesquisa/orquestracao.md) | Estado da arte em orquestração multiagente (MAST, Anthropic, Cognition, Kim et al., Vals.ai), falhas do nosso método e o desenho do gerador de equipes | 2026-10-16 |
| [`pesquisa/visual.md`](pesquisa/visual.md) | Visual: HIG da Apple, tokens do Orca, Maestri, diagnóstico do NarraTer e de onde vem a porcentagem de uso de cada CLI | 2026-10-14 |
