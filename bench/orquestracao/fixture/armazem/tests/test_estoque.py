import unittest
from decimal import Decimal as D

from armazem.estoque import ErroEstoque, Estoque


class TestEstoque(unittest.TestCase):
    def test_saida_dentro_do_lote(self):
        e = Estoque()
        e.entrada("X", 10, D("2.00"))
        e.entrada("X", 5, D("4.00"))
        self.assertEqual(e.saida("X", 4), D("8.00"))
        self.assertEqual((e.saldo("X"), e.valor("X")), (11, D("32.00")))

    def test_saldo_insuficiente(self):
        e = Estoque()
        e.entrada("X", 3, D("1.00"))
        with self.assertRaises(ErroEstoque):
            e.saida("X", 4)

    def test_sku_desconhecido(self):
        self.assertEqual((Estoque().saldo("Z"), Estoque().valor("Z")), (0, D("0.00")))
