export type LocalChatRole = "system" | "user" | "assistant";

export interface LocalChatMessage {
  role: LocalChatRole;
  content: string;
}

export type LocalChatConnector = (messages: readonly LocalChatMessage[]) => Promise<string>;

interface ChatChoice {
  message?: { content?: unknown };
}

interface ChatResponse {
  choices?: ChatChoice[];
  error?: { message?: unknown };
}

export function createLiveLocalChatConnector(args: {
  url?: string;
  model?: string;
  fetchImpl?: typeof fetch;
} = {}): LocalChatConnector {
  const url = args.url ?? process.env.BB_LMSTUDIO_URL ?? "http://127.0.0.1:1234/v1/chat/completions";
  const model = args.model ?? process.env.BB_LMSTUDIO_MODEL ?? "local-model";
  const fetchImpl = args.fetchImpl ?? fetch;

  return async (messages) => {
    try {
      const response = await fetchImpl(url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ model, messages, stream: false }),
      });

      if (!response.ok) {
        return `Local model unreachable (${response.status} ${response.statusText}).`;
      }

      const json = (await response.json()) as ChatResponse;
      const content = json.choices?.[0]?.message?.content;
      if (typeof content === "string" && content.trim().length > 0) {
        return content.trim();
      }

      const error = json.error?.message;
      if (typeof error === "string" && error.trim().length > 0) {
        return `Local model error: ${error.trim()}`;
      }
      return "Local model returned an empty reply.";
    } catch (error) {
      const detail = error instanceof Error && error.message ? ` (${error.message})` : "";
      return `Local model unreachable${detail}.`;
    }
  };
}
