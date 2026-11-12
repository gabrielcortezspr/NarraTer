import unittest
from datetime import date
from decimal import Decimal as D

from armazem.cupons import Cupom, Item, aplicar, buscar

CARRINHO = [Item("A", "Moda", D("100.00"), 2), Item("B", "casa", D("50.00"), 1)]


class TestCupons(unittest.TestCase):
    def test_percentual_com_teto(self):
        c = Cupom("DEZ", "percentual", D("10"), teto=D("20.00"))
        self.assertEqual(aplicar(CARRINHO, c, date(2026, 1, 1)).desconto, D("20.00"))

    def test_fixo_limitado_aos_elegiveis(self):
        c = Cupom("CASA", "fixo", D("80.00"), categorias={"casa"})
        self.assertEqual(aplicar(CARRINHO, c, date(2026, 1, 1)).desconto, D("50.00"))

    def test_minimo(self):
        c = Cupom("MIN", "fixo", D("5.00"), minimo=D("500.00"))
        self.assertEqual(aplicar(CARRINHO, c, date(2026, 1, 1)).motivo, "minimo")

    def test_buscar(self):
        self.assertEqual(buscar([Cupom("PRIMAVERA", "fixo", D("1"))], " primavera ").codigo, "PRIMAVERA")
