"""Regras de preço (REGRAS.md, seções 2 a 5). Todo valor é Decimal em reais."""

from dataclasses import dataclass
from decimal import ROUND_HALF_EVEN, Decimal

from .cambio import ErroCambio
from .modelo import Pedido

CENTAVO = Decimal("0.01")
ALIQUOTA = Decimal("0.12")
CATEGORIAS_ISENTAS = {"livros"}
FRETE = Decimal("25.00")
FRETE_GRATIS_A_PARTIR_DE = Decimal("300.00")


def centavos(valor: Decimal) -> Decimal:
    return valor.quantize(CENTAVO, rounding=ROUND_HALF_EVEN)


@dataclass
class Resumo:
    subtotal: Decimal
    desconto: Decimal
    imposto: Decimal
    frete: Decimal
    total: Decimal


def desconto_progressivo(subtotal: Decimal) -> Decimal:
    """Faixas da seção 2: 0% até 500, 5% de 500 a 1000, 8% acima de 1000."""
    if subtotal <= 500:
        return Decimal(0)
    if subtotal <= 1000:
        return (subtotal - 500) * Decimal("0.05")
    return 500 * Decimal("0.05") + (subtotal - 1000) * Decimal("0.08")


def calcular(pedido: Pedido, tabela=None) -> Resumo:
    if pedido.moeda == "BRL":
        taxa = Decimal(1)
    elif tabela is None:
        raise ErroCambio(f"pedido {pedido.id} em {pedido.moeda} sem tabela de câmbio")
    else:
        taxa = tabela.taxa(pedido.moeda, pedido.data)
    linhas = [(item, centavos(item.total * taxa)) for item in pedido.itens]
    subtotal = sum((v for _, v in linhas), Decimal(0))
    desconto = centavos(desconto_progressivo(subtotal))
    base = subtotal - desconto

    tributavel = sum((v for item, v in linhas if item.categoria not in CATEGORIAS_ISENTAS), Decimal(0))
    fator = base / subtotal if subtotal else Decimal(0)
    imposto = centavos(tributavel * fator * ALIQUOTA)

    frete = Decimal("0.00") if base >= FRETE_GRATIS_A_PARTIR_DE else FRETE
    total = base + imposto + frete
    return Resumo(subtotal=subtotal, desconto=desconto, imposto=imposto, frete=frete, total=total)
