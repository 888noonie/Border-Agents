import { createServer } from "node:http";
import { WebSocketServer } from "ws";

const PORT = 17387;
const PATH = "/border-buddies";

const hermesReplies = [
  "Signal caught. Want the sharp version?",
  "I can stay tucked and still watch the thread.",
  "Gateway link is live. Ask me anything.",
  "Sharp read: your border dock is connected.",
];

const server = createServer((_request, response) => {
  response.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
  response.end("Border Buddies gateway dev server\n");
});

const wss = new WebSocketServer({ server, path: PATH });

wss.on("connection", (socket) => {
  socket.on("message", (raw) => {
    let message;

    try {
      message = JSON.parse(String(raw));
    } catch {
      return;
    }

    if (message?.type === "hello") {
      socket.send(
        JSON.stringify({
          type: "status",
          gateway: "border-buddies-dev",
          provider: "grok",
          buddies: ["hermes"],
          message: "Hermes gateway ready",
        }),
      );
      return;
    }

    if (message?.type === "chat" && message.buddy === "hermes") {
      const echo = String(message.text ?? "").trim();
      const reply =
        echo.length > 0
          ? `Hermes: ${echo}`
          : hermesReplies[Math.floor(Math.random() * hermesReplies.length)];

      socket.send(
        JSON.stringify({
          type: "chat_reply",
          buddy: "hermes",
          text: reply,
          requestId: message.requestId,
        }),
      );
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
  console.log(`Buddy gateway listening on ws://127.0.0.1:${PORT}${PATH}`);
});
