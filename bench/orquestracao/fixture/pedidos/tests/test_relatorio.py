import contextlib
import io
import json
import unittest

from pedidos.relatorio import main

from .util import csv_temp


def rodar(*args):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        codigo = main(list(args))
    return codigo, out.getvalue(), err.getvalue()


class TestRelatorio(unittest.TestCase):
    def setUp(self):
        self.csv = csv_temp(
            'P-1,Acme Ltda,01/03/2026,A,1,"100,00",papelaria\n'
            'P-2,Bruna,02/03/2026,B,2,"50,00",livros\n'
        )

    def test_json(self):
        codigo, out, _ = rodar(self.csv, "--json")
        self.assertEqual(codigo, 0)
        dados = json.loads(out)
        self.assertEqual(dados[0]["id"], "P-1")
        self.assertEqual(dados[0]["total"], "137.00")
        self.assertEqual(dados[1]["data"], "2026-03-02")

    def test_filtro_cliente(self):
        _, out, _ = rodar(self.csv, "--json", "--cliente", "  acme ltda ")
        self.assertEqual([d["id"] for d in json.loads(out)], ["P-1"])

    def test_texto_tem_total_geral(self):
        _, out, _ = rodar(self.csv)
        self.assertIn("TOTAL GERAL 262.00", out)

    def test_erro_sai_com_2(self):
        codigo, _, err = rodar("/nao/existe.csv")
        self.assertEqual(codigo, 2)
        self.assertTrue(err.startswith("erro:"))


if __name__ == "__main__":
    unittest.main()
