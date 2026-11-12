"""Estoque com custo PEPS (REGRAS.md §3, docs/PEDIDOS.md §3)."""

import itertools
from decimal import ROUND_HALF_EVEN, Decimal

CENTAVO = Decimal("0.01")


class ErroEstoque(Exception):
    pass


class Estoque:
    def __init__(self):
        # (deposito, sku) -> lista de lotes [ordem, qtd, custo_unit], do mais antigo ao mais novo
        self._lotes = {}
        self._seq = itertools.count()

    def _fila(self, sku, deposito):
        return self._lotes.setdefault((deposito, sku), [])

    def entrada(self, sku, qtd, custo_unit, deposito="principal"):
        if not isinstance(qtd, int) or qtd <= 0:
            raise ValueError("qtd deve ser inteiro positivo")
        self._fila(sku, deposito).append([next(self._seq), qtd, Decimal(str(custo_unit))])

    def _retira(self, sku, qtd, deposito):
        """Tira qtd por PEPS e devolve os pedaços [(ordem, qtd, custo)]; não muda nada se faltar."""
        if not isinstance(qtd, int) or qtd <= 0:
            raise ValueError("qtd deve ser inteiro positivo")
        fila = self._fila(sku, deposito)
        if sum(l[1] for l in fila) < qtd:
            raise ErroEstoque(f"saldo insuficiente de {sku} em {deposito}")
        pedacos, falta = [], qtd
        while falta:
            lote = fila[0]
            usa = min(falta, lote[1])
            pedacos.append((lote[0], usa, lote[2]))
            lote[1] -= usa
            falta -= usa
            if lote[1] == 0:
                fila.pop(0)
        return pedacos

    @staticmethod
    def _custo(pedacos):
        return sum((q * c for _, q, c in pedacos), Decimal("0")).quantize(CENTAVO, ROUND_HALF_EVEN)

    def saida(self, sku, qtd, deposito="principal") -> Decimal:
        return self._custo(self._retira(sku, qtd, deposito))

    def transferir(self, sku, qtd, origem, destino) -> Decimal:
        if origem == destino:
            raise ValueError("origem igual ao destino")
        pedacos = self._retira(sku, qtd, origem)
        fila = self._fila(sku, destino)
        fila.extend([o, q, c] for o, q, c in pedacos)
        fila.sort(key=lambda l: l[0])
        return self._custo(pedacos)

    def saldo(self, sku, deposito="principal") -> int:
        return sum(l[1] for l in self._lotes.get((deposito, sku), []))

    def valor(self, sku, deposito="principal") -> Decimal:
        lotes = self._lotes.get((deposito, sku), [])
        return sum((l[1] * l[2] for l in lotes), Decimal("0")).quantize(CENTAVO, ROUND_HALF_EVEN)

    def saldo_total(self, sku) -> int:
        return sum(l[1] for (dep, s), lotes in self._lotes.items() if s == sku for l in lotes)
