# Registro de decisões

Por que o NarraTer é como é. Uma entrada por decisão que teve **alternativa real**. Se não havia
escolha, não é decisão. O registro também serve de base para relatório técnico ou artigo: cada
afirmação precisa poder ser conferida por quem não estava na sessão.

**Este arquivo só cresce.** Nunca reescreva uma entrada: decisão revista ganha entrada nova que
cita a antiga. O histórico de mudança de ideia também é informação.

## Formato (a partir de D09; D01–D08 migradas no mesmo dia, ver D09)

Cada entrada tem seis campos:

- **Contexto**: o problema, sem a solução.
- **Decisão**: o que fizemos.
- **Alternativas consideradas**: cada uma com o motivo concreto de ter sido recusada. É o campo que
  impede alguém de "consertar" a decisão sem saber por que ela existe.
- **Evidência**: o que sustenta a escolha. Cada item tem um rótulo:
  - `[medido]` número obtido por um método descrito e reproduzível (comando, máquina, repetições);
  - `[observado]` comportamento visto num teste ou execução, com o cenário;
  - `[citado]` fonte com caminho, versão ou commit (código, documentação, mensagem de commit);
  - `[inferido]` raciocínio sem medição. Vale, mas é a evidência mais fraca, e o texto diz isso.
- **Consequência**: o que isso custou ou travou.
- **Revisar se**: a condição observável que invalidaria a decisão. Uma decisão sem essa condição
  não pode ser refutada.

## Índice por área

| Área | Decisões |
|---|---|
| Comunicação entre agentes | D01, D02, D06, D08 |
| Persistência | D07 |
| CLI e distribuição | D03 |
| Escopo do produto | D04, D05 |
| Processo | D09 |

---

## D01 · O terminal é a API universal; o agente não precisa cooperar

`2026-10-01` · comunicação · commit `dcf3aaf` · mecânica em `src-tauri/src/screen.rs` e `ipc.rs`; estudo em [`pesquisa/maestri.md`](pesquisa/maestri.md) · *reconstruída em 2026-10-14 a partir dos commits*

**Contexto.** Entre 2026-07-05 e 2026-07-13 o NarraTer usou um protocolo cooperativo: o ask
entregava `[narrater de X #id]` e esperava o alvo responder com `narrater reply <id>` ou a tool MCP
`reply_message`, o fim do turno vinha de um *Stop hook* e as instruções iam por
`--append-system-prompt` e `--mcp-config`.

**Decisão.** Ler o estado do turno da tela renderizada (vt100 headless por PTY), entregar o prompt
quando o alvo está ocioso e devolver a tela no fim do turno. Instruções por skills e por
`CLAUDE.md`/`AGENTS.md`. MCP e hooks removidos.

**Alternativas consideradas.**
- *Manter o protocolo cooperativo* (resposta por ID, hooks, MCP), que é o modelo do Orca: recusada
  porque só funcionava com quem conhece o protocolo; um alvo que não conhece o `#id` deixa o ask
  travado até o timeout.
- *Cooperativo com uma camada por fornecedor* (flags e hooks específicos de cada CLI): recusada
  porque o custo cresce com cada fornecedor e quebra quando uma CLI muda.

**Evidência.**
- `[citado]` `bb49171` (resposta por `#id`) e `e13adc9` (idle por hook) implementaram o modelo
  cooperativo; a mensagem de `dcf3aaf` registra que "na prática só funcionava com o Claude Code".
- `[observado]` estudo do Maestri 0.19.4 rodando no Linux (`pesquisa/maestri.md` §1): o ask lê a
  tela e funciona com agentes de fornecedores diferentes sem flag nenhuma.
- `[citado]` o Orca, que é cooperativo, gasta boa parte da skill de orquestração tratando o worker
  que não segue o contrato (`skill-guides/orchestration.md`, seção "Canonical supervised loop",
  versão 1.4.214).
- Não há medição da taxa de falha do modelo cooperativo com agentes não Claude; a evidência da
  falha é a experiência registrada no commit.

**Consequência.** Qualquer agente que roda shell participa. Em troca, a resposta é a tela (sem
estrutura, limitada ao que cabe nela) e a detecção de turno depende de heurísticas que precisam
ser testadas contra cada CLI (`turn_probe`).

**Revisar se:** a detecção pela tela errar com frequência num agente que importa (medir com
`turn_probe` e com asks reais), ou se surgir um padrão de hooks adotado pela maioria das CLIs.

## D02 · Criar uma aresta não notifica os agentes

