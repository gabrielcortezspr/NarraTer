#!/usr/bin/env python3
"""Uma rodada do benchmark de orquestração (docs/pesquisa/comparacao-versoes.md).

Uso: rodar.py VERSAO REP   (VERSAO em A B C D; binários em $BENCH/bin/vX, fontes em $BENCH/vX)

Cada rodada começa zerada: HOME próprio (config do Claude, skills, sessões, ~/.config/narrater),
servidor tmux próprio, pasta de trabalho copiada do fixture, ambiente mínimo (só login e proxy).
O mesmo wrapper `claude` em ~/.local/bin fixa o modelo de cada agente pelo NARRATER_LABEL,
dá um --session-id conhecido e entrega o pedido ao Narrador como prompt inicial: o mesmo
caminho em todas as versões, sem digitar na interface.
"""

import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

AQUI = Path(__file__).resolve().parent
BENCH = Path(os.environ.get("BENCH", "/home/user/bench"))
CLAUDE_REAL = shutil.which("claude", path="/opt/node22/bin") or "/opt/node22/bin/claude"
DISPLAY = os.environ.get("BENCH_DISPLAY", ":77")
LIMITE_S = int(os.environ.get("BENCH_LIMITE_S", 40 * 60))
ATRASO_LIDER_S = 25

MODELOS = {"Narrador": "claude-opus-5-5", "DevA": "claude-sonnet-5-5", "DevB": "claude-haiku-5-5"}
FIM = "TAREFA CONCLUIDA"

PEDIDO = (
    "Você é o Narrador, líder de uma equipe com DevA e DevB (terminais conectados a você no NarraTer). "
    "Projeto: o repositório Python em {work}. Tarefa: (1) implementar docs/PEDIDO-MULTIMOEDA.md; "
    "(2) investigar e corrigir as duas reclamações de docs/RECLAMACOES.md, seguindo REGRAS.md. "
    "Distribua o trabalho entre DevA e DevB; você coordena, revisa e integra. Critério de pronto: todos os "
    "testes passam (cd {work} && python3 -m unittest discover -s tests -t .) e a funcionalidade nova tem testes. "
    "Ninguém vai responder perguntas durante o trabalho: decida com base nos documentos. Quando terminar e tiver "
    "confirmado os testes, escreva uma última mensagem com um resumo curto e a linha " + FIM + "."
)

ENV_PASSA = [
    "ANTHROPIC_BASE_URL", "HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "NO_PROXY", "no_proxy",
    "NODE_EXTRA_CA_CERTS", "SSL_CERT_FILE", "REQUESTS_CA_BUNDLE", "LANG",
]

WRAPPER = r"""#!/bin/sh
# Wrapper do benchmark: modelo fixo por agente, sessão conhecida, pedido inicial ao Narrador.
RUN="$(dirname "$HOME")"
case "$NARRATER_LABEL" in
  Narrador) M=__M_NARRADOR__ ;;
  DevA) M=__M_DEVA__ ;;
  DevB) M=__M_DEVB__ ;;
  *) M=__M_NARRADOR__ ;;
esac
SID=$(cat /proc/sys/kernel/random/uuid)
echo "$(date +%s.%N) $NARRATER_LABEL $M $SID $(pwd)" >> "$RUN/agentes.log"
[ "$(cat "$RUN/versao")" = A ] && cd "$(cat "$RUN/work_path")"
if [ "$NARRATER_LABEL" = Narrador ] && [ ! -e "$RUN/inicio" ]; then
  sleep __ATRASO__
  date +%s.%N > "$RUN/inicio"
  exec __CLAUDE__ "$(cat "$RUN/pedido.txt")" --model "$M" --session-id "$SID" "$@"
fi
exec __CLAUDE__ --model "$M" --session-id "$SID" "$@"
"""


def papeis(versao: str) -> dict:
    """Texto dos papéis padrão (leader/coder) da própria versão, como o seletor de agentes os copia."""
    fonte = (BENCH / f"v{versao}" / "src-tauri/src/roles.rs").read_text()
    if versao == "A":
        textos = re.findall(r'instructions: "((?:[^"\\]|\\.)*)"\.to_string\(\)', fonte)
        lider, dev = textos[0], textos[1]
    else:
        lider = re.search(r'const LEADER_INSTRUCTIONS: &str = "((?:[^"\\]|\\.)*)";', fonte).group(1)
        dev = re.search(r'const DEVELOPER_INSTRUCTIONS: &str = "((?:[^"\\]|\\.)*)";', fonte).group(1)
    desc = lambda s: s.replace('\\"', '"').replace("\\n", "\n")
    return {"leader": desc(lider), "coder": desc(dev)}


