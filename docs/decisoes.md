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
| Comunicação entre agentes | D01, D02, D06, D08, D20, D21, D24 |
| Leitor de estado | D10, D11, D12 |
| Persistência | D07 |
| Uso dos agentes | D17 |
| CLI e distribuição | D03 |
| Escopo do produto | D04, D05 |
| Processo | D09, D19 |
| Orquestração | D22, D23 |
| Design | D13, D14, D15, D16, D18 |

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

## D10 · Shell no tmux: primeiro plano lido do /proc, e job rodando conta como atividade

`2026-10-14` · leitor de estado · `foreground_busy`/`tmux_pane_pid` em `src-tauri/src/screen.rs`, `compute_turn` em `src-tauri/src/pty.rs` · estudo em [`pesquisa/leitor-de-estado.md`](pesquisa/leitor-de-estado.md) §5

**Contexto.** Os terminais persistentes rodam dentro do tmux. Ali o primeiro plano do PTY é o
cliente tmux, e o app só olhava a tela: um comando silencioso (`sleep`) parecia fim de turno e o
ask voltava antes da hora.

**Decisão.** Para shell local no tmux, pegar o pid do shell do painel (`tmux display-message
#{pane_pid}`, uma vez, com nova tentativa a cada 2 s se falhar) e ler `/proc/<pid>/stat`: há job
em primeiro plano quando o `tpgid` do terminal difere do grupo do shell. E, para todo shell,
enquanto há job em primeiro plano a tela é marcada como ativa: o turno só assenta 0,6 s depois de o
job acabar e o prompt ser redesenhado.

**Alternativas consideradas.**
- *`#{pane_current_command}` do tmux a cada tick*: um processo novo a cada 200 ms por terminal, e
  o nome do comando não diz se é o shell esperando ou um job (ex.: `bash` rodando um script).
- *Só ler o primeiro plano, sem marcar atividade*: medido, deixou 4/15 retornos prematuros de
  2–3 ms, no instante entre o fim do job e o redesenho do prompt.
- *Desligar o tmux para shells*: perderia a persistência dos terminais.

**Evidência.**
- `[medido]` `turn_eval`, tmux com comando silencioso e com rajada + silêncio: antes 6/6
  prematuros (−2,2 s e −3,2 s); depois 0/20, latência 603–802 ms (Fisher exato, p ≈ 4,3×10⁻⁶).
- `[medido]` só lendo o primeiro plano: 4/15 prematuros de 2–3 ms em `tmux-burst-then-silent`.
- `[observado]` app real sob Xvfb: `narrater ask` para um shell no tmux com `sleep 6; echo …`
  passou a durar ~7,2 s e devolver a saída (antes voltava em ~1 s, sem ela), 3/3.
- `[citado]` `proc(5)`: o campo 8 de `/proc/<pid>/stat` é o `tpgid`, o grupo em primeiro plano do
  terminal do processo.

**Consequência.** Shells remotos (SSH/Docker) continuam só com a tela, porque o `/proc` é local.
Um ask para shell passa a demorar, no máximo, ~0,8 s além do fim real.

**Revisar se:** surgir shell local em que o `tpgid` não reflita o job (ex.: um multiplexador
dentro do tmux), ou se o atraso de 0,6 s após o job pesar em fluxos com muitos comandos curtos.

## D11 · O título da janela entra no leitor; menu + título em repouso bastam para "attention"

`2026-10-14` · leitor de estado · `title_signal` e `classify` em `src-tauri/src/screen.rs` · estudo em [`pesquisa/leitor-de-estado.md`](pesquisa/leitor-de-estado.md) §1.2 e §5

**Contexto.** O leitor só olhava o conteúdo da tela e exigia 1,2 s de tela parada. Isso atrasava
toda resposta do Claude em ~1,3 s e, quando o Claude pisca um marcador durante o diálogo de
permissão, impedia de vez a detecção.

**Decisão.** Guardar o título (OSC 0/2) e usá-lo com o catálogo de glifos do Orca: spinner
(braile, `◐◑◒◓`, `✦`, `⏲`) = trabalhando; `✋` = permissão; `✳`/`◇` = em repouso. Com título em
repouso e rodapé sem marca de trabalho, o turno assenta em 0,4 s em vez de 1,2 s; com título em
repouso e menu de aprovação na tela, é `attention` na hora, sem exigir tela parada. Mudança de
título conta como atividade. Sem glifo conhecido, tudo segue como antes.

**Alternativas consideradas.**
- *Ignorar mudanças pequenas da tela (uma célula piscando)*: resolveria a piscada, mas spinners
  de uma célula também significam trabalho; um agente sem outro sinal passaria a parecer ocioso.
- *Estado "pronto" pegajoso como o CAO*: estabiliza a exibição, mas não resolve a detecção (a tela
  nunca chegava a "pronto").
- *Usar os hooks do Claude no app*: exato, mas contraria a D01.
- *Só o título, sem o rodapé*: o título fica `✳` também no diálogo de permissão; sozinho não
  distingue repouso de espera por aprovação.

**Evidência.**
- `[medido]` `turn_eval` com Claude Code 2.1.299 (`haiku`), turnos normais: antes n=9, 1.294–2.112
  ms (mediana 1.355); depois n=15, 466–670 ms (mediana 551), 0 prematuros (Mann–Whitney exato,
  p ≈ 1,5×10⁻⁶).