`2026-07-13` · comunicação · commit `18b4163` · `resolve_target` em `src-tauri/src/ipc.rs` · *reconstruída em 2026-10-14*

**Contexto.** Cada aresta nova enfileirava uma notificação de sistema nos dois agentes, e cada uma
auto-submetia um turno, gastando tokens mesmo sem nenhuma mensagem trafegar.

**Decisão.** A aresta só atualiza a tabela de rotas do backend. O agente descobre os colegas na
hora de mandar (`narrater list`) e o backend valida cada envio.

**Alternativas consideradas.**
- *Avisar os agentes ao conectar*: recusada pelo custo de um turno por agente a cada aresta.
- *Avisar só no próximo turno do agente, sem auto-submeter*: não foi implementada; dependeria de
  injetar texto no prompt de um agente que pode estar trabalhando.

**Evidência.**
- `[citado]` a mensagem de `18b4163` descreve o turno auto-submetido por aresta.
- `[inferido]` o custo em tokens não foi medido; o argumento é que um turno sem tarefa é gasto
  puro.

**Consequência.** Conectar é gratuito. Os agentes não têm a topologia na memória e precisam
consultar `list`; as skills dizem isso explicitamente.

**Revisar se:** agentes errarem destinatário ou deixarem de delegar por não consultar `list`.

## D03 · A CLI `narrater` é um binário Rust, não um script Python

`2026-10-08` · CLI · commit `ea446c6` · `src-tauri/src/bin/narrater-cli/` · *reconstruída em 2026-10-14*

**Contexto.** A CLI é chamada pelos agentes a cada ask, check e note. O script Python dependia de
um Python no sistema e em cada workspace remoto.

**Decisão.** Reescrever em Rust só com `std` + `serde_json`, com os mesmos comandos e mensagens.

**Alternativas consideradas.**
- *Manter o Python*: mais fácil de editar, mas mais lento e com uma dependência de runtime.
- *Shell script com curl*: é o que os workspaces remotos usam (`assets/narrater-remote.sh`); não
  serve localmente porque a CLI fala com um socket Unix e precisa montar JSON.

**Evidência.**
- `[medido]` segundo `ea446c6`: ~2 ms por chamada contra ~58 ms do Python. O método (máquina,
  repetições) não foi registrado no commit; a medição não pode ser reproduzida exatamente.
- `[observado]` paridade conferida com 103 linhas de comando contra o CLI Python (payload, stdout,
  stderr e exit code), segundo `ea446c6`.

**Consequência.** Um binário a mais no build (`beforeDevCommand` compila a CLI antes do app).
Mudança na CLI exige recompilar.

**Revisar se:** a CLI precisar rodar onde não há binário compilado para a arquitetura.

## D04 · Sem portais Android; o foco é agentes em terminais no PC

`2026-10-12` · escopo · commit `0812446` · *reconstruída em 2026-10-14*

**Contexto.** Os portais de dispositivo (adb, emulador, tile de toque) puxavam dependências e
manutenção para fora do núcleo do produto.

**Decisão.** Remover os portais Android, a skill `narrater-portal-devices` e as dependências
`image` e `roxmltree`.

**Alternativas consideradas.**
- *Manter como recurso secundário*: recusada pelo custo de manutenção numa área sem uso no foco
  atual.

**Evidência.**
- `[citado]` mensagem de `0812446`.
- `[inferido]` é um julgamento de escopo, sem medição de uso.

**Consequência.** Menos superfície. O app apaga a skill antiga onde a encontrar
(`RETIRED_SKILLS` em `ipc.rs`).

**Revisar se:** o fluxo de trabalho com apps mobile voltar a ser um caso de uso principal.

## D05 · Não fundir o NarraTer com o Orca; trazer as ideias

`2026-10-14` · escopo · commits `4c144b5`, `4a14591` · estudo em [`pesquisa/orca.md`](pesquisa/orca.md)

**Contexto.** O Orca (MIT) resolve o mesmo problema e tem uma camada de orquestração madura. A
ideia era juntar os dois projetos.

**Decisão.** Manter os códigos separados. Portar ideias e, como experimento opcional, falar com
uma instância do Orca pela CLI `--json` dele.

**Alternativas consideradas.**
- *Fork do Orca com o canvas dentro*: recusada pelo tamanho e pelo ritmo de mudança (evidência
  abaixo); manter o fork custaria mais que o próprio NarraTer.
