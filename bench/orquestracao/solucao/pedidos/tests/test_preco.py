import unittest
from datetime import date
from decimal import Decimal as D

from pedidos.modelo import Item, Pedido
from pedidos.preco import calcular, centavos, desconto_progressivo


def pedido(*itens):
    return Pedido(id="P", cliente="X", data=date(2026, 1, 1), itens=[Item(*i) for i in itens])


class TestPreco(unittest.TestCase):
    def test_sem_desconto_ate_500(self):
        self.assertEqual(desconto_progressivo(D("500.00")), 0)

    def test_desconto_na_segunda_faixa(self):
        self.assertEqual(centavos(desconto_progressivo(D("800.00"))), D("15.00"))
        self.assertEqual(centavos(desconto_progressivo(D("1000.00"))), D("25.00"))

    def test_livros_isentos(self):
        r = calcular(pedido(("L", 2, D("50.00"), "livros")))
        self.assertEqual(r.imposto, D("0.00"))

    def test_imposto_proporcional_ao_desconto(self):
        r = calcular(pedido(("A", 1, D("400.00"), "eletronicos"), ("L", 1, D("400.00"), "livros")))
        # desconto 15.00; base 785.00; imposto = 12% × 400 × 785/800 = 47.10
        self.assertEqual(r.desconto, D("15.00"))
        self.assertEqual(r.imposto, D("47.10"))

    def test_frete(self):
        self.assertEqual(calcular(pedido(("A", 1, D("299.99"), "livros"))).frete, D("25.00"))
        self.assertEqual(calcular(pedido(("A", 1, D("300.00"), "livros"))).frete, D("0.00"))

    def test_total(self):
        r = calcular(pedido(("A", 3, D("10.00"), "papelaria")))
        self.assertEqual(r.total, D("30.00") + D("3.60") + D("25.00"))

    def test_arredonda_cada_linha_meio_para_par(self):
        r = calcular(pedido(("A", 1, D("0.125"), "livros"), ("B", 1, D("0.135"), "livros")))
        self.assertEqual(r.subtotal, D("0.12") + D("0.14"))


if __name__ == "__main__":
    unittest.main()
