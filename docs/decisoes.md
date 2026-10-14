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
| Leitor de estado | D10, D11, D12 |
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
