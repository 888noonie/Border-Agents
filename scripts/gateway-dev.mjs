import { createServer } from "node:http";
import { WebSocketServer } from "ws";

const PORT = 17387;
const PATH = "/border-buddies";
const HERMES_PROVIDER = process.env.HERMES_PROVIDER?.trim() || "echo";
const HERMES_API_BASE = normalizeApiBase(process.env.HERMES_API_BASE);
const HERMES_API_KEY = process.env.HERMES_API_KEY?.trim() || "";
const HERMES_MODEL = process.env.HERMES_MODEL?.trim() || "";
const HERMES_SYSTEM_PROMPT =
  process.env.HERMES_SYSTEM_PROMPT?.trim() ||
  "You are Hermes, a concise desktop companion speaking through the Border Agents buddy gateway. Be direct, useful, and clear.";

const hermesReplies = [
  "Hermes here — dev gateway is live on your desktop dock.",
  "Border link ready. Send a message and I'll echo it back.",
  "Gateway connected. This is the first-connection test path.",
  "I'm listening. Ask me anything to verify the wire.",
];

function log(message, detail) {
  const stamp = new Date().toISOString();
  if (detail === undefined) {
    console.log(`[bb-gateway ${stamp}] ${message}`);
    return;
  }

  console.log(`[bb-gateway ${stamp}] ${message}`, detail);
}

function logError(message, error) {
  const stamp = new Date().toISOString();
  console.error(`[bb-gateway ${stamp}] ERROR: ${message}`, error);
}

function normalizeApiBase(value) {
  const raw = String(value ?? "").trim();
  if (!raw) {
    return "";
  }
  return raw.replace(/\/+$/, "");
}

function gatewayMode() {
  if (HERMES_PROVIDER === "echo") {
    return "echo";
  }

  if (!HERMES_API_BASE || !HERMES_MODEL) {
    return "misconfigured";
  }

  return "openai-compatible";
}

function gatewayStatusMessage() {
  const mode = gatewayMode();
  if (mode === "openai-compatible") {
    return `Hermes gateway ready (${HERMES_PROVIDER}: ${HERMES_MODEL})`;
  }

  if (mode === "misconfigured") {
    return "Hermes gateway missing HERMES_API_BASE or HERMES_MODEL";
  }

  return "Hermes gateway ready (fallback echo)";
}

async function askHermes(text) {
  const mode = gatewayMode();
  if (mode !== "openai-compatible") {
    const echo = String(text ?? "").trim();
    return echo.length > 0
      ? `Hermes echo: ${echo}`
      : hermesReplies[Math.floor(Math.random() * hermesReplies.length)];
  }

  const headers = {
    "content-type": "application/json",
  };

  if (HERMES_API_KEY) {
    headers.authorization = `Bearer ${HERMES_API_KEY}`;
  }

  const response = await fetch(`${HERMES_API_BASE}/chat/completions`, {
    method: "POST",
    headers,
    body: JSON.stringify({
      model: HERMES_MODEL,
      messages: [
        { role: "system", content: HERMES_SYSTEM_PROMPT },
        { role: "user", content: String(text ?? "") },
      ],
      temperature: 0.4,
      stream: false,
    }),
  });

  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new Error(`Hermes provider returned ${response.status}: ${body.slice(0, 500)}`);
  }

  const payload = await response.json();
  const reply = payload?.choices?.[0]?.message?.content;

  if (typeof reply !== "string" || !reply.trim()) {
    throw new Error("Hermes provider returned an empty reply");
  }

  return reply.trim();
}

const PRESENCE_PROTOCOL = "presence";
const PRESENCE_VERSION = 0;

function presenceEnvelope(kind, buddy, payload) {
  return JSON.stringify({
    protocol: PRESENCE_PROTOCOL,
    v: PRESENCE_VERSION,
    kind,
    buddy,
    ts: Date.now(),
    ...payload,
  });
}

