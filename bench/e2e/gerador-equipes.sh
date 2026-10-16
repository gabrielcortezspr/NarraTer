#!/bin/bash
# E2E do gerador de equipes, ask --check e pedido repetido, no app real sob Xvfb, sem gastar cota:
# os terminais são shells e o "claude" é um bash falso.
set -u
# Uso: bench/e2e/gerador-equipes.sh   (precisa de Xvfb :77 e do build de release em src-tauri/target/release)
# O caminho é curto de propósito: o socket do tmux não aceita caminhos longos.
BIN="$(cd "$(dirname "$0")/../.." && pwd)/src-tauri/target/release/narrater"
R=${E2E_DIR:-/tmp/nte}; rm -rf $R; mkdir -p $R/home/.config/narrater/scenes $R/home/.local/bin $R/tmux $R/out $R/work
W=$R/work; (cd $W && git init -q && printf 'import unittest\nclass T(unittest.TestCase):\n    def test_ok(self):\n        pass\n' > test_x.py && git add -A && git -c user.name=e -c user.email=e@x commit -qm init)
printf '#!/bin/sh\necho "fake claude $*" > "$HOME/claude-args-$NARRATER_LABEL.txt"\nexec bash\n' > $R/home/.local/bin/claude; chmod +x $R/home/.local/bin/claude
echo '{"persistent_sessions": true, "auto_teams": true}' > $R/home/.config/narrater/preferences.json
cat > $R/home/.config/narrater/scenes/default.json <<J
{"nodes": [
 {"id": "terminal-lead", "node_type": "terminal", "x": 0, "y": 0, "width": 520, "height": 360, "agent_type": "shell", "label": "Lead", "cwd": "$W"},
 {"id": "terminal-peer", "node_type": "terminal", "x": 0, "y": 460, "width": 520, "height": 360, "agent_type": "shell", "label": "Peer", "cwd": "$W"}],
 "edges": [{"id": "e1", "source": "terminal-lead", "target": "terminal-peer", "edge_type": "agent-pipe"}]}
J
cat > $R/lead.sh <<S
O=$R/out
narrater list > \$O/list1.txt 2>&1
narrater team templates > \$O/templates.txt 2>&1
narrater team create --template dev-reviewer --preset claude --model sonnet --task "Add a greeting function" --check "python3 -m unittest -q" > \$O/create-noreason.txt 2>&1
narrater team create --template dev-reviewer --preset claude --model sonnet --task "Add a greeting function" --check "python3 -m unittest -q" --reason "e2e: reviewer with a clean context" > \$O/create.txt 2>&1
sleep 6
narrater list > \$O/list2.txt 2>&1
narrater team create --template solo --reason "small, sequential task" > \$O/solo.txt 2>&1
narrater ask Peer "echo hello" --check "false" > \$O/ask-fail.txt 2>&1
narrater ask Peer "echo second" --check "python3 -m unittest -q" > \$O/ask-ok.txt 2>&1
narrater ask Peer "echo  HELLO" > \$O/ask-repeat.txt 2>&1
narrater recruit Zed --preset claude --model opus > \$O/recruit.txt 2>&1
echo done > \$O/lead-done
S
pkill -x narrater 2>/dev/null; sleep 1
cd $W && env -i HOME=$R/home DISPLAY=:77 TMUX_TMPDIR=$R/tmux PATH=$R/home/.local/bin:/usr/local/bin:/usr/bin:/bin TERM=xterm-256color LANG=C.UTF-8 \
  "$BIN" > $R/app.log 2>&1 &
APP=$!
for i in $(seq 60); do TMUX_TMPDIR=$R/tmux tmux -L narrater-local has-session -t narrater-terminal_lead 2>/dev/null && break; sleep 1; done
sleep 3
TMUX_TMPDIR=$R/tmux tmux -L narrater-local send-keys -t narrater-terminal_lead "bash $R/lead.sh" Enter
for i in $(seq 120); do [ -e $R/out/lead-done ] && break; sleep 1; done
# A member of the generated team tries to generate a team of its own
DEV=$(TMUX_TMPDIR=$R/tmux tmux -L narrater-local ls -F '#S' | grep -v 'lead\|peer' | head -1)
echo "member session: $DEV" > $R/out/member-session.txt
TMUX_TMPDIR=$R/tmux tmux -L narrater-local send-keys -t "$DEV" "narrater team create --template solo > $R/out/member.txt 2>&1" Enter
sleep 6
DISPLAY=:77 import -window root $R/out/final.png
sleep 4
cp $R/home/.config/narrater/scenes/default.json $R/out/scene-after.json 2>/dev/null
kill $APP; sleep 2; TMUX_TMPDIR=$R/tmux tmux -L narrater-local kill-server 2>/dev/null
echo E2E_FIM
