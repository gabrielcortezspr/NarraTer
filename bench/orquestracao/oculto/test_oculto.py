"""Testes escondidos do benchmark de orquestração (os agentes nunca veem este arquivo).

Rodar contra uma pasta de trabalho:  PYTHONPATH=<pasta> python3 -m unittest -v test_oculto
Os nomes das classes são os níveis do placar: Basico, Bordas, Defeitos, Regressao.
Valores esperados calculados à mão a partir de REGRAS.md e docs/PEDIDO-MULTIMOEDA.md.
"""

import contextlib
import io
import json
import os
import tempfile
import unittest
from datetime import date
from decimal import Decimal as D

CAB = "id,cliente,data,sku,qtd,preco_unit,categoria\n"
CAB_M = "id,cliente,data,sku,qtd,preco_unit,categoria,moeda\n"
CAMBIO = 'data,moeda,taxa\n02/03/2026,USD,"5,0000"\n05/03/2026,USD,"5,2000"\n02/03/2026,EUR,"6,0000"\n'


def arq(conteudo, sufixo=".csv"):
    fd, caminho = tempfile.mkstemp(suffix=sufixo)
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        f.write(conteudo)
    return caminho


def rodar(*args):
    from pedidos.relatorio import main

    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            codigo = main(list(args))
        except SystemExit as e:
            codigo = e.code
    return codigo, out.getvalue(), err.getvalue()


def json_de(linhas, cab=CAB_M, cambio=True, *extra):
    args = [arq(cab + linhas), "--json", *extra]
    if cambio:
        args += ["--cambio", arq(CAMBIO)]
    codigo, out, err = rodar(*args)
    if codigo != 0:
        raise AssertionError(f"saiu com {codigo}: {err}")
    return {d["id"]: d for d in json.loads(out)}


def tabela():
    from pedidos.cambio import TabelaCambio

    return TabelaCambio.carregar(arq(CAMBIO))


class Basico(unittest.TestCase):
    def test_pedido_em_dolar_convertido(self):
        d = json_de('P-1,Ana,02/03/2026,A,2,"10,00",papelaria,USD\n')["P-1"]
        self.assertEqual((d["subtotal"], d["imposto"], d["frete"], d["total"]), ("100.00", "12.00", "25.00", "137.00"))

    def test_campos_moeda_e_subtotal_original(self):
        d = json_de('P-1,Ana,02/03/2026,A,2,"10,00",papelaria,USD\n')["P-1"]
        self.assertEqual(d["moeda"], "USD")
        self.assertEqual(d["subtotal_original"], "20.00")

    def test_pedido_em_euro(self):
        d = json_de('P-1,Ana,02/03/2026,A,1,"10,00",papelaria,EUR\n')["P-1"]
        self.assertEqual(d["total"], "92.20")

    def test_sem_coluna_moeda_e_real(self):
        d = json_de('P-1,Ana,02/03/2026,A,1,"100,00",papelaria\n', CAB, False)["P-1"]
        self.assertEqual((d["moeda"], d["subtotal_original"], d["total"]), ("BRL", "100.00", "137.00"))

    def test_celula_vazia_e_real(self):
        d = json_de('P-1,Ana,02/03/2026,A,1,"100,00",papelaria,\n', CAB_M, False)["P-1"]
        self.assertEqual(d["moeda"], "BRL")

    def test_tabela_taxa_na_data(self):
        t = tabela()
        self.assertEqual(t.taxa("USD", date(2026, 3, 5)), D("5.2"))
        self.assertEqual(t.taxa("EUR", date(2026, 3, 2)), D("6"))
        self.assertEqual(t.taxa("BRL", date(2020, 1, 1)), D("1"))

    def test_saida_texto_mostra_moeda(self):
        codigo, out, _ = rodar(arq(CAB_M + 'P-1,Ana,02/03/2026,A,1,"1,00",x,USD\n'), "--cambio", arq(CAMBIO))
        self.assertEqual(codigo, 0)
        self.assertIn("USD", out)


