#!/usr/bin/env python3
"""Junta as métricas de todas as rodadas em CSVs e tabelas LaTeX para o relatório.

Uso: agregar.py [SAIDA]   (padrão: docs/relatorios/dados)
Lê $BENCH_RUNS/v?-r*/metricas.json (padrão $BENCH/runs; rodada 0 = piloto, fica de fora).
"""

import csv
import json
import os
import statistics as st
import sys
from pathlib import Path

BENCH = Path(os.environ.get("BENCH", "/home/user/bench"))
# As rodadas brutas ficam fora do repositório; uma cópia das métricas de cada estudo fica em
# resultados/<data>/ (BENCH_RUNS=bench/orquestracao/resultados/2026-10-15 refaz as tabelas dela)
RUNS = Path(os.environ.get("BENCH_RUNS", BENCH / "runs"))
RAIZ = Path(__file__).resolve().parents[2]
SAIDA = Path(sys.argv[1]) if len(sys.argv) > 1 else RAIZ / "docs/relatorios/dados"
VERSOES = {"A": "e708d7b", "B": "16cee0e", "C": "95fb11a", "D": "db3d535", "E": "8e2e5fa", "E-sl0": "8e2e5fa sem statusline", "F": "26e3c06"}
ORDEM = list(VERSOES)
# Rodadas que entram: 1 a 3 de cada versão (desenho do estudo). A rodada 0 é piloto; as extras da D
# (r4, D-sl0) foram interrompidas a pedido e ficam fora.
REPS = (1, 2, 3)
AGENTES = ("Narrador", "DevA", "DevB")
CONSULTAS = {"check", "inbox", "wait", "asks", "list", "list_peers", "check_messages", "whoami", "peers",
             "canvas_list_nodes", "status"}


def chave(m):
    return m["versao"] + (f"-{m['variante']}" if m.get("variante") else "")


def carrega():
    rodadas = []
    for f in sorted(RUNS.glob("v*-r*/metricas.json")):
        m = json.loads(f.read_text())
        if m["rep"] in REPS and chave(m) in VERSOES:
            m["versao"] = chave(m)
            rodadas.append(m)
    return rodadas


