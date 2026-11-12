# NarraTer × estado da arte: falhas do nosso método de orquestração e o gerador de equipes

> Estudo feito em 2026-10-16 **lendo** artigos, blogs de engenharia e documentação oficial, mais um levantamento
> do código do NarraTer (`main` em `dca6b51`). Nada aqui foi rodado: o que vem de fora é `[citado]`, o que
> vem do nosso benchmark é `[medido]` (D19, `pesquisa/comparacao-versoes.md`) e o resto é `[inferido]`.
> Conferimos direto na fonte os quatro números que mais pesam na recomendação (Kim et al., Vals.ai, Cognition
> 2026, estudo de topologia de 2026); os demais vieram de um levantamento e estão com o link para conferir.

## TL;DR

| Pergunta | Resposta curta |
|---|---|
| O protocolo (terminal como API, aresta como permissão, `ask` lendo o transcript) está errado? | Não. Ele é a parte forte: transporte agnóstico, topologia explícita, resposta verbatim e histórico completo são o que a literatura pede. |
| Onde está a falha? | **Na política, não no transporte.** Montamos equipe sem perguntar se vale a pena, damos o trabalho por pronto pelo fim do turno (não pelo critério de aceite), passamos tarefa e resultado como conversa livre, e deixamos vários agentes escrevendo no mesmo diretório. |
| E o benchmark? | Ele nunca mediu a pergunta principal: **equipe contra agente sozinho**. Falta o braço "Narrador sozinho", e a tarefa está no teto (31/31). |
| O gerador de orquestração vale? | Vale, se for um **gerador de equipes a partir de modelos (templates)**, com um portão de custo e com desmontagem. Não deve inventar topologia livre nem ser um agente a mais rodando o tempo todo. |

## 1. O que a pesquisa e a indústria dizem

### 1.1 Por que sistemas multiagente falham

