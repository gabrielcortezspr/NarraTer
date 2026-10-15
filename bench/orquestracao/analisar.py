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
    # O que o agente fez com as ferramentas: coordenar, verificar (rodar testes), ler, editar
    atividade = {"coordenacao": 0, "verificacao": 0, "leitura": 0, "edicao": 0, "outros": 0}
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
                cmd = entrada.get("command", "") if nome == "Bash" else ""
                if k or nome in ("ToolSearch", "Skill"):
                    atividade["coordenacao"] += 1
                elif "unittest" in cmd or "pytest" in cmd or "python3 -m pedidos" in cmd:
                    atividade["verificacao"] += 1
                elif nome in ("Edit", "Write", "MultiEdit", "NotebookEdit") or ESCRITA_BASH.search(cmd):
                    atividade["edicao"] += 1
                elif nome in ("Read", "Glob", "Grep") or re.match(r"\s*(cd [^&;]+(&&|;)\s*)?(cat|sed -n|head|tail|grep|ls|git (diff|status|log)|find|wc)\b", cmd):
                    atividade["leitura"] += 1
                else:
                    atividade["outros"] += 1
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
        "atividade": atividade,
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


def eventos(jsonl: Path):
    """Linha do tempo bruta de uma sessão: (ts, tipo, dados)."""
    out = []
    for linha in jsonl.read_text(errors="replace").splitlines():
        try:
            ev = json.loads(linha)
        except json.JSONDecodeError:
            continue
        if "timestamp" not in ev:
            continue
        t = ts(ev["timestamp"])
        msg = ev.get("message", {})
        if ev.get("type") == "assistant":
            for b in msg.get("content", []):
                if b.get("type") == "tool_use":
                    out.append((t, "chamada", {"id": b["id"], "nome": b["name"], "entrada": b.get("input", {})}))
            if msg.get("stop_reason") == "end_turn":
                out.append((t, "fim_turno", {}))
        elif ev.get("type") == "user":
            c = msg.get("content")
            if isinstance(c, str):
                for m in FALA.finditer(c):
                    out.append((t, "recebe", {"de": m.group(1).strip()}))
            elif isinstance(c, list):
                for b in c:
                    if b.get("type") == "text":
                        for m in FALA.finditer(b.get("text", "")):
                            out.append((t, "recebe", {"de": m.group(1).strip()}))
                    elif b.get("type") == "tool_result":
                        r = b.get("content")
                        r = r if isinstance(r, str) else json.dumps(r, ensure_ascii=False)
                        out.append((t, "resultado", {"id": b.get("tool_use_id"), "texto": r}))
    return sorted(out, key=lambda e: e[0])


def alvos(nome, entrada, labels):
    """Para quem uma chamada de mensageria do líder fala."""
    k = eh_mensageria(nome, entrada)
    if k not in ("ask", "send", "send_message", "ask_agent", "broadcast", "broadcast_message"):
        return []
    texto = json.dumps(entrada, ensure_ascii=False)
    if k.startswith("broadcast"):
        return list(labels)
    if "to" in entrada:
        return [entrada["to"]] if entrada["to"] in labels else []
    # CLI: narrater ask DevA "..." / ask --batch '{"DevA": ...}' — o primeiro rótulo citado após "ask"
    cmd = entrada.get("command", "")
    pos = cmd.find("narrater ask")
    trecho = cmd[pos:] if pos >= 0 else texto
    if "--batch" in trecho[:40]:
        return [l for l in labels if f'"{l}"' in trecho or f"\\\"{l}\\\"" in trecho]
    achados = sorted(((trecho.find(l), l) for l in labels if trecho.find(l) >= 0))
    return [achados[0][1]] if achados else []


