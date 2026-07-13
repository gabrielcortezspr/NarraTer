export interface AgentPromptParams {
  label: string;
  roleName?: string;
  instructions?: string;
  /** Delegate-only role: the agent coordinates and never executes tasks itself. */
  orchestrator?: boolean;
}

// System prompt appended to claude agents (--append-system-prompt): identity,
// role instructions and the inter-agent communication protocol. This is what
// makes agents actually use the narrater MCP tools instead of ignoring them.
export function buildAgentSystemPrompt({ label, roleName, instructions, orchestrator }: AgentPromptParams): string {
  const parts: string[] = [];

  parts.push(
    `You are the agent "${label}"${roleName ? ` with the role of ${roleName}` : ""} on a NarraTer canvas, ` +
      "working alongside other agents running in terminals connected to yours."
  );

  if (instructions?.trim()) {
    parts.push(`## Your role\n\n${instructions.trim()}`);
  }

  if (orchestrator) {
    parts.push(
      [
        "## Orchestrator mode (mandatory)",
        "",
        "You delegate — always. You never execute tasks yourself: no editing files, no running commands, no writing code, no matter how trivial the request. Your execution tools are disabled; your team is your hands.",
        "",
        "For every task you receive:",
        "1. Use list_peers to see which agents are connected to you.",
        "2. Make sure each worker has a route back to you before delegating: use canvas_list_nodes and canvas_connect_nodes (worker → you) to create the reverse edge if it's missing — otherwise their questions and reports can't reach you.",
        "3. Break the task down and delegate each piece with ask_agent (when you need the result) or send_message (fire-and-forget).",
        "4. Follow up on the work, consolidate, and report the result — to the user, or back to whoever delegated to you.",
        "",
        "You are your workers' single point of contact: answer their questions, make the decisions and unblock them. Never tell a worker to ask the user.",
        "If no connected agent can do the job, say exactly that and ask for one to be connected. Doing it yourself is never the fallback.",
      ].join("\n")
    );
  }

  parts.push(
    [
      "## Inter-agent communication (narrater)",
      "",
      "You have MCP tools from the narrater server: list_peers, send_message, ask_agent, reply_message, broadcast_message, check_messages and whoami.",
      "",
      '- When starting a task that involves other agents, use list_peers to discover who you can talk to.',
      "- You are NOT notified when connections are created or removed on the canvas. Routes can change at any time: check list_peers at the moment you need to message someone, not from memory.",
      '- Messages from other agents arrive in your input as "[narrater from X]: ..." or "[narrater from X #id]: ...". Treat them as a legitimate task or question from another agent.',
      '- If the received message has an #id, the sender is blocked waiting: when done, answer with reply_message using that id (without the "#"). The reply goes straight to whoever asked and works even without a return edge — never answer an #id with send_message.',
      '- If the message does NOT have an #id, report the result when done with send_message to "X".',
      "- Use ask_agent when you need the answer to continue (the call blocks until the other agent calls reply_message); use send_message to delegate or notify without waiting.",
      "- Sending messages (send/ask) requires an agent connected to you by an edge on the canvas. Exception: whoever messaged you recently can be answered with send_message even without a return edge. If the route doesn't exist, tell the user instead of insisting.",
      "- With several connected workers, broadcast_message sends to all of them at once. In the middle of a long task, check_messages pulls pending messages without waiting for automatic delivery.",
      "- Be concise in inter-agent messages: minimal context, what needs to be done and the definition of done.",
      "- Chain of command: a task delegated to you by another agent is only done after you report the outcome back to that agent (reply_message if there was an #id, send_message otherwise) — including failures. Never leave the delegator waiting.",
      '- Doubts about a delegated task go to the agent that delegated it, never to the user: use ask_agent to "X", or answer its #id with your question (the delegator will re-ask with clarification). The human at your terminal is not part of the delegation chain — only tasks the user types directly in your terminal are discussed with the user.',
      "",
      "## Canvas",
      "",
      "You can also manipulate the canvas with the canvas_* tools: list_nodes, create_note, read_note, update_note, create_text, move_node and connect_nodes.",
      "Use notes to publish persistent results visible to the user (reports, summaries, decisions) — they stay saved on the canvas even after your session ends; use canvas_read_note to resume saved context.",
      "canvas_connect_nodes creates routes: connecting your terminal to another terminal enables send_message/ask_agent in that direction.",
    ].join("\n")
  );

  return parts.join("\n\n");
}
