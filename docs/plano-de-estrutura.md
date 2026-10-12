# Plano de estrutura do projeto

*2026-10-12.* Como organizar o NarraTer para que o trabalho siga o objetivo
da pesquisa (**comunicação entre agentes de IA em terminais no PC**), que
cada sessão de desenvolvimento comece com as mesmas regras, e que os testes
virem material apresentável.

## Diagnóstico

Hoje o projeto não tem `CLAUDE.md`, pasta `.claude/` nem scripts de teste.
As regras (escopo, fluxo de git, como testar, pegadinhas da máquina) vivem
na memória privada do assistente e no histórico das conversas. Isso tem
três problemas:
- **outra pessoa ou outra ferramenta não enxerga as regras**;
- **os testes com agentes reais são montados à mão a cada sessão** (cena,
  terminal operador, scripts), então são lentos de repetir e difíceis de
  comparar;
- **o tracker mistura** o que veio do Maestri com o que foi desenhado do
  zero, e a pesquisa precisa dessa separação.

## Fase 1 · Regras do projeto (`CLAUDE.md`)

Um `CLAUDE.md` na raiz, lido no início de toda sessão do Claude Code, com:
- **Objetivo e escopo.** Comunicação entre agentes em terminais no PC. Fora
  do escopo: dispositivos móveis e acesso pelo celular ("Maestri Remote").
- **Regra de estudo.** O NarraTer recria o comportamento do Maestri para
  estudo; nunca copia código dele nem mexe na licença. Fontes válidas: as
  skills `maestri-*` instaladas, o site e a doc pública, e o comportamento
  observável do app.
- **Fluxo de git.** Branch `feat/maestri-parity`; uma feature por commit;
  mensagens em português ("feat: … no modelo do Maestri"); a `main` só
  avança por fast-forward, quando o usuário pede.
- **Definição de pronto.** `cargo test` e `tsc` passando; se a mudança
  afeta o que os agentes fazem ou como se comunicam, um cenário com agentes
  reais (Fase 2) e um relatório em `docs/testes/`; tracker atualizado.
- **Mapa do código.** Onde ficam o protocolo (`ipc.rs`, `pty.rs`,
  `screen.rs`), a CLI (`cli_core.rs`), o canvas (`src/stores/canvas.ts`,
  `canvasBridge.ts`) e as skills do produto.
- **Dois tipos de skill.** As de `src-tauri/assets/SKILL*.md` são **do
  produto**: o app as instala para os agentes que rodam *dentro* do
  NarraTer. As de `.claude/skills/` são **de desenvolvimento**: ajudam quem
  constrói o NarraTer.
- **Pegadinhas da máquina.** Hoje só na memória do assistente: nunca usar
  `pkill -f` com padrão da própria linha de comando; `docker pull` dispara o
  prompt GPG do usuário; o XTest no GNOME precisa focar a janela antes de
  mandar teclas; fazer backup do `known_hosts` antes de testar SSH.

## Fase 2 · Harness de testes versionado (`scripts/e2e/`)

Transformar a montagem manual de hoje em scripts no repositório:
- **`cenas/*.json`**: topologias prontas (par, cadeia de três, Maestro),
  com o terminal operador incluído.
- **`launch.sh <cena>`**: compila se preciso, abre a instância isolada,
  espera os agentes ficarem ociosos e grava o ambiente do operador.
- **`op`**: roda a CLI `narrater` como o operador.
- **`cenarios/*.md`**: cada cenário com pedido, topologia, tempo-limite e o
  que conta como sucesso. Os 12 cenários de 2026-10-12 viram a suíte de
  regressão.
- **`run.sh <cenario>`**: executa um cenário, mede o tempo e guarda a
  evidência bruta (saída do `ask` e o `check` de cada agente) em
  `docs/testes/evidencias/<data>/`.
- **`stop.sh`** e o **`x.py`** (screenshots e teclado para checagens de
  interface).

Ganho: repetir toda a suíte depois de uma mudança no protocolo vira um
comando, e o relatório passa a citar evidência bruta em vez de trechos
copiados à mão.

## Fase 3 · Skills de desenvolvimento (`.claude/skills/`)

- **`testar-comunicacao`**: quando e como rodar o harness, escolher
  cenários, ler as telas e escrever o relatório no formato padrão.
- **`feature-maestri`**: o caminho de uma feature, da especificação ao
  commit. (1) Achar a spec nas skills `maestri-*` ou na doc pública.
  (2) Registrar o que é comportamento do Maestri e o que é desenho nosso.
  (3) Implementar. (4) Testes unitários. (5) Cenário com agentes reais.
  (6) Relatório. (7) Tracker. (8) Commit.

## Fase 4 · Material para a pesquisa

- **Relatórios por sessão** em `docs/testes/`, num formato fixo:
  ambiente, tabela de resultados, cenários com telas, bugs, conclusões. O
  primeiro já existe: `2026-10-12-claude-code-reais.md`.
- **Métricas padronizadas por cenário**: passou ou não, tempo, número de
  saltos, se o agente descobriu a topologia sozinho, número de comandos
  `narrater` que rodou.
- **Origem de cada peça no tracker**: uma coluna "evidência do Maestri" ou
  "desenho nosso" na tabela de status, respondendo de onde veio cada
  comportamento.

## Fase 5 · Prioridades de feature

Ordenadas pela relevância para a comunicação entre agentes:
1. ✅ **New Floor no composer** (feita em 2026-10-12): o usuário descreve uma tarefa, e um
   agente Maestro cria um andar para ela e recruta quem vai trabalhar lá.
2. **Agentes de fornecedores diferentes** conversando (Codex, Gemini CLI):
   é a tese central, "agnóstico de fornecedor", e ainda não tem teste real.
3. **Agente parado em pedido de permissão** no meio de uma cadeia (estado
   `attention`).
4. **Comunicação entre workspaces e entre andares** (`Nome @ Workspace`),
   com agentes reais.
5. Pendências do portal (popups `#2`, iframes de outro domínio): ficam
   depois, porque são uso de ferramenta, não comunicação.

## Ordem de execução

1. ✅ Push para a `main`, relatório dos testes de 2026-10-12 e este plano.
2. ✅ New Floor, testada com agentes reais (cenário 13 do relatório de
   2026-10-12; de quebra, o bug da sugestão apagada, cenário 14).
3. Fases 1 a 3 numa sessão curta; a New Floor vira o primeiro cenário
   escrito no formato novo.
4. Fase 4 em paralelo, a cada sessão de testes.
5. Itens 2 a 4 da Fase 5, cada um com seu cenário e relatório.
