# Regras de negócio

Valores em reais, com `Decimal`. Arredondamento sempre para centavos, meio-para-par
(`ROUND_HALF_EVEN`).

## 1. Importação

O CSV tem uma linha por item, com as colunas `id, cliente, data, sku, qtd, preco_unit, categoria`.

- `data` no formato `dd/mm/aaaa`; `preco_unit` com vírgula decimal e sem separador de milhar
  (entre aspas no CSV, ex. `"4,50"`).
- `qtd` é inteiro positivo; `categoria` é comparada sem diferenciar maiúsculas.
- As linhas de um pedido **podem aparecer em qualquer ordem no arquivo, inclusive intercaladas com
  linhas de outros pedidos**: todas as linhas com o mesmo `id` formam um único pedido. Os pedidos
  saem na ordem da primeira linha de cada um. `cliente` e `data` vêm da primeira linha do pedido.
- Erro de formato: `ErroImportacao` com o número da linha na mensagem.

## 2. Desconto progressivo

Calculado sobre o subtotal (soma de `qtd × preco_unit` de cada item, cada linha arredondada para
centavos), **por faixas, como uma tabela de imposto de renda**: cada alíquota vale só para a parte
do subtotal dentro da sua faixa.

| Faixa do subtotal | Desconto sobre a parte na faixa |
|---|---|
| até 500,00 | 0% |
| de 500,00 a 1.000,00 | 5% |
| acima de 1.000,00 | 8% |

Exemplo: subtotal 1.200,00 → 0 + 5% × 500 + 8% × 200 = 41,00.

## 3. Imposto

12% sobre os itens tributáveis. A categoria `livros` é isenta. O desconto reduz a base de forma
proporcional: imposto = 12% × (soma dos itens tributáveis) × (subtotal − desconto) / subtotal.

## 4. Frete

25,00 por pedido; grátis quando `subtotal − desconto` é pelo menos 300,00.

## 5. Total

total = subtotal − desconto + imposto + frete.

## 6. Relatório

`python -m pedidos.relatorio ARQUIVO.csv [--cliente NOME] [--json]`

- `--cliente` filtra pelo nome exato, sem diferenciar maiúsculas e ignorando espaços nas pontas.
- `--json` imprime uma lista com `id, cliente, data (ISO), itens, subtotal, desconto, imposto,
  frete, total` (valores como texto, ex. `"41.00"`).
- Erro de importação ou arquivo ausente: mensagem `erro: …` na saída de erro e código de saída 2.