def linha(m):
    t = m["total"]
    p = m["placar"]
    o = p.get("oculto", {})
    row = {
        "versao": m["versao"], "rep": m["rep"], "idx": ORDEM.index(m["versao"]), "concluiu": int(m["motivo"] == "concluida"),
        "duracao_min": round(m["duracao_s"] / 60, 2) if m["duracao_s"] else "",
        "oculto_ok": p.get("oculto_ok", 0), "oculto_total": p.get("oculto_total", 0),
        **{f"oculto_{k.lower()}": o.get(k, {}).get("ok", 0) for k in ("Basico", "Bordas", "Defeitos", "Regressao")},
        "proprios_ok": p.get("proprios_ok", 0), "proprios_total": p.get("proprios_total", 0),
        "processados_k": round(t["tokens"].get("processados", 0) / 1000, 1),
        "saida_k": round(t["tokens"].get("saida", 0) / 1000, 1),
        "cache_lido_k": round(t["tokens"].get("cache_lido", 0) / 1000, 1),
        "cache_criado_k": round(t["tokens"].get("cache_criado", 0) / 1000, 1),
        "custo_usd": t["custo_usd"], "chamadas_api": t["chamadas_api"],
        "mensagens": t["mensagens_entre_agentes"],
        # Chamadas de coordenação: toda chamada à CLI/MCP do NarraTer; consultas = as que só
        # perguntam estado (check, inbox, wait, list...), que é onde um protocolo ruim gasta turnos
        "coordenacao": sum(t["mensageria"].values()),
        "consultas": sum(v for k, v in t["mensageria"].items() if k in CONSULTAS), "compactacoes": t["compactacoes"],
        "edicoes_lider": t["edicoes_lider"], "escritas_bash_lider": t["escritas_bash_lider"],
        "entrega_s": m["latencias"]["entrega_mediana_s"] if m["latencias"]["entrega_mediana_s"] is not None else "",
        "deteccao_s": m["latencias"]["deteccao_mediana_s"] if m["latencias"]["deteccao_mediana_s"] is not None else "",
        "volta_s": m["latencias"]["volta_mediana_s"] if m["latencias"]["volta_mediana_s"] is not None else "",
        "retorno_s": m["latencias"]["retorno_mediana_s"] if m["latencias"]["retorno_mediana_s"] is not None else "",
        "paralelo_frac": round(m["latencias"]["paralelo_s"] / m["duracao_s"], 2) if m["duracao_s"] else "",
        "ocupacao_devs_frac": round(sum(m["latencias"]["ocupado_s"].values()) / (2 * m["duracao_s"]), 2) if m["duracao_s"] else "",
        "so_lider_min": round((m["duracao_s"] - m["latencias"]["algum_dev_s"]) / 60, 2) if m["duracao_s"] else "",
        "algum_dev_min": round(m["latencias"]["algum_dev_s"] / 60, 2),
        "turnos_devs": sum(m["latencias"]["rodadas_dev"].values()),
        "asks_prematuros": (m.get("prematuros") or {}).get("prematuros", "") if m.get("prematuros") else "",
        "pedidos_lider": m["agentes"].get("Narrador", {}).get("pedidos", 0),
        "repeticoes_tela": m["agentes"].get("Narrador", {}).get("repeticoes_tela", 0),
        "linhas_diff": m["diff"]["linhas_adicionadas"] + m["diff"]["linhas_removidas"],
    }
    for a in AGENTES:
        ag = m["agentes"].get(a, {})
        row[f"{a}_processados_k"] = round(ag.get("tokens", {}).get("processados", 0) / 1000, 1)
        row[f"{a}_saida_k"] = round(ag.get("tokens", {}).get("saida", 0) / 1000, 1)
        row[f"{a}_ctx0_k"] = round(ag.get("contexto_inicial", 0) / 1000, 1)
        row[f"{a}_pico_k"] = round(ag.get("contexto_pico", 0) / 1000, 1)
        row[f"{a}_api"] = ag.get("chamadas_api", 0)
        row[f"{a}_edicoes"] = ag.get("edicoes", 0)
        row[f"{a}_custo"] = ag.get("custo_usd") or 0
        for k in ("coordenacao", "verificacao", "leitura", "edicao"):
            row[f"{a}_{k}"] = ag.get("atividade", {}).get(k, 0)
    return row


def mann_whitney(x, y):
    """U e p bilateral exato (permutação) — sem scipy. Com 3 contra 3, o menor p possível é 0,10."""
    from itertools import combinations
    def u(a, b):
        return sum((ai > bi) + 0.5 * (ai == bi) for ai in a for bi in b)
    obs = u(x, y)
    todos = list(x) + list(y)
    n = len(x)
    centro = len(x) * len(y) / 2
    extremos = total = 0
    for idx in combinations(range(len(todos)), n):
        a = [todos[i] for i in idx]
        b = [todos[i] for i in range(len(todos)) if i not in idx]
        total += 1
        if abs(u(a, b) - centro) >= abs(obs - centro) - 1e-9:
            extremos += 1
    return obs, extremos / total


def resumo(rows, campo):
    vals = [r[campo] for r in rows if r[campo] != ""]
    if not vals:
        return ("", "", "", "")
    return (round(st.mean(vals), 2), min(vals), max(vals), round(st.stdev(vals), 2) if len(vals) > 1 else 0)


