import { useCallback, useEffect, useRef, useState } from "react";
import { bbLog } from "./bbDiagnostics";
import { BuddyGatewayClient } from "./gatewayClient";
import {
  type GatewayConnectionState,
  type GatewayMedia,
  type GatewayMessage,
  type GatewaySource,
} from "./gatewayProtocol";
import type { GatewaySettings } from "./gatewaySettings";

export type BuddyGatewayController = {
  state: GatewayConnectionState;
  detail: string | null;
  busy: boolean;
  connect: () => void;
  disconnect: () => void;
  sendChat: (
    buddyId: string,
    payload: {
      text: string;
      purpose?: string;
      context?: string;
    },
  ) => boolean;
};

type UseBuddyGatewayOptions = {
  settings: GatewaySettings;
  source: GatewaySource;
  enabled?: boolean;
  onBubble?: (buddyId: string, text: string, media?: GatewayMedia) => void;
  onStatus?: (detail: string) => void;
};

const CONNECTION_LABELS: Record<GatewayConnectionState, string> = {
  idle: "Not connected",
  connecting: "Connecting…",
  connected: "Gateway connected",
  disconnected: "Gateway offline",
  error: "Gateway unreachable",
};

export function connectionLabelForState(state: GatewayConnectionState) {
  return CONNECTION_LABELS[state];
}

export function useBuddyGateway({
  settings,
  source,
  enabled = true,
  onBubble,
  onStatus,
}: UseBuddyGatewayOptions): BuddyGatewayController {
  const [state, setState] = useState<GatewayConnectionState>("idle");
  const [detail, setDetail] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const clientRef = useRef<BuddyGatewayClient | null>(null);
  const onBubbleRef = useRef(onBubble);
  const onStatusRef = useRef(onStatus);

  useEffect(() => {
    onBubbleRef.current = onBubble;
    onStatusRef.current = onStatus;
  }, [onBubble, onStatus]);

  useEffect(() => {
    if (!enabled) {
      clientRef.current?.dispose();
      clientRef.current = null;
      setState("idle");
      setDetail(null);
      setBusy(false);
      return;
    }

    const client = new BuddyGatewayClient({
      url: settings.url,
      source,
      autoReconnect: settings.autoConnect,
      handlers: {
        onStateChange: (nextState, nextDetail) => {
          setState(nextState);
          setDetail(nextDetail ?? null);
          void bbLog("info", "gateway state", {
            source,
            state: nextState,
            detail: nextDetail ?? null,
          });

          if (nextState !== "connecting") {
            setBusy(false);
          }
        },
        onMessage: (message: GatewayMessage) => {
          if (message.type === "bubble" || message.type === "chat_reply") {
            const media = message.type === "chat_reply" ? message.media : undefined;
            onBubbleRef.current?.(message.buddy, message.text, media);
            setBusy(false);
          }

          if (message.type === "status") {
            const statusDetail = message.message ?? `${message.gateway} ready`;
            setDetail(statusDetail);
            onStatusRef.current?.(statusDetail);
          }

          if (message.type === "error") {
            setDetail(message.message);
            setBusy(false);
          }
        },
      },
    });

    clientRef.current = client;

    if (settings.autoConnect) {
      client.connect();
    }

    return () => {
      client.dispose();
      if (clientRef.current === client) {
        clientRef.current = null;
      }
    };
  }, [enabled, settings.autoConnect, settings.url, source]);

  const connect = useCallback(() => {
    const client = clientRef.current;

    if (!client) {
      return;
    }

    client.configure(settings.url, settings.autoConnect);
    client.connect();
  }, [settings.autoConnect, settings.url]);

  const disconnect = useCallback(() => {
    clientRef.current?.disconnect();
  }, []);

  const sendChat = useCallback((buddyId: string, payload: { text: string; purpose?: string; context?: string }) => {
    const client = clientRef.current;
    const trimmed = payload.text.trim();

    if (!client || !trimmed) {
      return false;
    }

    if (client.connectionState !== "connected") {
      connect();
      return false;
    }

    const requestId = client.sendChat(buddyId, {
      text: trimmed,
      purpose: payload.purpose,
      context: payload.context?.trim() || undefined,
    });

    if (!requestId) {
      return false;
    }

    setBusy(true);
    return true;
  }, [connect]);

  return {
    state,
    detail,
    busy,
    connect,
    disconnect,
    sendChat,
  };
}
