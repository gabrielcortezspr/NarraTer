"""Leitura do CSV de pedidos (uma linha por item; ver REGRAS.md, seção 1)."""

import csv
from datetime import date, datetime
from decimal import Decimal, InvalidOperation

from .modelo import Item, Pedido

COLUNAS = ("id", "cliente", "data", "sku", "qtd", "preco_unit", "categoria")
MOEDAS = {"BRL", "USD", "EUR"}


class ErroImportacao(ValueError):
    pass


def _data(texto: str, linha: int) -> date:
    try:
        return datetime.strptime(texto.strip(), "%d/%m/%Y").date()
    except ValueError:
        raise ErroImportacao(f"linha {linha}: data inválida '{texto}'") from None


def _decimal(texto: str, linha: int) -> Decimal:
    t = texto.strip()
    if "." in t:
        raise ErroImportacao(f"linha {linha}: use vírgula decimal, sem separador de milhar ('{texto}')")
    try:
        valor = Decimal(t.replace(",", "."))
    except InvalidOperation:
        raise ErroImportacao(f"linha {linha}: valor inválido '{texto}'") from None
    if valor < 0:
        raise ErroImportacao(f"linha {linha}: valor negativo '{texto}'")
    return valor


def _qtd(texto: str, linha: int) -> int:
    try:
        q = int(texto.strip())
    except ValueError:
        raise ErroImportacao(f"linha {linha}: quantidade inválida '{texto}'") from None
    if q <= 0:
        raise ErroImportacao(f"linha {linha}: quantidade deve ser positiva")
    return q


def ler_linhas(caminho) -> list[dict]:
    with open(caminho, newline="", encoding="utf-8") as f:
        leitor = csv.DictReader(f)
        faltando = [c for c in COLUNAS if c not in (leitor.fieldnames or [])]
        if faltando:
            raise ErroImportacao(f"colunas ausentes: {', '.join(faltando)}")
        linhas = []
        for n, bruto in enumerate(leitor, start=2):
            bruto = {k: (v or "") for k, v in bruto.items() if k is not None}
            bruto["_linha"] = n
            linhas.append(bruto)
        return linhas


def _moeda(texto: str, linha: int) -> str:
    m = texto.strip().upper() or "BRL"
    if m not in MOEDAS:
        raise ErroImportacao(f"linha {linha}: moeda desconhecida '{texto}'")
    return m


def ler_pedidos(caminho) -> list[Pedido]:
    """Agrupa as linhas do CSV em pedidos, na ordem da primeira linha de cada pedido."""
    grupos: dict[str, list[dict]] = {}
    for l in ler_linhas(caminho):
        grupos.setdefault(l["id"].strip(), []).append(l)
    pedidos = []
    for pid, grupo in grupos.items():
        primeira = grupo[0]
        pedido = Pedido(
            id=pid,
            cliente=primeira["cliente"].strip(),
            data=_data(primeira["data"], primeira["_linha"]),
            moeda=_moeda(primeira.get("moeda", ""), primeira["_linha"]),
        )
        for l in grupo:
            n = l["_linha"]
            if _moeda(l.get("moeda", ""), n) != pedido.moeda:
                raise ErroImportacao(f"pedido {pid}: moedas misturadas (linha {n})")
            pedido.itens.append(
                Item(
                    sku=l["sku"].strip(),
                    qtd=_qtd(l["qtd"], n),
                    preco_unit=_decimal(l["preco_unit"], n),
                    categoria=l["categoria"].strip().lower(),
                )
            )
        pedidos.append(pedido)
    return pedidos
