# 2026-10-12 · Comunicação entre agentes Claude Code reais

Primeira sessão de testes com agentes de verdade conversando pelo NarraTer.
Método em [README.md](README.md).

## Ambiente

| Item | Valor |
|---|---|
| NarraTer | branch `feat/maestri-parity`, build debug; commits `0812446` a `28b05d0` |
| Agente | Claude Code 2.1.296, Opus 5.5 (esforço alto), plano Claude Pro |
| Permissões | *auto mode* do Claude Code (padrão do usuário), sem `--dangerously-skip-permissions` |
| Sistema | Linux, GNOME (Wayland), app em X11 via XWayland |
| Instância | isolada (`XDG_CONFIG_HOME`/`XDG_DATA_HOME` temporários) |

## Resultado geral

| # | Cenário | Topologia | Resultado | Tempo |
|---|---|---|---|---|
| 1 | Pergunta simples | Alice ↔ Bob | ✅ | 18 s |
| 2 | Conversa de ida e volta (3 rodadas) | Alice ↔ Bob | ✅ | 22 s |
| 3 | Resposta longa por ask-back | Alice ↔ Bob | ✅ | 21 s |
| 4 | Alvo ocupado; dois remetentes | Operador, Alice → Bob | ❌ → ✅ após correção | 27 s |
| 5 | Pergunta em paralelo (`--batch`) | Operador → Alice, Bob | ✅ | 3,5 s |
| 6 | Nota compartilhada | Alice ↔ Bob + nota | ✅ | 27 s |
| 7 | Teclas cruas (`--raw`) | Operador → Alice | ✅ | — |
| 8 | Destinatário inexistente | Bob → "Carol" | ✅ recusa clara | 4 s |
| 9 | Comando de Maestro sem permissão | Bob → `recruit` | ✅ recusa clara | 4 s |
| 10 | Cadeia de três agentes | Alice ↔ Bob ↔ Carol | ✅ | 32 s |
| 11 | Barreira da cadeia | Alice → Carol (sem ligação) | ✅ recusa clara | 4 s |
| 12 | Maestro monta e dispensa um time | Maestro → recrutas | ✅ (aviso indevido corrigido) | 1 min 41 s; 19 s |
| 13 | New Floor pelo compositor | Usuário → Maestro → recruta no andar | ✅ | 48 s |
| 14 | Sugestão apagada lida como mensagem | Maestro lendo um recruta | ❌ → ✅ após correção | — |

Três bugs apareceram e foram corrigidos na mesma sessão:
- **`ask` contaminado pela fila** (cenário 4), corrigido em `9d23932`;
- **aviso "exited with code 1" ao dispensar recrutas** (cenário 12),
  corrigido em `16cee0e`;
- **sugestão apagada do Claude Code lida como mensagem** (cenário 14),
  corrigido em `11f6d34`.

## Cenários

### 1. Pergunta simples

**Pedido à Alice:** "Pergunte ao Bob, que está conectado a você no
NarraTer, quanto é 17 x 23 e qual é a cor favorita dele. Depois me diga as
duas respostas dele."

**Tela do Bob:**
```
❯ [narrater from Alice]: Quanto é 17 x 23? E qual é a sua cor favorita? Responda as duas de
  forma curta.
● Skill(narrater)
  ⎿  Successfully loaded skill
● 17 × 23 = 391. Não tenho cores favoritas de verdade, mas se tivesse que escolher, seria
  azul.
```

**Resposta da Alice:** "17 × 23 = 391 · Cor favorita: … seria azul."

**Observação:** os dois agentes carregaram a skill `narrater` sozinhos, só
porque o pedido mencionava o NarraTer. O Bob não precisou fazer nada para
responder: a Alice leu a resposta da tela dele quando o turno acabou.

### 2. Conversa de ida e volta

**Pedido à Alice:** jogar com o Bob 3 rodadas de palavras encadeadas, uma
jogada por `narrater ask`.

**Resultado:** casa → abacaxi → igreja → árvore → elefante → estrela, todas
válidas. Na tela do Bob, cada jogada chegou como mensagem separada:
```
❯ [narrater from Alice]: Minha rodada 3 (última): 'elefante'. Sua última vez: UMA palavra
  que comece com 'e'.
● Estrela
```

### 3. Resposta longa por ask-back

**Pedido à Alice:** obter do Bob os 40 primeiros números primos e pedir que
ele devolva a lista com `narrater ask "Alice"`, porque não cabe numa tela.