// Dev-only "soul": react to body interaction events with presence cues so both
// directions of the protocol can be exercised against the browser body that
// already works. A real soul replaces this with the LLM presence-tool loop.
function handlePresenceInteraction(socket, message) {
  const buddy = String(message.buddy);

  switch (message.kind) {
    case "summoned":
      socket.send(presenceEnvelope("express", buddy, { emotion: "happy" }));
      socket.send(presenceEnvelope("say", buddy, { text: "You called?" }));
      return;
    case "grabbed":
      socket.send(presenceEnvelope("express", buddy, { emotion: "alert" }));
      return;
    case "dropped":
      socket.send(presenceEnvelope("express", buddy, { emotion: "neutral" }));
      return;
    case "dismissed":
      socket.send(presenceEnvelope("express", buddy, { emotion: "sleepy" }));
      return;
    default:
      return;
  }
}

function buildGatewayPrompt(message) {
  const userText = String(message?.text ?? "").trim();
  const context = typeof message?.context === "string" ? message.context.trim() : "";
  const purpose = typeof message?.purpose === "string" ? message.purpose.trim() : "";

  if (!context) {
    return userText;
  }

  const sections = [
    purpose ? `Purpose: ${purpose}` : "",
    "[Authorized context]",
    context,
    "[User request]",
    userText,
  ].filter(Boolean);

  return sections.join("\n\n");
}

process.on("uncaughtException", (error) => {
  logError("uncaught exception", error);
  process.exit(1);
});

process.on("unhandledRejection", (reason) => {
  logError("unhandled rejection", reason);
  process.exit(1);
});

const server = createServer((request, response) => {
  log(`HTTP ${request.method} ${request.url}`);
  response.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
  response.end("Border Buddies gateway dev server\n");
});

server.on("error", (error) => {
  logError(`HTTP server failed on 127.0.0.1:${PORT}`, error);
  process.exit(1);
});

const wss = new WebSocketServer({ server, path: PATH });

wss.on("error", (error) => {
  logError("WebSocket server error", error);
});

wss.on("connection", (socket, request) => {
  const remote = request.socket.remoteAddress ?? "unknown";
  log(`client connected from ${remote}`);

  socket.on("error", (error) => {
    logError("socket error", error);
  });

  socket.on("close", () => {
    log(`client disconnected from ${remote}`);
  });

  socket.on("message", (raw) => {
    let message;

    try {
      message = JSON.parse(String(raw));
    } catch (error) {
      logError("invalid JSON from client", { raw: String(raw), error });
      return;
    }

    const source = message?.source ? String(message.source) : "unknown";
    log("message", { source, ...message });

    if (message?.protocol === PRESENCE_PROTOCOL && message?.kind && message?.buddy) {
      handlePresenceInteraction(socket, message);
      return;
    }

    if (message?.type === "hello") {
      socket.send(
        JSON.stringify({
          type: "status",
          gateway: "hermes-gateway",
          provider: HERMES_PROVIDER,
          buddies: ["hermes"],
          message: gatewayStatusMessage(),
        }),
      );
      return;
    }

    if (message?.type === "chat" && message.buddy === "hermes") {
      void (async () => {
        try {
          const reply = await askHermes(buildGatewayPrompt(message));

          socket.send(
            JSON.stringify({
              type: "chat_reply",
              buddy: "hermes",
              text: reply,
              requestId: message.requestId,
            }),
          );
          log("chat_reply", {
            source,
            buddy: "hermes",
            provider: HERMES_PROVIDER,
            purpose: message.purpose ?? null,
            hasContext: typeof message.context === "string" && message.context.trim().length > 0,
            requestId: message.requestId,
          });
        } catch (error) {
          const errorMessage = error instanceof Error ? error.message : String(error);
          logError("Hermes request failed", errorMessage);
          socket.send(
            JSON.stringify({
              type: "error",
              message: errorMessage,
              code: "hermes_request_failed",
            }),
          );
        }
      })();
      return;
    }

    if (message?.type === "placement" && message.buddy === "hermes") {
      socket.send(
        JSON.stringify({
          type: "bubble",
          buddy: "hermes",
          text: "Placement synced through the gateway.",
        }),
      );
    }
  });
});

server.listen(PORT, "127.0.0.1", () => {
  log(`listening on ws://127.0.0.1:${PORT}${PATH}`, {
    provider: HERMES_PROVIDER,
    mode: gatewayMode(),
    apiBase: HERMES_API_BASE || null,
    model: HERMES_MODEL || null,
  });
});