- *NarraTer embutido no Orca*: stacks incompatíveis (Electron/TypeScript contra Tauri/Rust).
- *Contribuir um canvas para o Orca*: possível pela licença, mas seria uma feature enorme num
  código com regras rígidas, e o NarraTer perderia a topologia como permissão.

**Evidência.**
- `[medido]` linhas de código, contadas em 2026-10-14 com `find … | xargs cat | wc -l`: Orca
  (commit `dd4d0fa5`, `src/`, `.ts`/`.tsx` sem testes) 2.077.207; NarraTer 11.588 de Rust +
  9.183 de TypeScript. Razão de cerca de 100×.
- `[citado]` o README do Orca diz "we ship daily"; não medimos a cadência de releases.
- `[citado]` `package.json` do Orca: Electron 43, React 19, node-pty.

**Consequência.** O NarraTer mantém a identidade (canvas, topologia, zero cooperação). Uma ponte
com o Orca, se vier, depende de uma CLI de terceiros que muda rápido e precisa ser opcional.

**Revisar se:** o Orca publicar uma API de extensão estável, ou se o NarraTer passar a precisar
de uma parte grande do que o Orca já tem (editor, review de diff, integrações).

## D06 · Na orquestração, cooperação é opcional e melhora o resultado; nunca é exigida

`2026-10-14` · comunicação · estudo em [`pesquisa/orca.md`](pesquisa/orca.md) §6 · plano em [`backlog.md`](backlog.md) etapa O

**Contexto.** O estudo do Orca mostrou o que perdemos com o ask só observado: nada era durável, a
resposta não tem estrutura nem sucesso/falha, o líder fica bloqueado e pergunta do worker se
confunde com resposta.

**Decisão.** Trazer a durabilidade e o modelo de eventos (ask assíncrono, `wait`, asks
persistidos) sem pedir nada ao agente, e oferecer mensagens tipadas (`done`, `question`, `reply`)
como caminho opcional: se o agente usar, o resultado fica estruturado; se não usar, a tela vale.

**Alternativas consideradas.**
- *Adotar o contrato do Orca* (Run/Task/Dispatch, heartbeat e `worker_done` obrigatórios):
  recusada porque é o modelo cooperativo abandonado em D01.
- *Não mudar nada*: recusada pelas perdas listadas no contexto.

**Evidência.**
- `[citado]` contrato do worker do Orca: `skill-guides/orchestration/references/worker-contract.md`
  e `src/main/runtime/orchestration/preamble.ts` (versão 1.4.214).
- `[observado]` D01: a experiência de julho com resposta obrigatória por ID.
- `[inferido]` que mensagens tipadas opcionais vão melhorar a qualidade das respostas ainda não
  foi testado; é a hipótese da etapa O3.

**Consequência.** Duas fontes para o fim de um ask (tela ou mensagem tipada), e o backend precisa
decidir qual vale quando chegam as duas.

**Revisar se:** na O3, os agentes ignorarem as mensagens tipadas na maioria dos casos (não
compensam), ou se a tela e a mensagem tipada divergirem com frequência.

## D07 · Asks persistidos em JSON Lines, não em SQLite

`2026-10-14` · persistência · commit `95fb11a` · mecânica em `src-tauri/src/asks.rs` · etapa O2 do [`backlog.md`](backlog.md)

**Contexto.** A etapa O2 pedia asks duráveis, e o plano (vindo do Orca, que usa SQLite com
migrações) falava em `rusqlite`.

**Decisão.** Um arquivo `~/.config/narrater/asks.jsonl`, uma linha por mudança de estado (a última
linha de um id vale), compactado na carga para os 500 mais recentes e reescrito por arquivo
temporário + rename. Os registros ficam num `HashMap` em memória.

**Alternativas consideradas.**
- *SQLite via `rusqlite`*: recusada por trazer dependência C, tempo de compilação e migrações de
  schema para um dado que cabe na memória (evidência abaixo).
- *Um JSON inteiro reescrito a cada mudança* (como as cenas): recusada porque cada mudança de
  estado reescreveria até 4 MB.

**Evidência.**
- `[medido]` bancada `asks_bench` (`cargo test --release asks_bench -- --ignored --nocapture`),
  Xeon 2,8 GHz com 4 vCPUs, 3 rodadas: pior caso de 2.000 linhas com respostas de 8 KB (16,8 MB)
  carrega e compacta em 100–110 ms, para 4,2 MB com 500 registros; gravar um registro custa
  7,7–7,9 µs.
