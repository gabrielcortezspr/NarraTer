# Pedido: pedidos em várias moedas

A loja começou a vender para fora. Os pedidos agora podem vir em reais, dólares ou euros.

## Importação

- Nova coluna **opcional** `moeda` no CSV. Valores aceitos: `BRL`, `USD`, `EUR`, sem diferenciar
  maiúsculas (`usd` vale). Coluna ausente ou célula vazia significa `BRL`.
- Moeda fora dessa lista: `ErroImportacao` com o número da linha.
- Todas as linhas de um pedido têm a mesma moeda. Se não tiverem: `ErroImportacao` cuja mensagem
  contém o `id` do pedido.
- `Pedido` ganha o atributo `moeda` (texto em maiúsculas, padrão `"BRL"`); `preco_unit` continua
  na moeda do pedido.

## Câmbio

- Novo módulo `pedidos/cambio.py` com a classe `TabelaCambio`:
  - `TabelaCambio.carregar(caminho)` lê um CSV com as colunas `data, moeda, taxa` (`data` em
    `dd/mm/aaaa`; `taxa` com vírgula decimal, entre aspas, = quantos reais vale 1 unidade da moeda).
    Exemplo em `dados/cambio.csv`.
  - `tabela.taxa(moeda, data)` devolve um `Decimal`: a taxa da própria data ou, se não houver, a da
    **data anterior mais recente**. Para `BRL` é sempre `1`. Sem nenhuma taxa até aquela data:
    `ErroCambio` (subclasse de `ValueError`), com a moeda na mensagem.
- Conversão: cada linha do pedido vira reais como `qtd × preco_unit × taxa`, arredondada para
  centavos (meio-para-par). O subtotal em reais é a soma das linhas convertidas.
- **Todas as regras de REGRAS.md (faixas de desconto, imposto, frete) continuam em reais** e valem
  sobre os valores já convertidos.
- `preco.calcular(pedido, tabela=None)`: pedido em moeda estrangeira sem tabela → `ErroCambio`.
  Pedidos em BRL continuam funcionando sem tabela.

## Relatório

- Nova opção `--cambio CAMINHO` (a tabela de câmbio).
- Algum pedido fora de BRL e nenhuma `--cambio`, ou taxa inexistente: `erro: …` na saída de erro
  e código de saída 2 (como os outros erros).
- `--json` ganha dois campos em cada pedido: `moeda` (a original) e `subtotal_original` (soma de
  `qtd × preco_unit` na moeda original, arredondada para centavos, como texto). Os demais valores
  são em reais.
- A saída em texto ganha uma coluna com a moeda.

## Aceite

Os testes que já existem continuam passando; a funcionalidade nova tem testes próprios.
