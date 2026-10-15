#!/usr/bin/env python3
"""Métricas de uma rodada, lidas das transcrições do Claude Code (iguais em todas as versões).

Uso: analisar.py RUN [RUN...]   → grava RUN/metricas.json e imprime um resumo.

Tokens: cada chamada à API aparece em várias linhas do .jsonl (uma por bloco de conteúdo) com o
mesmo message.id e o mesmo usage; contamos cada id uma vez. "Processados" = entrada + cache lido +
cache criado: o tamanho do contexto que o modelo leu, que não muda se o cache acertou ou não.
"""

import json
import re
import sys
from datetime import datetime
from pathlib import Path

ESCRITA_BASH = re.compile(r"(cat\s*>|tee\s|sed\s+-i|open\([^)]*['\"]w['\"]|write_text|>\s*pedidos/|>\s*tests/|python3?\s+-\s*<<)")
# US$ por milhão de tokens, preço de API (skill claude-api, tabela de 2026-10-06). Cache lido = 0,1×
# a entrada (Opus e Sonnet 5.5 documentados em US$ 0,20); cache criado = 1,25× (TTL de 5 min) ou
# 2× (1 h), separados quando o usage traz cache_creation.ephemeral_*. É uma estimativa: a conta do
# experimento é de assinatura, não paga por token.
PRECOS = {
    "claude-opus-5-5": {"entrada": 4.00, "saida": 20.00, "cache_lido": 0.20},
    "claude-sonnet-5-5": {"entrada": 2.00, "saida": 10.00, "cache_lido": 0.20},
    "claude-haiku-5-5": {"entrada": 0.10, "saida": 0.50, "cache_lido": 0.01},
}

FALA = re.compile(r"\[narrater from ([^\]#(]+)")


def ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()


def agentes(run: Path):
    """label → lista de (modelo, session id), na ordem de início."""
    out = {}
    for linha in (run / "agentes.log").read_text().splitlines():
        p = linha.split()
        if len(p) >= 4:
            out.setdefault(p[1], []).append((p[2], p[3]))
    return out


def eh_mensageria(nome, entrada):
    if nome.startswith("mcp__narrater__"):
        return nome.removeprefix("mcp__narrater__")
    if nome == "Bash":
        cmd = entrada.get("command", "")
        m = re.search(r"(?:^|[;&|\s])narrater\s+([a-z-]+)", cmd)
        if m:
            return m.group(1)
    return None


def metricas_sessao(jsonl: Path):
    vistos = set()
    tok = {"entrada": 0, "saida": 0, "cache_lido": 0, "cache_criado": 0, "cache_criado_1h": 0}
    contextos = []
    ferramentas = {}
    mensageria = {}
    edicoes = 0
    escritas_bash = 0
    compactacoes = 0
    recebidas = {}
    prompts = 0
    tempos = []
    erros_ask = 0
    for linha in jsonl.read_text(errors="replace").splitlines():
        try:
            ev = json.loads(linha)
        except json.JSONDecodeError:
            continue
        if "timestamp" in ev:
            tempos.append(ts(ev["timestamp"]))
        tipo = ev.get("type")
        if tipo == "system" and (ev.get("subtype") == "compact_boundary"):
            compactacoes += 1
        if ev.get("isCompactSummary"):
            compactacoes += 0  # o boundary já contou
        if tipo == "user":
            c = ev.get("message", {}).get("content")
            textos = []
            if isinstance(c, str):
                textos = [c]
            elif isinstance(c, list):
                for b in c:
                    if b.get("type") == "text":
                        textos.append(b.get("text", ""))
                    elif b.get("type") == "tool_result":
                        r = b.get("content")
                        r = r if isinstance(r, str) else json.dumps(r, ensure_ascii=False)
                        if re.search(r"timed? ?out|timeout", r, re.I) and "narrater" in r.lower():
                            erros_ask += 1
            for t in textos:
                if ev.get("isMeta") or t.startswith("<command-") or t.startswith("<local-command"):
                    continue
                prompts += 1
                for m in FALA.finditer(t):
                    de = m.group(1).strip()
                    recebidas[de] = recebidas.get(de, 0) + 1
        if tipo != "assistant":
            continue
        msg = ev.get("message", {})
        for b in msg.get("content", []):
            if b.get("type") == "tool_use":
                nome, entrada = b.get("name", ""), b.get("input", {})
                ferramentas[nome] = ferramentas.get(nome, 0) + 1
                if nome in ("Edit", "Write", "MultiEdit", "NotebookEdit"):
                    edicoes += 1
                if nome == "Bash" and ESCRITA_BASH.search(entrada.get("command", "")):
                    escritas_bash += 1
                k = eh_mensageria(nome, entrada)
                if k:
                    mensageria[k] = mensageria.get(k, 0) + 1
        mid = msg.get("id")
        if not mid or mid in vistos or not msg.get("usage"):
            continue
        vistos.add(mid)
        u = msg["usage"]
        e, cr, cc, s = (u.get("input_tokens", 0), u.get("cache_read_input_tokens", 0),
                        u.get("cache_creation_input_tokens", 0), u.get("output_tokens", 0))
        tok["entrada"] += e
        tok["cache_lido"] += cr
        tok["cache_criado"] += cc
        tok["cache_criado_1h"] += (u.get("cache_creation") or {}).get("ephemeral_1h_input_tokens", 0)
        tok["saida"] += s
        contextos.append(e + cr + cc)
    tok["processados"] = tok["entrada"] + tok["cache_lido"] + tok["cache_criado"]
    return {
        "tokens": tok,
        "chamadas_api": len(vistos),
        "contexto_inicial": contextos[0] if contextos else 0,
        "contexto_pico": max(contextos) if contextos else 0,
        "contexto_final": contextos[-1] if contextos else 0,
        "compactacoes": compactacoes,
        "prompts_recebidos": prompts,
        "falas_recebidas": recebidas,
        "ferramentas": ferramentas,
        "mensageria": mensageria,
        "edicoes": edicoes,
        "escritas_bash": escritas_bash,
        "avisos_timeout": erros_ask,
        "primeiro": min(tempos) if tempos else None,
        "ultimo": max(tempos) if tempos else None,
    }