- `[medido]` uso real no teste ponta a ponta: 10 linhas, 3,6 KB.
- `[citado]` chat, rotinas, links e cenas já persistem em JSON em `~/.config/narrater/`
  (`chat.rs`, `routines/mod.rs`, `names.rs`, `scene.rs`).

**Consequência.** Sem consultas ad hoc nem transações. Um ask em andamento quando o app para volta
como `unknown`, nunca com um desfecho inventado.

**Revisar se:** a carga passar de ~1 s no uso real, se precisarmos de consultas entre asks (ex.:
dependências entre tarefas, etapa O8), ou se dois processos precisarem escrever no arquivo.

## D08 · Todo ask roda em segundo plano; `wait` sem ids só coleta os `--no-wait`

`2026-10-14` · comunicação · commit `95fb11a` · `start_ask`/`run_ask` em `src-tauri/src/ipc.rs` e `AskStore::wait` · etapa O1

**Contexto.** O ask síncrono prendia o líder até o alvo terminar, e se a ferramenta de shell do
líder estourasse o tempo, o resultado se perdia.

**Decisão.** Todo ask vira um registro e roda numa task em segundo plano; o `ask` síncrono é
`--no-wait` + espera pelo próprio id e imprime o mesmo que antes. Um `wait` sem ids devolve só os
asks `--no-wait` ainda não coletados, um por chamada; `wait <id>` devolve qualquer ask do chamador.

**Alternativas consideradas.**
- *Dois caminhos separados, síncrono e assíncrono*: recusada porque duplicaria a lógica de entrega
  e o síncrono continuaria perdendo o resultado em timeout.
- *`wait` sem ids coletando também os síncronos*: recusada porque o líder receberia de novo
  respostas que já viu.
- *Eventos empurrados para o terminal do líder* (digitar o resultado quando chega): recusada
  porque interromperia o turno do líder e repetiria o custo da D02.

**Evidência.**
- `[observado]` teste ponta a ponta em 2026-10-14 (app real sob Xvfb, três shells conectados):
  o `wait` devolveu primeiro o ask mais curto (~2,8 s) e depois o mais longo; `wait --timeout 1`
  listou o pendente; `wait <id>` retomou; `--all` devolveu os dois; após matar e reiniciar o app,
  o ask em andamento voltou como `unknown`.
- `[observado]` `cargo test`: 79 testes da biblioteca e 6 da CLI passando, incluindo
  `wait_any_returns_the_first_to_finish_and_collects_it` e `wait_all_and_explicit_ids`.
- `[inferido]` o backend não sabe se a resposta de um ask síncrono chegou ao chamador; por isso
  ela não entra na fila do `wait` sem ids.

**Consequência.** Um líder cujo shell foi morto acha o id em `narrater asks` e coleta com
`wait <id>`.

**Revisar se:** agentes perderem respostas de asks síncronos com frequência (sinal para o
backend confirmar a entrega ao chamador).

## D09 · Toda decisão tem evidência classificada e uma condição de revisão

`2026-10-14` · processo · `docs/decisoes.md`, skills `encerrar` e `pesquisa`, `CLAUDE.md`

**Contexto.** O projeto precisa de margem para explicação científica: cada escolha deve poder ser
justificada e contestada por quem não participou da sessão. O formato anterior (contexto, decisão,
uma alternativa, consequência) não separava o que foi medido do que foi suposto.

**Decisão.** Formato de seis campos com evidência rotulada (`[medido]`, `[observado]`,
`[citado]`, `[inferido]`) e "Revisar se". D01–D08 foram migradas para o novo formato no mesmo dia
em que o arquivo foi criado, sem mudar nenhuma decisão; a regra de só acrescentar vale daqui em
diante.

**Alternativas consideradas.**
- *Manter D01–D08 no formato antigo e adicionar entradas de complemento*: recusada porque o
  arquivo tinha horas de vida e entradas duplicadas atrapalhariam a leitura.
- *Exigir medição em toda decisão*: recusada porque decisões de escopo ou de produto nem sempre
  são mensuráveis; o rótulo `[inferido]` deixa isso explícito em vez de esconder.

**Evidência.**
- `[citado]` pedido do usuário na sessão de 2026-10-14.
- `[observado]` a migração expôs lacunas: D03 cita uma medição sem método, D02 e D04 não têm
  medição, e D07 ganhou uma bancada reproduzível que antes não existia.

**Consequência.** Registrar uma decisão leva mais tempo, e uma decisão sem evidência fica marcada
como tal.

**Revisar se:** o custo do formato fizer decisões deixarem de ser registradas.
