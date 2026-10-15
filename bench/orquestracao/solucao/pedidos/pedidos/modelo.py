from dataclasses import dataclass, field
from datetime import date
from decimal import Decimal


@dataclass
class Item:
    sku: str
    qtd: int
    preco_unit: Decimal
    categoria: str

    @property
    def total(self) -> Decimal:
        return self.qtd * self.preco_unit


@dataclass
class Pedido:
    id: str
    cliente: str
    data: date
    itens: list[Item] = field(default_factory=list)
    moeda: str = "BRL"