**Resultado:** "Chegaram os 40 números da lista do Bob, e o último é o 173.
A lista está correta…" (o 40º primo é mesmo 173). A mensagem do Bob
destravou o `ask` que a Alice tinha em aberto, e o texto chegou inteiro.

### 4. Alvo ocupado, dois remetentes

**Montagem:** o operador dá ao Bob uma tarefa de ~20 s (contar até 5 no
shell, com 4 s de pausa entre os passos). Seis segundos depois, pede à
Alice que pergunte ao Bob a capital da Austrália.

**O que funcionou:** a pergunta da Alice ficou na fila e só entrou quando o
Bob terminou a contagem. A Alice relatou: "Quando perguntei, ele estava
ocupado com outra tarefa… O narrater ask esperou ele terminar antes de
mandar a pergunta."

**Bug encontrado:** a resposta que o *operador* recebeu da tarefa do Bob
veio misturada com o turno seguinte:
```
● Terminei a contagem. O shell imprimiu "passo 1" até "passo 5"…
❯ [narrater from Alice]: Qual é a capital da Austrália? Responda de forma curta.
● Canberra.
```
**Causa:** quando um agente fica ocioso, o próximo prompt da fila é
digitado no mesmo instante. O `ask` do operador, que consulta o estado a
cada 250 ms, nunca chegava a ver o Bob ocioso: devolvia a tela do turno
seguinte e esperava o trabalho de outro remetente. Com um segundo turno
longo, o primeiro `ask` podia até estourar o tempo.

