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
