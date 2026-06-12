import { createServer } from "node:http";
import { WebSocketServer } from "ws";

const PORT = 17387;
const PATH = "/border-buddies";
// Which dev "soul" drives the bodies: "echo" (default — react to interaction with
// simple cues) or "wizard" (run the Act 0 onboarding Host script). A real soul
// runtime replaces both; this just selects the scripted driver for live testing.
const BB_SOUL = process.env.BB_SOUL?.trim() || "echo";
const HERMES_PROVIDER = process.env.HERMES_PROVIDER?.trim() || "echo";
const HERMES_API_BASE = normalizeApiBase(process.env.HERMES_API_BASE);
const HERMES_API_KEY = process.env.HERMES_API_KEY?.trim() || "";
const HERMES_MODEL = process.env.HERMES_MODEL?.trim() || "";
// Image-generation model for the `/image` command. xAI ships grok-2-image on the same
// OpenAI-compatible base; override per provider via HERMES_IMAGE_MODEL.
const HERMES_IMAGE_MODEL = process.env.HERMES_IMAGE_MODEL?.trim() || "grok-2-image";
const HERMES_SYSTEM_PROMPT =
  process.env.HERMES_SYSTEM_PROMPT?.trim() ||
  "You are Hermes, a concise desktop companion speaking through the Border Agents buddy gateway. Be direct, useful, and clear.";

// Per-buddy session state for commands: the last free-text prompt (for /retry) and a
// live model override (for /model). A real soul persists this behind governance; the
// dev soul keeps it in process. Keyed by buddy id.
const sessionState = new Map();
function sessionFor(buddy) {
  let state = sessionState.get(buddy);
  if (!state) {
    state = { lastPrompt: "", modelOverride: "" };
    sessionState.set(buddy, state);
  }
  return state;
}
function activeModel(buddy) {
  return sessionFor(buddy).modelOverride || HERMES_MODEL;
}

// --- slash commands (hand-mirror of src/buddyCapabilities.ts DEFAULT_HERMES_COMMANDS) ---
// The canonical specs live in TS; this gateway mirrors them until the real soul runtime
// consumes the TS model directly (same pattern as the wizard script mirror above).
const SLASH_COMMANDS = [
  { name: "image", args: "<prompt>", summary: "Generate an image from a prompt and show it in the torso.", action: "generate_image" },
  { name: "help", args: "", summary: "List what this buddy can take in and put out.", action: "help" },
  { name: "clear", args: "", summary: "Clear the output surface back to the session card.", action: "clear" },
  { name: "model", args: "<id>", summary: "Switch the active model for this session.", action: "set_model" },
  { name: "retry", args: "", summary: "Re-run your last prompt.", action: "retry" },
];

function parseCommand(text) {
  const trimmed = String(text ?? "").trimStart();
  if (!trimmed.startsWith("/")) {
    return null;
  }
  const match = trimmed.slice(1).match(/^(\S+)\s*([\s\S]*)$/);
  if (!match) {
    return null;
  }
  const spec = SLASH_COMMANDS.find((command) => command.name === match[1].toLowerCase());
  if (!spec) {
    return null;
  }
  return { spec, rest: match[2].trim() };
}

function formatCommandHelp() {
  const lines = SLASH_COMMANDS.map((command) => {
    const usage = command.args ? `/${command.name} ${command.args}` : `/${command.name}`;
    return `${usage} — ${command.summary}`;
  });
  return `Hermes commands:\n${lines.join("\n")}`;
}

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

async function askHermes(text, buddy = "hermes") {
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
      model: activeModel(buddy),
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

// --- image generation + extraction ---------------------------------------------
//
// `/image` calls the OpenAI-compatible images endpoint and returns the bytes inline as
// base64 (the bodies have no HTTP/TLS, so the gateway always delivers bytes, never a
// URL). Plain replies that embed an image (markdown/HTML/data-URL/bare URL) are also
// detected and inlined, which is the real xAI case where Grok answers with HTML.

async function generateImage(prompt) {
  if (gatewayMode() !== "openai-compatible") {
    throw new Error("image generation needs a configured provider (HERMES_API_BASE/MODEL)");
  }
  const headers = { "content-type": "application/json" };
  if (HERMES_API_KEY) {
    headers.authorization = `Bearer ${HERMES_API_KEY}`;
  }
  const response = await fetch(`${HERMES_API_BASE}/images/generations`, {
    method: "POST",
    headers,
    body: JSON.stringify({
      model: HERMES_IMAGE_MODEL,
      prompt,
      n: 1,
      response_format: "b64_json",
    }),
  });
  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new Error(`Image provider returned ${response.status}: ${body.slice(0, 300)}`);
  }
  const payload = await response.json();
  const item = payload?.data?.[0];
  if (item && typeof item.b64_json === "string" && item.b64_json) {
    return { surface: "image", mediaType: "image/png", dataBase64: item.b64_json, caption: prompt };
  }
  if (item && typeof item.url === "string" && item.url) {
    return await fetchImageAsMedia(item.url, prompt);
  }
  throw new Error("Image provider returned an unrecognized payload");
}

