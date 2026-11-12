"""Cupons de desconto (REGRAS.md §2)."""

from dataclasses import dataclass
from datetime import date
from decimal import ROUND_HALF_EVEN, Decimal

CENTAVO = Decimal("0.01")


@dataclass
class Item:
    sku: str
    categoria: str
    preco: Decimal
    qtd: int


@dataclass
class Cupom:
    codigo: str
    tipo: str  # "percentual" | "fixo"
    valor: Decimal = Decimal("0")
    validade: date = date.max
    minimo: Decimal = Decimal("0")
    categorias: set | None = None
    teto: Decimal | None = None


@dataclass
class Resultado:
    desconto: Decimal
    motivo: str | None = None


def buscar(catalogo, codigo: str):
    alvo = codigo.strip().lower()
    for c in catalogo:
        if c.codigo.strip().lower() == alvo:
            return c
    return None


def _elegiveis(carrinho, cupom):
    if cupom.categorias is None:
        return list(carrinho)
    cats = {c.lower() for c in cupom.categorias}
    return [i for i in carrinho if i.categoria.lower() in cats]


def aplicar(carrinho, cupom: Cupom, hoje: date) -> Resultado:
    if hoje >= cupom.validade:
        return Resultado(Decimal("0.00"), "expirado")
    itens = _elegiveis(carrinho, cupom)
    if not itens:
        return Resultado(Decimal("0.00"), "sem_itens")
    soma = sum((i.preco * i.qtd for i in itens), Decimal("0"))
    if soma < cupom.minimo:
        return Resultado(Decimal("0.00"), "minimo")
    if cupom.tipo == "percentual":
        desconto = soma * cupom.valor / 100
    elif cupom.tipo == "fixo":
        desconto = min(cupom.valor, soma)
    else:
        raise ValueError(f"tipo de cupom desconhecido: {cupom.tipo}")
    desconto = desconto.quantize(CENTAVO, ROUND_HALF_EVEN)
    if cupom.teto is not None:
        desconto = min(desconto, cupom.teto)
    return Resultado(desconto, None)
