#!/usr/bin/env python3
"""Braço "sozinho" contra a equipe: tabela para docs/relatorios/quando-orquestrar.tex.

Uso: solo.py SOLO_DIR EQUIPE_DIR [SAIDA]
  SOLO_DIR   pasta com vH-solo-r*/metricas.json (ex. $BENCH/runs ou resultados/2026-10-16)
  EQUIPE_DIR pasta com vG-r*/metricas.json e vF-r*/metricas.json (resultados/2026-10-15)
  SAIDA      padrão docs/relatorios/dados/solo.tex

Mesmas métricas do estudo de versões (analisar.py); só as que existem com um agente só.
"""

import json
import statistics as st
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from agregar import mann_whitney  # noqa: E402

RAIZ = Path(__file__).resolve().parents[2]


def rodadas(pasta: Path, padrao: str):
    out = []
    for f in sorted(pasta.glob(f"{padrao}/metricas.json")):
        m = json.loads(f.read_text())
        if m["rep"] in (1, 2, 3):
            out.append(m)
    return out


def valores(ms):
    v = {"dur": [], "tok": [], "usd": [], "api": [], "oculto": [], "diff": [], "pedidos": []}
    for m in ms:
        t = m["total"]
        v["dur"].append(m["duracao_s"] / 60)
        v["tok"].append(t["tokens"]["processados"] / 1e6)
        v["usd"].append(t["custo_usd"])
        v["api"].append(t["chamadas_api"])
        v["oculto"].append(m["placar"].get("oculto_ok", 0))
        v["diff"].append(m["diff"]["linhas_adicionadas"] + m["diff"]["linhas_removidas"])
        v["pedidos"].append(m["agentes"].get("Narrador", {}).get("pedidos", 0))
    return v


def br(x, fmt):
    return fmt.format(x).replace(".", "{,}")


def celula(xs, fmt):
    if not xs:
        return "---"
    return f"{br(st.mean(xs), fmt)} ({br(min(xs), fmt)}--{br(max(xs), fmt)})"


def main():
    solo_dir, equipe_dir = Path(sys.argv[1]), Path(sys.argv[2])
    saida = Path(sys.argv[3]) if len(sys.argv) > 3 else RAIZ / "docs/relatorios/dados/solo.tex"
    braços = {"Sozinho (H)": valores(rodadas(solo_dir, "vH-solo-r*")),
              "Equipe (G)": valores(rodadas(equipe_dir, "vG-r*")),
              "Equipe (F)": valores(rodadas(equipe_dir, "vF-r*"))}
    linhas = [("Testes escondidos (de 31)", "oculto", "{:.0f}"), ("Duração (min)", "dur", "{:.1f}"),
              ("Tokens processados (milhões)", "tok", "{:.2f}"), ("Custo de API equivalente (US\\$)", "usd", "{:.2f}"),
              ("Chamadas à API (todos os agentes)", "api", "{:.0f}"), ("Pedidos do Narrador a outros", "pedidos", "{:.0f}"),
              ("Linhas no diff", "diff", "{:.0f}")]
    nomes = list(braços)
    tex = ["\\begin{tabular}{l" + "r" * len(nomes) + "}", "\\toprule",
           " & ".join([""] + nomes) + " \\\\", "\\midrule"]
    for rotulo, k, fmt in linhas:
        tex.append(" & ".join([rotulo] + [celula(braços[n][k], fmt) for n in nomes]) + " \\\\")
    tex += ["\\bottomrule", "\\end{tabular}"]
    saida.parent.mkdir(parents=True, exist_ok=True)
    saida.write_text("\n".join(tex) + "\n")

    s, g = braços["Sozinho (H)"], braços["Equipe (G)"]
    print(json.dumps({n: {k: [round(x, 3) for x in v] for k, v in b.items()} for n, b in braços.items()}, indent=1))
    for k in ("dur", "tok", "usd", "api"):
        if s[k] and g[k]:
            u, p = mann_whitney(s[k], g[k])
            print(f"sozinho x G  {k}: U={u} p={p:.2f}")
    print(f"tabela em {saida}")


if __name__ == "__main__":
    main()