async function fetchImageAsMedia(url, caption) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`could not fetch image (${response.status})`);
  }
  const mediaType = response.headers.get("content-type")?.split(";")[0] || "image/png";
  const bytes = Buffer.from(await response.arrayBuffer());
  return { surface: "image", mediaType, dataBase64: bytes.toString("base64"), caption };
}

const DATA_URL_RE = /data:(image\/[a-z0-9.+-]+);base64,([A-Za-z0-9+/=]+)/i;
const MARKDOWN_IMG_RE = /!\[[^\]]*\]\(\s*(\S+?)\s*\)/;
const HTML_IMG_RE = /<img[^>]+src=["']([^"']+)["']/i;
const BARE_IMG_URL_RE = /(https?:\/\/[^\s"')]+\.(?:png|jpe?g|gif|webp)(?:\?[^\s"')]*)?)/i;

// If `text` embeds an image, return it as inline media (fetching the bytes if it's a
// URL); otherwise null. Used to turn an "image as HTML/markdown" reply into a real
// rendered image.
async function extractImageFromText(text, caption) {
  const value = String(text ?? "");
  const dataMatch = value.match(DATA_URL_RE);
  if (dataMatch) {
    return { surface: "image", mediaType: dataMatch[1], dataBase64: dataMatch[2], caption };
  }
  const urlMatch = value.match(MARKDOWN_IMG_RE) || value.match(HTML_IMG_RE) || value.match(BARE_IMG_URL_RE);
  if (urlMatch) {
    try {
      return await fetchImageAsMedia(urlMatch[1] ?? urlMatch[0], caption);
    } catch (error) {
      logError("inline image fetch failed", error instanceof Error ? error.message : String(error));
      return null;
    }
  }
  return null;
}

// Strip image markup from a reply so the text card / bubble doesn't show raw HTML when
// the image itself is rendered separately.
function stripImageMarkup(text) {
  return String(text ?? "")
    .replace(DATA_URL_RE, "")
    .replace(MARKDOWN_IMG_RE, "")
    .replace(HTML_IMG_RE, "")
    .replace(BARE_IMG_URL_RE, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

const PRESENCE_PROTOCOL = "presence";
const PRESENCE_VERSION = 0;
const TO_BODY_RELAY_KINDS = new Set(["target_acquired", "target_moved", "target_lost"]);
const TO_EFFECTOR_RELAY_KINDS = new Set(["target_drag_requested"]);

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

// The body's last persisted placement, per buddy. A real soul keeps this in durable
// memory behind the governance boundary; the dev soul keeps it in process so the
// restart-restores-placement loop (incl. tucked) can be demonstrated end-to-end.
const lastPosition = new Map();

const DEFAULT_HYDRATE_POSITION = { mode: "anchored", edge: "right", offset: { x: 24, y: 48 } };

// --- command + text dispatch (transport-agnostic) -----------------------------
//
// `emit` abstracts the two transports: presence cues (desktop body) vs a single
// chat_reply (browser). handleUserText parses a slash command or routes free text to
// the provider, surfacing image output through emit.output(). `providerPrompt` lets the
// chat path inject authorized context while command detection still sees the raw text.

async function handleUserText(buddy, text, emit, providerPrompt = text) {
  const command = parseCommand(text);
  if (command) {
    await runCommand(buddy, command, emit);
    return;
  }
  sessionFor(buddy).lastPrompt = text;
  emit.thinking();
  try {
    const reply = await askHermes(providerPrompt, buddy);
    const media = await extractImageFromText(reply, "Image");
    emit.happy();
    if (media) {
      emit.output(media);
      emit.say(stripImageMarkup(reply) || "Here's your image.");
    } else {
      emit.output({ surface: "text", text: reply });
      emit.say(reply);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    logError("text handler failed", message);
    emit.alert();
    emit.say("I couldn't reach my brain just now.");
  }
}

async function runCommand(buddy, { spec, rest }, emit) {
  switch (spec.action) {
    case "help":
      emit.output({ surface: "text", text: formatCommandHelp() });
      emit.say("Here's what I can do.");
      return;
    case "clear":
      emit.output({ surface: "session" });
      emit.say("Output cleared.");
      return;
    case "set_model":
      if (!rest) {
        emit.say("Usage: /model <id>");
        return;
      }
      sessionFor(buddy).modelOverride = rest;
      emit.say(`Model set to ${rest} for this session.`);
      return;
    case "retry": {
      const last = sessionFor(buddy).lastPrompt;
      if (!last) {
        emit.say("Nothing to retry yet — send a prompt first.");
        return;
      }
      await handleUserText(buddy, last, emit);
      return;
    }
    case "generate_image": {
      if (!rest) {
        emit.say("Usage: /image <prompt>");
        return;
      }
      emit.thinking();
      try {
        const media = await generateImage(rest);
        emit.output(media);
        emit.happy();
        emit.say("Here's your image.");
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        logError("image generation failed", message);
        emit.alert();
        emit.say(`Image generation failed: ${message}`);
      }
      return;
    }
    default:
      emit.say("That command isn't wired yet.");
  }
}

function presenceEmit(socket, buddy) {
  return {
    thinking: () => socket.send(presenceEnvelope("express", buddy, { emotion: "thinking" })),
    happy: () => socket.send(presenceEnvelope("express", buddy, { emotion: "happy" })),
    alert: () => socket.send(presenceEnvelope("express", buddy, { emotion: "alert" })),
    say: (text) => socket.send(presenceEnvelope("say", buddy, { text })),
    output: (payload) => socket.send(presenceEnvelope("output", buddy, payload)),
  };
}

// Buffering emit for the request/reply chat path: collects one chat_reply. Express cues
// don't apply to the browser; text comes from an output(text) card or the say() line,
// with any image/file media riding alongside.
function chatEmit() {
  const buffer = { bodyText: null, speech: null, media: null };
  return {
    emit: {
      thinking: () => {},
      happy: () => {},
      alert: () => {},
      say: (text) => {
        buffer.speech = text;
      },
      output: (payload) => {
        if (payload.surface === "text") {
          buffer.bodyText = payload.text ?? "";
        } else if (payload.surface === "image" || payload.surface === "file") {
          buffer.media = {
            surface: payload.surface,
            mediaType: payload.mediaType,
            dataBase64: payload.dataBase64,
            caption: payload.caption,
          };
        }
      },
    },
    result: () => ({
      text: buffer.bodyText ?? buffer.speech ?? "",
      media: buffer.media ?? undefined,
    }),
  };
}

// Dev-only "soul": react to body interaction events with presence cues so both
// directions of the protocol can be exercised against the browser body that
// already works. A real soul replaces this with the LLM presence-tool loop.
function handlePresenceInteraction(socket, message) {
  const buddy = String(message.buddy);

  switch (message.kind) {
    case "attached": {
      // Complete the handshake: a real soul replies to `attached` with a `hydrate`
      // snapshot. Restore the buddy's last placement if we have one (so a tucked
      // buddy comes back tucked after a restart); otherwise greet it onto the desktop.
      const saved = lastPosition.get(buddy);
      const position = saved ?? DEFAULT_HYDRATE_POSITION;
      const tucked = position.mode === "tucked";
      socket.send(
        presenceEnvelope("hydrate", buddy, {
          position,
          emotion: tucked ? "sleepy" : "happy",
          speech: tucked ? undefined : "Wired up — hello from the gateway.",
        }),
      );
      return;
    }
    case "summoned":
      // Popped back out of a tuck — forget the tucked placement so it doesn't re-tuck
      // on the next reconnect.
      lastPosition.delete(buddy);
      socket.send(presenceEnvelope("express", buddy, { emotion: "happy" }));
      socket.send(presenceEnvelope("say", buddy, { text: "You called?" }));
      return;
    case "grabbed":
      socket.send(presenceEnvelope("express", buddy, { emotion: "alert" }));
      return;
    case "dropped":
      // Persist where the buddy came to rest (free point or tucked-to-edge), so the
      // next hydrate puts it back there.
      if (message.at && typeof message.at === "object") {
        lastPosition.set(buddy, message.at);
      }
      socket.send(presenceEnvelope("express", buddy, { emotion: "neutral" }));
      return;
    case "dismissed":
      socket.send(presenceEnvelope("express", buddy, { emotion: "sleepy" }));
      return;
    case "said": {
      // The user typed to the buddy through the on-body input box. Route through the
      // shared dispatch: slash commands fire their action, free text goes to the
      // provider, and image output comes back as an `output` cue (rendered in the torso).
      const text = typeof message.text === "string" ? message.text.trim() : "";
      if (!text) {
        return;
      }
      void handleUserText(buddy, text, presenceEmit(socket, buddy));
      return;
    }
    case "target_drag_requested":
      // Moving a native OS window is a governed effector. The body only reports the
      // request; a later driver/soul path decides whether and how to move the window.
      log("target move requested", {
        buddy,
        targetId: message.targetId,
        delta: message.delta,
      });
      socket.send(presenceEnvelope("say", buddy, { text: "Window move request noted." }));
      return;
    default:
      return;
  }
}

// Wizard onboarding Host — the scripted driver for Act 0 ("first contact").
//
// Canonical script: src/wizardOnboarding.ts (ONBOARDING_ACTS[0]). The strings/cues
// below mirror it by hand until the real soul runtime consumes the TS model directly.
// Per docs/WIZARD_ONBOARDING_SCRIPT.md the Host is an unbranded persona ("host"), not
// Hermes; it puppets the same dumb body and hands off to the real companion at Act 5.
const HOST_BUDDY = "host";

function wizardHostAct0(socket, message) {
  const buddy = String(message.buddy || HOST_BUDDY);

  switch (message.kind) {
    case "attached": {
      // First contact: float to the right edge, look curious, attend the user, greet.
      socket.send(
        presenceEnvelope("move_to", buddy, {
          position: { mode: "anchored", edge: "right", offset: { x: 24, y: 48 } },
        }),
      );
      socket.send(presenceEnvelope("express", buddy, { emotion: "curious" }));
      socket.send(presenceEnvelope("attention", buddy, { focus: "user" }));
      socket.send(
        presenceEnvelope("say", buddy, {
          text: "Hi — I'm your setup host. Two minutes to get you wired up. Ready?",
        }),
      );
      return;
    }
    case "clicked": {
      // Advancing event for Act 0 → Act 1. The settings panel isn't wired yet, so we
      // acknowledge on the body and log the panel-open intent at the seam boundary.
      socket.send(presenceEnvelope("express", buddy, { emotion: "happy" }));
      socket.send(presenceEnvelope("say", buddy, { text: "Great — opening setup…" }));
      log("wizard: Act 0 complete; open panel at Act 1 (connect)", { buddy });
      return;
    }
    case "said": {
      // Onboarding is pre-connection, so the Host doesn't route to a provider yet —
      // it just acknowledges and keeps the user moving through setup.
      const text = typeof message.text === "string" ? message.text.trim() : "";
      socket.send(presenceEnvelope("express", buddy, { emotion: "happy" }));
      socket.send(
        presenceEnvelope("say", buddy, {
          text: text ? `Got it — "${text}". Let's finish setup first.` : "Let's finish setup first.",
        }),
      );
      return;
    }
    default:
      // Acts 1–5 land in later commits; ignore other body events for now.
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
const clients = new Set();

function relayPresenceCue(sender, message) {
  const json = JSON.stringify(message);
  let count = 0;
  for (const client of clients) {
    if (client === sender || client.readyState !== client.OPEN) {
      continue;
    }
    client.send(json);
    count += 1;
  }
  log("relayed presence cue", { kind: message.kind, buddy: message.buddy, clients: count });
}

wss.on("error", (error) => {
  logError("WebSocket server error", error);
});

wss.on("connection", (socket, request) => {
  clients.add(socket);
  const remote = request.socket.remoteAddress ?? "unknown";
  log(`client connected from ${remote}`);

  socket.on("error", (error) => {
    logError("socket error", error);
  });

  socket.on("close", () => {
    clients.delete(socket);
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
      if (TO_BODY_RELAY_KINDS.has(message.kind)) {
        relayPresenceCue(socket, message);
        return;
      }
      if (TO_EFFECTOR_RELAY_KINDS.has(message.kind)) {
        relayPresenceCue(socket, message);
      }
      if (BB_SOUL === "wizard") {
        wizardHostAct0(socket, message);
      } else {
        handlePresenceInteraction(socket, message);
      }
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
          const rawText = typeof message.text === "string" ? message.text.trim() : "";
          const { emit, result } = chatEmit();
          // Commands parse against the raw text; free text uses the context-injected
          // prompt for the provider call.
          await handleUserText("hermes", rawText, emit, buildGatewayPrompt(message));
          const { text, media } = result();

          socket.send(
            JSON.stringify({
              type: "chat_reply",
              buddy: "hermes",
              text,
              media,
              requestId: message.requestId,
            }),
          );
          log("chat_reply", {
            source,
            buddy: "hermes",
            provider: HERMES_PROVIDER,
            purpose: message.purpose ?? null,
            hasContext: typeof message.context === "string" && message.context.trim().length > 0,
            hasMedia: Boolean(media),
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
    soul: BB_SOUL,
    provider: HERMES_PROVIDER,
    mode: gatewayMode(),
    apiBase: HERMES_API_BASE || null,
    model: HERMES_MODEL || null,
  });
});
