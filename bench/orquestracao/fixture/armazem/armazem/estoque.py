"""Estoque com custo PEPS (REGRAS.md §3)."""

from decimal import ROUND_HALF_EVEN, Decimal

CENTAVO = Decimal("0.01")


class ErroEstoque(Exception):
    pass


class Estoque:
    def __init__(self):
        # sku -> lista de lotes [qtd, custo_unit], do mais antigo ao mais novo
        self._lotes = {}

    def entrada(self, sku, qtd, custo_unit):
        if not isinstance(qtd, int) or qtd <= 0:
            raise ValueError("qtd deve ser inteiro positivo")
        self._lotes.setdefault(sku, []).append([qtd, Decimal(str(custo_unit))])

    def saida(self, sku, qtd) -> Decimal:
        if not isinstance(qtd, int) or qtd <= 0:
            raise ValueError("qtd deve ser inteiro positivo")
        fila = self._lotes.setdefault(sku, [])
        if sum(l[0] for l in fila) < qtd:
            raise ErroEstoque(f"saldo insuficiente de {sku}")
        custo = Decimal("0")
        custo_lote = fila[0][1]
        falta = qtd
        while falta:
            lote = fila[0]
            usa = min(falta, lote[0])
            custo += usa * custo_lote
            lote[0] -= usa
            falta -= usa
            if lote[0] == 0:
                fila.pop(0)
        return custo.quantize(CENTAVO, ROUND_HALF_EVEN)

    def saldo(self, sku) -> int:
        return sum(l[0] for l in self._lotes.get(sku, []))

    def valor(self, sku) -> Decimal:
        lotes = self._lotes.get(sku, [])
        return sum((l[0] * l[1] for l in lotes), Decimal("0")).quantize(CENTAVO, ROUND_HALF_EVEN)
