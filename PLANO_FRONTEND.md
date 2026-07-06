# Plano de Otimização e Design do Frontend — NarraTer

Diagnóstico feito sobre o código atual (React 18 + Vite + Tailwind + React Flow/xyflow + xterm.js + zustand + framer-motion, rodando em Tauri 2). O plano está dividido em 4 fases, ordenadas por impacto: primeiro correções de fundação, depois performance, depois design/beleza, depois polimento.

---

## Fase 0 — Fundação (correções que destravam o resto)

### 0.1 Pinar `react` e `react-dom` no package.json ⚠️
`react` e `react-dom` **não estão declarados** em `dependencies` — foram instalados como peer deps e o lockfile resolveu **React 19.2.7**, enquanto `@types/react` é v18. Isso é uma bomba-relógio: qualquer `npm install` futuro pode trocar a versão do React silenciosamente, e framer-motion 11 / xyflow 12 têm comportamentos diferentes entre 18 e 19.
- Adicionar `"react": "^18.3"` e `"react-dom": "^18.3"` explícitos (ou migrar de vez para 19 e atualizar os types — decisão única, mas explícita).

### 0.2 Embutir as fontes (JetBrains Mono não está sendo carregada)
`tailwind.config.js` e o tema do xterm referenciam `"JetBrains Mono", "Cascadia Code", "Fira Code"`, mas **nenhuma dessas fontes é importada** — o app cai no monospace do sistema. Como é app Tauri (deve funcionar offline), self-hostar via `@fontsource/jetbrains-mono` (só os pesos 400/500/700) e importar no `index.css`. Ganho estético imediato e de graça.
- Bônus: adicionar **Inter** (`@fontsource-variable/inter`) para a UI (sidebar, botões, modais) — hoje a UI usa a fonte do sistema, que varia entre Windows/Linux/macOS.

### 0.3 Unificar a fonte de verdade do estado do canvas
Hoje existem **duas cópias** de nodes/edges: o zustand store (`stores/canvas.ts`) e o estado local do React Flow (`useNodesState`/`useEdgesState` no `Canvas`), sincronizados por 4 `useEffect`s + refs espelho. Consequências:
- Posições/tamanhos só chegam ao store no Ctrl+S (fechar sem salvar perde layout).
- Cada mudança no store **substitui o array local inteiro** → re-render de todos os nós.
- Bugs sutis de divergência (ex.: nota editada + edge criada = merge manual).

Refatorar para o padrão recomendado do xyflow: **zustand como única fonte de verdade**, com `onNodesChange`/`onEdgesChange` aplicando `applyNodeChanges`/`applyEdgeChanges` direto no store, e o `<ReactFlow>` controlado lendo do store com selectors. Isso elimina os efeitos de sincronização, os refs espelho e habilita o autosave da Fase 3.

---

## Fase 1 — Performance

### 1.1 Renderer WebGL no xterm (maior ganho individual)
Com vários terminais no canvas, o renderer DOM padrão do xterm é o gargalo dominante. Adicionar `@xterm/addon-webgl` com fallback para o renderer padrão em caso de perda de contexto. Ganho típico: 5–10x em throughput de escrita, essencial quando agentes despejam output.

### 1.2 Culling de nós fora da viewport
Passar `onlyRenderVisibleElements` no `<ReactFlow>` para não renderizar tiles fora da tela. Cuidado: os terminais precisam manter o processo PTY vivo ao desmontar — mover o ciclo de vida do PTY do `useEffect` do `TerminalTile` para um manager fora do React (o `usePty`/store), de modo que desmontar o tile não mate a sessão, apenas o xterm. O buffer do xterm (scrollback) é restaurado ao remontar via replay do backend ou mantendo a instância `XTerm` viva num cache por id.

### 1.3 LOD por zoom (performance + estética)
Abaixo de um limiar de zoom (~0.4), o conteúdo do terminal é ilegível mas continua custando render. Usar `useStore(s => s.transform[2])` para trocar o corpo do tile por um **card simplificado** (ícone do agente, label, dot de status, badges) quando o zoom está baixo. Menos DOM, mais bonito no overview de "mapa de agentes".

