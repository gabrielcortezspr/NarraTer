"""Testes escondidos da tarefa `armazem` (os agentes nunca veem este arquivo).

Rodar contra uma pasta de trabalho:  PYTHONPATH=<pasta> python3 -m unittest -v test_armazem
Classes = níveis do placar: um por módulo (funcionalidade nova, defeito e regressão) e Sutil, com as
regras fáceis de deixar passar (as que um revisor tende a pegar). Valores calculados à mão a partir de
REGRAS.md e docs/PEDIDOS.md; os da tabela Price conferidos com Decimal de 50 dígitos.
"""

import unittest
from datetime import date
from decimal import Decimal as D


def frete(*a, **k):
    from armazem.frete import cotar

    return cotar(*a, **k)


def cupons():
    from armazem import cupons as c

    return c


def estoque():
    from armazem.estoque import ErroEstoque, Estoque

    return Estoque(), ErroEstoque


def parcelar(*a):
    from armazem.parcelas import parcelar as p

    return p(*a)


HOJE = date(2026, 9, 30)


class Frete(unittest.TestCase):
    def test_defeito_1kg_exato_e_ate_1kg(self):
        self.assertEqual(frete("01310-100", D("1"), D("100.00")).tarifa, D("15.00"))

    def test_defeito_5kg_exato_e_ate_5kg(self):
        self.assertEqual(frete("50000-000", D("5"), D("100.00")).tarifa, D("33.00"))

    def test_defeito_10kg_exato_e_ate_10kg(self):
        self.assertEqual(frete("70000000", D("10"), D("100.00")).tarifa, D("45.00"))

    def test_peso_cubico_maior_que_o_real(self):
        c = frete("90000-000", D("2"), D("100.00"), (50, 40, 30))  # 60000/6000 = 10 kg
        self.assertEqual((c.tarifa, c.peso_cobrado), (D("40.00"), D("10")))

    def test_peso_real_maior_que_o_cubico(self):
        c = frete("01310-100", D("8"), D("100.00"), (10, 10, 10))
        self.assertEqual((c.tarifa, c.peso_cobrado), (D("35.00"), D("8")))

    def test_arredonda_para_meio_quilo_sem_dimensoes(self):
        c = frete("01310-100", D("1.01"), D("100.00"))
        self.assertEqual((c.tarifa, c.peso_cobrado), (D("22.00"), D("1.5")))

    def test_cubico_fracionario_arredonda(self):
        c = frete("01310-100", D("1"), D("100.00"), (31, 30, 20))  # 3,1 kg -> 3,5
        self.assertEqual(c.peso_cobrado, D("3.5"))

    def test_regressao_gratis_mantem_seguro(self):
        c = frete("80010-000", D("2"), D("300.00"))
        self.assertEqual((c.tarifa, c.seguro, c.total), (D("0.00"), D("1.50"), D("1.50")))

    def test_regressao_cep_fora_das_faixas(self):
        with self.assertRaises(ValueError):
            frete("00999-000", D("1"), D("10.00"))

    def test_regressao_excedente_por_fracao(self):
        self.assertEqual(frete("69005-040", D("12"), D("50.00")).tarifa, D("72.00"))


class Cupons(unittest.TestCase):
    def setUp(self):
        self.c = cupons()
        self.carrinho = [self.c.Item("A", "moda", D("100.00"), 2), self.c.Item("B", "casa", D("50.00"), 1)]

    def test_defeito_vale_no_ultimo_dia(self):
        cupom = self.c.Cupom("PRIMAVERA", "percentual", D("10"), validade=HOJE)
        r = self.c.aplicar(self.carrinho, cupom, HOJE)
        self.assertEqual((r.desconto, r.motivo), (D("25.00"), None))

    def test_expirado_no_dia_seguinte(self):
        cupom = self.c.Cupom("PRIMAVERA", "percentual", D("10"), validade=date(2026, 9, 29))
        self.assertEqual(self.c.aplicar(self.carrinho, cupom, HOJE).motivo, "expirado")

    def test_leve3_pague2(self):
        cupom = self.c.Cupom("L3P2", "leve_pague", leve=3, pague=2)
        r = self.c.aplicar([self.c.Item("M", "moda", D("30.00"), 3)], cupom, HOJE)
        self.assertEqual((r.desconto, r.motivo), (D("30.00"), None))

    def test_leve3_pague2_com_sete_unidades(self):
        cupom = self.c.Cupom("L3P2", "leve_pague", leve=3, pague=2)
        self.assertEqual(self.c.aplicar([self.c.Item("M", "moda", D("30.00"), 7)], cupom, HOJE).desconto, D("60.00"))

    def test_leve_pague_soma_linhas_do_mesmo_sku(self):
        cupom = self.c.Cupom("L3P2", "leve_pague", leve=3, pague=2)
        carrinho = [self.c.Item("M", "moda", D("30.00"), 2), self.c.Item("M", "moda", D("30.00"), 1)]
        self.assertEqual(self.c.aplicar(carrinho, cupom, HOJE).desconto, D("30.00"))

    def test_leve_pague_so_nos_elegiveis(self):
        cupom = self.c.Cupom("L2P1", "leve_pague", leve=2, pague=1, categorias={"casa"})
        carrinho = [self.c.Item("M", "moda", D("30.00"), 2), self.c.Item("C", "casa", D("12.00"), 2)]
        self.assertEqual(self.c.aplicar(carrinho, cupom, HOJE).desconto, D("12.00"))

    def test_regressao_percentual_meio_para_par(self):
        cupom = self.c.Cupom("Q", "percentual", D("15"))
        self.assertEqual(self.c.aplicar([self.c.Item("X", "a", D("10.10"), 1)], cupom, HOJE).desconto, D("1.52"))

    def test_regressao_ordem_dos_motivos(self):
        cupom = self.c.Cupom("V", "fixo", D("5"), validade=date(2026, 1, 1), categorias={"jardim"})
        self.assertEqual(self.c.aplicar(self.carrinho, cupom, HOJE).motivo, "expirado")