- `[medido]` permissão: antes 0/3 detectados em 180 s; depois 3/3, 103–222 ms após o
  `PermissionRequest`.
- `[observado]` títulos `✳ …` / `◐ …` / `◑ …` nas transcrições; piscada de 0,6 s em 3 execuções
  e ausente em 11 (causa não identificada).
- `[observado]` `tests/turn_transcripts.rs`: o leitor antigo falha em todas as 6 transcrições
  (1,3 s de atraso; 24,8 s com a piscada) e o novo passa em todas.
- `[citado]` glifos: Orca `src/shared/agent-title-glyphs.ts` (1.4.214). Os de Gemini, braile e
  `✋` **não foram verificados** aqui.

**Consequência.** O leitor passa a depender, para o Claude, de um detalhe da TUI que pode mudar
entre versões; as transcrições de teste denunciam isso. A tela continua sendo a base para
qualquer agente sem título conhecido.

**Revisar se:** uma versão nova do Claude Code mudar os glifos (o teste de transcrições falha),
ou se aparecer `attention` falso com título em repouso e uma lista numerada "1. Yes…" numa
resposta comum.

## D12 · O leitor de estado é medido contra um oráculo, e transcrições reais viram teste

`2026-10-14` · leitor de estado / processo · `src-tauri/examples/turn_eval.rs`, `turn_replay.rs`, `tests/turn_transcripts.rs`, `tests/transcripts/`

**Contexto.** As heurísticas eram testadas contra telas escritas à mão e pelo `turn_probe`, sem
número de acerto. Não havia como dizer se uma mudança melhorava ou piorava.

**Decisão.** Um avaliador que roda programas reais e compara o leitor com um oráculo independente
(timestamp do próprio comando; hooks `Stop` e `PermissionRequest` do Claude, só no avaliador).
Cada execução grava a saída bruta; algumas gravações viram fixtures com o momento do oráculo, e um
teste de integração as reproduz exigindo: não prematuro, latência ≤ 1 s e estado esperado.

**Alternativas consideradas.**
- *Continuar só com testes de tela escritos à mão*: não pegaram a piscada nem o atraso de 1,3 s.
- *Rotular o fim do turno à mão nas gravações*: mais lento e subjetivo que um hook que dispara no
  evento real.
- *Hook `Notification` como oráculo da permissão*: medido, chega ~5,9 s depois do diálogo.

**Evidência.**
- `[observado]` o avaliador encontrou três problemas que os testes existentes não viam: shell no
  tmux, piscada no diálogo de permissão e o oráculo errado da permissão.
- `[observado]` o teste de transcrições distingue os dois leitores (antigo falha, novo passa).

**Consequência.** Toda regra nova que lê a tela de uma CLI deve vir com uma transcrição em
`tests/transcripts/`. Medir o Claude gasta chamadas de modelo (pequenas, com `haiku`).

**Revisar se:** o oráculo de alguma CLI não existir (sem hooks); aí a referência terá de ser
rotulada à mão, e o método precisa dizer isso.

## D13 · Perfil de design "Palco": moldura neutra, cor só para estado

`2026-10-14` · design · perfil em [`design.md`](design.md) · estudo em [`pesquisa/visual.md`](pesquisa/visual.md)

**Contexto.** O front era percebido como duro. O violeta saturado aparecia em logo, seleção,
arestas, rótulos, atalhos e minimapa; os níveis de fundo quase não se distinguiam; o texto
apagado não passava no contraste mínimo. Orca e Maestri, que o usuário usou como referência,
seguem a HIG da Apple.

**Decisão.** Um perfil com metáfora própria ("o canvas é um palco, os terminais são personagens,
as arestas são falas") e seis regras: a moldura recua; cor é estado; profundidade por luz (base →
elevado → flutuante); vidro só na camada de controle; movimento só quando algo acontece;
legibilidade primeiro. Tokens únicos em `src/index.css`, nunca hex nos componentes.

**Alternativas consideradas.**
- *Copiar o Orca*: monocromático puro funciona para uma IDE de abas, mas o NarraTer é um grafo; sem
  nenhuma cor o usuário perde a leitura de quem está trabalhando à distância.
