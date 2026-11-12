#!/usr/bin/env bash
# Contexto de retomada. O hook SessionStart em .claude/settings.json roda isto no início de
# cada sessão do Claude Code e a saída entra no contexto do agente: ele abre a sessão já
# sabendo onde o trabalho parou.
#
# Mantenha curto: tudo o que sai daqui ocupa contexto em toda sessão.

cd "$(dirname "$0")/.." || exit 0

echo "## Retomada — $(date '+%Y-%m-%d %H:%M')"
echo
echo "### Branch"
git branch --show-current 2>/dev/null || echo "(sem git)"
echo
echo "### Últimos commits"
git log -5 --format='%h %ad %s' --date=short 2>/dev/null || echo "(sem git)"
echo
echo "### Working tree"
pendente=$(git status --short 2>/dev/null)
[ -n "$pendente" ] && echo "$pendente" || echo "limpo"
echo
echo "### docs/estado.md"
cat docs/estado.md 2>/dev/null || echo "(ausente — crie com /encerrar)"