class Estoque(unittest.TestCase):
    def test_defeito_saida_entre_lotes(self):
        e, _ = estoque()
        e.entrada("CAD-01", 10, D("2.00"))
        e.entrada("CAD-01", 10, D("3.00"))
        self.assertEqual(e.saida("CAD-01", 15), D("35.00"))
        self.assertEqual((e.saldo("CAD-01"), e.valor("CAD-01")), (5, D("15.00")))

    def test_defeito_saida_em_tres_lotes(self):
        e, _ = estoque()
        for c in ("1", "2", "4"):
            e.entrada("X", 5, D(c))
        self.assertEqual(e.saida("X", 12), D("23.00"))

    def test_depositos_separados(self):
        e, _ = estoque()
        e.entrada("X", 10, D("1.00"), deposito="loja")
        self.assertEqual((e.saldo("X"), e.saldo("X", deposito="loja")), (0, 10))

    def test_transferir(self):
        e, _ = estoque()
        e.entrada("X", 10, D("2.00"))
        e.entrada("X", 10, D("3.00"))
        self.assertEqual(e.transferir("X", 15, "principal", "loja"), D("35.00"))
        self.assertEqual((e.saldo("X"), e.saldo("X", deposito="loja")), (5, 15))
        self.assertEqual((e.valor("X"), e.valor("X", deposito="loja")), (D("15.00"), D("35.00")))

    def test_transferir_para_o_mesmo_deposito(self):
        e, _ = estoque()
        e.entrada("X", 1, D("1.00"))
        with self.assertRaises(ValueError):
            e.transferir("X", 1, "principal", "principal")

    def test_transferir_sem_saldo_nao_muda_nada(self):
        e, Erro = estoque()
        e.entrada("X", 3, D("1.00"))
        with self.assertRaises(Erro):
            e.transferir("X", 5, "principal", "loja")
        self.assertEqual((e.saldo("X"), e.saldo("X", deposito="loja")), (3, 0))

    def test_saldo_total(self):
        e, _ = estoque()
        e.entrada("X", 4, D("1.00"))
        e.entrada("X", 6, D("1.00"), deposito="loja")
        e.transferir("X", 1, "principal", "cd")
        self.assertEqual(e.saldo_total("X"), 10)

    def test_regressao_saida_sem_saldo_nao_muda(self):
        e, Erro = estoque()
        e.entrada("X", 3, D("1.00"))
        with self.assertRaises(Erro):
            e.saida("X", 5)
        self.assertEqual((e.saldo("X"), e.valor("X")), (3, D("3.00")))