### 1.4 Batching do pipe agent→nota
O listener global de `pty_output` no `Canvas` faz `setNodes` mapeando **todos os nós a cada chunk** de output. Com um agente verboso isso é um `setState` global por chunk (dezenas/segundo):
- Bufferizar chunks por nota e aplicar em batch a cada ~100ms (ou `requestAnimationFrame`).
- Atualizar **só o nó alvo** (via `updateNodeData` do store, não map do array inteiro).
- **Cap no tamanho do conteúdo** da nota (ex.: 200 KB, mantendo o fim) — hoje cresce sem limite e o `<textarea>` re-renderiza a string inteira a cada chunk.
- Desligar o `isAgentLive` após ~2s sem output (hoje fica "ao vivo" para sempre).

### 1.5 Eventos PTY escopados por id
Cada `TerminalTile` registra `listen("pty_output")` global e filtra por id — com N terminais, cada chunk acorda N listeners. Emitir do backend eventos por canal (`pty_output:{id}`) ou manter **um único listener** no app que despacha para um registry de callbacks por id (mais simples, não toca o Rust).

### 1.6 Memoização dos nós customizados
Envolver `TerminalTile`, `NoteTile`, `AgentNoteEdge` e `AgentPipeEdge` em `React.memo`. O React Flow re-renderiza nós custom a cada mudança de qualquer nó; com memo, arrastar um tile não re-renderiza os outros.

### 1.7 SketchLayer: pré-computar paths
`strokeToPath()` (perfect-freehand + montagem da string SVG) roda para **todos os strokes a cada render** — e cada `pointermove` causa um render. Computar o `d` uma vez no `commitStroke` e armazenar no store; só o stroke corrente é recalculado ao vivo. Considerar throttle de `addPoint` por rAF.

### 1.8 Higiene de bundle e código
- Lazy-load de `AgentPicker` e `RoleManager` (`React.lazy`) — são modais, não precisam no bundle inicial.
- `rollup-plugin-visualizer` uma vez para auditar (xterm + xyflow são os pesados legítimos; verificar se framer-motion justifica o peso — hoje só faz `whileTap` e fades que CSS resolve; candidata a remoção ou troca por `motion/mini`).
- IDs com `crypto.randomUUID()` em vez de `Date.now()+contador` módulo-level.
- O handler global de teclado no `Canvas` é re-registrado a cada mudança de nodes/edges (dep `handleSave`); estabilizar com ref.

---

## Fase 2 — Design / Beleza

### 2.1 Sistema de design: tokens + primitivas
O maior débito visual é **inconsistência por hardcode**: `#1a1a1a`, `#8b5cf6`, `#2a2a2a` etc. aparecem dezenas de vezes em `style={{}}` inline e classes arbitrárias `bg-[#...]`, apesar de o Tailwind já ter os tokens `canvas.*`/`accent.*`. Plano:
- Consolidar a paleta em CSS variables (`index.css`) referenciadas pelo Tailwind config (padrão `hsl(var(--...))`), incluindo os tokens de agente e de status (running/idle/exited/spawning).
- Criar primitivas em `src/components/ui/`: `Button` (variantes primary/ghost/danger/toggle), `Badge` (agente, papel, fila, pipe — hoje há 4 implementações quase iguais no header do tile), `Menu` (dropdown do editor + context menu da sidebar compartilham estilo), `Modal`, `Tooltip`, `Kbd`.
- Trocar `title=""` por um `Tooltip` real (posicionado, com atalho renderizado em `<Kbd>`), — os atalhos Ctrl+T/S/D hoje são invisíveis sem hover longo.

### 2.2 Toolbar do canvas redesenhada
Hoje são botões soltos no canto com estilos divergentes (uns inline style, outros classes). Redesenhar como **uma barra flutuante única** (pill com `backdrop-blur`, borda sutil, sombra), agrupando: Terminal (primário) · Nota · Desenho (com sub-toolbar de cor/tamanho embutida que expande) · separador · Salvar/estado. Botões com estados hover/active/focus-visible consistentes.

### 2.3 Estado vazio e onboarding
Canvas vazio hoje = tela preta com pontinhos. Adicionar empty state central: logo/wordmark, "Crie seu primeiro terminal", botões grandes (Terminal / Nota) e os 3 atalhos principais. Some ao criar o primeiro nó.

### 2.4 Nascimento e posicionamento de nós
- Novos nós hoje nascem em posições fixas que se empilham (`80 + counter%5*60`). Criar no **centro da viewport atual** (via `screenToFlowPosition`) com pequeno offset em cascata, e animar entrada (scale 0.96→1 + fade, 150ms).
- Context menu de botão direito no canvas: "Novo terminal aqui / Nova nota aqui" na posição do cursor.

