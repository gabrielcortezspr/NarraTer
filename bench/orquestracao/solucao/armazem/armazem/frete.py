"""Cotação de frete (REGRAS.md §1, docs/PEDIDOS.md §1)."""

from dataclasses import dataclass
from decimal import ROUND_CEILING, ROUND_HALF_EVEN, Decimal

REGIOES = [("SE", 1000, 39999), ("NE", 40000, 65999), ("N", 66000, 69999), ("CO", 70000, 79999), ("S", 80000, 99999)]

# até 1 kg, até 5 kg, até 10 kg, por kg acima de 10
TARIFAS = {
    "SE": (Decimal("15.00"), Decimal("22.00"), Decimal("35.00"), Decimal("3.00")),
    "S": (Decimal("18.00"), Decimal("26.00"), Decimal("40.00"), Decimal("3.50")),
    "CO": (Decimal("20.00"), Decimal("30.00"), Decimal("45.00"), Decimal("4.00")),
    "NE": (Decimal("22.00"), Decimal("33.00"), Decimal("50.00"), Decimal("4.50")),
    "N": (Decimal("28.00"), Decimal("42.00"), Decimal("60.00"), Decimal("6.00")),
}
GRATIS_MINIMO = Decimal("299.00")
GRATIS_PESO_MAX = Decimal("30")
SEGURO = Decimal("0.005")
CENTAVO = Decimal("0.01")


@dataclass
class Cotacao:
    regiao: str
    tarifa: Decimal
    seguro: Decimal
    total: Decimal
    peso_cobrado: Decimal


def regiao(cep: str) -> str:
    digitos = cep.replace("-", "")
    if len(digitos) != 8 or not digitos.isdigit():
        raise ValueError(f"CEP inválido: {cep!r}")
    prefixo = int(digitos[:5])
    for nome, ini, fim in REGIOES:
        if ini <= prefixo <= fim:
            return nome
    raise ValueError(f"CEP fora das faixas: {cep!r}")


def peso_cobrado(peso_kg, dimensoes=None) -> Decimal:
    peso = Decimal(str(peso_kg))
    if dimensoes is not None:
        c, l, a = (Decimal(str(x)) for x in dimensoes)
        peso = max(peso, c * l * a / Decimal(6000))
    return (peso * 2).to_integral_value(rounding=ROUND_CEILING) / 2


def tarifa(reg: str, peso: Decimal) -> Decimal:
    ate1, ate5, ate10, por_kg = TARIFAS[reg]
    if peso <= 1:
        return ate1
    if peso <= 5:
        return ate5
    if peso <= 10:
        return ate10
    excedente = (peso - 10).to_integral_value(rounding=ROUND_CEILING)
    return ate10 + por_kg * excedente


def cotar(cep: str, peso_kg, valor, dimensoes=None) -> Cotacao:
    reg = regiao(cep)
    valor = Decimal(str(valor))
    peso = peso_cobrado(peso_kg, dimensoes)
    t = tarifa(reg, peso)
    if valor >= GRATIS_MINIMO and reg in ("SE", "S") and peso <= GRATIS_PESO_MAX:
        t = Decimal("0.00")
    seguro = (valor * SEGURO).quantize(CENTAVO, ROUND_HALF_EVEN)
    return Cotacao(reg, t, seguro, t + seguro, peso)