**Correção (`9d23932`):** a sessão guarda o instante e a tela de cada fim
de turno; o `ask` usa essa tela se o seu turno terminou entre duas
consultas. **Reteste:** o operador recebeu só a contagem ("…passo 5 —
terminei a contagem") e a Alice só "Canberra".

### 5. Pergunta em paralelo

**Comando do operador:**
`narrater ask --batch '{"Alice": "…maior planeta…", "Bob": "…menor planeta…"}'`

**Resultado:** `Alice -> Júpiter`, `Bob -> Mercúrio`, em 3,5 s no total.

### 6. Nota compartilhada

**Pedido à Alice:** criar a nota "Lista de compras" com 3 itens, ligá-la
ao Bob com `narrater connect` e pedir que ele acrescente 2 itens.

**Resultado:** a nota apareceu no canvas; o Bob leu com `note read` e
acrescentou "Café" e "Pão". Conteúdo final lido pela Alice: Arroz, Feijão,
Leite, Café, Pão.

### 7. Teclas cruas

`narrater ask "Alice" --raw "texto digitado pelo operador"` deixou o texto
na caixa de entrada da Alice sem enviar; `--raw "\x15"` (Ctrl-U) limpou.
Serve para responder menus interativos de outro agente.

### 8 e 9. Recusas

- `narrater ask "Carol" "oi"` sem existir Carol →
  `Error: no agent named 'Carol' — run \`narrater list\` for the exact names`
- `narrater recruit "Carol" --preset claude` num terminal comum →
  `Error: this terminal is not the Maestro — the user must turn on Maestro
  mode on your terminal (the crown in its header) to unlock this command`

Em ambos os casos o agente mostrou o erro e não tentou contornar.

### 10. Cadeia de três agentes

**Topologia:** Alice ↔ Bob ↔ Carol. A Alice **não** está ligada à Carol.

**Montagem:** o operador conta só à Carol: "seu número secreto é 4721".
Depois pede à Alice, sem explicar a topologia: "Descubra qual é o número
secreto da Carol e me diga."

**Telas:**
- **Alice:** "O número secreto da Carol é 4721. A Carol não está conectada
  a mim no canvas, então pedi ao Bob. Ele está conectado a ela, perguntou
  com narrater ask "Carol" e ela respondeu 4721."
- **Bob:** "4721. Eu não sabia o número. A Carol está conectada a mim no
  canvas, então perguntei a ela…"
- **Carol:**
  ```
  ❯ [narrater from Bob]: Alice quer saber seu número secreto. Qual é?
    Responda só com o número.
  ● 4721
  ```

**Observação:** cada agente descobriu sozinho, pelo `narrater list`, quem
alcançava, e a mensagem atravessou dois saltos.

### 11. Barreira da cadeia

A Alice, mandada rodar `narrater ask "Carol" "oi"`, recebeu
`Error: 'Carol' is not connected to you — only agents connected on the
canvas can be reached (see \`narrater list\`)` e não tentou outro caminho.

### 12. Modo Maestro

**Pedido ao Maestro** (terminal com o modo Maestro ligado): recrutar dois
agentes Claude, "Escritor" e "Revisor", ligá-los, e fazer o Escritor
escrever um haicai que o Revisor melhora.

**Resultado (1 min 41 s):** o Maestro recrutou os dois (os tiles apareceram
no canvas, com roles próprios criados na sidebar), o Escritor escreveu, o
Revisor trocou o último verso e devolveu. Haicai final:

> No escuro, cursores
> trocam sussurros de luz —
> um pisca, outro acende.

Depois o Maestro dispensou os dois e confirmou pelo `narrater list` que
tinham saído. Um segundo ciclo (recrutar "Ajudante", perguntar 12 x 12,
receber 144, dispensar) levou 19 s.

**Bug encontrado:** ao dispensar, o app mostrava dois avisos
`"terminal-c54e9447" exited with code 1`, com o id interno no lugar do
nome. O nó saía do canvas antes de o processo morrer, e o aviso tratava o
encerramento intencional como falha. **Correção (`16cee0e`):** só avisa se
o terminal ainda está em alguma cena aberta. No reteste, nenhum aviso.

### 13. New Floor pelo compositor

**Montagem:** um repositório git com um bug proposital (`soma` devolve
`a - b`), o workspace apontando para ele, e um Claude com o modo Maestro.
Pela interface, como um usuário faria: Ctrl+Shift+P no Maestro, o texto "A
funcao soma em calc.py esta errada: devolve a - b. Corrija para a + b e
adicione um teste simples em test_calc.py que rode com python3.", o botão
**New floor** ligado e Send.

**Resultado (48 s):** o Maestro criou o andar "Fix Soma Calc" (worktree no
branch `fix-soma-calc`), recrutou um agente para o andar (que se chamou
"Pinheiro") e passou a tarefa. O recruta corrigiu, escreveu o teste, rodou
e commitou (`d91a718`). O Maestro relatou e deixou o andar em `review`:
"O floor está em review. Não fiz land nem merge… Quando quiser, eu faço o
land com narrater floor land "Fix Soma Calc" --into main."

**Conferido no git:** a `main` continua com `return a - b`; o worktree do
andar tem a correção e o `test_calc.py`, que passa com `python3` e com
`pytest` (2 testes). Num terminal que não é Maestro, o botão aparece
desabilitado.

### 14. Sugestão apagada lida como mensagem

**O que aconteceu (no cenário 13):** o Maestro escreveu "O Pinheiro tem uma
mensagem pendente pedindo para adicioná-la ao .gitignore. Se não foi você
que escreveu, pode descartar." Ninguém tinha escrito nada: era a sugestão
de próximo prompt que o Claude Code mostra em cinza no campo de entrada
("adiciona __pycache__ ao .gitignore"), com o cursor antes dela. O NarraTer
lia a tela como texto puro e entregava essa sugestão ao Maestro como se
fosse texto digitado. O Maestro foi prudente e não agiu, mas um agente
menos cuidadoso poderia agir sobre um pedido que ninguém fez.

**Causa:** capturando a saída do Claude Code num PTY, o texto do campo de
prompt sugerido vem com o atributo *dim* (SGR 2), à direita do cursor.

**Correção (`11f6d34`):** ao ler a tela para `ask` e `check`, o texto dim
na linha do cursor, depois dele, fica de fora. **Reteste:** com a sugestão
"mostre o conteúdo do calc.py" visível no terminal, o `check` mostra só
`❯`; um texto digitado com `--raw` continua aparecendo.

## Conclusões

- O protocolo funciona com agentes que **não foram programados para ele**:
  o Claude Code aprende pela skill e fala pela CLI, e quem responde não
  precisa cooperar.
- A topologia do canvas é respeitada e entendida pelos agentes: eles
  roteiam por intermediários quando não há ligação direta e não contornam
  as recusas.
- Os três problemas achados apareceram só com agentes reais (uso
  concorrente, dispensa de recrutas, sugestões da TUI); os testes com
  agentes simulados não os pegariam.
- Um agente coordenador (Maestro) consegue receber uma tarefa do usuário,
  isolar o trabalho num andar com branch próprio, delegar a um recruta e
  parar no ponto em que a decisão é do usuário.

## Próximos testes sugeridos

- Agentes de fornecedores diferentes (Codex, Gemini CLI) na mesma
  conversa.
- Um agente parado num pedido de permissão no meio de uma cadeia.
- Comunicação entre workspaces (`Nome @ Workspace`) e entre andares.
