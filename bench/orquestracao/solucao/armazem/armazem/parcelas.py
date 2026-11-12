"""Parcelamento (REGRAS.md §4, docs/PEDIDOS.md §4)."""

import calendar
from dataclasses import dataclass
from datetime import date, timedelta
from decimal import ROUND_DOWN, ROUND_HALF_EVEN, Decimal

CENTAVO = Decimal("0.01")
TAXA = Decimal("0.0199")


@dataclass
class Parcela:
    numero: int
    vencimento: date
    valor: Decimal


def vencimento(primeira: date, k: int) -> date:
    """Vencimento da parcela k (1 = primeira)."""
    m = primeira.month - 1 + (k - 1)
    ano, mes = primeira.year + m // 12, m % 12 + 1
    dia = min(primeira.day, calendar.monthrange(ano, mes)[1])
    d = date(ano, mes, dia)
    if d.weekday() >= 5:
        d += timedelta(days=7 - d.weekday())
    return d


def _valores(total: Decimal, n: int):
    if n <= 3:
        base = (total / n).quantize(CENTAVO, ROUND_DOWN)
        return [base + (total - base * n)] + [base] * (n - 1)
    i = TAXA
    pmt = total * i / (1 - (1 + i) ** -n)
    p = pmt.quantize(CENTAVO, ROUND_HALF_EVEN)
    ultima = (pmt * n).quantize(CENTAVO, ROUND_HALF_EVEN) - p * (n - 1)
    return [p] * (n - 1) + [ultima]


def parcelar(total, n: int, primeira: date):
    if not isinstance(n, int) or not 1 <= n <= 12:
        raise ValueError("n deve estar entre 1 e 12")
    total = Decimal(str(total))
    return [Parcela(k, vencimento(primeira, k), v) for k, v in enumerate(_valores(total, n), start=1)]


def total_pago(total, n: int) -> Decimal:
    return sum((p.valor for p in parcelar(total, n, date(2026, 1, 1))), Decimal("0"))
