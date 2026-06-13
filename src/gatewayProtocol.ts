export const GATEWAY_PROTOCOL_VERSION = 1;

export const DEFAULT_GATEWAY_URL = "ws://127.0.0.1:17387/border-buddies";

export type GatewaySource =
  | "border-dock"
  | "buddy-window"
  | "browser-extension"
  | "browser-preview"
  | "gateway-dev";

export type GatewayConnectionState =
  | "idle"
  | "connecting"
  | "connected"
  | "disconnected"
  | "error";

export type GatewayHelloMessage = {
  type: "hello";
  source: GatewaySource;
  version: number;
  buddy?: string;
};

export type GatewayStatusMessage = {
  type: "status";
  gateway: string;
  provider?: string;
  buddies?: string[];
  message?: string;
};

export type GatewayPlacementMessage = {
  type: "placement";
  buddy: string;
  placement: unknown;
};

export type GatewayBubbleMessage = {
  type: "bubble";
  buddy: string;
  text: string;
};

export type GatewayChatMessage = {
  type: "chat";
  buddy: string;
  text: string;
  requestId: string;
  purpose?: string;
  context?: string;
};

// Rich media attached to a reply (an image or file), delivered as inline base64 — the
// browser renders it as a data URL; mirrors the presence `output` cue for the bodies.
export type GatewayMedia = {
  surface: "image" | "file";
  mediaType: string;
  dataBase64: string;
  caption?: string;
};

export type GatewayChatReplyMessage = {
  type: "chat_reply";
  buddy: string;
  text: string;
  requestId?: string;
  media?: GatewayMedia;
};

export type GatewayErrorMessage = {
  type: "error";
  message: string;
  code?: string;
};

export type GatewayMessage =
  | GatewayHelloMessage
  | GatewayStatusMessage
  | GatewayPlacementMessage
  | GatewayBubbleMessage
  | GatewayChatMessage
  | GatewayChatReplyMessage
  | GatewayErrorMessage;

export function parseGatewayMessage(raw: unknown): GatewayMessage | null {
  if (!raw || typeof raw !== "object" || !("type" in raw)) {
    return null;
  }

  const message = raw as GatewayMessage;

  switch (message.type) {
    case "hello":
    case "status":
    case "placement":
    case "bubble":
    case "chat":
    case "chat_reply":
    case "error":
      return message;
    default:
      return null;
  }
}

export function createRequestId() {
  return `req-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}