class Parcelas(unittest.TestCase):
    def test_defeito_dia_31(self):
        v = [p.vencimento for p in parcelar(D("100.00"), 3, date(2026, 3, 31))]
        self.assertEqual(v, [date(2026, 3, 31), date(2026, 4, 30), date(2026, 6, 1)])  # 31/05 é domingo

    def test_ano_bissexto(self):
        v = [p.vencimento for p in parcelar(D("100.00"), 2, date(2028, 1, 31))]
        self.assertEqual(v, [date(2028, 1, 31), date(2028, 2, 29)])

    def test_juros_seis_parcelas(self):
        valores = [p.valor for p in parcelar(D("1000.00"), 6, date(2026, 4, 15))]
        self.assertEqual(valores, [D("178.47")] * 5 + [D("178.44")])

    def test_juros_quatro_parcelas(self):
        self.assertEqual([p.valor for p in parcelar(D("500.00"), 4, date(2026, 4, 15))], [D("131.28")] * 4)

    def test_juros_doze_parcelas(self):
        valores = [p.valor for p in parcelar(D("1200.00"), 12, date(2026, 4, 15))]
        self.assertEqual(valores, [D("113.40")] * 11 + [D("113.42")])

    def test_total_pago(self):
        from armazem.parcelas import total_pago

        self.assertEqual((total_pago(D("1000.00"), 6), total_pago(D("90.00"), 3)), (D("1070.79"), D("90.00")))

    def test_regressao_sem_juros_ate_tres(self):
        self.assertEqual([p.valor for p in parcelar(D("100.00"), 3, date(2026, 3, 10))], [D("33.34"), D("33.33"), D("33.33")])

    def test_regressao_n_fora_da_faixa(self):
        with self.assertRaises(ValueError):
            parcelar(D("10.00"), 0, date(2026, 3, 10))


class Sutil(unittest.TestCase):
    """Regras escritas, mas fáceis de deixar passar."""

    def test_frete_gratis_negado_pelo_peso_cubico(self):
        c = frete("01310-100", D("5"), D("400.00"), (100, 60, 40))  # 40 kg cúbicos > 30
        self.assertEqual(c.tarifa, D("125.00"))

    def test_frete_gratis_com_30kg_cubicos_exatos(self):
        self.assertEqual(frete("01310-100", D("5"), D("400.00"), (100, 60, 30)).tarifa, D("0.00"))

    def test_excedente_sobre_o_peso_arredondado(self):
        self.assertEqual(frete("01310-100", D("10.4"), D("100.00")).tarifa, D("38.00"))  # 10,5 kg: +1 kg

    def test_leve_pague_usa_o_menor_preco(self):
        c = cupons()
        cupom = c.Cupom("L3P2", "leve_pague", leve=3, pague=2)
        carrinho = [c.Item("M", "moda", D("30.00"), 2), c.Item("M", "moda", D("20.00"), 1)]
        self.assertEqual(c.aplicar(carrinho, cupom, HOJE).desconto, D("20.00"))

    def test_leve_pague_respeita_teto(self):
        c = cupons()
        cupom = c.Cupom("L3P2", "leve_pague", leve=3, pague=2, teto=D("100.00"))
        self.assertEqual(c.aplicar([c.Item("M", "moda", D("50.00"), 9)], cupom, HOJE).desconto, D("100.00"))

    def test_leve_pague_minimo_sobre_elegiveis(self):
        c = cupons()
        cupom = c.Cupom("L3P2", "leve_pague", leve=3, pague=2, minimo=D("100.00"), categorias={"MODA"})
        carrinho = [c.Item("M", "Moda", D("30.00"), 3), c.Item("T", "casa", D("300.00"), 1)]
        self.assertEqual(c.aplicar(carrinho, cupom, HOJE).motivo, "minimo")

    def test_transferido_mais_antigo_sai_primeiro_no_destino(self):
        e, _ = estoque()
        e.entrada("X", 5, D("1.00"))
        e.entrada("X", 5, D("9.00"), deposito="loja")
        e.transferir("X", 5, "principal", "loja")
        self.assertEqual(e.saida("X", 5, deposito="loja"), D("5.00"))

    def test_transferido_mais_novo_sai_depois_no_destino(self):
        e, _ = estoque()
        e.entrada("X", 5, D("9.00"), deposito="loja")
        e.entrada("X", 5, D("1.00"))
        e.transferir("X", 5, "principal", "loja")
        self.assertEqual(e.saida("X", 5, deposito="loja"), D("45.00"))

    def test_custo_arredondado_so_no_final(self):
        e, _ = estoque()
        e.entrada("X", 3, D("0.333"))
        self.assertEqual(e.saida("X", 3), D("1.00"))

    def test_vencimento_referencia_e_a_primeira(self):
        v = [p.vencimento for p in parcelar(D("100.00"), 3, date(2026, 1, 31))]
        self.assertEqual(v, [date(2026, 2, 2), date(2026, 3, 2), date(2026, 3, 31)])

    def test_virada_de_ano_com_fim_de_semana(self):
        v = [p.vencimento for p in parcelar(D("100.00"), 3, date(2026, 12, 30))]
        self.assertEqual(v, [date(2026, 12, 30), date(2027, 2, 1), date(2027, 3, 1)])

    def test_juros_sobra_na_ultima(self):
        valores = [p.valor for p in parcelar(D("99.90"), 5, date(2026, 4, 15))]
        self.assertEqual(valores, [D("21.19")] * 4 + [D("21.18")])
