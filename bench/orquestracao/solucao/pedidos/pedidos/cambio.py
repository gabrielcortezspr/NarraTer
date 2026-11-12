"""Tabela de câmbio (docs/PEDIDO-MULTIMOEDA.md)."""

import csv
from bisect import bisect_right
from datetime import date, datetime
from decimal import Decimal


class ErroCambio(ValueError):
    pass


class TabelaCambio:
    def __init__(self, taxas: dict[str, list[tuple[date, Decimal]]]):
        self._taxas = {m: sorted(v) for m, v in taxas.items()}

    @classmethod
    def carregar(cls, caminho) -> "TabelaCambio":
        taxas: dict[str, list[tuple[date, Decimal]]] = {}
        with open(caminho, newline="", encoding="utf-8") as f:
            for l in csv.DictReader(f):
                d = datetime.strptime(l["data"].strip(), "%d/%m/%Y").date()
                taxas.setdefault(l["moeda"].strip().upper(), []).append((d, Decimal(l["taxa"].strip().replace(",", "."))))
        return cls(taxas)

    def taxa(self, moeda: str, data: date) -> Decimal:
        moeda = moeda.upper()
        if moeda == "BRL":
            return Decimal(1)
        serie = self._taxas.get(moeda, [])
        i = bisect_right([d for d, _ in serie], data)
        if i == 0:
            raise ErroCambio(f"sem taxa de {moeda} até {data:%d/%m/%Y}")
        return serie[i - 1][1]