def latencias(linhas: dict):
    """Entrega (líder pede → fala aparece no dev) e retorno (dev termina o turno → líder fica sabendo).

    Retorno: depois de cada fim de turno de um dev que recebeu uma fala, o primeiro momento em que
    o líder recebe uma fala desse dev ou o resultado de uma chamada de mensageria (ask, wait,
    check, check_messages, inbox) que cita o rótulo do dev.
    """
    lider = linhas.get("Narrador", [])
    devs = [l for l in linhas if l != "Narrador"]
    pedidos = [(t, d) for t, tipo, d in lider if tipo == "chamada"]
    entrega = []
    for dev in devs:
        envios = sorted(t for t, d in pedidos for a in alvos(d["nome"], d["entrada"], devs) if a == dev)
        usados = set()
        for t, tipo, d in linhas[dev]:
            if tipo == "recebe" and d["de"] == "Narrador":
                antes = [e for e in envios if e <= t + 1 and e not in usados]
                if antes:
                    usados.add(antes[-1])
                    entrega.append(max(0.0, t - antes[-1]))
    chamadas = {d["id"]: d for t, tipo, d in lider if tipo == "chamada"}
    aprende = []
    for t, tipo, d in lider:
        if tipo == "recebe":
            aprende.append((t, d["de"]))
        elif tipo == "resultado" and d["id"] in chamadas:
            c = chamadas[d["id"]]
            k = eh_mensageria(c["nome"], c["entrada"])
            if k in ("ask", "ask_agent", "wait", "check", "check_messages", "inbox"):
                for dev in devs:
                    if dev in d["texto"] or dev in json.dumps(c["entrada"]):
                        aprende.append((t, dev))
    retorno = []
    for dev in devs:
        trabalhando = False
        for t, tipo, d in linhas[dev]:
            if tipo == "recebe":
                trabalhando = True
            elif tipo == "fim_turno" and trabalhando:
                trabalhando = False
                depois = [ta for ta, de in aprende if de == dev and ta >= t - 0.5]
                if depois:
                    retorno.append(min(depois) - t)
    # Detecção: o líder já estava bloqueado esperando (ask/wait/ask_agent chamado antes do fim do
    # turno do dev e com resultado depois). É a latência do protocolo em si, sem o tempo que o
    # líder levou para ir buscar a resposta.
    resultados = {d["id"]: t for t, tipo, d in lider if tipo == "resultado"}
    bloqueios = []
    for t, tipo, d in lider:
        if tipo == "chamada" and eh_mensageria(d["nome"], d["entrada"]) in ("ask", "ask_agent", "wait") and d["id"] in resultados:
            bloqueios.append((t, resultados[d["id"]], d))
    # Um ask --batch ou um wait sem ids só volta quando o último termina: conta só o último fim de
    # turno dentro de cada bloqueio (os anteriores esperaram o colega, não o protocolo).
    fins = sorted(t for dev in devs for t, tipo, _ in linhas[dev] if tipo == "fim_turno")
    deteccao = []
    for ini, fim, c in bloqueios:
        dentro = [t for t in fins if ini < t <= fim + 0.5]
        if dentro:
            deteccao.append(max(0.0, fim - max(dentro)))
    # Fala de volta (modelo da versão A): o dev chama send/reply e a fala aparece no líder
    volta = []
    for dev in devs:
        envios = [t for t, tipo, d in linhas[dev] if tipo == "chamada"
                  and eh_mensageria(d["nome"], d["entrada"]) in ("send", "send_message", "reply", "reply_message")]
        for te in envios:
            rec = [t for t, tipo, d in lider if tipo == "recebe" and d["de"] == dev and t >= te - 1]
            if rec:
                volta.append(min(rec) - te)
    # Ocupação: um dev está trabalhando do momento em que recebe uma fala até o fim do turno.
    # Paralelo = tempo com os dois devs trabalhando ao mesmo tempo.
    intervalos = {}
    for dev in devs:
        ini = None
        for t, tipo, d in linhas[dev]:
            if tipo == "recebe" and ini is None:
                ini = t
            elif tipo == "fim_turno" and ini is not None:
                intervalos.setdefault(dev, []).append((ini, t))
                ini = None
    ocupado = {dev: round(sum(b - a for a, b in iv), 1) for dev, iv in intervalos.items()}
    paralelo = 0.0
    if len(devs) == 2 and all(d in intervalos for d in devs):
        for a0, a1 in intervalos[devs[0]]:
            for b0, b1 in intervalos[devs[1]]:
                paralelo += max(0.0, min(a1, b1) - max(a0, b0))
    # União dos intervalos: tempo com pelo menos um dev trabalhando (o resto é só o líder)
    todos = sorted(iv for ivs in intervalos.values() for iv in ivs)
    uniao, cur = 0.0, None
    for a, b in todos:
        if cur and a <= cur[1]:
            cur = (cur[0], max(cur[1], b))
        else:
            if cur:
                uniao += cur[1] - cur[0]
            cur = (a, b)
    if cur:
        uniao += cur[1] - cur[0]
    med = lambda xs: round(sorted(xs)[len(xs) // 2], 1) if xs else None
    return {"ocupado_s": ocupado, "algum_dev_s": round(uniao, 1), "paralelo_s": round(paralelo, 1),
            "rodadas_dev": {dev: len(iv) for dev, iv in intervalos.items()},
            "deteccao_s": [round(x, 1) for x in deteccao], "deteccao_mediana_s": med(deteccao),
            "volta_s": [round(x, 1) for x in volta], "volta_mediana_s": med(volta),"entrega_s": [round(x, 1) for x in entrega], "retorno_s": [round(x, 1) for x in retorno],
            "entrega_mediana_s": med(entrega), "retorno_mediana_s": med(retorno)}


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
    linhas = {}
    for label, sessoes in agentes(run).items():
        evs = []
        for _, sid in sessoes:
            for f in projetos.glob(f"*/{sid}.jsonl"):
                evs += eventos(f)
        linhas[label] = sorted(evs, key=lambda e: e[0])
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
        "latencias": latencias(linhas),
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
              f"edicoes_lider={m['total']['edicoes_lider']} entrega~{m['latencias']['entrega_mediana_s']}s "
              f"retorno~{m['latencias']['retorno_mediana_s']}s deteccao~{m['latencias']['deteccao_mediana_s']}s "
              f"volta~{m['latencias']['volta_mediana_s']}s")
        for label, a in m["agentes"].items():
            print(f"   {label:<9} {a['modelo']:<18} proc={a['tokens']['processados']:>9,} saida={a['tokens']['saida']:>6,} "
                  f"ctx0={a['contexto_inicial']:>6,} pico={a['contexto_pico']:>7,} api={a['chamadas_api']:>3} "
                  f"msg={a['mensageria']} rec={a['falas_recebidas']} ed={a['edicoes']} bashw={a['escritas_bash']}")


if __name__ == "__main__":
    main()
