# Visual: Apple, Orca e Maestri — e o uso dos agentes

> Estudo feito em 2026-10-14. Fontes: Apple Human Interface Guidelines (texto oficial obtido do
> JSON das páginas `dark-mode`, `materials`, `color` e `typography` em developer.apple.com);
> código do **Orca** 1.4.214 (`docs/STYLEGUIDE.md`, `src/renderer/src/assets/main.css`,
> `src/main/rate-limits/`, `src/renderer/src/components/status-bar/`), lido sem rodar; **Maestri**
> só por fontes secundárias (Product Hunt, Setapp, página de lançamento) e pelo estudo de
> 2026-10-01, que olhou a versão Linux 0.19.4 por dentro mas não a interface. O NarraTer foi
> capturado rodando sob Xvfb (`img/narrater-antes.png`).

## TL;DR

- Os dois apps que você admira seguem o mesmo princípio da Apple: **a interface recua e o conteúdo
  aparece**. O Orca chama isso de "monocromático e quieto": cinzas neutros na moldura e cor só
  para estado. O Maestri é nativo (Swift/SwiftUI, macOS 26) e herda os materiais do sistema.
- O NarraTer hoje faz o contrário: o violeta saturado está no logo, na seleção, nas arestas, nos
  rótulos, nos atalhos e no minimapa, e os níveis de superfície quase não se distinguem.
- A porcentagem de uso existe para várias CLIs, cada uma por um caminho. Para o Claude há duas
  rotas: o endpoint OAuth de uso e o `rate_limits` que o Claude Code entrega à statusline.

## 1. Apple (HIG)

`[citado]` Texto oficial das HIG, 2026-10-14.

- **Modo escuro não é inversão.** "The color palette in Dark Mode includes dimmer background
  colors and brighter foreground colors… these colors aren’t necessarily inversions of their
  light counterparts."
- **Profundidade por brilho, não por sombra.** "In Dark Mode, the system uses two sets of
  background colors — called base and elevated… The base colors are dimmer, making background
  interfaces appear to recede, and the elevated colors are brighter, making foreground interfaces
  appear to advance." Popovers e folhas sobem de base para elevado.
- **Contraste**: "no lower than 4.5:1… For custom foreground and background colors, strive for a
  contrast ratio of 7:1, especially in small text."
- **Cores semânticas**, não valores fixos: label/secondaryLabel e fundos em níveis
  (primário, secundário, terciário).
- **Materiais**: Liquid Glass é "a distinct functional layer for controls and navigation elements
  — like tab bars and sidebars — that floats above the content layer". "Don’t use Liquid Glass in
  the content layer." "Use Liquid Glass effects sparingly." A variante *regular* desfoca e ajusta
  a luminosidade do fundo para manter a legibilidade; use-a em barras laterais e popovers.
- **Tipografia (macOS)**: corpo padrão de 13 pt, mínimo de 10 pt; SF Pro como fonte do sistema,
  SF Mono para monoespaçado.

## 2. Orca

`[citado]` `docs/STYLEGUIDE.md` e o bloco `.dark` de `main.css`.

- Identidade "monochrome and quiet": "neutral grays carry the chrome, color is reserved for state
  (selection ring, destructive, git decorations)". "Reach for muted/accent/border before reaching
  for color."
- Tokens em pares superfície/texto (`background`, `card`, `popover`, `muted`, `accent`,
  `border`, `ring`, `sidebar`…), definidos uma vez em CSS e usados pelo Tailwind.
- Modo escuro: canvas `#0a0a0a`, card/popover `#171717`, secondary/muted `#262626`, accent (hover)
  `#404040`, texto `#fafafa`, texto apagado `#a1a1a1`, **bordas `rgb(255 255 255 / 0.07)`**,
  input `/ 0.15`, primária quase branca (`#e5e5e5`).
- Tipografia: Geist variável; corpo 14 px, linhas densas 13 px, meta 11 px em maiúsculas com
  `letter-spacing: 0.05em`, `0.01em` no corpo.
- Raio base de 10 px (botões `rounded-md`, cards `rounded-xl`, badges `rounded-full`).
- Sombra em três níveis só: borda fina, leve elevação, e `0 10px 24px rgba(0,0,0,.18)` para o
  que flutua. "Don't add a fourth level."
- Tons por `color-mix` contra os tokens, nunca hex novo.
- **Uso**: barra de status com um segmento por provedor; cada janela (5 h, 7 dias) é uma barra de
  4 px com a porcentagem em `tabular-nums`; cinza abaixo de 60%, amarelo de 60% a 80%, vermelho
  acima (`provider-extra-usage-section.tsx`); o usuário escolhe "usado" ou "restante".