def cena(versao: str, work: Path) -> dict:
    p = papeis(versao)
    def no(nid, label, role, role_name, color, x, y):
        n = {
            "id": nid, "node_type": "terminal", "x": x, "y": y, "width": 560, "height": 380,
            "agent_type": "claude", "label": label, "role_id": role, "role_name": role_name,
            "role_color": color, "instructions": p[role], "skip_permissions": True,
        }
        if versao != "A":
            n["cwd"] = str(work)
        return n
    nodes = [
        no("terminal-narrador", "Narrador", "leader", "Leader", "#8b5cf6", 0, 0),
        no("terminal-deva", "DevA", "coder", "Developer", "#3b82f6", 640, -220),
        no("terminal-devb", "DevB", "coder", "Developer", "#3b82f6", 640, 220),
    ]
    pares = [("terminal-narrador", "terminal-deva"), ("terminal-narrador", "terminal-devb")]
    if versao == "A":  # arestas dirigidas: rota de volta explícita
        pares += [(b, a) for a, b in pares]
    edges = [{"id": f"edge-{i}", "source": a, "target": b, "edge_type": "agent-pipe"} for i, (a, b) in enumerate(pares)]
    return {"nodes": nodes, "edges": edges}


def prepara(versao: str, rep: int) -> Path:
    run = BENCH / "runs" / f"v{versao}-r{rep}"
    if run.exists():
        shutil.rmtree(run)
    home, work = run / "home", run / "work" / "pedidos"
    (home / ".local/bin").mkdir(parents=True)
    (home / ".config/narrater/scenes").mkdir(parents=True)
    (run / "tmux").mkdir()
    shutil.copytree(AQUI / "fixture/pedidos", work, ignore=shutil.ignore_patterns("__pycache__"))
    git = ["git", "-c", "user.name=bench", "-c", "user.email=bench@local"]
    subprocess.run(git + ["init", "-q"], cwd=work, check=True)
    subprocess.run(git + ["add", "-A"], cwd=work, check=True)
    subprocess.run(git + ["commit", "-qm", "estado inicial"], cwd=work, check=True)

    (run / "versao").write_text(versao)
    (run / "work_path").write_text(str(work))
    (run / "pedido.txt").write_text(PEDIDO.format(work=work))
    w = (WRAPPER.replace("__M_NARRADOR__", MODELOS["Narrador"]).replace("__M_DEVA__", MODELOS["DevA"])
         .replace("__M_DEVB__", MODELOS["DevB"]).replace("__ATRASO__", str(ATRASO_LIDER_S))
         .replace("__CLAUDE__", CLAUDE_REAL))
    (home / ".local/bin/claude").write_text(w)
    (home / ".local/bin/claude").chmod(0o755)
    # Onboarding do Claude já feito e a pasta de trabalho confiável (o resto a versão faz sozinha)
    (home / ".claude.json").write_text(json.dumps({
        "hasCompletedOnboarding": True, "theme": "dark", "bypassPermissionsModeAccepted": True,
        "projects": {str(work): {"hasTrustDialogAccepted": True}},
    }))
    (home / ".config/narrater/scenes/default.json").write_text(json.dumps(cena(versao, work), ensure_ascii=False, indent=1))
    return run


def ambiente(run: Path) -> dict:
    env = {k: os.environ[k] for k in ENV_PASSA if k in os.environ}
    env.update({
        "HOME": str(run / "home"), "DISPLAY": DISPLAY, "TMUX_TMPDIR": str(run / "tmux"),
        "PATH": f"{run}/home/.local/bin:/opt/node22/bin:/usr/local/bin:/usr/bin:/bin",
        "TERM": "xterm-256color", "LANG": os.environ.get("LANG", "C.UTF-8"),
        # Container roda como root: sem isto o Claude recusa --dangerously-skip-permissions
        "IS_SANDBOX": "1",
    })
    return env