class Bordas(unittest.TestCase):
    def test_taxa_da_data_anterior_mais_recente(self):
        t = tabela()
        self.assertEqual(t.taxa("USD", date(2026, 3, 4)), D("5"))
        self.assertEqual(t.taxa("USD", date(2026, 3, 31)), D("5.2"))

    def test_sem_taxa_anterior_e_erro_cambio(self):
        from pedidos.cambio import ErroCambio

        self.assertTrue(issubclass(ErroCambio, ValueError))
        with self.assertRaisesRegex(ErroCambio, "USD"):
            tabela().taxa("USD", date(2026, 3, 1))

    def test_moeda_minuscula(self):
        from pedidos.importa import ler_pedidos

        p = ler_pedidos(arq(CAB_M + 'P-1,Ana,02/03/2026,A,1,"1,00",x,usd\n'))[0]
        self.assertEqual(p.moeda, "USD")

    def test_moeda_desconhecida_cita_linha(self):
        from pedidos.importa import ErroImportacao, ler_pedidos

        with self.assertRaisesRegex(ErroImportacao, "linha 3"):
            ler_pedidos(arq(CAB_M + 'P-1,Ana,02/03/2026,A,1,"1,00",x,BRL\nP-2,Ana,02/03/2026,A,1,"1,00",x,GBP\n'))

    def test_moedas_misturadas_cita_pedido(self):
        from pedidos.importa import ErroImportacao, ler_pedidos

        with self.assertRaisesRegex(ErroImportacao, "P-77"):
            ler_pedidos(arq(CAB_M + 'P-77,Ana,02/03/2026,A,1,"1,00",x,USD\nP-77,Ana,02/03/2026,B,1,"1,00",x,EUR\n'))

    def test_cli_estrangeira_sem_cambio_sai_com_2(self):
        codigo, _, err = rodar(arq(CAB_M + 'P-1,Ana,02/03/2026,A,1,"1,00",x,USD\n'), "--json")
        self.assertEqual(codigo, 2)
        self.assertTrue(err.startswith("erro:"))

    def test_cli_sem_taxa_sai_com_2(self):
        codigo, _, err = rodar(arq(CAB_M + 'P-1,Ana,01/03/2026,A,1,"1,00",x,USD\n'), "--json", "--cambio", arq(CAMBIO))
        self.assertEqual(codigo, 2)
        self.assertTrue(err.startswith("erro:"))

    def test_arredonda_cada_linha_convertida(self):
        d = json_de('P-1,Ana,02/03/2026,A,1,"0,025",livros,USD\nP-1,Ana,02/03/2026,B,1,"0,027",livros,USD\n')["P-1"]
        # 0,125 → 0,12 e 0,135 → 0,14 (meio-para-par); original 0,052 → 0,05
        self.assertEqual((d["subtotal"], d["subtotal_original"]), ("0.26", "0.05"))

    def test_faixas_de_desconto_em_reais(self):
        d = json_de('P-1,Ana,02/03/2026,A,1,"160,00",eletronicos,USD\n')["P-1"]
        self.assertEqual((d["subtotal"], d["desconto"], d["imposto"], d["frete"], d["total"]), ("800.00", "15.00", "94.20", "0.00", "879.20"))

    def test_frete_gratis_avaliado_em_reais(self):
        d = json_de('P-1,Ana,02/03/2026,A,1,"70,00",livros,USD\n')["P-1"]
        self.assertEqual((d["frete"], d["total"]), ("0.00", "350.00"))

    def test_euro_taxa_anterior_e_limite_do_frete(self):
        d = json_de('P-1,Ana,03/03/2026,L,1,"50,00",livros,EUR\n')["P-1"]
        self.assertEqual((d["subtotal"], d["imposto"], d["frete"], d["total"]), ("300.00", "0.00", "0.00", "300.00"))

    def test_calcular_estrangeiro_sem_tabela(self):
        from pedidos.cambio import ErroCambio
        from pedidos.importa import ler_pedidos
        from pedidos.preco import calcular

        p = ler_pedidos(arq(CAB_M + 'P-1,Ana,02/03/2026,A,1,"1,00",x,USD\n'))[0]
        with self.assertRaises(ErroCambio):
            calcular(p)
        self.assertEqual(calcular(p, tabela=tabela()).subtotal, D("5.00"))


