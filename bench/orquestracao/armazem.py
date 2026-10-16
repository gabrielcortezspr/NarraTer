#!/usr/bin/env python3
"""Tarefa `armazem`: sozinho × equipe paralela × dev + revisor (docs/relatorios/equipe-ou-sozinho.tex).

Uso: armazem.py RUNS_DIR [SAIDA]
  RUNS_DIR  pasta com vH-armazem-{solo,equipe,revisor}-r{1,2,3}/metricas.json
            ($BENCH/runs, ou a cópia em resultados/<data>/)
  SAIDA     padrão docs/relatorios/dados/armazem.tex

Imprime os valores por rodada e os testes de Mann–Whitney entre as montagens.
"""

import json
import statistics as st
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from agregar import mann_whitney  # noqa: E402

RAIZ = Path(__file__).resolve().parents[2]
MONTAGENS = {"solo": "Sozinho", "equipe": "Equipe paralela", "revisor": "Dev + revisor"}
NIVEIS = ("Frete", "Cupons", "Estoque", "Parcelas", "Sutil")


def rodadas(pasta: Path, variante: str):
    out = []
    for f in sorted(pasta.glob(f"vH-armazem-{variante}-r*/metricas.json")):
        m = json.loads(f.read_text())
        if m["rep"] in (1, 2, 3):
            out.append(m)
    return out


def valores(ms):
    v = {k: [] for k in ("dur", "tok", "usd", "api", "oculto", "sutil", "usd_teste", "pedidos", "diff", "proprios")}
    for m in ms:
        t, p = m["total"], m["placar"]
        ok = p.get("oculto_ok", 0)
        v["dur"].append(m["duracao_s"] / 60 if m["duracao_s"] else float("nan"))
        v["tok"].append(t["tokens"]["processados"] / 1e6)
        v["usd"].append(t["custo_usd"])
        v["api"].append(t["chamadas_api"])
        v["oculto"].append(ok)
        v["sutil"].append(p.get("oculto", {}).get("Sutil", {}).get("ok", 0))
        v["usd_teste"].append(t["custo_usd"] / ok * 100 if ok else float("nan"))  # centavos por teste aprovado
        v["pedidos"].append(m["agentes"].get("Narrador", {}).get("pedidos", 0))
        v["diff"].append(m["diff"]["linhas_adicionadas"] + m["diff"]["linhas_removidas"])
        v["proprios"].append(p.get("proprios_ok", 0))
    return v


def br(x, fmt):
    return fmt.format(x).replace(".", "{,}")


def celula(xs, fmt):
    if not xs:
        return "---"
    return f"{br(st.mean(xs), fmt)} ({br(min(xs), fmt)}--{br(max(xs), fmt)})"


def main():
    pasta = Path(sys.argv[1])
    saida = Path(sys.argv[2]) if len(sys.argv) > 2 else RAIZ / "docs/relatorios/dados/armazem.tex"
    rods = {k: rodadas(pasta, k) for k in MONTAGENS}
    vals = {k: valores(v) for k, v in rods.items()}
    linhas = [("Testes escondidos (de 46)", "oculto", "{:.0f}"), ("\\quad dos quais Sutil (de 12)", "sutil", "{:.0f}"),
              ("Duração (min)", "dur", "{:.1f}"), ("Tokens processados (milhões)", "tok", "{:.2f}"),
              ("Custo de API equivalente (US\\$)", "usd", "{:.2f}"),
              ("Custo por teste aprovado (centavos)", "usd_teste", "{:.1f}"),
              ("Chamadas à API (todos os agentes)", "api", "{:.0f}"), ("Pedidos a outros agentes", "pedidos", "{:.0f}"),
              ("Linhas no diff", "diff", "{:.0f}")]
    tex = ["\\begin{tabular}{l" + "r" * len(MONTAGENS) + "}", "\\toprule",
           " & ".join([""] + list(MONTAGENS.values())) + " \\\\", "\\midrule"]
    for rotulo, k, fmt in linhas:
        tex.append(" & ".join([rotulo] + [celula(vals[m][k], fmt) for m in MONTAGENS]) + " \\\\")
    tex += ["\\bottomrule", "\\end{tabular}"]
    saida.parent.mkdir(parents=True, exist_ok=True)
    saida.write_text("\n".join(tex) + "\n")

    # Falhas por nível e por rodada (P5: onde a equipe erra)
    for k, ms in rods.items():
        for m in ms:
            o = m["placar"].get("oculto", {})
            falhas = {n: o[n]["total"] - o[n]["ok"] for n in NIVEIS if n in o and o[n]["ok"] < o[n]["total"]}
            print(f"{k:8} r{m['rep']} oculto={m['placar'].get('oculto_ok')} falhas={falhas} motivo={m['motivo']}")
    print(json.dumps({k: {c: [round(x, 3) for x in v] for c, v in b.items()} for k, b in vals.items()}))
    for a, b in (("solo", "equipe"), ("solo", "revisor"), ("equipe", "revisor")):
        for c in ("dur", "tok", "usd", "oculto", "usd_teste"):
            if vals[a][c] and vals[b][c]:
                u, p = mann_whitney(vals[a][c], vals[b][c])
                print(f"{a} x {b} {c}: U={u} p={p:.2f}")
    print(f"tabela em {saida}")


if __name__ == "__main__":
    main()
