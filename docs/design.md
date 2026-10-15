# Perfil de design do NarraTer — "Palco"

Regras visuais do app. Os valores vivem em `src/index.css` (tokens) e `tailwind.config.js`
(classes); este arquivo explica **papéis e regras**. A pesquisa que embasa cada escolha está em
[`pesquisa/visual.md`](pesquisa/visual.md) e as decisões em [`decisoes.md`](decisoes.md) (D13–D16).

## A ideia

O NarraTer é um palco. O canvas é o **palco escuro**; cada terminal é um **personagem**; as arestas
são as **falas** entre eles; o usuário é o narrador. Daí saem as regras:

1. **O palco recua, os personagens aparecem.** A moldura (barra lateral, toolbar, minimapa) é
   neutra e quieta. Quem chama atenção é o conteúdo: os terminais e o que eles dizem.
2. **Cor é estado, não decoração.** O violeta do narrador marca só seleção, foco e um agente
   falando. Verde, laranja e vermelho dizem "pronto", "precisa de você" e "caiu". Nada é colorido
   só por enfeite.
3. **Profundidade por luz.** Como no modo escuro da Apple: a base é a mais escura; o que está à
   frente é mais claro (base → elevado → flutuante). Sombra só para o que flutua.
4. **Vidro só na camada de controle.** Barra lateral, toolbar, paleta e menus são translúcidos e
   desfocam o palco atrás (o equivalente web do material *regular* do Liquid Glass). Tiles e
   terminais são conteúdo: superfície sólida.
5. **Movimento quer dizer algo aconteceu.** Uma fala atravessa a aresta, um agente trabalha, um
   pedido de permissão aparece. Em repouso, nada se mexe.
6. **Legível primeiro.** Texto da moldura ≥ 11 px; contraste ≥ 4,5:1 para qualquer texto que
   carregue informação (alvo 7:1 em texto pequeno).

## Cor

### Superfícies (do fundo para a frente)

| Token | Valor | Uso |
|---|---|---|
| `--bg` | `#0b0b0c` | O palco (canvas) |
| `--term` | `#121214` | Corpo do terminal, rebaixado dentro do tile |
| `--surface` | `#1c1c1e` | Tiles, cards (elevado) |
| `--surface-2` | `#242426` | Cabeçalhos de seção, campos |
| `--raised` | `#2c2c2e` | Popovers, menus, tooltips (flutuante) |
| `--glass` | `rgb(28 28 30 / 0.72)` + blur 24 px | Barra lateral, toolbar, paleta |
| `--hover` / `--press` | branco 6% / 10% | Fundo de linha e botão em hover / pressionado |
| `--line` / `--line-strong` | branco 8% / 14% | Bordas e divisórias / borda em hover |

### Texto

| Token | Valor | Contraste (palco · tile · popover) | Uso |
|---|---|---|---|
| `--text` | `#f5f5f7` | 18,1 · 15,6 · 12,8 | Títulos, nomes, conteúdo |
| `--text-2` | `#ebebf5` a 72% | ≥ 6 em todas | Texto secundário |
| `--text-3` | `#ebebf5` a 55% | 5,4 · 5,2 · 4,7 | Meta, rótulos pequenos, ícones em repouso |
| `--text-4` | `#ebebf5` a 30% | < 3 | **Só** decorativo ou desabilitado; nunca informação |

### Acento e estados

| Token | Valor | Significado |
|---|---|---|
| `--accent` | `#8f7cff` | O narrador: seleção, foco, agente **trabalhando**, fala atravessando |
| `--accent-text` | `#b0a4ff` | Acento como texto (≥ 6,4:1) |
| `--ok` | `#30d158` | Pronto (ocioso, aceita prompt) |
| `--warn` | `#ff9f0a` | Precisa de você (permissão, menu) |
| `--queue` | `#ffd60a` | Mensagem esperando na fila |
| `--danger` / `--danger-text` | `#ff453a` / `#ff6961` | Processo caiu, ação destrutiva |
| `--info` | `#64d2ff` | Links e ações informativas |

Tons translúcidos saem de `color-mix(in srgb, <token> X%, transparent)` ou das classes do
Tailwind com opacidade (`bg-accent/15`), nunca de um hex novo.

### Identidade dos agentes

Cada CLI tem uma cor de **identidade** (`src/lib/agents.ts`), usada só no ícone do agente e no
fio fino do topo do tile, nunca em bordas, fundos grandes ou arestas. Identidade diz *quem é*;
estado diz *como está*; os dois nunca dividem a mesma forma (ícone × ponto).

## Tipografia

- **Inter Variable** (já embarcada; métricas próximas da SF Pro, que não existe no Linux) para a
  interface; **JetBrains Mono** para terminais, caminhos e código.
- Escala: 11 px (meta, rótulos em maiúsculas com `tracking 0.06em`), 12 px (secundário, cabeçalho
  dos tiles), 13 px (corpo, padrão do macOS), 15 px (títulos de painel).
- Números que mudam (filas, porcentagens, contadores) em `tabular-nums`.
- Corpo com `letter-spacing: -0.006em`; nada abaixo de 11 px na moldura.

## Forma

- Raio: 6 px (controles pequenos, chips), 8 px (botões, campos), 12 px (tiles, painéis, menus),
  cápsula (toolbar, badges de estado).
