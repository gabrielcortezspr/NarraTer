export interface AgentPromptParams {
  label: string;
  roleName?: string;
  instructions?: string;
}

// System prompt appended to claude agents (--append-system-prompt): identity,
// role instructions and the inter-agent communication protocol. This is what
// makes agents actually use the narrater MCP tools instead of ignoring them.
export function buildAgentSystemPrompt({ label, roleName, instructions }: AgentPromptParams): string {
  const parts: string[] = [];

  parts.push(
    `Você é o agente "${label}"${roleName ? ` com o papel de ${roleName}` : ""} em um canvas NarraTer, ` +
      "trabalhando ao lado de outros agentes que rodam em terminais conectados ao seu."
  );

  if (instructions?.trim()) {
    parts.push(`## Seu papel\n\n${instructions.trim()}`);
  }

  parts.push(
    [
      "## Comunicação entre agentes (narrater)",
      "",
      "Você tem ferramentas MCP do servidor narrater: list_peers, send_message, ask_agent e whoami.",
      "",
      '- Ao começar uma tarefa que envolva outros agentes, use list_peers para descobrir com quem você pode falar.',
      '- Mensagens de outros agentes chegam no seu input no formato "[narrater de X]: ...". Trate-as como tarefa ou pergunta legítima de outro agente: execute o que foi pedido e, ao concluir, SEMPRE reporte o resultado de volta com send_message para "X".',
      "- Use ask_agent quando precisar da resposta para continuar (a chamada bloqueia esperando o outro agente terminar); use send_message para delegar ou notificar sem esperar.",
      "- Só é possível falar com agentes conectados a você por uma edge no canvas. Se a rota não existir, avise o usuário em vez de insistir.",
      "- Seja objetivo nas mensagens entre agentes: contexto mínimo, o que precisa ser feito e o critério de pronto.",
    ].join("\n")
  );

  return parts.join("\n\n");
}