def main():
    rodadas = carrega()
    if not rodadas:
        sys.exit("nenhuma rodada com metricas.json")
    SAIDA.mkdir(parents=True, exist_ok=True)
    rows = [linha(m) for m in rodadas]
    with open(SAIDA / "rodadas.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0]))
        w.writeheader()
        w.writerows(rows)

    campos = [k for k in rows[0] if k not in ("versao", "rep", "idx")]
    por_versao = []
    for v in VERSOES:
        rs = [r for r in rows if r["versao"] == v]
        if not rs:
            continue
        d = {"versao": v, "idx": ORDEM.index(v), "n": len(rs)}
        for c in campos:
            media, mn, mx, dp = resumo(rs, c)
            d[c] = media
            d[c + "_min"] = mn
            d[c + "_max"] = mx
            d[c + "_dp"] = dp
        por_versao.append(d)
    with open(SAIDA / "versoes.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(por_versao[0]))
        w.writeheader()
        w.writerows(por_versao)

    # Tabelas LaTeX: média e (mín–máx) por versão
    def cel(d, c, fmt="{:.1f}"):
        if d[c] == "":
            return "--"
        a, b = d[c + "_min"], d[c + "_max"]
        return f"{fmt.format(d[c])} \\tiny({fmt.format(a)}--{fmt.format(b)})"

    # Dois grupos de tabelas: o estudo original (A–D) e o que veio depois (D como referência, E, F…).
    # Sufixo "" e "depois" no nome do arquivo e da macro LaTeX.
    grupos = {"": ["A", "B", "C", "D"], "depois": ["D", "E", "E-sl0", "F"]}
    with open(SAIDA / "colunas.tex", "w") as f:
        for suf, vs in grupos.items():
            ds = [d for d in por_versao if d["versao"] in vs]
            f.write(f"\\newcommand{{\\colspec{suf}}}{{@{{}}l" + "l" * len(ds) + "@{}}\n")
            f.write(f"\\newcommand{{\\cabecalho{suf}}}{{ & " + " & ".join(f"\\versao{{{d['versao']}}}" for d in ds) + " \\\\}\n")

    def tabela(nome, linhas):
        for suf, vs in grupos.items():
            ds = [d for d in por_versao if d["versao"] in vs]
            with open(SAIDA / f"{nome}{'-' + suf if suf else ''}.tex", "w") as f:
                for rotulo, c, fmt in linhas:
                    f.write(rotulo + " & " + " & ".join(cel(d, c, fmt) for d in ds) + " \\\\\n")

    tabela("tab-resultado", [
        ("Concluiu (fração)", "concluiu", "{:.2f}"),
        ("Duração (min)", "duracao_min", "{:.1f}"),
        ("Testes escondidos (de 31)", "oculto_ok", "{:.1f}"),
        ("\\quad Básico (7)", "oculto_basico", "{:.1f}"),
        ("\\quad Bordas (12)", "oculto_bordas", "{:.1f}"),
        ("\\quad Defeitos (5)", "oculto_defeitos", "{:.1f}"),
        ("\\quad Regressão (7)", "oculto_regressao", "{:.1f}"),
        ("Testes da equipe", "proprios_total", "{:.0f}"),
        ("Linhas no diff", "linhas_diff", "{:.0f}"),
    ])
    tabela("tab-tokens", [
        ("Tokens processados (mil)", "processados_k", "{:.0f}"),
        ("\\quad Narrador", "Narrador_processados_k", "{:.0f}"),
        ("\\quad DevA", "DevA_processados_k", "{:.0f}"),
        ("\\quad DevB", "DevB_processados_k", "{:.0f}"),
        ("Tokens de saída (mil)", "saida_k", "{:.1f}"),
        ("Custo de API equivalente (US\\$)", "custo_usd", "{:.2f}"),
        ("Chamadas à API", "chamadas_api", "{:.0f}"),
    ])
    tabela("tab-contexto", [
        ("Contexto inicial, Narrador (mil)", "Narrador_ctx0_k", "{:.1f}"),
        ("Contexto inicial, DevA (mil)", "DevA_ctx0_k", "{:.1f}"),
        ("Contexto inicial, DevB (mil)", "DevB_ctx0_k", "{:.1f}"),
        ("Pico de contexto, Narrador (mil)", "Narrador_pico_k", "{:.1f}"),
        ("Pico de contexto, DevA (mil)", "DevA_pico_k", "{:.1f}"),
        ("Pico de contexto, DevB (mil)", "DevB_pico_k", "{:.1f}"),
        ("Compactações", "compactacoes", "{:.1f}"),
    ])
    tabela("tab-coordenacao", [
        ("Mensagens entregues na tela de um agente", "mensagens", "{:.1f}"),
        ("Chamadas de coordenação (CLI/MCP)", "coordenacao", "{:.1f}"),
        ("\\quad das quais consultas de estado", "consultas", "{:.1f}"),
        ("Chamadas à API do Narrador", "Narrador_api", "{:.1f}"),
        ("Entrega (s)", "entrega_s", "{:.1f}"),
        ("Detecção (s)", "deteccao_s", "{:.1f}"),
        ("Volta, só A (s)", "volta_s", "{:.1f}"),
        ("Retorno até o líder saber (s)", "retorno_s", "{:.1f}"),
        ("Tempo com algum dev trabalhando (min)", "algum_dev_min", "{:.1f}"),
        ("Tempo só do Narrador (min)", "so_lider_min", "{:.1f}"),
        ("Turnos de trabalho dos devs (soma)", "turnos_devs", "{:.1f}"),
        ("Ocupação média dos devs", "ocupacao_devs_frac", "{:.2f}"),
        ("Fração com os dois devs ativos", "paralelo_frac", "{:.2f}"),
        ("Pedidos do Narrador aos devs", "pedidos_lider", "{:.1f}"),
        ("\\quad repetidos por resposta cortada", "repeticoes_tela", "{:.1f}"),
        ("Asks prontos antes da hora (C+)", "asks_prematuros", "{:.1f}"),
        ("Narrador: chamadas de coordenação", "Narrador_coordenacao", "{:.1f}"),
        ("Narrador: rodou os testes", "Narrador_verificacao", "{:.1f}"),
        ("Narrador: leituras de código", "Narrador_leitura", "{:.1f}"),
        ("Narrador: edições (inclui via Bash)", "Narrador_edicao", "{:.1f}"),
        ("DevA: chamadas Edit/Write", "DevA_edicoes", "{:.1f}"),
        ("DevB: chamadas Edit/Write", "DevB_edicoes", "{:.1f}"),
    ])
    # Testes entre pares de versões nas métricas principais, nos mesmos dois grupos das tabelas
    pares_grupos = {"": [("A", "B"), ("B", "C"), ("C", "D"), ("A", "D")],
                    "depois": [("D", "E"), ("E", "E-sl0"), ("D", "F"), ("E", "F"), ("A", "F")]}
    tem = {r["versao"] for r in rows}
    metricas = [("Duração", "duracao_min"), ("Tokens processados", "processados_k"), ("Custo", "custo_usd"),
                ("Pedidos do Narrador", "pedidos_lider"), ("Turnos dos devs", "turnos_devs")]
    with open(SAIDA / "colunas-testes.tex", "w") as fc, open(SAIDA / "testes.csv", "w") as g:
        g.write("metrica,par,U,p\n")
        for suf, pares in pares_grupos.items():
            pares = [p for p in pares if p[0] in tem and p[1] in tem]
            fc.write(f"\\newcommand{{\\colspectestes{suf}}}{{@{{}}l" + "l" * len(pares) + "@{}}\n")
            fc.write(f"\\newcommand{{\\cabecalhotestes{suf}}}{{ & " + " & ".join(f"{a}$\\times${b}" for a, b in pares) + " \\\\}\n")
            with open(SAIDA / f"tab-testes{'-' + suf if suf else ''}.tex", "w") as f:
                for rotulo, c in metricas:
                    cels = []
                    for a, b in pares:
                        xa = [r[c] for r in rows if r["versao"] == a and r[c] != ""]
                        xb = [r[c] for r in rows if r["versao"] == b and r[c] != ""]
                        U, pv = mann_whitney(xa, xb)
                        cels.append(f"{U:g} / {pv:.2f}")
                        g.write(f"{c},{a}-{b},{U:g},{pv:.3f}\n")
                    f.write(rotulo + " & " + " & ".join(cels) + " \\\\\n")
    print(f"{len(rows)} rodadas → {SAIDA}")
    for d in por_versao:
        print(f"v{d['versao']} n={d['n']} dur={d['duracao_min']} oculto={d['oculto_ok']} proc={d['processados_k']}k "
              f"US${d['custo_usd']} msgs={d['mensagens']}")


if __name__ == "__main__":
    main()
