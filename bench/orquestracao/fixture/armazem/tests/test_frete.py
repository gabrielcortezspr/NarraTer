import unittest
from decimal import Decimal as D

from armazem.frete import cotar


class TestFrete(unittest.TestCase):
    def test_regiao_e_tarifa(self):
        c = cotar("30140-071", D("3"), D("100.00"))
        self.assertEqual((c.regiao, c.tarifa, c.seguro, c.total), ("SE", D("22.00"), D("0.50"), D("22.50")))

    def test_excedente_acima_de_10kg(self):
        self.assertEqual(cotar("69005040", D("12.3"), D("50.00")).tarifa, D("60.00") + 3 * D("6.00"))

    def test_frete_gratis_mantem_seguro(self):
        c = cotar("80010-000", D("2"), D("300.00"))
        self.assertEqual((c.tarifa, c.seguro), (D("0.00"), D("1.50")))

    def test_cep_invalido(self):
        with self.assertRaises(ValueError):
            cotar("1234-567", D("1"), D("10"))
