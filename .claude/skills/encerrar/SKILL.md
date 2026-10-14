---
name: encerrar
description: Rotina de fechamento de sessão do NarraTer. Registra as decisões da sessão em docs/decisoes.md, atualiza a etapa em docs/backlog.md, reescreve docs/estado.md com onde parou, roda as verificações e propõe a mensagem de commit. Use quando o usuário disser /encerrar, "vou parar por aqui", "fechar a sessão", "salvar onde paramos" ou antes de commitar o fim de uma sessão de trabalho.
---

# Encerrar sessão

Objetivo: a próxima sessão (sua, de outro agente ou de outra pessoa) começar sabendo exatamente
onde esta parou. O hook `SessionStart` imprime `docs/estado.md` na abertura, então o que você
escrever aqui é a primeira coisa que o próximo agente vai ler.

## Passos

1. **Resuma a sessão em duas ou três linhas** para o usuário: o que foi feito, o que foi
   verificado, o que ficou pela metade. Sem lista de arquivos; isso o `git log` conta.

2. **Registre as decisões da sessão em `docs/decisoes.md`.** Uma entrada por decisão que teve
   **alternativa real**: escolha de desenho, troca de abordagem, conclusão de pesquisa que mudou o
   rumo. Se não havia escolha, não é decisão.

   Numeração sequencial e permanente (`D07`, `D08`…), os quatro campos fixos (Contexto, Decisão,
   Alternativa recusada, Consequência) e a linha de referência com data, área, commit e o arquivo
   onde a regra vive. Atualize o índice por área no topo.

   **O arquivo só cresce.** Decisão revista ganha entrada nova citando a antiga; nunca edite nem
   apague a original. O campo que mais importa é *Alternativa recusada*: é o que impede alguém de
   "consertar" a decisão daqui a um mês.

   Na dúvida, o teste é: "se alguém mexer nisso em três semanas sem saber o porquê, quebra algo
   ou repete um erro?" Se sim, registre.

3. **Atualize a etapa em `docs/backlog.md`.** Uma linha de citação sob o título da etapa:
   concluída com data e commits, ou em andamento com o que já entrou. Não reescreva o plano da
   etapa; a diferença entre plano e execução é informação. Etapa nova que surgiu na sessão entra
   no fim, com o mesmo formato.

4. **Se a sessão produziu pesquisa** (estudo de outro projeto, comparação, medição), confira que
   ela está em `docs/pesquisa/` no formato da skill `pesquisa` e listada em `docs/README.md`.

5. **Reescreva `docs/estado.md` inteiro** (nunca acrescente), mantendo as cinco seções fixas:
   - `Última sessão`: data, quem e o branch.
   - `Onde parou`: estado factual, no máximo quatro linhas.
   - `Próximo passo`: uma ação concreta, com o arquivo por onde começar. A *primeira* coisa a
     fazer, não uma lista de desejos.
   - `Decisões pendentes`: só o que bloqueia ou muda o rumo. Resolvido sai.
   - `Lembrar`: armadilhas de curto prazo. Regra permanente vai para `CLAUDE.md`.

   Se passar de ~30 linhas, algo ali pertence a um commit, a `CLAUDE.md` ou a `docs/`.

6. **Se a sessão mexeu em código, rode as verificações** antes de propor commit:

   ```sh
   cd src-tauri && cargo test     # backend
   npx tsc --noEmit               # frontend
   ```

   Se mexeu na detecção de turno ou na entrega de prompt (`screen.rs`, `pty.rs`, `ipc.rs`) e há
   um agente instalado, rode também o `turn_probe` (ver `docs/narrater-cli.md`). Falha não fecha
   sessão: ou corrige, ou vira linha em `Lembrar` com o motivo de ter ficado. Se uma verificação
   não pôde rodar (falta de dependência do sistema, por exemplo), diga isso explicitamente.

7. **Confira o `git status`.** Se houver mudança sem commit, liste em uma linha o que é e proponha
   uma mensagem de commit em português (`tipo: descrição` + corpo com o porquê). **Não commite sem
   o usuário pedir.** Se o working tree estiver limpo, diga isso.

8. **Feche com uma linha**: o próximo passo, e como retomar (abrir `claude` no diretório; o hook
   faz o resto).

## O que não fazer

- Não duplicar no `estado.md` o que está no `git log`, em `CLAUDE.md` ou em `decisoes.md`.
- Não transformar `decisoes.md` em changelog. Lista de arquivos é trabalho do `git log`; ali vai
  só o raciocínio. Sessão sem bifurcação real não gera entrada, e está tudo bem.
- Não reescrever entrada antiga de `decisoes.md`, nem para corrigir o português.
- Não reescrever `CLAUDE.md` por reflexo. Só se uma *regra permanente* mudou nesta sessão.
- Não deixar o `estado.md` descrever o passado em detalhe. Ele aponta para frente.