- *Manter o visual e só ajustar valores*: a cor como decoração era o problema de fundo, não o tom.
- *Imitar o Liquid Glass em tudo*: a própria HIG proíbe na camada de conteúdo ("Don’t use Liquid
  Glass in the content layer").

**Evidência.**
- `[citado]` HIG (dark-mode, materials, color, typography) e `docs/STYLEGUIDE.md` do Orca.
- `[medido]` contraste WCAG (script em Python, sRGB): texto apagado antigo `#6b7280` 3,52:1 e
  `#555` 2,28:1 sobre o tile; tokens novos `--text-3` 5,22:1 e `--text-2` ≥ 6:1.
- `[medido]` cores fixas nos componentes: 440 classes com hex trocadas por tokens; restam só
  paletas que são dados (cores que o usuário escolhe).
- `[observado]` capturas antes/depois em `docs/pesquisa/img/`.
- `[inferido]` "mais bonito e intuitivo" não foi medido com usuários.

**Consequência.** Componentes novos precisam seguir `docs/design.md`. Não há modo claro ainda (a
HIG recomenda os dois).

**Revisar se:** usuários não distinguirem estados à distância, ou se o modo claro for pedido
(os tokens já estão em um lugar só para isso).

## D14 · O acento é um violeta contido (#8f7cff), não monocromático

`2026-10-14` · design · `--accent` em `src/index.css`

**Contexto.** O violeta era a identidade do NarraTer, mas usado em tudo.

**Decisão.** Manter um violeta como "o narrador", mais claro e menos saturado (`#8f7cff`, e
`#b0a4ff` quando é texto), restrito a seleção, foco, agente trabalhando e fala atravessando.

**Alternativas consideradas.**
- *Monocromático (Orca)*: perde a identidade e o sinal de "trabalhando" no grafo.
- *Manter `#8b5cf6`*: como texto sobre o popover dá 4,3:1, abaixo do mínimo; o novo dá 6,4:1
  (`--accent-text`).
- *Azul do sistema da Apple*: confundiria com links e com a identidade do Gemini.

**Evidência.**
- `[medido]` `#8f7cff` sobre o palco 6,08:1; `#b0a4ff` 8,99:1 (palco) e 6,37:1 (popover).
- `[observado]` captura ativa: os agentes trabalhando ficam visíveis à distância.

**Consequência.** O violeta deixa de marcar papéis, atalhos e arestas em repouso.

**Revisar se:** o violeta confundir com a cor de identidade de algum agente novo.

## D15 · A fonte da interface continua Inter

`2026-10-14` · design · `tailwind.config.js`, `src/index.css`

**Contexto.** A HIG usa SF Pro e o Orca usa Geist.

**Decisão.** Manter Inter Variable (já embarcada), com corpo de 13 px e tracking levemente
negativo, e JetBrains Mono no terminal.

**Alternativas consideradas.**
- *SF Pro*: `[inferido]` a licença da Apple restringe o uso às plataformas Apple; não foi
  verificada aqui, e por isso não entra.
- *Geist*: dependência nova sem ganho claro sobre a Inter.
- *Fonte do sistema*: no Linux varia entre distribuições (Cantarell, Ubuntu, DejaVu), quebrando a
  consistência.

**Evidência.** `[citado]` HIG typography (macOS: corpo 13 pt, mínimo 10 pt); `[inferido]` a Inter
tem métricas próximas da SF Pro.

**Consequência.** Nenhuma dependência nova.

**Revisar se:** surgir uma fonte com licença livre mais próxima da SF Pro, ou se a Inter não
renderizar bem no WebKitGTK em alguma distribuição.

## D16 · No grafo, arestas em repouso não têm rótulo; o movimento conta o que acontece

`2026-10-14` · design · `src/components/AgentPipeEdge`, `AgentNoteEdge`, `src/index.css`

**Contexto.** Cada aresta tinha um rótulo fixo ("agent pipe", "agent→note"), brilho e tracejado
violeta sempre ligados.

**Decisão.** Aresta agente ↔ agente: curva fina neutra em repouso; acende com fluxo lento quando
um lado trabalha; um cometa de luz atravessa quando uma mensagem passa; chip amarelo com o número
da fila; no hover, "A ↔ B". Aresta agente → nota: tracejado discreto, sem animação. Alças de
conexão só aparecem no hover do nó.

**Alternativas consideradas.**
- *Rótulo sempre visível (antes)*: com muitas arestas vira ruído e esconde o que mudou.
- *Cor de identidade nas arestas*: misturaria identidade com estado (D13).
- *Seta indicando direção*: as conexões são não direcionadas (aresta é permissão para os dois
  lados).

**Evidência.**
- `[observado]` captura ativa: fila e trabalho legíveis sem rótulo fixo.
- `[inferido]` a descoberta da conversa por hover/clique não foi testada com usuários.

**Consequência.** O histórico da aresta continua no clique; o cometa depende do ledger em memória
(o mesmo da etapa O).

**Revisar se:** usuários não perceberem que a aresta é clicável, ou não encontrarem as alças.

## D17 · O uso do Claude vem da statusline, que o NarraTer embrulha sem nunca sobrescrever

`2026-10-15` · uso dos agentes · `src-tauri/src/usage.rs`, `src-tauri/src/cli_core.rs` (`usage-report`), `src/stores/usage.ts`

**Contexto.** Queremos mostrar a porcentagem das janelas de 5 h e 7 dias do plano do Claude e o
contexto de cada sessão (V4). Havia duas rotas (`pesquisa/visual.md` §5): o endpoint OAuth de uso
e o JSON que o Claude Code entrega à statusline a cada resposta.

**Decisão.** Ficamos com a statusline. Na inicialização, o app instala
`~/.local/share/narrater/claude-statusline.sh` em `statusLine.command` do `settings.json` do
Claude. O script repassa o payload para `narrater usage-report`, que o envia ao backend pelo
socket, no máximo uma vez a cada 2 s por terminal. Depois roda a statusline anterior do usuário
com o mesmo payload, e a saída dela continua aparecendo. Regras do instalador (`usage::plan`):
- slot vazio: instala;
- slot com outro comando: embrulha (guarda o comando em `claude-statusline.user`);
- slot já nosso: não mexe;
- vazio depois de já termos instalado: o usuário tirou de propósito, e não reinstalamos (marcador
  `claude-statusline.installed`).

Um `settings.json` que não pode ser lido nunca é reescrito. Fora de um terminal do NarraTer
(`NARRATER_ID` ausente), o script só roda o comando do usuário. Os limites do plano persistem em
`~/.config/narrater/usage.json`.

**Alternativas consideradas.**
- *Endpoint OAuth (`/api/oauth/usage`)*: não exige configuração, mas o Orca registra 429 sob
  polling, e ele lê o token de credenciais do usuário. A statusline já traz o dado a cada turno,
  sem requisição extra.
- *Política do Orca (não instalar se o usuário tem statusline)*: deixaria sem uso justamente quem
  personalizou a statusline. Embrulhar preserva a saída dela.
- *Hook do Claude*: os hooks não recebem `rate_limits`, e exigir hook fere D01. A statusline é
  opcional: sem ela, o leitor de estado continua funcionando (D06).

**Evidência.**
- `[citado]` documentação oficial da statusline do Claude Code. O payload tem
  `context_window.used_percentage`, `cost.total_cost_usd`, `model.display_name` e
  `rate_limits.five_hour/seven_day.{used_percentage,resets_at}`; `rate_limits` só existe em
  Pro/Max, depois da primeira resposta. A execução tem debounce de 300 ms. Uma statusline própria
  esconde as dicas do rodapé, inclusive "esc to interrupt".
- `[medido]` impacto no leitor de estado (D12). Comando: `NARRATER_EVAL_STATUSLINE=1 cargo run
  --release --example turn_eval -- --reps 3 --claude` (Claude Code 2.1.300, este container).
  Resultado: 12/12 corretos, 0 prematuros, latência ociosa de 455–628 ms, contra 466–670 ms sem
  statusline. "esc to interrupt" aparece 0 vezes na tela, contra 560 sem. A regra do título (D11)
  cobre o rodapé perdido. Transcrição: `tests/transcripts/claude-2.1.300-statusline-silent-tool.*`.
- `[observado]` ponta a ponta no app real sob Xvfb:
  - o instalador preservou `"theme": "dark"`;
  - o Claude do tile Dev disparou o script;
  - `usage.json` recebeu five_hour 0% e seven_day 7%;
  - a barra lateral mostrou "Claude 5-hour 0%, Weekly 7%", e o tile mostrou o anel de contexto
    "4%".
- `[citado]` testes `usage::tests` (slot nunca sobrescrito; payload documentado; sessão por chave
  de API sem limites mas com contexto).

**Consequência.** Mexemos num arquivo de configuração do usuário (com backup implícito em
`.user`). Com statusline, o rodapé perde "esc to interrupt", e o leitor depende mais do título.
Codex e Gemini ainda não têm rota.

**Revisar se:** o Claude Code deixar de mandar `rate_limits` à statusline ou passar a mandá-los
aos hooks; o leitor errar com statusline numa versão nova (rodar o `turn_eval` acima); ou usuários
reclamarem do embrulho.

## D18 · Conexões são cordas com física: qualquer borda, contornam os nós, balançam e dormem

`2026-10-15` · design · `src/lib/rope.ts`, `src/lib/useRope.ts`, `src/components/{AgentPipeEdge,AgentNoteEdge,RopeEdge}`, `src/components/ui/NodeHandles.tsx`

**Contexto.** As arestas saíam só das alças esquerda e direita e eram uma Bézier fixa. Com nós
acima ou abaixo, a curva dava voltas, e os terminais passavam por cima do traço.

**Decisão.** Cada conexão é uma corda simulada:
- Verlet com restrições de distância, folga de 5%, gravidade leve, amortecimento 0,9 e
  12 iterações;
- as pontas ficam no ponto da borda voltado para o centro do outro nó (qualquer lado), e isso é
  recalculado quando os nós se movem;
- os pontos internos são empurrados para fora das caixas de todos os nós, com 16 px de folga;
- o caminho é desenhado com Catmull-Rom → Bézier;
- a simulação só roda enquanto algo se move e dorme depois de 18 quadros calmos;
- `prefers-reduced-motion` resolve direto, sem animar.

Alças nos quatro lados e `ConnectionMode.Loose`: dá para soltar a conexão em qualquer borda.

**Alternativas consideradas.**
- *Bézier com alças fixas (antes)*: é a causa do problema.
- *Roteamento ortogonal (estilo diagrama)*: contorna bem, mas fica rígido e técnico, contra o
  perfil "Palco" (D13). Também exige um roteador global (A* em grade) a cada movimento.
- *Desenhar as arestas acima dos nós*: o traço passaria por cima do conteúdo do terminal.
- *Biblioteca de física (matter.js e afins)*: dependência grande para cerca de 20 pontos por
  corda.

**Evidência.**
- `[medido]` `node --experimental-strip-types scripts/rope-check.ts`, com três cenários (nada no
  caminho, um terminal no meio, dois terminais):
  - 0 pontos dentro de obstáculos e 0 segmentos cruzando;
  - assenta em 50–119 quadros;
  - âncoras nos lados certos.
- `[observado]` app real sob Xvfb. A corda Lead↔Dev contorna a nota "Plano". Ao arrastar Dev, as
  âncoras trocam de lado, e a corda balança e assenta (`pesquisa/img/cordas.gif`,
  `pesquisa/img/narrater-v4-cordas.png`).
- `[inferido]` o custo é baixo: a simulação é O(pontos × obstáculos × iterações) e só roda em
  movimento. Não medimos com mais de 10 nós.

**Consequência.** O caminho é escrito direto no SVG (sem render do React por quadro). Uma corda
pode ficar presa entre dois nós muito próximos (o empurrão vai para o lado mais perto, sem
planejar a rota). Os ids das alças mudaram (`top|right|bottom|left` e `*-in`); as arestas antigas
continuam desenhando, porque a corda ignora a alça.

**Revisar se:** o canvas com dezenas de nós perder quadros ao arrastar; cordas ficarem presas ou
cruzarem nós em layouts reais; ou usuários preferirem rotas retas.

## D19 · Versões se comparam por um benchmark fixo: mesma tarefa, testes escondidos, rodada zerada

`2026-10-15` · processo · `bench/orquestracao/`, skill `benchmark`, `docs/relatorios/orquestracao-versoes.tex`

**Contexto.** Queríamos saber quanto cada mudança de orquestração custou ou rendeu em tokens,
contexto, eficiência e trabalho feito. Sem um método fixo, cada comparação vira impressão.

**Decisão.**
- **Tarefa:** o repositório inventado `pedidos`, com regras próprias, uma funcionalidade que
  atravessa os três módulos e dois defeitos descritos só como reclamação de cliente.
- **Julgamento:** 31 testes escondidos em quatro níveis.
- **Equipe:** três agentes com modelo fixo por papel (Narrador Opus 5.5, DevA Sonnet 5.5, DevB
  Haiku 5.5), com os papéis padrão de cada versão.
- **Condições:** toda rodada começa zerada (HOME, tmux, sessões e ambiente próprios); ordem
  contrabalançada; métricas tiradas das transcrições do Claude Code, que são iguais em todas as
  versões.
- **Métrica principal de tokens:** os processados (entrada + cache lido + cache criado).

**Alternativas consideradas.**
- *Calculadora de expressões*: o modelo sabe o exercício de cor, o líder o faria sozinho e todas as
  versões tirariam nota máxima.
- *Encurtador de links com HTTP e banco*: realista, mas difícil de deixar igual entre rodadas, e a
  nota fica menos objetiva.
- *Mesmo modelo nos três agentes*: mais simples. Misturar modelos fixos por papel foi pedido do
  usuário (orquestração é sobre diversidade) e continua sendo controle, porque cada papel tem o
  mesmo modelo em todas as versões.
- *Custo em US$ como métrica principal*: depende de acerto de cache entre rodadas próximas.
- *Digitar o pedido na interface (xdotool)*: frágil. Um wrapper `claude` entrega o pedido como
  prompt inicial pelo mesmo caminho em todas as versões.

**Evidência.**
- `[medido]` A solução de referência passa 31/31 nos testes escondidos, e o repositório inicial 5/31
  (`oculto/placar.py`).
- `[medido]` Doze rodadas (2026-10-15), resumidas em `pesquisa/comparacao-versoes.md`:
  - todas tiraram 31/31;
  - as métricas de custo separam A de D sem sobreposição;
  - a resposta cortada aparece como mecanismo.
- `[observado]` A primeira rodada de teste revelou que o Claude recusa `--dangerously-skip-permissions`
  como root. `IS_SANDBOX=1` resolve e vale para todas as versões.

**Consequência.** Toda mudança de orquestração pode ser medida antes de ir para a main. A tarefa atual
bate no teto de qualidade, então mede só custo. Uma rodada custa cerca de US$ 0,70 a 1,00 em preço de
API e leva de 2 a 6 minutos; 12 rodadas levam cerca de 50 minutos.

**Revisar se:** uma versão tirar menos de 31/31 (a tarefa voltou a medir qualidade); o formato das
transcrições do Claude Code mudar; ou entrarem agentes de outros fornecedores, que pedem métricas
que não dependam do `.jsonl` do Claude.

## D20 · A resposta do `ask` e o fim do turno vêm do transcript do agente quando ele existe

`2026-10-15` · comunicação entre agentes · `src-tauri/src/transcript.rs`, `ipc.rs` (`run_ask`, `TranscriptCheck`)

**Contexto.** O benchmark (D19) mostrou dois defeitos do `ask` lido da tela:
- a resposta cortada (a tela tem cerca de 13 linhas);
- a resposta dada antes da hora: o leitor dava o turno por encerrado 4 a 5 s depois da entrega, antes
  de o agente começar.

Os dois fazem o Narrador repetir pedidos. No pior caso, ele deixa de confiar no `ask`.

**Decisão.** Para o Claude Code, o `ask` lê o transcript JSON Lines que o próprio Claude grava em
`~/.claude/projects/<pasta>/`:
- **Âncora.** Antes de enfileirar o pedido, anota o tamanho de cada transcript da pasta do agente e do
  arquivo que a statusline informou (D17).
- **Resposta.** No fim do turno, devolve o texto escrito depois da última chamada de ferramenta, no
  arquivo que recebeu o pedido (identificado pela mensagem inteira).
- **Fim do turno.** Se a tela diz ocioso mas o transcript não registrou o fim do turno, o `ask` continua
  esperando, até 120 s. O fim é `system/turn_duration`, um texto com `end_turn` ou o próximo pedido.
- **Fallback.** Sem transcript, a tela vale como antes. Nada exige cooperação do agente (D01, D05).

**Alternativas consideradas.**
- *Scrollback do emulador* (a O5 original): o Claude Code roda em tela alternativa, então o
  histórico é vazio (`[medido]`, `pesquisa/orca.md` §7).
- *Histórico do tmux com a tela alternativa desligada* (`CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1`):
  funciona, mas muda a rolagem do tile (o modo de cópia do tmux) e ainda depende da tela para saber
  o fim. Fica como segunda fonte futura, para agentes sem transcript.
- *Modelo do Orca* (`worker-read` + `worker_done` obrigatório): não ancora na entrega, corta cada
  bloco em 1.200 caracteres e exige que o agente coopere.
- *Resposta empurrada do Agent Teams*: só existe dentro do Claude; a ideia é a mesma, e a fazemos sem
  depender dos arquivos internos de times.
- *Usar só o `end_turn`*: o bloco de pensamento traz `end_turn` 15 s antes do texto
  (`[observado]`, caso real virou teste).

**Evidência.**
- `[medido]` Leitor contra transcripts reais: acha a resposta certa em 126 de 126 pedidos das rodadas
  C a G (incluindo batch), e nenhum casa com o transcript do outro dev.
- `[medido]` Benchmark, 3 rodadas por versão:
  - asks prontos antes da hora: D 4 de 27, E 12 de 31, F e G 0 de 40;
  - respostas cortadas: D 1 a 5 por rodada, G 0;
  - tokens processados: D 3,26 a 3,59 milhões, G 2,01 a 2,85 milhões;
  - custo: D 1,02 e G 0,74 US$ (A, presa ao Claude, 0,69);
  - D contra G sem sobreposição em tokens, custo e pedidos (Mann–Whitney U = 9, p = 0,10).
- `[citado]` Testes `transcript::tests`, com o turno real do ask prematuro e o caso do batch.

**Consequência.**
- O transcript é formato interno do Claude Code e pode mudar sem aviso. O fallback para a tela evita
  quebra, e o benchmark é o alarme.
- Codex e outros agentes ainda leem a tela até ganharem leitor próprio.
- Um turno legítimo de mais de 120 s em que a tela fique parada ainda cai na tela, por segurança.

**Revisar se:** o Claude Code mudar o formato ou o local do transcript; o benchmark voltar a mostrar
asks antes da hora ou respostas trocadas; ou existir uma fonte oficial de "fim de turno" que não
exija cooperação.

## D21 · O histórico de atividade é gravado pelo backend, nunca por um agente

`2026-10-15` · comunicação entre agentes · `src-tauri/src/activity.rs`, `src/components/ActivityPanel`, `src/stores/activity.ts`

**Contexto.** Para verificar o que aconteceu numa orquestração (quem pediu o quê, quando cada agente
trabalhou, se a resposta chegou inteira), os fatos estavam espalhados:
- as mudanças de estado iam ao front sem ficar salvas;
- os asks ficavam em `asks.jsonl`, sem os estados;
- as mensagens ficavam num registro em memória.

O usuário pediu um histórico verificável e explicitamente não escrito por IA.

**Decisão.** O backend grava cada evento no momento em que ele acontece, em
`~/.config/narrater/activity.jsonl` (JSON Lines, gira em 5 MB, 5.000 eventos em memória):
- estado do turno, início e saída de terminal;
- pedido, entrega, nota quando o transcript contradiz a tela (D20) e resposta, com estado, duração e
  origem (transcript ou tela);
- mensagens diretas.

O painel Activity lê e filtra esse registro e só dobra a exibição das trocas de estado de menos de 2 s.
O arquivo fica intacto.

**Alternativas consideradas.**
- *Pedir aos agentes que registrem o que fazem* (notas, `narrater done`): depende de cooperação
  (D01), e o registro seria a palavra do agente, não o fato.
- *Reaproveitar o `asks.jsonl`*: não tem estados nem mensagens, e é compactado para 500 registros.
- *Só em memória, como o ledger*: some ao reiniciar, e verificar depois é justamente o objetivo.
- *Gravar só estados estáveis* (com debounce no backend): perderia exatamente as trocas rápidas que
  explicam um ask antes da hora (D20). Por isso a dobra acontece só na exibição.

**Evidência.**
- `[observado]` Rodada de teste do benchmark com o build do painel (vH-r0): 36 eventos até a primeira
  resposta. Nos dois asks iniciais, as notas "screen looks idle, transcript says the turn is still
  running" aparecem logo após a entrega, o caso da D20 visto ao vivo.
- `[observado]` Reaberto o app com o mesmo HOME, o histórico da rodada anterior continuou no painel.
- `[observado]` O WebKitGTK do Tauri ignora o `line-clamp` do CSS em texto pré-formatado. A prévia
  recolhida é cortada no código.

**Consequência.**
- Mais uma escrita em disco por evento: o volume é de dezenas por minuto com agentes ativos.
- O texto de pedidos e respostas fica truncado em 4.000 caracteres no registro; o texto inteiro de um
  ask continua no `asks.jsonl`.

**Revisar se:** o arquivo crescer mais rápido que a rotação aguenta; ou o painel precisar de eventos
de outros agentes (Codex, Gemini) que hoje só chegam pela tela, sem nada novo a registrar.

## D22 · O padrão é um agente só; montar equipe precisa de motivo

`2026-10-16` · orquestração · `docs/relatorios/quando-orquestrar.pdf`, `docs/pesquisa/orquestracao.md`, `bench/orquestracao/solo.py`

**Contexto.** O estudo de versões (D19) só comparou equipe contra equipe. Nunca perguntamos se montar a
equipe compensa. A literatura de 2025 e 2026 diz que, em código e em tarefas que um agente sozinho já
resolve, normalmente não compensa. O próximo passo do produto é um gerador automático de equipes, que
precisa de um padrão.

**Decisão.** Quando alguém (o usuário, o Narrador ou o futuro gerador) decide como atacar uma tarefa, a
resposta padrão é **um agente só**. Montar equipe exige pelo menos um motivo explícito:
- duas ou mais subtarefas independentes, com arquivos separados;
- um verificador com contexto limpo (revisor ou QA), de preferência de outro fornecedor;
- um contexto que não cabe num agente.

O motivo fica registrado (nota no canvas). A equipe de código preferida é "um escritor + um revisor".

**Alternativas consideradas.**
- *Equipe por padrão, como hoje no pedido do benchmark e no modo Maestro*: medimos que custa mais e
  demora mais pela mesma qualidade.
- *Deixar a decisão inteiramente ao líder, sem regra*: a Anthropic relata líderes criando 50 subagentes
  para perguntas simples, e o MAST mostra que a especificação fraca do sistema é a maior família de
  falhas (~44%).
- *Nunca orquestrar*: a literatura mostra ganhos reais em trabalho paralelo de leitura (+80,8%, Kim et
  al.) e com revisor de contexto limpo (Cognition, ~2 bugs por PR). O produto existe para esses casos.

**Evidência.**
- `[medido]` Braço "sozinho" do benchmark (`BENCH_VARIANTE=solo`, 3 rodadas, versão H `73fab8b`) contra a
  equipe G (3 rodadas):
  - 31/31 nos dois;
  - 1,6 contra 3,9 min;
  - 0,36 contra 2,50 milhões de tokens processados;
  - 8 contra 49 chamadas à API;
  - US$ 0,55 contra 0,74.

  Nenhuma dessas métricas se sobrepõe (Mann–Whitney U = 0, p = 0,10, o mínimo com n = 3). Contra a
  equipe F, o tempo também não se sobrepõe; o custo sim (p = 0,70). Só o Narrador da G, coordenando,
  custou US$ 0,32–0,41, de 60% a 75% do Opus sozinho fazendo tudo. Dados em
  `bench/orquestracao/resultados/2026-10-16/`.
- `[citado]` Fontes externas:
  - Kim et al. 2025: coordenar não paga acima de ~45% de acerto do agente sozinho; −39% a −70% em
    tarefas sequenciais.
  - Vals.ai 2026-10-09: equipes custam de 1,8× a 5,1×, com 1 ganho significativo em 4. Opus 5.5 médio:
    −0,3 ponto a 2,2× o custo.
  - Tran e Kiela 2026: com o mesmo orçamento, o agente sozinho empata ou ganha.

  Todas em `docs/relatorios/quando-orquestrar.pdf`.

**Consequência.**
- O pedido do benchmark, o modo Maestro e a skill `narrater-manager` precisam da regra do portão.
- O gerador de equipes (backlog O11) tem "sozinho" como primeiro modelo.
- O benchmark precisa de uma tarefa que o agente sozinho não sature e de uma paralelizável. Sem elas,
  não há como medir o caso em que a equipe deveria ganhar.

**Revisar se:** numa tarefa sem teto (agente sozinho abaixo de ~70%) ou paralelizável, a equipe ganhar em
qualidade ou em custo por teste aprovado; ou se os modelos mudarem a relação de preço entre o líder e os
devs a ponto de a coordenação ficar barata.

## D23 · O gerador de equipes preenche modelos conhecidos; "Automatic teams" é uma chave do usuário

`2026-10-16` · orquestração · `src/lib/teamCommands.ts`, `src/lib/teamTemplates.ts`, `src-tauri/src/preferences.rs`, `src-tauri/assets/SKILL*.md`

**Contexto.** Montar uma equipe exigia N comandos (`recruit`, `role create`, `note create`, `connect`), sempre em
estrela e amontoados à direita, sem modelo por agente. O usuário pediu um gerador: descrever a equipe ("dev, QA
e design fazendo X") ou deixar o Narrador decidir sozinho, com uma opção liga/desliga. A D22 fixou que o padrão é
um agente só.

**Decisão.** `narrater team create` monta a equipe inteira numa chamada (terminais com agente, modelo e papel; as
conexões; uma nota de tarefa com o critério de aceite, para todos; uma nota de porquê, para quem pediu):
- **Modelos, não invenção.** O gerador preenche formatos conhecidos (`solo`, `dev-reviewer`, `dev-qa`, `readers`,
  `parallel-devs`), descritos como dados, cada um com "quando compensa" e o custo medido ou lido; `--spec` aceita
  um formato próprio em JSON.
- **O portão.** `solo` é um modelo e não cria terminal, só registra a decisão. Para os outros, `--reason` é obrigatório.
- **Quem pode.** Um terminal Maestro, ou qualquer agente quando o usuário liga **Automatic teams** (botão na barra;
  `auto_teams` nas preferências, desligado por padrão). Membros de uma equipe gerada não geram equipes. As skills
  dizem que, com a chave desligada, o Maestro só monta equipe quando o usuário pede.
- **Fora do escopo:** desmontar equipes (futuro). As equipes montadas à mão e as cenas do benchmark não mudam.

**Alternativas consideradas.**
- *Um agente gerador dedicado, sempre rodando*: custaria uma sessão inteira para escrever um JSON; o pedido vai ao
  agente que já está ali.
- *Gerar a topologia livremente por LLM* (AutoAgents, AgentVerse): os resultados bons da literatura escolhem entre
  formatos conhecidos (AFlow, MaAS, MasRouter), e o estudo de topologia de 2026 mostra que só a topologia move a
  nota em 30 pontos. Formatos fixos são verificáveis e medíveis no benchmark.
- *A chave liberar qualquer agente, inclusive membros*: abriria equipes de equipes; o Orca usa limite de profundidade.
- *Exigir confirmação do usuário a cada equipe*: fica como terceiro estado possível da chave, se o liga/desliga não bastar.

**Evidência.**
- `[medido]` D22 e `relatorios/equipe-ou-sozinho.pdf`: o agente sozinho venceu as duas tarefas no tempo, com a
  mesma nota; a equipe paralela perdeu pelo membro mais lento e pelo tempo fixo do líder. Isso vai nos custos dos
  modelos e na regra das skills.
- `[observado]` Teste de ponta a ponta no app real (Xvfb, build de release, terminais shell e um `claude` falso):
  - `team create` sem `--reason` foi recusado;
  - com `--reason`, criou Dev e Reviewer à direita, ligados como you ↔ Dev ↔ Reviewer, com as duas notas;
  - o `claude` recebeu `--model sonnet`, e modelo e equipe ficaram salvos na cena;
  - `solo` só criou a nota;
  - um membro tentando gerar equipe foi recusado;
  - o `list` mostrou "Automatic teams on".
  Roteiro: `docs/pesquisa/img/gerador-equipe.png` e o script descrito em `pesquisa/orquestracao.md` §8.

**Consequência.**
- Uma preferência nova e um campo `team` nos nós; o gerador roda no frontend (dono do grafo), sob o timeout de 10 s
  do `canvas_request`. Com `--isolate`, a criação dos andares entra nesse tempo.
- O custo de cada modelo de equipe é texto: precisa ser atualizado quando o benchmark medir mais formatos.

**Revisar se:** um formato medido no benchmark ganhar do agente sozinho (vira o padrão sugerido); a chave
liga/desliga se mostrar fraca (agentes montando equipes sem motivo) e precisar do estado "propor e confirmar"; ou a
geração precisar de mais de 10 s.

## D24 · "Pronto" é verificado pelo ambiente: `ask --check` roda o critério de aceite no backend

`2026-10-16` · comunicação entre agentes · `src-tauri/src/ipc.rs` (`run_check`), `src-tauri/src/asks.rs`

**Contexto.** O D20 resolveu quando o turno acaba, mas "pronto" continuava sendo a palavra do agente. Na
literatura, verificação é ~23,5% das falhas de sistemas multiagente (MAST), 75,8% das falhas em trajetórias de
código com declaração de status eram um "pronto" falso, e juízes LLM não o detectam (AUROC ≤ 0,65).

**Decisão.** `narrater ask … --check "CMD"`: quando o turno do alvo termina como `done`, o backend roda `CMD` com
`sh -c` no projeto do alvo (timeout de 10 min). Saída 0 acrescenta "Acceptance check passed" à resposta; qualquer
outra coisa termina o ask com o estado novo `failed`, com a saída do comando (até 3.000 caracteres finais) antes da
resposta do agente. Vale para `--batch` e `--no-wait`. Junto, o mesmo `ask` marca pedidos repetidos (mesmo autor,
mesmo alvo, última hora, mesmo texto ou 90% das palavras), sem bloquear, e avisa na resposta e no painel Activity.

**Alternativas consideradas.**
- *Pedir ao agente que rode os testes e diga o resultado*: continua sendo a palavra dele, e exige cooperação (D01).
- *Um agente juiz*: AUROC ≤ 0,65 na literatura, e custa um turno a mais.
- *Hook `TaskCompleted` do Claude* (Agent Teams): só para o Claude (D01).
- *Bloquear o pedido repetido*: refazer pode ser legítimo, por exemplo depois de um `failed`; o aviso basta.

**Evidência.**
- `[citado]` MAST (arXiv:2503.13657); Advani 2026 (arXiv:2606.09863); MetaGPT: rodar testes como retorno
  rendeu de 4 a 5 pontos.
- `[medido]` Testes de unidade: `run_check` com saída 0, código 3 com stdout e stderr, diretório e diretório
  inexistente; `same_prompt` e `repeat_of` (mesmo autor, outro autor, outro alvo, fora da janela).
- `[observado]` De ponta a ponta no app real:
  - `--check false` voltou `failed` com a resposta do agente depois;
  - `--check "python3 -m unittest -q"` voltou "passed";
  - um `ask` igual a outro de antes, mudando só a caixa e o espaço, veio com o aviso apontando o pedido anterior.

**Consequência.** O app passa a rodar um comando escolhido por um agente no projeto de outro. Isso não dá poder
novo, porque os agentes já têm shell, mas o comando roda com o ambiente do app, não no terminal do alvo.

**Revisar se:** algum agente usar `--check` para rodar coisas que não são verificação; ou um projeto precisar do
ambiente do terminal do alvo (virtualenv ativado, variáveis) para o check passar.
