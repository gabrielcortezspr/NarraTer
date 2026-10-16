# Comparação de versões: custo da orquestração

Estudo de 2026-10-15. Relatório completo, com método, tabelas, gráficos e ameaças à validade:
[`../relatorios/orquestracao-versoes.pdf`](../relatorios/orquestracao-versoes.pdf) (fonte `.tex` ao lado).
Como repetir: skill `benchmark` e `bench/orquestracao/`. Dados brutos (métricas por rodada e
transcrições compactadas): `bench/orquestracao/resultados/2026-10-15/`.

## Pergunta

Quanto custou ou rendeu cada mudança no jeito de orquestrar, em **tokens, contexto, eficiência e
trabalho feito**? Quatro versões:

- **A** `e708d7b`: MCP cooperativo, só com Claude;
- **B** `16cee0e`: CLI e skill agnósticas, `ask` lido da tela;
- **C** `95fb11a`: `ask` assíncrono e durável;
- **D** `db3d535`: leitor de estado novo e statusline.

## Método, em uma linha

Mesma tarefa em todas as rodadas: o repositório inventado `pedidos`, com uma funcionalidade de várias
moedas e dois defeitos plantados. Três agentes com modelos fixos: Narrador Opus 5.5, DevA Sonnet 5.5 e
DevB Haiku 5.5. A nota vem de 31 testes escondidos. São 3 rodadas por versão, cada uma começando
zerada, em ordem contrabalançada. As métricas saem das transcrições do Claude Code.

## Resultados

| | A | B | C | D |
|---|---|---|---|---|
| Testes escondidos | 31/31 nas 3 | 31/31 | 31/31 | 31/31 |
| Duração (min) | 2,2 (2,2–2,2) | 2,9 (2,4–3,7) | 3,5 (2,8–4,2) | 4,6 (3,9–5,6) |
| Tokens processados (milhões) | 1,84 | 2,10 | 2,50 | 3,44 |
| Custo de API equivalente (US$) | 0,69 | 0,73 | 0,85 | 1,02 |
| Pedidos do Narrador aos devs | 3–5 | 3–6 | 4–8 | 7–10 |
| Repetidos por resposta cortada | 0, 0, 0 | 1, 1, 0 | 2, 1, 3 | 1, 5, 1 |
| Detecção do fim do turno (s) | — | 1,7–3,1 | 1,6–2,3 | 1,6–1,9 |
| Contexto inicial por agente (mil) | 31–33 | 30–33 | 30–33 | 30–33 |

## O que aprendemos

1. `[medido]` **A qualidade bateu no teto**: as 12 rodadas tiraram 31/31. A tarefa mede custo, não
   capacidade. Para separar versões pela qualidade, o próximo estudo precisa de uma tarefa mais difícil.
2. `[medido]` **A latência do protocolo é de segundos; a diferença está nas rodadas de trabalho.**
   - a entrega leva de 0,2 a 0,5 s, e a detecção de 1,6 a 3,1 s;
   - a duração cresce em minutos, junto com o número de pedidos do Narrador e de turnos dos devs;
   - nenhum `ask` deu timeout nem "precisa de você".
3. `[medido]` **A resposta do `ask` lida da tela chega cortada.** Isso aconteceu em todas as rodadas de C
   e D e em duas de B, e o Narrador precisou pedir de novo. Na A, o dev manda o relatório inteiro por
   mensagem, e isso não aconteceu nenhuma vez. É o único mecanismo que aparece rodada a rodada.
4. `[medido]` **O custo fixo do protocolo é pequeno.** Um agente começa com 30 a 33 mil tokens,
   quase tudo do próprio Claude Code. O MCP mais o prompt anexado da A pesam cerca de 0,7 mil a mais
   que a skill. Não houve nenhuma compactação, e o pico foi de 93 mil tokens.
