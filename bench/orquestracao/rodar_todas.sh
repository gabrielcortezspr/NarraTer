#!/bin/bash
# As 12 rodadas do benchmark, em ordem contrabalançada: nenhuma versão roda sempre logo depois
# da mesma vizinha (o cache de prompt do servidor dura até 1 h e baratearia a sucessora).
# Uso: rodar_todas.sh [ORDEM...]   ex.: rodar_todas.sh A1 B1 C1 D1
set -u
AQUI="$(cd "$(dirname "$0")" && pwd)"
ORDEM=("$@")
[ ${#ORDEM[@]} -eq 0 ] && ORDEM=(A1 B1 C1 D1  D2 C2 B2 A2  C3 A3 D3 B3)
for item in "${ORDEM[@]}"; do
  v=${item:0:1}; r=${item:1}
  echo "=== $(date +%T) v$v r$r"
  python3 "$AQUI/rodar.py" "$v" "$r"
  python3 "$AQUI/analisar.py" "${BENCH:-/home/user/bench}/runs/v$v-r$r" | head -1
done
echo "=== $(date +%T) fim"