## 3. Maestri

`[citado, fontes secundárias]` Página de lançamento, Product Hunt e Setapp.

- Nativo em Swift/SwiftUI com motor de canvas próprio, sem Electron nem web views; exige macOS
  26.2, a geração do Liquid Glass. Por isso a moldura (barras, popovers, barra lateral) vem dos
  materiais do sistema.
- Não há inspeção da interface nem do código visual: as afirmações sobre o visual do Maestri são
  `[inferido]` a partir da plataforma.

## 4. Diagnóstico do NarraTer

`[observado]` Captura de 2026-10-14 (`img/narrater-antes.png`) e `src/index.css`,
`tailwind.config.js`.

| Ponto | Hoje | Referência |
|---|---|---|
| Cor | Violeta `#8b5cf6` no logo, seleção, arestas pontilhadas, rótulos "agent pipe", atalhos, papéis e borda do minimapa | Cor só para estado (Orca); acento usado com parcimônia (Apple) |
| Níveis de superfície | `#0d0d0d` / `#141414` / `#1a1a1a` / `#222`: quase iguais, sem base × elevado claros | Base escuro, elevado mais claro (Apple); `#0a0a0a` → `#171717` → `#262626` (Orca) |
| Bordas | Cinzas sólidos `#2a2a2a` | Branco a 7% (Orca), que se adapta a qualquer fundo |
| Texto pequeno | Cabeçalho dos tiles com ~9–10 px, cinza sobre cinza | Mínimo de 10 pt e contraste ≥ 4,5:1 (Apple) |
| Tiles | Retângulos com cabeçalho em bloco escuro e uma faixa preta de scrollbar à direita | Raio de ~10 px, cabeçalho integrado, scrollbar discreta |
| Arestas | Pontilhadas, violeta, com rótulo em cada uma | Linha discreta; cor e animação só quando há conversa |
| Barra lateral | Lista de atalhos ocupa metade da altura | Atalhos na paleta (`Ctrl+Space`), barra lateral para navegar |
| Fonte | Inter | SF Pro (Apple) / Geist (Orca); no Linux não há SF Pro |

## 5. Uso (porcentagem) por CLI

`[citado]` `src/main/rate-limits/` do Orca. Este container não tem Codex nem Gemini.

> **Correção (2026-10-15).** A versão anterior desta seção dizia que este container usa o Claude
> Code com chave de API, sem janelas de assinatura. Estava errado. `[observado]` O payload da
> statusline aqui traz `rate_limits` reais (5 h 0%, 7 dias 7%), ou seja, é uma conta de plano. A
> rota (b) foi a escolhida e está implementada (D17).

| CLI | De onde vem | Observação |
|---|---|---|
| Claude Code | (a) `GET https://api.anthropic.com/api/oauth/usage` com o token de `~/.claude/.credentials.json` → `five_hour`, `seven_day`, limites por modelo, com `resets_at` | O Orca registra que o endpoint responde 429 sob polling |
| Claude Code | (b) `rate_limits` que o Claude Code ≥ 2.1.80 passa à **statusline** a cada turno (`used_percentage`, `resets_at`) | Sem custo de requisição, mas exige configurar a statusline do usuário |
| Codex | `codex app-server` + RPC `account/rateLimits/read`; reserva `chatgpt.com/backend-api/wham/usage` | Usa a própria CLI instalada |
| Gemini CLI | `cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota` com o OAuth de `~/.gemini` | Endpoint interno |
| Cursor, Grok, Kimi, OpenCode… | Fetchers próprios no mesmo diretório | — |

Modelo de dados comum do Orca (`shared/rate-limit-types.ts`): por janela, `usedPercent`,
`windowMinutes` (300 ou 10080), `resetsAt` e uma descrição do reset.

## 6. Resultado do primeiro passo (2026-10-14)

`[observado]` Capturas sob Xvfb, mesma cena de três shells:

- antes: `img/narrater-antes.png`;
- depois, em repouso: `img/narrater-depois.png`;
- depois, com dois agentes trabalhando e uma mensagem na fila: `img/narrater-depois-ativo.png`.

O perfil está em [`../design.md`](../design.md); as decisões em D13–D16.

## 7. Uso e cordas (2026-10-15)

`[observado]` No app real: a barra lateral com o uso do Claude e o anel de contexto no tile
(`img/narrater-v4-cordas.png`), e as cordas contornando a nota e balançando ao arrastar um
terminal (`img/cordas.gif`). Decisões em D17 e D18.