**MAST** (Cemri et al., 2025, <https://arxiv.org/abs/2503.13657>): 14 modos de falha em três famílias, tirados de 150
traços anotados (κ = 0,88) `[citado]`.

- **Projeto do sistema, ~44%:** repetir passos (15,7%), não saber quando parar (12,4%), desobedecer à especificação (11,8%).
- **Desalinhamento entre agentes, ~32%:** raciocínio que não bate com a ação (13,2%), desvio da tarefa (7,4%), não pedir
  esclarecimento (6,8%).
- **Verificação, ~23,5%:** verificação errada (9,1%), verificação ausente ou incompleta (8,2%), término prematuro (6,2%).

Melhorar prompts e papéis ajudou pouco. O ChatDev subiu de 25% para 34% com prompts melhores e para 41% mudando a
topologia (um chefe com a palavra final e uma verificação de objetivo). Os autores avisam que checagens só no fim e em
baixo nível são insuficientes, e que verificadores costumam ser superficiais.

Continuações `[citado]`:
- **Who&When** (<https://arxiv.org/abs/2505.00212>): o melhor método automático acha o agente culpado em 53,5% das falhas
  e o passo decisivo em 14,2%.
- **Falso sucesso** (<https://arxiv.org/abs/2606.09863>): 75,8% das falhas em trajetórias de código com declaração de status
  eram um "pronto" falso, e juízes LLM não passaram de AUROC 0,65 para detectá-lo.

`[inferido]` As duas falhas que corrigimos em E–G cabem no MAST: o ask pronto antes da hora é término prematuro, e a
resposta cortada é perda de informação. O que ainda não tratamos é a família inteira de **verificação**: hoje "pronto"
é "o turno do agente acabou", que é exatamente o "pronto" declarado que esses estudos mostram não ser confiável.

### 1.2 Quando mais agentes compensam

**Kim et al., "Towards a Science of Scaling Agent Systems"** (Google/MIT, 180 configurações,
<https://arxiv.org/abs/2512.08296>) `[citado, conferido na fonte]`:
- coordenar dá retorno decrescente ou negativo quando o agente sozinho já passa de **~45%**;
- coordenação centralizada ganha 80,8% em tarefas paralelizáveis e perde de 39% a 70% em raciocínio sequencial;
- agentes independentes amplificam erros 17,2×, e a coordenação centralizada contém isso em 4,4×.

**Vals.ai, "Do Agent Teams Pay Off?"** (2026-10-09, <https://www.vals.ai/blogs/multi-agent-vibe-code-bench>), construindo apps
inteiros com um líder e até 5 subagentes `[citado, conferido na fonte]`:

| Modelo (esforço) | Sozinho → equipe | Custo | Tempo mediano |
|---|---|---|---|
| GPT 6 Sol (médio) | 77,6% → 84,9% (**+7,3, único significativo**) | 2,5× | 12,4 → 11,9 min |
| GPT 6 Sol (máx.) | 89,0% → 90,4% | 1,8× | 38,4 → 27,6 min |
| Opus 5.5 (médio) | 91,5% → 91,2% | 2,2× | 20,3 → 26,6 min |
| Opus 5.5 (máx.) | 89,8% → 93,2% | 5,1× (US$ 23,77 → 122) | 74,7 → 174,1 min |

Outros estudos na mesma direção `[citado]`:
- **Tran e Kiela, 2026** (<https://arxiv.org/abs/2604.02460>): com o mesmo orçamento de tokens de raciocínio, um agente
  sozinho empata ou ganha em raciocínio de vários passos.
- **"Rethinking the bounds of LLM reasoning"** (<https://arxiv.org/abs/2402.18272>): um agente sozinho bem instruído chega
  perto da melhor discussão entre agentes.
- **"Stop Overvaluing MAD"** (<https://arxiv.org/abs/2502.08788>): o debate raramente vence o raciocínio encadeado; modelos
  **diferentes** entre si ajudam.

Em código `[citado]`:
- **Agentless** (<https://arxiv.org/abs/2407.01489>) é um pipeline fixo, sem agente planejando: 32% no SWE-bench Lite a
  US$ 0,70.
- A análise do leaderboard do SWE-bench (<https://arxiv.org/pdf/2506.17208>) não conseguiu dizer que uma arquitetura vence
  a outra.

### 1.3 O que funciona em sistemas de produção

| Sistema | Como coordena | Como volta o resultado | Como sabe que acabou |
|---|---|---|---|
| Anthropic, pesquisa multiagente ([link](https://www.anthropic.com/engineering/multi-agent-research-system)) | Líder Opus + subagentes Sonnet; regras de esforço no prompt (fato simples = 1 agente; comparação = 2–4; complexo = 10+) | Subagente grava em arquivo e devolve referência ("telefone sem fio" evitado) | O líder decide; avaliação pelo estado final |
| Cognition 2025/2026 ([1](https://cognition.com/blog/dont-build-multi-agents), [2](https://cognition.com/blog/multi-agents-working)) | **Escrita em uma linha só**; outros agentes contribuem inteligência, não ações | Revisor com contexto limpo vê só o diff: ~2 bugs por PR, ~58% graves | — |
| OpenAI Agents SDK ([link](https://openai.github.io/openai-agents-python/multi_agent/)) | Agentes como ferramenta (gerente fica no controle) ou handoff (especialista assume) | Saída estruturada | Laço avaliador até aprovar |
| LangGraph ([link](https://www.langchain.com/blog/benchmarking-multi-agent-architectures)) | Supervisor ou swarm | Supervisor perdia ao parafrasear; repassar a resposta verbatim (`forward_message`) deu ~50% de melhora | — |
| Magentic-One ([link](https://arxiv.org/html/2411.04468v1)) | Livro de tarefas (fatos, plano) + livro de progresso a cada passo ("está satisfeito? está em laço? há progresso?") | Pelo livro | Contador de estagnação = 2 dispara replanejamento; sem os livros, −31% no GAIA |
| MetaGPT ([link](https://arxiv.org/html/2308.00352v7)) | Procedimentos fixos; agentes trocam **documentos**, não diálogo | PRD, desenho, interfaces | Rodar os testes (até 3 tentativas): +4 a +5 pontos |
| Claude Agent Teams (`pesquisa/agent-teams.md`) | Lista de tarefas com dependências e caixa de correio | Resposta final empurrada ao líder | Hooks `TaskCompleted`/`TeammateIdle` podem recusar o "pronto" |
| Google A2A ([link](https://a2a-protocol.org/latest/specification/)) | Tarefa com estados (submitted, working, input-required, completed, failed, canceled, rejected) | "Mensagens não devem carregar o resultado": resultado é *Artifact* | Estado terminal da tarefa |

Estudo de topologia em código do zero (<https://arxiv.org/abs/2607.27877>) `[citado, conferido na fonte]`: só mudar a
topologia move a nota em mais de 30 pontos e dobra o tempo. Pipelines estruturados convergem mais rápido e com mais
qualidade, e **supervisão gerencial pesada piora o resultado**.

Geradores automáticos de equipe `[citado]`:
- AutoAgents (<https://arxiv.org/abs/2309.17288>) e AgentVerse (<https://arxiv.org/abs/2308.10848>) inventam especialistas
  por tarefa.
- ADAS (<https://arxiv.org/html/2408.08435>), AFlow (<https://arxiv.org/abs/2410.10762>), MaAS
  (<https://arxiv.org/pdf/2502.04180>) e MasRouter (<https://arxiv.org/abs/2502.11133>) **buscam ou roteiam entre peças e
  topologias conhecidas**, com saída antecipada para consultas fáceis.
- O MaAS chega a 6–45% do custo das linhas de base, e o AFlow faz modelos pequenos baterem um grande "a 4,55% do custo".

`[inferido]` O que dá certo na geração automática é escolher bem entre formatos conhecidos, inclusive o formato "um agente
só", e não inventar equipes novas a cada pedido.

## 2. Como o NarraTer faz hoje

- **Transporte (D01, D20):** `ask` entrega pela tela e lê a resposta final inteira no transcript (Claude) ou na tela. A
  aresta é permissão (`resolve_target`, `src-tauri/src/ipc.rs`). O histórico é gravado pelo backend (D21).
- **Montar equipe:** só pelo modo Maestro (a coroa no terminal):
  - `narrater recruit` cria um terminal à direita de quem chamou (`freeSpot`, `src/lib/canvasReach.ts`) e liga uma aresta
    `agent-pipe` só com quem chamou (`src/lib/managerCommands.ts:180-183`), então a equipe é sempre uma estrela;
  - **não há escolha de modelo:** `--command "claude --model …"` vira preset Custom e perde o `--allowedTools` e o bloqueio
    de edição do orquestrador (`src/lib/agents.ts:84-104`);
  - não há limite de agentes nem confirmação;
  - montar uma equipe são N chamadas (`recruit`, `role create`, `note create`, `connect`), cada uma com timeout de 10 s de
    `canvas_request`.
- **A política está na skill `narrater-manager`** (`src-tauri/assets/SKILL-manager.md`): "reuse antes de recrutar", sem
  regra de tamanho, de custo ou de desmontagem.
- **Desmontar:** só `dismiss` de um em um. Notas e roles criados para a tarefa ficam (`roles.json`, `.narrater/roles/`).
  O O7 (reusar ou dispensar) não foi feito.
- **Preferências:** `src-tauri/src/preferences.rs` existe (só `persistent_sessions`), mas nenhuma tela usa.
- **Benchmark (D19, `bench/orquestracao/rodar.py:33`):** o pedido manda o Narrador "distribuir o trabalho entre DevA e
  DevB", todos no mesmo diretório. O critério de pronto é textual ("todos os testes passam") e confirmado pelo próprio
  Narrador.

## 3. Falhas do nosso método diante da proposta

Cada falha vem com a evidência e com o que a derrubaria.

**F1 · Equipe por padrão, sem portão.** `[medido]` No nosso benchmark, o custo e o tempo crescem com o número de rodadas
do líder, e a qualidade não muda (31/31 em todas as versões). `[citado]` Fora daqui:
- acima de ~45% de acerto do agente sozinho, coordenar não paga;
- em código, equipes custam de 1,8× a 5,1× para ganho quase sempre não significativo;
- o caso de pesquisa da Anthropic chama código de encaixe ruim para multiagente.

`[inferido]` Para tarefas como a do benchmark, um Opus sozinho provavelmente faria igual por menos. **Não sabemos
porque nunca medimos.** *Derrubaria:* um braço "sozinho" no benchmark custando o mesmo ou mais que a equipe.

**F2 · O benchmark não tem linha de base.** `[observado]` As versões A–G comparam equipe contra equipe. A tarefa está no
teto, e o pedido obriga a dividir. Isso responde "qual protocolo é mais barato", mas não "vale montar equipe". `[citado]`
Os estudos sérios sempre comparam com o agente sozinho no mesmo orçamento (Kim et al., Tran e Kiela, Vals.ai).

**F3 · "Pronto" é o fim do turno, não o critério de aceite.** `[observado]` O D20 resolve quando o turno termina, não se a
tarefa foi feita. `[citado]` Um quarto das falhas do MAST é de verificação; "pronto" falso é a falha dominante em código, e
juízes LLM não detectam isso (AUROC ≤ 0,65). O que funciona é olhar o ambiente: testes, diff, código de saída
(MetaGPT, harness longo da Anthropic, hook `TaskCompleted`).

`[inferido]` Isso **cabe na D01 sem cooperação**: o backend pode rodar um comando de aceite depois do turno, sem pedir
nada ao agente.

**F4 · Tarefa e resultado como conversa livre.** `[observado]` O `ask` leva texto livre e devolve texto livre para o
contexto do líder. `[citado]`
- Anthropic e A2A separam mensagem de artefato;
- o MetaGPT troca documentos;
- a Anthropic exige objetivo, formato de saída, ferramentas e limites em toda delegação;
- sem isso, há trabalho duplicado e lacunas.

`[inferido]` Já temos o artefato visível, que é a nota no canvas, e não usamos para isso. Ponto a nosso favor: o D20 já
devolve a resposta **verbatim**, que é a correção do LangGraph para o telefone sem fio.

**F5 · Vários escritores no mesmo diretório.** `[observado]` No benchmark, DevA e DevB editam o mesmo repositório.
`[citado]` A Cognition diz que funciona quando "a escrita fica em uma linha só", e o Agent Teams diz para não editar o mesmo
arquivo em paralelo. `[inferido]` Os andares com worktree (`--floor`) já resolvem isso, mas nada leva o líder a usá-los.

**F6 · O líder no meio de tudo.** `[medido]` Na G3, o Narrador ficou 59 s sozinho e o resto foi espera pelos devs. As
voltas ao líder entre fases são o custo que sobra (O8). `[citado]` Supervisão pesada piora o resultado (estudo de
topologia); lista de tarefas com dependências e pega automática é a saída do Agent Teams.

**F7 · Sem detecção de laço ou estagnação.** `[citado]` Repetir passos é o modo de falha nº 1 do MAST (15,7%). O
Magentic-One replaneja depois de 2 passos sem progresso. `[observado]` No NarraTer, nada percebe um líder pedindo a mesma
coisa de novo. O E2 com 14 pedidos foi exatamente isso, ainda que causado pelo ask prematuro.

**F8 · Uma equipe sem modelos diferentes.** `[citado]` Modelos diferentes ajudam no debate e na revisão, e o revisor com
contexto limpo é a peça multiagente com melhor evidência em código. `[inferido]` Esta é a vantagem única da D01, porque
podemos pôr um Codex revisando um Claude, mas o `recruit` nem escolhe modelo.

**Onde já estamos certos** `[inferido]`:
- A estrela com aresta como permissão é coordenação **centralizada**, a que contém erro (4,4× contra 17,2×).
- O painel Activity e o `asks.jsonl` dão o traço completo por agente, que o Who&When mostra ser necessário porque a
  depuração automática não localiza a falha.
- A resposta verbatim pelo transcript já está feita.
- Nada disso exige que o agente coopere.

## 4. O gerador de orquestração

### 4.1 Opinião

A ideia é boa e resolve um atrito real: hoje montar uma equipe custa N comandos manuais ou N chamadas do líder, e o
resultado é sempre uma estrela amontoada à direita. Mas a pesquisa (§1.2, §1.3) diz duas coisas que mudam o formato:

1. **O ganho está em escolher bem, inclusive escolher não montar equipe.** O gerador precisa ter "um agente só" e
   "agente + revisor" como respostas válidas, e elas devem ser as mais comuns.
2. **Modelos (templates) vencem invenção livre.** AFlow, MaAS e MasRouter roteiam entre formatos conhecidos, e os
   pipelines estruturados venceram no estudo de topologia.

O gerador não deve desenhar uma equipe nova a cada pedido. Ele escolhe um modelo e preenche os papéis e a tarefa.

### 4.2 Desenho proposto `[inferido]`

**Uma peça só, a especificação de equipe (dados, não prompt).** É um JSON com:
- membros (nome, preset, **modelo**, role, andar ou worktree, e se escreve ou só lê);
- arestas;
- notas iniciais (especificação da tarefa e critério de aceite);
- comando de aceite;
- limite de agentes;
- multiplicador de custo esperado.

O backend aplica a especificação inteira de uma vez, com layout em grupo e um id de equipe, e desmonta pelo mesmo id. Isso
vira `narrater team create spec.json` e `narrater team dismiss <id>`, mais uma tela no app.

**Modelos de partida:**

| Modelo | Quando | Custo esperado |
|---|---|---|
| Sozinho | Padrão; tarefa sequencial ou acoplada | 1× |
| Dev + revisor | Mudança que importa; revisor com contexto limpo, de outro fornecedor se houver | ~1,3–2× |
| Dev + QA (testes primeiro) | Comportamento novo com critério verificável | ~2× |
| Leque de leitura (N só leem + 1 sintetiza) | Investigar, comparar, auditar | proporcional a N |
| Devs em paralelo | Só com arquivos disjuntos, cada um no seu worktree | ~2–3× |

**Três entradas para o mesmo mecanismo:**
1. **Pelo app, sem IA:** o usuário escolhe um modelo, escreve a tarefa e o app monta tudo. Custa zero token.
2. **Pedindo a um agente** ("quero dev, QA e design fazendo X"): o agente traduz o pedido numa especificação e chama
   `team create`. Não precisa ser um agente novo; é o próprio Narrador com a skill de equipes. Um "agente gerador"
   separado custaria uma sessão inteira só para escrever um JSON.
3. **Automático, com a chave ligada:** o Narrador recebe a tarefa e passa pelo **portão**:
   - monta equipe só se houver pelo menos 2 subtarefas independentes com arquivos disjuntos, ou se a tarefa pedir um
     verificador com contexto limpo, ou se o contexto não couber num agente;
   - senão, faz sozinho;
   - a decisão e o motivo vão para uma nota, para o usuário ver por que montou ou não.

**A chave (preferência global "O Narrador pode montar equipes"):**
- **Desligada:** o Narrador nunca recruta sozinho.
- **Ligada:** o Narrador monta equipes sozinho, com limite de agentes (proposta: 4) e desmontagem ao terminar.

Se quiser, um terceiro estado depois, "propor e esperar confirmação", reaproveita o `attention` que já existe. Para começar,
liga e desliga, como você sugeriu. A preferência entra em `preferences.rs` (que já existe sem tela) e o backend recusa o
`team create` automático quando ela está desligada. Isso é aresta como permissão aplicada à equipe.

**O que vem junto e corrige as falhas do §3:**
- **comando de aceite (F3):** o backend roda o comando no fim do turno; se falhar, o "pronto" vira `attention` com a saída;
- **especificação da tarefa como nota ligada ao membro (F4);**
- **um escritor por worktree (F5);**
- **modelo por membro (F8).**

### 4.3 Como medir antes de construir

Este é o O9 ampliado e precisa da sua cota, por isso não foi rodado. O benchmark ganha três braços com a mesma tarefa e os
mesmos 31 testes:

- **Sozinho:** o Narrador Opus, sem devs.
- **Dev + revisor.**
- **Equipe de hoje (G).**

Junto, entram uma tarefa mais difícil (alvo: abaixo de ~70% no braço sozinho) e uma paralelizável.

A métrica principal passa a ser **custo por teste escondido aprovado**, mais o número de voltas ao líder.

Se o braço sozinho ganhar na tarefa atual, o portão do §4.2 está justificado com dado nosso, e o padrão do gerador vira
"sozinho".

## 5. O que incorporar (e o que não)

**Incorporar, nesta ordem:**
1. **Braço "sozinho" no benchmark (F1, F2).** Barato de fazer (o `rodar.py` já monta a cena) e decide o resto.
2. **Comando de aceite no `ask` e na tarefa (F3).** Agnóstico, sem cooperação, e ataca a maior família de falhas.
3. **Especificação de equipe + `team create/dismiss` + modelos de partida (gerador, F4, F5, F8).**
4. **A chave e o portão na skill**, com as regras de tamanho e custo (Anthropic, Agent Teams).
5. **O8 (tarefas com dependências e pega automática) e detecção de laço (F6, F7).**

**Não incorporar:**
- **Debate ou discussão entre agentes:** o retorno medido é fraco.
- **Swarm livre:** a Cognition o chama de distração, e a amplificação de erro chega a 17,2×.
- **Juiz LLM decidindo o "pronto":** AUROC ≤ 0,65.
- **Gerador que inventa topologia livre:** sem evidência a favor em código.
- **Um agente gerador permanente.**

## 6. Resultado do braço "sozinho" (2026-10-16)

Medido como descrito em §4.3. Relatório completo em `../relatorios/quando-orquestrar.pdf`, decisão D22.

| | Sozinho (H, 1 terminal, Opus) | Equipe G | Equipe F |
|---|---|---|---|
| Testes escondidos | 31/31 nas 3 | 31/31 | 31/31 |
| Duração (min) | 1,6 (1,4–1,8) | 3,9 (2,8–5,2) | 2,8 (2,3–3,7) |
| Tokens processados (milhões) | 0,36 | 2,50 | 1,73 |
| Custo (US$) | 0,55 (0,52–0,58) | 0,74 (0,63–0,88) | 0,65 (0,50–0,82) |
| Chamadas à API | 8 | 49 | 38 |

O que os números mostram:
- `[medido]` O agente sozinho vence a G em tempo, tokens, chamadas e custo, sem sobreposição (p = 0,10, o
  mínimo com n = 3). Contra a F, vence em tempo; o custo se sobrepõe (p = 0,70).
- `[medido]` Só o Narrador da G, coordenando, custou US$ 0,32–0,41: de 60% a 75% do Opus sozinho fazendo
  tudo. O custo cai menos que os tokens porque, na equipe, o grosso dos tokens é do Haiku, que é barato.
- `[inferido]` Confirma F1: nesta tarefa, a equipe só acrescenta custo. Falta a tarefa em que ela deveria
  ganhar (sem teto, ou paralelizável), que continua na O9.

## 7. Teste onde a equipe deveria ganhar: tarefa `armazem` (2026-10-16)

Relatório: `../relatorios/equipe-ou-sozinho.pdf` (as previsões P1–P5 foram registradas no commit `52688a3`,
antes das rodadas). A tarefa tem quatro módulos independentes, com 46 testes escondidos, 12 deles de regras
sutis. Foram 3 rodadas por montagem, na versão H.

| | Sozinho (Opus) | Equipe paralela (Opus + Sonnet + Haiku) | Dev + revisor (Opus + Sonnet) |
|---|---|---|---|
| Testes escondidos | 46/46 nas 3 | 46/46 | 46/46 |
| Duração (min) | **1,8** (1,7–1,9) | 2,8 (2,6–3,0) | 2,3 (2,2–2,4) |
| Tokens (milhões) | **0,36** | 1,51 | 0,58 |
| Custo (US$) | 0,56 | 0,60 (0,47–0,78) | 0,76 |

O que os números mostram:
- `[medido]` O agente sozinho foi o mais rápido nas duas comparações, sem sobreposição (p = 0,10). A equipe custou o
  mesmo (p = 1,0), porque os tokens a mais foram do Haiku. O revisor custou 36% a mais e aprovou as três entregas.
- `[medido]` A equipe perdeu no tempo:
  - o Haiku levou de 109 a 127 s nos seus dois módulos, o dobro do Sonnet;
  - os dois devs trabalharam ao mesmo tempo só de 39 a 59 s;
  - o Narrador ficou sozinho de 39 a 49 s.
- `[medido]` O protocolo não falhou: 0 de 15 asks prematuros e 0 respostas cortadas.
- `[inferido]` A previsão P1 (a equipe ganha em tarefa paralela) foi refutada pela forma da equipe e pelo tamanho
  das partes, não pelo protocolo. Isso aponta para o portão do gerador:
  - cada parte precisa valer bem mais que ~1 min de coordenação;
  - equipes paralelas com modelos de mesma velocidade;
  - revisor só quando quem escreve erra. O próximo teste é "Sonnet + revisor" contra o Opus sozinho.

## 8. O que foi feito a partir deste estudo (2026-10-16)

As falhas do §3 e o gerador do §4, implementados sem mudar as equipes montadas à mão nem o benchmark:

| Falha (§3) | O que mudou | Onde |
|---|---|---|
| F1, F2: equipe sem portão; sem linha de base | Braço "sozinho" medido (§6, §7); D22; portão nas skills e no `--reason` do gerador | `bench/orquestracao`, `SKILL*.md` |
| F3: "pronto" = fim do turno | `ask --check "CMD"`: o backend roda o aceite; falha vira `failed` (D24) | `ipc.rs` (`run_check`) |
| F4: tarefa como conversa | Skill ensina delegação como especificação; o gerador cria a nota de tarefa com o aceite | `SKILL.md`, `teamCommands.ts` |
| F5: escritores no mesmo diretório | Regra "dois membros nunca escrevem os mesmos arquivos" nos papéis; `--isolate` dá um worktree (andar) a cada escritor | `teamTemplates.ts` |
| F7: pedido repetido | `repeat_of` marca o pedido e avisa na resposta e no Activity | `asks.rs`, `ipc.rs` |
| F8: sem escolha de modelo | `recruit --model`, campo Model no seletor, modelo salvo na cena e mostrado no terminal | `agents.ts`, `managerCommands.ts` |
| Gerador (§4) | `narrater team create/templates`, chave Automatic teams (D23) | `teamCommands.ts`, `preferences.rs` |

Ficou de fora: F6 (o líder entre as fases, O8) e desmontar equipes.

`[observado]` Verificação de ponta a ponta no app real, com `bench/e2e/gerador-equipes.sh` (Xvfb, build de release,
terminais shell e um `claude` falso, sem gastar cota). O que aconteceu:
- sem `--reason`, a criação foi recusada;
- `dev-reviewer` criou Dev e Reviewer ligados como you ↔ Dev ↔ Reviewer, com as notas de tarefa e de porquê
  (`img/gerador-equipe.png`);
- o `claude` recebeu `--model sonnet`, e o modelo e a equipe ficaram salvos na cena;
- `solo` só registrou a decisão, e um membro foi impedido de gerar equipe;
- `--check false` voltou `failed`, e `--check "python3 -m unittest -q"` voltou "passed";
- o pedido repetido veio com o aviso.

Falta medir no benchmark a montagem "gerada": o Narrador com Automatic teams ligado decide sozinho, e a pergunta é
se ele escolhe "solo" quando deve.
