"""Relatório de pedidos: python -m pedidos.relatorio ARQUIVO.csv [--cliente NOME] [--json]"""

import argparse
import json
import sys

from .importa import ErroImportacao, ler_pedidos
from .preco import calcular


def _filtra(pedidos, cliente):
    if cliente is None:
        return pedidos
    alvo = cliente.strip().casefold()
    return [p for p in pedidos if p.cliente.strip().casefold() == alvo]


def _linha_json(pedido, resumo) -> dict:
    return {
        "id": pedido.id,
        "cliente": pedido.cliente,
        "data": pedido.data.isoformat(),
        "itens": len(pedido.itens),
        "subtotal": str(resumo.subtotal),
        "desconto": str(resumo.desconto),
        "imposto": str(resumo.imposto),
        "frete": str(resumo.frete),
        "total": str(resumo.total),
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(prog="pedidos.relatorio")
    parser.add_argument("arquivo")
    parser.add_argument("--cliente")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    try:
        pedidos = _filtra(ler_pedidos(args.arquivo), args.cliente)
    except (ErroImportacao, OSError) as e:
        print(f"erro: {e}", file=sys.stderr)
        return 2

    resumos = [(p, calcular(p)) for p in pedidos]
    if args.json:
        print(json.dumps([_linha_json(p, r) for p, r in resumos], ensure_ascii=False, indent=2))
        return 0

    print(f"{'pedido':<10} {'cliente':<16} {'data':<10} {'subtotal':>10} {'desconto':>9} {'imposto':>9} {'frete':>7} {'total':>10}")
    geral = 0
    for p, r in resumos:
        print(
            f"{p.id:<10} {p.cliente[:16]:<16} {p.data:%d/%m/%Y} {r.subtotal:>10} {r.desconto:>9} "
            f"{r.imposto:>9} {r.frete:>7} {r.total:>10}"
        )
        geral += r.total
    print(f"TOTAL GERAL {geral}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
