---
name: pesquisa
description: Registra no repositório um estudo de outro projeto, ferramenta ou abordagem (ex. Maestri, Orca) para que a pesquisa não se perca entre sessões. Use quando o usuário pedir para estudar, analisar, comparar ou "ver como X funciona", ou quando uma sessão descobrir algo sobre outro projeto que influencie o NarraTer. Também use para atualizar um estudo existente.
---

# Pesquisa

Os estudos ficam em `docs/pesquisa/<assunto>.md`, um arquivo por projeto ou tema, e estão
listados em `docs/README.md`. O objetivo é que uma sessão futura consiga responder "por que
fizemos assim?" e "o que o X faz?" sem refazer a pesquisa.

## Antes de pesquisar

1. Leia `docs/README.md` e veja se já existe estudo sobre o assunto em `docs/pesquisa/`. Se
   existir, **atualize** esse arquivo em vez de criar outro.
2. Leia `docs/decisoes.md` para saber o que já foi decidido a partir de estudos anteriores.

## Como pesquisar

- **Prefira a fonte primária**: o código-fonte, a instalação local, a saída da própria CLI. Clone
  repositórios externos no scratchpad, nunca dentro do repositório do NarraTer.
- **Separe o que foi visto do que foi inferido.** Ler o código não é rodar o programa. Diga qual
  dos dois você fez. Use os mesmos rótulos de `docs/decisoes.md` nas afirmações que sustentam uma
  conclusão: `[medido]`, `[observado]`, `[citado]`, `[inferido]`.
- **Medições têm método.** Registre o comando exato, a versão/commit medida, a máquina e as
  repetições, para que o número possa ser reproduzido. Número sem método é `[inferido]`.
- **Cite caminhos e versões**: `src/main/runtime/orchestration/preamble.ts` na versão 1.4.214 vale
  mais que "o Orca faz assim". Projetos mudam; a citação permite conferir depois.
- Compare sempre com o NarraTer apontando arquivos dos dois lados.

## Formato do arquivo

```markdown
# NarraTer × <Projeto>: <subtítulo>

> Estudo feito em AAAA-MM-DD <como: lendo o código / rodando a versão X>. <Versão, licença,
> link>. <O que NÃO foi verificado.>

## TL;DR
<tabela curta de comparação + a conclusão em duas ou três frases>

## 1. Como <Projeto> funciona
## 2. Como o NarraTer faz hoje
## 3. Correlação / onde ganhamos / onde perdemos
## 4. O que incorporar (e o que não)
```

As seções podem crescer; o cabeçalho com data, método e limites é obrigatório.

**Atualizando um estudo**: acrescente uma seção nova com data (`## N. <tema> (AAAA-MM-DD)`) em vez
de reescrever as antigas, a menos que a antiga esteja factualmente errada. Nesse caso corrija e
diga no texto o que mudou.

## Depois de pesquisar

1. Se o arquivo é novo, adicione uma linha em `docs/README.md` (tabela de pesquisa).
2. **Conclusões que mudam o rumo viram decisão, com a evidência do estudo** em `docs/decisoes.md` (formato da skill
   `encerrar`), com o estudo citado na linha de referência. A pesquisa explica; a decisão fixa.
3. Itens de trabalho que saíram do estudo entram em `docs/backlog.md`, apontando para a seção do
   estudo.
4. Responda ao usuário com o resumo no chat; o arquivo é o registro, não a resposta.

## O que não fazer

- Não copiar trechos longos do projeto estudado; resuma e cite o caminho. Se for reaproveitar
  material (regras, textos), confira a licença e registre a atribuição.
- Não apresentar inferência como fato.
- Não criar um estudo por sessão sobre o mesmo projeto; um arquivo por assunto.