5. `[medido]` **A < D em tempo, tokens, custo, pedidos e turnos**, sem sobreposição: Mann–Whitney
   U = 0, p = 0,10, o menor p possível com n = 3. Entre versões vizinhas há sobreposição. C e D
   diferem nos tokens (D sempre acima), sem mecanismo identificado. `[inferido]` Os candidatos são a
   statusline mudando a tela que vira resposta e a variação da estratégia do Narrador.
6. `[inferido]` **A versão A não é a recomendação**, porque exige MCP e prompt de sistema e prende o
   NarraTer ao Claude (D01). O que ela ensina cabe no protocolo agnóstico: devolver no `ask` o texto
   inteiro do último turno, lido do scrollback do emulador e não só da tela visível. Isso está no
   backlog como O5 (já planejada; agora com evidência).

## O que fica em aberto

- Mais rodadas (≥ 8 por versão) para separar versões vizinhas.
- Uma tarefa mais difícil (sem teto) e agentes de outros fornecedores, para medir o que B–D ganham
  com a D01.
- Repetir depois de O5 para ver se as repetições somem.

## Depois do estudo: E, F e G (2026-10-15, à tarde)

Cada versão nova corrige uma falha vista nas rodadas da anterior. O método é o mesmo, com 3 rodadas por
versão, e o relatório tem a seção 7 com tabelas e testes.

| | D | E | E sem statusline | F | G |
|---|---|---|---|---|---|
| O que muda | — | `ask` lê a resposta no transcript (O5) | E sem statusline | transcript também decide o fim do turno | cada ask acha o próprio turno (batch) |
| Duração (min) | 4,6 | 5,3 | 2,7 | 2,8 | 3,9 |
| Tokens processados (milhões) | 3,44 | 2,94 | 1,77 | 1,73 | 2,50 |
| Custo (US$) | 1,02 | 0,97 | 0,73 | 0,65 | 0,74 |
| Pedidos do Narrador | 7–10 | 5–14 | 3–4 | 3–7 | 3–6 |
| Asks prontos antes da hora | 4/27 | 12/31 | 3/14 | 0/16 | 0/24 |
| Repetidos por resposta cortada | 1, 5, 1 | 0, 2, 1 | 0, 0, 0 | 0, 1, 0 | 0, 0, 0 |

1. `[medido]` **A E achou uma falha maior que a tela cortada: o ask pronto antes da hora.** Ele voltava 4 a 5 s
   depois da entrega, com a tela mostrando o próprio pedido. Na E2, isso aconteceu em 8 de 16 asks, e o
   Narrador deixou de confiar no `ask`: 14 pedidos e 10 minutos. Isso já acontecia na C e na D, então parte
   das "respostas cortadas" do estudo eram respostas antes da hora.
2. `[medido]` **Na F e na G, as falhas do protocolo foram a zero** (0 de 40 asks antes da hora; 0 respostas
   cortadas na G). O custo voltou ao nível da A, sem prender o NarraTer ao Claude. D contra G não se sobrepõe
   em tokens, custo e pedidos.
3. `[medido]` **O que ainda varia é o trabalho dos agentes.** Na G3, o Narrador ficou sozinho 59 s; o resto foi
   o DevB (Haiku) trabalhando 243 s seguidos. O próximo ganho é tirar o Narrador do caminho entre as fases
   (O8).
4. `[inferido]` **Statusline: suspeita não confirmada** (E contra E sem statusline, p = 0,20, com a E2 inflada pela
   falha 1). Fica para a O9.


## Equipe contra agente sozinho (2026-10-16)

O mesmo pedido a um terminal só (Opus 5.5, 3 rodadas) tirou 31/31 em 1,6 min, com 0,36 milhão de tokens e
US$ 0,55. A equipe G levou 3,9 min, com 2,50 milhões e US$ 0,74. `[medido]` Nenhuma das métricas se sobrepõe.
Detalhes em `orquestracao.md` §6 e `../relatorios/quando-orquestrar.pdf`; decisão D22.