def custo(modelo, t):
    p = PRECOS.get(modelo)
    if not p:
        return None
    c1h = t.get("cache_criado_1h", 0)
    c5m = t["cache_criado"] - c1h
    return round((t["entrada"] * p["entrada"] + t["saida"] * p["saida"] + t["cache_lido"] * p["cache_lido"]
                  + c5m * p["entrada"] * 1.25 + c1h * p["entrada"] * 2) / 1e6, 4)


def soma(dicts):
    out = {}
    for d in dicts:
        for k, v in d.items():
            if isinstance(v, (int, float)):
                out[k] = out.get(k, 0) + v
    return out


def diff_stats(patch: Path):
    add = rem = 0
    arquivos = set()
    if patch.exists():
        for l in patch.read_text(errors="replace").splitlines():
            if l.startswith("+++ b/"):
                arquivos.add(l[6:])
            elif l.startswith("+") and not l.startswith("+++"):
                add += 1
            elif l.startswith("-") and not l.startswith("---"):
                rem += 1
    return {"linhas_adicionadas": add, "linhas_removidas": rem, "arquivos": sorted(arquivos)}


def analisa(run: Path):
    rodada = json.loads((run / "rodada.json").read_text())
    placar = json.loads((run / "placar.json").read_text() or "{}")
    projetos = run / "home/.claude/projects"
    por_agente = {}
    for label, sessoes in agentes(run).items():
        ms = []
        for modelo, sid in sessoes:
            achados = list(projetos.glob(f"*/{sid}.jsonl"))
            if achados:
                m = metricas_sessao(achados[0])
                m["modelo"] = modelo
                ms.append(m)
        if not ms:
            continue
        m = ms[0] if len(ms) == 1 else {
            **ms[-1],
            "tokens": soma([x["tokens"] for x in ms]),
            "chamadas_api": sum(x["chamadas_api"] for x in ms),
            "contexto_pico": max(x["contexto_pico"] for x in ms),
            "edicoes": sum(x["edicoes"] for x in ms),
        }
        m["sessoes"] = len(ms)
        m["custo_usd"] = custo(m["modelo"], m["tokens"])
        por_agente[label] = m
    total_tok = soma([a["tokens"] for a in por_agente.values()])
    mens = soma([a["mensageria"] for a in por_agente.values()])
    out = {
        **rodada,
        "placar": placar,
        "diff": diff_stats(run / "diff.patch"),
        "agentes": por_agente,
        "total": {
            "tokens": total_tok,
            "custo_usd": round(sum(a["custo_usd"] or 0 for a in por_agente.values()), 4),
            "chamadas_api": sum(a["chamadas_api"] for a in por_agente.values()),
            "mensageria": mens,
            "mensagens_entre_agentes": sum(sum(a["falas_recebidas"].values()) for a in por_agente.values()),
            "compactacoes": sum(a["compactacoes"] for a in por_agente.values()),
            "edicoes_lider": por_agente.get("Narrador", {}).get("edicoes", 0),
            "escritas_bash_lider": por_agente.get("Narrador", {}).get("escritas_bash", 0),
        },
    }
    (run / "metricas.json").write_text(json.dumps(out, ensure_ascii=False, indent=1))
    return out


def main():
    for arg in sys.argv[1:]:
        m = analisa(Path(arg))
        t = m["total"]["tokens"]
        p = m["placar"]
        print(f"v{m['versao']} r{m['rep']} {m['motivo']:<10} dur={m['duracao_s'] and round(m['duracao_s'])}s "
              f"oculto={p.get('oculto_ok')}/{p.get('oculto_total')} proprios={p.get('proprios_ok')}/{p.get('proprios_total')} "
              f"US${m['total']['custo_usd']:.2f} proc={t.get('processados', 0):,} saida={t.get('saida', 0):,} msgs={m['total']['mensagens_entre_agentes']} "
              f"edicoes_lider={m['total']['edicoes_lider']}")
        for label, a in m["agentes"].items():
            print(f"   {label:<9} {a['modelo']:<18} proc={a['tokens']['processados']:>9,} saida={a['tokens']['saida']:>6,} "
                  f"ctx0={a['contexto_inicial']:>6,} pico={a['contexto_pico']:>7,} api={a['chamadas_api']:>3} "
                  f"msg={a['mensageria']} rec={a['falas_recebidas']} ed={a['edicoes']} bashw={a['escritas_bash']}")


if __name__ == "__main__":
    main()
