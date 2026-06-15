import { describe, expect, it } from "vitest";
import { createLiveLocalChatConnector } from "../liveLocalChat";

describe("live local chat connector", () => {
  it("posts an OpenAI-compatible non-streaming chat completion request", async () => {
    const calls: unknown[] = [];
    const fetchImpl = (async (_url: string | URL | Request, init?: RequestInit) => {
      calls.push(init);
      return {
        ok: true,
        status: 200,
        statusText: "OK",
        json: async () => ({ choices: [{ message: { content: "local reply" } }] }),
      } as Response;
    }) as typeof fetch;

    const connector = createLiveLocalChatConnector({
      url: "http://127.0.0.1:1234/v1/chat/completions",
      model: "test-local",
      fetchImpl,
    });

    await expect(connector([{ role: "user", content: "hi" }])).resolves.toBe("local reply");
    expect(calls).toHaveLength(1);
    expect(JSON.parse(String((calls[0] as RequestInit).body))).toMatchObject({
      model: "test-local",
      stream: false,
      messages: [{ role: "user", content: "hi" }],
    });
  });

  it("fails honestly when the local endpoint is unreachable", async () => {
    const fetchImpl = (async () => {
      throw new Error("ECONNREFUSED");
    }) as typeof fetch;
    const connector = createLiveLocalChatConnector({ fetchImpl });

    await expect(connector([{ role: "user", content: "hi" }])).resolves.toMatch(/^Local model unreachable/);
  });
});