- Elevação: (1) borda `--line`; (2) tile = `--surface` + borda + destaque interno de 1 px no topo
  (`inset 0 1px 0 rgb(255 255 255 / 0.04)`) + sombra suave; (3) flutuante = `--raised` +
  `0 12px 32px rgb(0 0 0 / 0.45)`. Não há quarto nível.
- Seleção: anel de 1,5 px em `--accent` com halo de 4 px a 20%.

## O grafo

| Elemento | Repouso | Ativo |
|---|---|---|
| **Aresta agente ↔ agente** | Curva fina (1,5 px), branco a 18%, sólida, sem rótulo | Uma fala atravessa: um pulso de luz em `--accent` percorre a curva; a linha acende até 60% |
| Aresta com fila | — | Chip `--queue` no meio da curva com o número de mensagens |
| Aresta com agente trabalhando | — | Linha a 35% e fluxo lento em `--accent` |
| **Aresta agente → nota** | Tracejada, branco a 14% | Acende em hover |
| Hover na aresta | Linha a 45% + rótulo "A ↔ B" | Clique abre o histórico |
| **Alças de conexão** | Invisíveis | Aparecem ao passar o mouse no tile; crescem e ganham halo `--accent` |
| Arrastando conexão | Linha tracejada em `--accent` | — |

Regra: em repouso o grafo é só desenho; quem diz o que está acontecendo é o movimento.

**Cordas (D18).** Toda conexão é uma corda: sai do ponto da borda voltado para o outro nó
(qualquer lado), pende com folga leve, contorna os outros nós com 16 px de respiro em vez de
passar por baixo deles, balança quando um nó é arrastado e para quando assenta. As alças ficam
nos quatro lados, e uma conexão pode ser solta em qualquer borda.

## Tiles

- Superfície `--surface`, raio 12 px, borda `--line`, fio de identidade de 1 px no topo.
- Cabeçalho integrado (mesma superfície, 34 px): ponto de estado, ícone do agente na cor de
  identidade, **nome** em 12 px `--text`, papel como chip neutro. Ações (prompt, editor, fechar)
  ficam em `--text-3` e aparecem ao passar o mouse; os indicadores de estado (fila, chat não
  lido, Maestro) ficam sempre visíveis.
- Corpo do terminal em `--term`, com 8 px de respiro e scrollbar discreta.
- Estados do ponto: trabalhando = `--accent` pulsando; pronto = `--ok`; precisa de você = `--warn`
  com anel; iniciando = `--text-3`; caiu = `--danger`.

## Moldura

- Barra lateral e toolbar em `--glass` (blur 24 px, saturação 160%), borda `--line`.
- Seções da barra lateral com rótulo de 11 px em maiúsculas; item ativo com fundo `--hover` e
  marcador `--accent` de 2 px. Atalhos ficam na paleta (`Ctrl+Space`), não na barra lateral.
- Canvas com pontos a 6% de branco, espaçados 24 px.
- Minimapa em vidro, nós em `--text-4`, sem borda colorida.

## Movimento

- Hover e cor: 120 ms `ease-out`. Painéis e menus: 180 ms `cubic-bezier(0.2, 0.8, 0.2, 1)`.
- Pulso de fala: 900 ms ao longo da aresta. Ponto "trabalhando": 1,6 s de respiração.
- `prefers-reduced-motion`: sem pulsos nem fluxos; só mudança de cor.

## Painel Activity

Painel à direita (Ctrl+J), em `--glass` como a moldura. É o histórico verificável do que os agentes
fizeram (D21):
- no topo, cada agente com a mesma luz de estado do tile, e clicar filtra por ele;
- abaixo, os filtros por tipo (All, States, Asks, Messages) e o `#id` de um ask, que mostra só aquele
  pedido;
- na lista, o mais novo em cima, com uma linha por evento: ponto de cor, horário em `tabular-nums`,
  quem, o quê.

Cores dos pontos (cor é estado, nunca enfeite):
- pedido na fila `--queue`; pedido entregue (a fala atravessando) `--accent`; trabalhando `--accent`;
- pronto `--ok`; precisa de você `--warn`; falha ou saída `--danger`;
- o que não é estado (nota, mensagem, início) fica neutro em `--text-3`;
- filtro ativo é seleção, então usa `bg-accent/20`; o ícone do título fica neutro.

Tipografia: título 15 px; linhas 12 px; meta (horário, duração, contadores) 11 px em `tabular-nums`,
que é o piso da moldura. Inter no texto, e JetBrains Mono só no `#id` do ask.

Pedido e resposta aparecem recolhidos em 2 linhas e desdobram no clique. Trocas de estado desfeitas em
menos de 2 s ficam escondidas, com um contador para exibi-las. O painel entra em 180 ms com
`--ease`, sem animação com `prefers-reduced-motion`.

## Uso dos agentes (etapa V4)

Implementado para o Claude (D17). Na barra lateral, a seção **Usage** mostra uma barra fina (4 px)
por janela do plano (5 h, semanal), com porcentagem, a hora do reset e a idade do dado. No
cabeçalho do tile, um anel de 14 px mostra o contexto usado pela sessão, e o tooltip traz modelo,
custo e as janelas do plano. A cor fica em `--text-3` até 60%, `--queue` até 80% e `--danger`
acima. A porcentagem usa `tabular-nums`.
