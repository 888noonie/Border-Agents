import { DEFAULT_GATEWAY_URL } from "./gatewayProtocol";

export const GATEWAY_SETTINGS_STORAGE_KEY = "border-buddies:gateway:v1";

export type GatewaySettings = {
  url: string;
  autoConnect: boolean;
};

export const DEFAULT_GATEWAY_SETTINGS: GatewaySettings = {
  url: DEFAULT_GATEWAY_URL,
  autoConnect: true,
};

export function normalizeGatewaySettings(
  candidate: Partial<GatewaySettings> | null | undefined,
): GatewaySettings {
  const url = String(candidate?.url ?? DEFAULT_GATEWAY_URL).trim() || DEFAULT_GATEWAY_URL;

  return {
    url,
    autoConnect: candidate?.autoConnect !== false,
  };
}

export function loadStoredGatewaySettings(
  fallback: GatewaySettings = DEFAULT_GATEWAY_SETTINGS,
): GatewaySettings {
  try {
    const raw = localStorage.getItem(GATEWAY_SETTINGS_STORAGE_KEY);
    if (!raw) {
      return fallback;
    }

    return normalizeGatewaySettings(JSON.parse(raw) as Partial<GatewaySettings>);
  } catch {
    return fallback;
  }
}
