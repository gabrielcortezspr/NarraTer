"""Parcelamento (REGRAS.md §4)."""

from dataclasses import dataclass
from datetime import date, timedelta
from decimal import ROUND_DOWN, Decimal

CENTAVO = Decimal("0.01")


@dataclass
class Parcela:
    numero: int
    vencimento: date
    valor: Decimal


def vencimento(primeira: date, k: int) -> date:
    """Vencimento da parcela k (1 = primeira)."""
    m = primeira.month - 1 + (k - 1)
    d = primeira.replace(year=primeira.year + m // 12, month=m % 12 + 1)
    if d.weekday() >= 5:
        d += timedelta(days=7 - d.weekday())
    return d


def parcelar(total, n: int, primeira: date):
    if not isinstance(n, int) or not 1 <= n <= 12:
        raise ValueError("n deve estar entre 1 e 12")
    total = Decimal(str(total))
    base = (total / n).quantize(CENTAVO, ROUND_DOWN)
    valores = [base + (total - base * n)] + [base] * (n - 1)
    return [Parcela(k, vencimento(primeira, k), v) for k, v in enumerate(valores, start=1)]
