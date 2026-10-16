import unittest
from datetime import date
from decimal import Decimal as D

from armazem.parcelas import parcelar


class TestParcelas(unittest.TestCase):
    def test_centavos_na_primeira(self):
        self.assertEqual([p.valor for p in parcelar(D("100.00"), 3, date(2026, 3, 10))], [D("33.34"), D("33.33"), D("33.33")])

    def test_vencimentos_mensais(self):
        self.assertEqual([p.vencimento for p in parcelar(D("90"), 3, date(2026, 3, 10))],
                         [date(2026, 3, 10), date(2026, 4, 10), date(2026, 5, 11)])  # 10/05 é domingo

    def test_n_invalido(self):
        with self.assertRaises(ValueError):
            parcelar(D("10"), 13, date(2026, 1, 5))
