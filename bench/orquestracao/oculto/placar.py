"""Placar de uma pasta de trabalho: testes escondidos por nível + testes da própria pasta.

Uso: python3 placar.py PASTA  → JSON na saída padrão.
Cada suíte roda num processo próprio, com PYTHONPATH apontando para a pasta.
"""

import json
import os
import re
import subprocess
import sys

AQUI = os.path.dirname(os.path.abspath(__file__))


def rodar(cmd, cwd, env):
    p = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, timeout=300)
    return p.stderr + p.stdout


def contar(saida):
    """Lê a saída verbosa do unittest: nome (modulo.Classe) ... ok|FAIL|ERROR."""
    por_classe = {}
    for m in re.finditer(r"^(\w+) \(([\w.]+)\)\S*.*? \.\.\. (ok|FAIL|ERROR|skipped.*)$", saida, re.M):
        partes = m.group(2).split(".")
        classe = partes[-2] if partes[-1] == m.group(1) and len(partes) > 1 else partes[-1]
        ok, total = por_classe.get(classe, (0, 0))
        por_classe[classe] = (ok + (m.group(3) == "ok"), total + 1)
    return por_classe


def main(pasta):
    pasta = os.path.abspath(pasta)
    env = dict(os.environ, PYTHONPATH=pasta, PEDIDOS_RAIZ=pasta, PYTHONDONTWRITEBYTECODE="1")
    oculto = contar(rodar([sys.executable, "-m", "unittest", "-v", "test_oculto"], AQUI, env))
    proprio_saida = rodar([sys.executable, "-m", "unittest", "discover", "-v", "-s", "tests", "-t", "."], pasta, env)
    proprio = contar(proprio_saida)
    ok = sum(o for o, _ in oculto.values())
    total = sum(t for _, t in oculto.values())
    print(json.dumps({
        "oculto": {k: {"ok": o, "total": t} for k, (o, t) in sorted(oculto.items())},
        "oculto_ok": ok,
        "oculto_total": total,
        "proprios_ok": sum(o for o, _ in proprio.values()),
        "proprios_total": sum(t for _, t in proprio.values()),
    }, ensure_ascii=False))


if __name__ == "__main__":
    main(sys.argv[1])
