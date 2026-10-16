# Regras de negócio

Valores em reais, com `Decimal`. Arredondamento para centavos sempre meio-para-par
(`ROUND_HALF_EVEN`). Os quatro módulos de `armazem/` são independentes: nenhum importa outro.

## 1. Frete (`armazem/frete.py`)

`cotar(cep, peso_kg, valor) -> Cotacao(regiao, tarifa, seguro, total)`

- O CEP pode vir com ou sem hífen. Depois de tirar o hífen, precisa ter exatamente 8 dígitos; senão,
  `ValueError`. A região sai dos 5 primeiros dígitos:

  | Região | Faixa (5 primeiros dígitos) |
  |---|---|
  | SE | 01000 a 39999 |
  | NE | 40000 a 65999 |
  | N | 66000 a 69999 |
  | CO | 70000 a 79999 |
  | S | 80000 a 99999 |

  Fora das faixas (ex. 00xxx): `ValueError`.
- Tarifa pela faixa do **peso cobrado** (os limites são inclusivos: 1 kg exato é "até 1 kg"):

  | Região | até 1 kg | até 5 kg | até 10 kg | por kg acima de 10 |
  |---|---|---|---|---|
  | SE | 15,00 | 22,00 | 35,00 | 3,00 |
  | S | 18,00 | 26,00 | 40,00 | 3,50 |
  | CO | 20,00 | 30,00 | 45,00 | 4,00 |
  | NE | 22,00 | 33,00 | 50,00 | 4,50 |
  | N | 28,00 | 42,00 | 60,00 | 6,00 |

  Acima de 10 kg: tarifa de "até 10 kg" + o valor por kg **ou fração** excedente (10,2 kg cobra 1 kg a mais).
- Hoje o peso cobrado é o peso informado.
- **Frete grátis** (tarifa 0,00) quando o valor do pedido é pelo menos 299,00, a região é SE ou S e o
  peso cobrado é no máximo 30 kg.
- **Seguro**: 0,5% do valor do pedido, arredondado para centavos. É cobrado **sempre**, mesmo com
  frete grátis.
- `total = tarifa + seguro`.

## 2. Cupons (`armazem/cupons.py`)

`aplicar(carrinho, cupom, hoje) -> Resultado(desconto, motivo)`

- O carrinho é uma lista de `Item(sku, categoria, preco, qtd)`; o valor de um item é `preco × qtd`.
- Itens **elegíveis**: os das categorias do cupom (`cupom.categorias`; `None` = todas).
  Categorias são comparadas sem diferenciar maiúsculas.
- O cupom vale **até a data de validade, inclusive**.
- O **mínimo** do cupom é comparado com a soma dos itens elegíveis, não com o carrinho inteiro.
- `percentual`: `valor`% da soma dos itens elegíveis, arredondado para centavos e limitado ao
  `teto` (se houver).
- `fixo`: `valor` em reais, mas nunca maior que a soma dos itens elegíveis.
- Recusa: desconto 0,00 e `motivo` igual a `"expirado"`, `"sem_itens"` (nenhum item elegível) ou
  `"minimo"`, verificados nessa ordem. Aceito: `motivo` é `None`.
- `buscar(catalogo, codigo)` acha o cupom pelo código sem diferenciar maiúsculas e ignorando espaços
  nas pontas; `None` se não existir.

## 3. Estoque (`armazem/estoque.py`)

Custo pelo método **PEPS (FIFO)**: cada entrada vira um lote `(qtd, custo_unit)`; a saída consome os
lotes mais antigos primeiro.

- `entrada(sku, qtd, custo_unit)`; `qtd` inteiro positivo, senão `ValueError`.
- `saida(sku, qtd) -> Decimal`: devolve o custo da saída, a soma de `qtd × custo_unit` de cada lote
  consumido, **arredondada para centavos só no final**. Uma saída maior que o saldo levanta
  `ErroEstoque`, e o estoque **não muda**.
- `saldo(sku) -> int`; `valor(sku) -> Decimal` (soma dos lotes restantes, arredondada no final).
  SKU nunca visto: saldo 0 e valor 0,00.

## 4. Parcelas (`armazem/parcelas.py`)

`parcelar(total, n, primeira) -> list[Parcela(numero, vencimento, valor)]`

- `n` de 1 a 12; fora disso, `ValueError`.
- Sem juros: cada parcela é `total / n` arredondado **para baixo** em centavos; os centavos que
  sobram vão para a **primeira** parcela. A soma das parcelas é sempre o total.
- Vencimentos: a parcela 1 vence em `primeira`; a parcela k vence no mesmo dia do mês, k − 1 meses
  depois. Quando o mês não tem esse dia, vence no último dia do mês. O dia de referência é sempre
  o de `primeira` (31/01 → 28/02 → 31/03), nunca o da parcela anterior.
- Vencimento em sábado ou domingo passa para a segunda-feira seguinte. O ajuste vale só para aquela
  parcela; a seguinte continua calculada a partir de `primeira`.