class Defeitos(unittest.TestCase):
    def test_desconto_por_faixas_acima_de_1000(self):
        d = json_de('P-2087,Acme Ltda,02/03/2026,A,1,"1200,00",papelaria\n', CAB, False)["P-2087"]
        self.assertEqual((d["desconto"], d["imposto"], d["total"]), ("41.00", "139.08", "1298.08"))

    def test_desconto_progressivo_2000(self):
        from pedidos.preco import centavos, desconto_progressivo

        self.assertEqual(centavos(desconto_progressivo(D("2000.00"))), D("105.00"))

    def test_desconto_por_faixas_apos_conversao(self):
        d = json_de('P-1,Ana,02/03/2026,A,1,"300,00",livros,USD\n')["P-1"]
        self.assertEqual(d["desconto"], "65.00")

    def test_linhas_intercaladas_formam_um_pedido(self):
        from pedidos.importa import ler_pedidos

        ps = ler_pedidos(arq(CAB + 'P-1,Ana,02/03/2026,A,1,"1,00",x\nP-2,Beto,02/03/2026,B,1,"1,00",x\nP-1,Outro,09/03/2026,C,1,"1,00",x\n'))
        self.assertEqual([p.id for p in ps], ["P-1", "P-2"])
        self.assertEqual(len(ps[0].itens), 2)
        self.assertEqual((ps[0].cliente, ps[0].data), ("Ana", date(2026, 3, 2)))

    def test_intercalado_paga_um_frete(self):
        ds = json_de(
            'P-2104,Bruna Reis,02/03/2026,A,1,"100,00",papelaria\n'
            'P-9,Ana,02/03/2026,L,1,"50,00",livros\n'
            'P-2104,Bruna Reis,02/03/2026,B,1,"100,00",papelaria\n',
            CAB,
            False,
        )
        self.assertEqual(len(ds), 2)
        self.assertEqual((ds["P-2104"]["frete"], ds["P-2104"]["total"]), ("25.00", "249.00"))
        self.assertEqual(ds["P-9"]["total"], "75.00")


class Regressao(unittest.TestCase):
    def test_arquivo_exemplo(self):
        raiz = os.environ.get("PEDIDOS_RAIZ", ".")
        codigo, out, _ = rodar(os.path.join(raiz, "dados", "exemplo.csv"), "--json")
        self.assertEqual(codigo, 0)
        ds = {d["id"]: d for d in json.loads(out)}
        self.assertEqual([ds[k]["total"] for k in ("P-1001", "P-1002", "P-1003")], ["195.20", "169.48", "1026.88"])

    def test_filtro_cliente_com_moedas(self):
        ds = json_de('P-1,Acme,02/03/2026,A,1,"1,00",x,USD\nP-2,Bia,02/03/2026,A,1,"1,00",x,BRL\n', CAB_M, True, "--cliente", " ACME ")
        self.assertEqual(list(ds), ["P-1"])

    def test_livros_isentos_apos_conversao(self):
        d = json_de('P-1,Ana,02/03/2026,L,1,"10,00",livros,EUR\n')["P-1"]
        self.assertEqual(d["imposto"], "0.00")

    def test_arquivo_ausente(self):
        self.assertEqual(rodar("/nao/existe.csv")[0], 2)

    def test_ponto_decimal_continua_recusado(self):
        from pedidos.importa import ErroImportacao, ler_pedidos

        with self.assertRaises(ErroImportacao):
            ler_pedidos(arq(CAB + "P-1,Ana,02/03/2026,A,1,10.00,x\n"))

    def test_limites_das_faixas(self):
        from pedidos.preco import centavos, desconto_progressivo

        self.assertEqual(centavos(desconto_progressivo(D("500.00"))), D("0.00"))
        self.assertEqual(centavos(desconto_progressivo(D("1000.00"))), D("25.00"))

    def test_json_mantem_campos_antigos(self):
        d = json_de('P-1,Ana,02/03/2026,A,1,"1,00",x\n', CAB, False)["P-1"]
        for k in ("id", "cliente", "data", "itens", "subtotal", "desconto", "imposto", "frete", "total"):
            self.assertIn(k, d)
        self.assertEqual(d["data"], "2026-03-02")


if __name__ == "__main__":
    unittest.main()
