import {
  createRequestId,
  GATEWAY_PROTOCOL_VERSION,
  parseGatewayMessage,
  type GatewayConnectionState,
  type GatewayMessage,
  type GatewaySource,
} from "./gatewayProtocol";

export type GatewayClientHandlers = {
  onStateChange?: (state: GatewayConnectionState, detail?: string) => void;
  onMessage?: (message: GatewayMessage) => void;
};

type BuddyGatewayClientOptions = {
  url: string;
  source: GatewaySource;
  autoReconnect?: boolean;
  reconnectDelayMs?: number;
  handlers?: GatewayClientHandlers;
};

export class BuddyGatewayClient {
  private url: string;
  private source: GatewaySource;
  private autoReconnect: boolean;
  private reconnectDelayMs: number;
  private handlers: GatewayClientHandlers;
  private socket: WebSocket | null = null;
  private reconnectTimer: number | null = null;
  private manualClose = false;
  private state: GatewayConnectionState = "idle";

  constructor(options: BuddyGatewayClientOptions) {
    this.url = options.url;
    this.source = options.source;
    this.autoReconnect = options.autoReconnect ?? true;
    this.reconnectDelayMs = options.reconnectDelayMs ?? 2400;
    this.handlers = options.handlers ?? {};
  }

  get connectionState() {
    return this.state;
  }

  configure(url: string, autoReconnect = true) {
    this.url = url;
    this.autoReconnect = autoReconnect;
  }

  connect() {
    this.manualClose = false;
    this.clearReconnectTimer();

    if (this.socket?.readyState === WebSocket.OPEN) {
      return;
    }

    if (this.socket) {
      this.socket.close();
      this.socket = null;
    }

    this.setState("connecting");

    try {
      const socket = new WebSocket(this.url);
      this.socket = socket;

      socket.addEventListener("open", () => {
        this.setState("connected");
        this.send({
          type: "hello",
          source: this.source,
          version: GATEWAY_PROTOCOL_VERSION,
        });
      });

      socket.addEventListener("message", (event) => {
        const payload = safeJson(event.data);
        const message = parseGatewayMessage(payload);

        if (message) {
          this.handlers.onMessage?.(message);
        }
      });

      socket.addEventListener("close", () => {
        this.socket = null;

        if (this.manualClose) {
          this.setState("idle");
          return;
        }

        this.setState("disconnected");
        this.scheduleReconnect();
      });

      socket.addEventListener("error", () => {
        this.setState("error", "Gateway unreachable");
      });
    } catch (error) {
      this.setState("error", error instanceof Error ? error.message : "Gateway unreachable");
      this.scheduleReconnect();
    }
  }

  disconnect() {
    this.manualClose = true;
    this.clearReconnectTimer();
    this.socket?.close();
    this.socket = null;
    this.setState("idle");
  }

  send(message: GatewayMessage) {
    if (this.socket?.readyState !== WebSocket.OPEN) {
      return false;
    }

    this.socket.send(JSON.stringify(message));
    return true;
  }

  sendChat(
    buddyId: string,
    payload: {
      text: string;
      purpose?: string;
      context?: string;
    },
  ) {
    const requestId = createRequestId();
    const sent = this.send({
      type: "chat",
      buddy: buddyId,
      text: payload.text,
      requestId,
      purpose: payload.purpose,
      context: payload.context,
    });

    return sent ? requestId : null;
  }

  dispose() {
    this.disconnect();
    this.handlers = {};
  }

  private setState(state: GatewayConnectionState, detail?: string) {
    this.state = state;
    this.handlers.onStateChange?.(state, detail);
  }

  private scheduleReconnect() {
    if (!this.autoReconnect || this.manualClose) {
      return;
    }

    this.clearReconnectTimer();
    this.reconnectTimer = window.setTimeout(() => {
      this.connect();
    }, this.reconnectDelayMs);
  }

  private clearReconnectTimer() {
    if (this.reconnectTimer) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }
}

function safeJson(raw: unknown) {
  if (typeof raw !== "string") {
    return null;
  }

  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return null;
  }
}