### 2.5 Polimento dos tiles
- **TerminalTile**: gradiente sutil de 1px no topo na cor do agente (identidade rápida ao escanear o canvas); dot de status com `animate-pulse` quando `running`; overlay elegante quando `exited` (fundo escurecido + "Processo encerrado · Reiniciar"); handles de conexão maiores no hover (8px é difícil de acertar) com anel na cor do agente.
- **NoteTile**: título editável (hoje é "Nota" fixo, e `label` já existe no modelo); auto-scroll para o fim quando agente está escrevendo; botão copiar conteúdo; opcional: toggle de preview markdown.
- **Edges**: hover state (espessura +1, cor mais viva); no `agent-pipe`, animação de fluxo direcional (dash animado) para comunicar a direção da rota; label da edge com contagem de mensagens trafegadas.

### 2.6 Sidebar
- Wordmark "NarraTer" com um mínimo de identidade (peso, cor accent no "Ter", ícone).
- Item ativo da história com barra lateral accent em vez de só background.
- Modo colapsado hoje mostra só a seta — mostrar ícones das seções (histórias/papéis) com tooltip.
- Excluir história: **confirmação** (hoje deleta direto do context menu, destrutivo e irreversível).

### 2.7 Feedback e erros
Erros hoje vão para `console.error` (spawn de PTY, save, editor). Adicionar **toasts** discretos (canto inferior direito, mesmo dark theme): erro ao salvar, agente caiu (código de saída ≠ 0), editor não encontrado. Toast de "História salva" substitui o estado verde do botão.

---

## Fase 3 — UX estrutural

### 3.1 Autosave
Com o store unificado (0.3), debounce de ~1.5s em qualquer mutação de nodes/edges/notas → `saveHistoria`. Indicador discreto "Salvo · agora" na toolbar (o Ctrl+S vira força-save). Salvar também no evento de fechamento da janela Tauri (`onCloseRequested`).

### 3.2 Undo/redo do canvas
Ctrl+Z hoje só desfaz desenho. Com o store unificado, adicionar histórico (zundo ou snapshot manual) para criar/mover/redimensionar/deletar nós e edges. Delete acidental de um terminal (que mata o processo!) precisa de undo ou confirmação.

### 3.3 Busca/paleta de comandos (Ctrl+K)
Com muitos tiles, achar um agente é pan-and-squint. Paleta estilo command palette: filtrar por label/papel/tipo, Enter dá `fitView` no nó e o seleciona. Também expõe ações (nova nota, salvar, trocar história).

### 3.4 Acessibilidade
`focus-visible` em todos os controles (hoje só inputs têm), `aria-label` nos botões só-ícone (fechar, editor, toggle sidebar), navegação por teclado nos menus dropdown, `prefers-reduced-motion` respeitado nas animações.

---

## Ordem de execução sugerida

| # | Item | Esforço | Impacto |
|---|------|---------|---------|
| 1 | 0.1 Pinar React + 0.2 Fontes | XS | Alto (estabilidade + visual imediato) |
| 2 | 1.1 WebGL xterm + 1.6 memo | S | Alto (perf) |
| 3 | 0.3 Store unificado | M | Alto (destrava 3.1/3.2, corrige perda de layout) |
| 4 | 1.4 Batching pipe→nota + 1.5 eventos por id | S | Alto (perf com agentes verbosos) |
| 5 | 2.1 Tokens + primitivas UI | M | Alto (consistência visual) |
| 6 | 2.2 Toolbar + 2.4 spawn de nós + 2.3 empty state | S | Alto (primeira impressão) |
| 7 | 3.1 Autosave + 2.7 toasts | S | Alto (confiança) |
| 8 | 1.2 Culling + 1.3 LOD | M | Médio/Alto (escala) |
| 9 | 2.5 Tiles + 2.6 sidebar | M | Médio (polish) |
| 10 | 3.2 Undo + 3.3 Ctrl+K + 3.4 a11y + 1.7/1.8 | M | Médio |

Fases 1–7 da tabela formam um bom primeiro ciclo: o app fica visivelmente mais bonito, mais rápido com múltiplos agentes e para de perder trabalho não salvo.
