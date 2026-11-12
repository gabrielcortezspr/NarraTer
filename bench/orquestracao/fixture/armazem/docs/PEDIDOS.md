# Pedidos de funcionalidade (abertos)

Os quatro são independentes: cada um mexe só no próprio módulo e nos testes dele. As regras atuais
continuam valendo (`REGRAS.md`); o que muda está descrito aqui.

## 1. Frete: peso cúbico

- `cotar(cep, peso_kg, valor, dimensoes=None)`: `dimensoes` é `(comprimento, largura, altura)` em
  centímetros.
- Peso cúbico = comprimento × largura × altura / 6000, em kg.
- **Peso cobrado** = o maior entre o peso informado e o peso cúbico, arredondado **para cima** ao
  próximo múltiplo de 0,5 kg (1,0 continua 1,0; 1,01 vira 1,5). O arredondamento vale também
  quando não há dimensões.
- Todas as regras que hoje usam o peso (faixas, excedente acima de 10 kg, limite do frete grátis)
  passam a usar o peso cobrado.
- `Cotacao` ganha o campo `peso_cobrado` (Decimal, em kg).

## 2. Cupons: leve N, pague M

- Novo tipo de cupom, `"leve_pague"`, com os campos `leve` e `pague` (ex. leve 3, pague 2).
- Para cada SKU elegível, a cada `leve` unidades, `leve − pague` saem de graça:
  desconto do SKU = (unidades // leve) × (leve − pague) × preço.
- Linhas do carrinho com o mesmo SKU contam juntas (as quantidades se somam). Se tiverem preços
  diferentes, vale o **menor** preço entre elas.
- Validade, itens elegíveis, mínimo e teto valem como nos outros tipos.

## 3. Estoque: depósitos e transferência

- `entrada`, `saida`, `saldo` e `valor` ganham o argumento nomeado `deposito="principal"`. Cada
  depósito tem os próprios lotes.
- `transferir(sku, qtd, origem, destino) -> Decimal`: tira da origem por PEPS, com as mesmas regras
  da saída (inclusive não mudar nada se faltar saldo), e os lotes entram no destino **com o custo
  original**. Devolve o custo transferido, arredondado para centavos no final. Origem igual ao
  destino: `ValueError`.
- No destino, um lote transferido mantém a antiguidade da entrada original: se ele entrou no
  sistema antes de um lote que já estava no destino, é consumido antes dele.
- `saldo_total(sku) -> int`: soma dos saldos de todos os depósitos.

## 4. Parcelas: juros acima de 3 parcelas

- Até 3 parcelas, nada muda (sem juros).
- De 4 a 12 parcelas: tabela Price com taxa de 1,99% ao mês:
  PMT = total × i / (1 − (1 + i)^−n).
  As parcelas 1 a n − 1 valem PMT arredondado para centavos; a última vale
  `arredonda(PMT × n) − soma das anteriores`. (Os centavos que sobram ficam na última, não na
  primeira como no caso sem juros.)
- Nova função `total_pago(total, n) -> Decimal`: a soma das parcelas que `parcelar` devolveria.
- Os vencimentos seguem as regras atuais.
