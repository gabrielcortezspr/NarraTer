import unittest
from datetime import date
from decimal import Decimal

from pedidos.importa import ErroImportacao, ler_pedidos

from .util import csv_temp


class TestImporta(unittest.TestCase):
    def test_agrupa_linhas_consecutivas(self):
        c = csv_temp(
            'P-1,Ana,01/03/2026,A,1,"10,00",papelaria\n'
            'P-1,Ana,01/03/2026,B,2,"5,50",livros\n'
            'P-2,Beto,02/03/2026,C,1,"7,00",papelaria\n'
        )
        pedidos = ler_pedidos(c)
        self.assertEqual([p.id for p in pedidos], ["P-1", "P-2"])
        self.assertEqual(len(pedidos[0].itens), 2)
        self.assertEqual(pedidos[0].itens[1].preco_unit, Decimal("5.50"))
        self.assertEqual(pedidos[0].data, date(2026, 3, 1))

    def test_categoria_minuscula(self):
        p = ler_pedidos(csv_temp('P-1,Ana,01/03/2026,A,1,"10,00",LIVROS\n'))[0]
        self.assertEqual(p.itens[0].categoria, "livros")

    def test_data_invalida_cita_linha(self):
        with self.assertRaisesRegex(ErroImportacao, "linha 3"):
            ler_pedidos(csv_temp('P-1,Ana,01/03/2026,A,1,"1,00",x\nP-2,Ana,2026-03-01,A,1,"1,00",x\n'))

    def test_ponto_decimal_recusado(self):
        with self.assertRaises(ErroImportacao):
            ler_pedidos(csv_temp("P-1,Ana,01/03/2026,A,1,10.00,x\n"))

    def test_quantidade_positiva(self):
        with self.assertRaises(ErroImportacao):
            ler_pedidos(csv_temp('P-1,Ana,01/03/2026,A,0,"1,00",x\n'))

    def test_coluna_ausente(self):
        with self.assertRaisesRegex(ErroImportacao, "categoria"):
            ler_pedidos(csv_temp('P-1,Ana,01/03/2026,A,1,"1,00"\n', cabecalho="id,cliente,data,sku,qtd,preco_unit\n"))


if __name__ == "__main__":
    unittest.main()