def sessao_lider(run: Path):
    log = run / "agentes.log"
    if not log.exists():
        return None
    for linha in log.read_text().splitlines():
        partes = linha.split()
        if len(partes) >= 4 and partes[1] == "Narrador":
            achados = list((run / "home/.claude/projects").glob(f"*/{partes[3]}.jsonl"))
            if achados:
                return achados[0]
    return None


def terminou(jsonl: Path) -> float | None:
    """Momento (epoch) da primeira resposta do Narrador com a linha de fim."""
    for linha in jsonl.read_text(errors="replace").splitlines():
        try:
            ev = json.loads(linha)
        except json.JSONDecodeError:
            continue
        if ev.get("type") != "assistant":
            continue
        for bloco in ev.get("message", {}).get("content", []):
            if bloco.get("type") == "text" and FIM in bloco.get("text", ""):
                from datetime import datetime
                return datetime.fromisoformat(ev["timestamp"].replace("Z", "+00:00")).timestamp()
    return None


def encerra(run: Path, app: subprocess.Popen, env: dict):
    subprocess.run(["import", "-window", "root", str(run / "final.png")], env={"DISPLAY": DISPLAY}, check=False)
    if (run / "versao").read_text() != "A":
        for alvo in ("narrater-terminal_narrador", "narrater-terminal_deva", "narrater-terminal_devb"):
            with open(run / f"tela-{alvo.split('_')[-1]}.txt", "w") as f:
                subprocess.run(["tmux", "-L", "narrater-local", "capture-pane", "-p", "-S", "-", "-t", alvo],
                               env=env, stdout=f, stderr=subprocess.DEVNULL, check=False)
        subprocess.run(["tmux", "-L", "narrater-local", "kill-server"], env=env, check=False, stderr=subprocess.DEVNULL)
    app.send_signal(signal.SIGTERM)
    try:
        app.wait(15)
    except subprocess.TimeoutExpired:
        app.kill()
    # Qualquer claude que tenha sobrado com este HOME
    home = str(run / "home")
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            environ = Path(f"/proc/{pid}/environ").read_bytes()
        except OSError:
            continue
        if f"HOME={home}".encode() in environ.split(b"\0") and int(pid) != os.getpid():
            try:
                os.kill(int(pid), signal.SIGKILL)
            except OSError:
                pass


def main():
    versao, rep = sys.argv[1].upper(), int(sys.argv[2])
    binario = BENCH / "bin" / f"v{versao}" / "narrater"
    run = prepara(versao, rep)
    env = ambiente(run)
    work = Path((run / "work_path").read_text())
    print(f"[bench] v{versao} r{rep} em {run}", flush=True)
    with open(run / "app.log", "w") as log:
        app = subprocess.Popen([str(binario)], cwd=work, env=env, stdout=log, stderr=subprocess.STDOUT)
    t0 = time.time()
    fim = None
    motivo = "limite"
    while time.time() - t0 < LIMITE_S + ATRASO_LIDER_S:
        time.sleep(10)
        if app.poll() is not None:
            motivo = f"app saiu ({app.returncode})"
            break
        lider = sessao_lider(run)
        if lider and (fim := terminou(lider)):
            motivo = "concluida"
            time.sleep(5)
            break
    encerra(run, app, env)
    inicio = float((run / "inicio").read_text()) if (run / "inicio").exists() else None
    placar = subprocess.run([sys.executable, str(AQUI / "oculto/placar.py"), str(work)], capture_output=True, text=True)
    (run / "placar.json").write_text(placar.stdout)
    subprocess.run(["git", "add", "-A"], cwd=work, check=False)
    with open(run / "diff.patch", "w") as f:
        subprocess.run(["git", "diff", "--cached", "HEAD", "--", ".", ":(exclude).narrater"], cwd=work, stdout=f, check=False)
    resumo = {"versao": versao, "rep": rep, "motivo": motivo, "inicio": inicio, "fim": fim,
              "duracao_s": (fim - inicio) if (fim and inicio) else None}
    (run / "rodada.json").write_text(json.dumps(resumo, indent=1))
    print(f"[bench] {json.dumps(resumo)}", flush=True)
    print(f"[bench] placar {placar.stdout.strip()}", flush=True)


if __name__ == "__main__":
    main()
