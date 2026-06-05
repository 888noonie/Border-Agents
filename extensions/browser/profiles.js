(function attachBorderBuddyProfiles(global) {
  "use strict";

  const providerLabels = {
    claude: "Claude",
    codex: "Codex",
    custom: "Custom",
    grok: "Grok",
    lm_studio: "LM Studio",
    ollama: "Ollama",
    openrouter: "OpenRouter",
  };

  const memoryLabels = {
    off: "Off",
    purpose_graded: "Purpose graded",
    reference_only: "Reference only",
  };

  const profiles = {
    hermes: {
      schemaVersion: 1,
      identity: {
        id: "hermes",
        name: "Hermes",
        shortName: "Hermes",
        ownerKind: "model",
        ownerLabel: "Grok",
        role: "Fast Signal Companion",
      },
      adapterDefaults: {
        provider: "grok",
        modelLabel: "Grok subscription",
        connectionLabel: "Not connected",
      },
      authorityDefaults: {
        allowAction: false,
        allowExternalShare: false,
        memoryMode: "purpose_graded",
      },
      appearance: {
        color: "#2f7dff",
        accentColor: "#7df9ff",
        defaultEdge: "right",
        defaultDockSlot: 0.58,
      },
      supportedSurfaces: ["border", "browser"],
    },
  };

  function createDefaultSettings(profile) {
    return {
      enabled: true,
      hermesEnabled: true,
      websocketSync: false,
      websocketUrl: "ws://127.0.0.1:17387/border-buddies",
      provider: profile.adapterDefaults.provider,
      modelLabel: profile.adapterDefaults.modelLabel,
      connectionLabel: profile.adapterDefaults.connectionLabel,
      allowAction: profile.authorityDefaults.allowAction,
      allowExternalShare: profile.authorityDefaults.allowExternalShare,
      memoryMode: profile.authorityDefaults.memoryMode,
    };
  }

  function normalizeSettings(profile, settings) {
    const defaults = createDefaultSettings(profile);
    const candidate = settings && typeof settings === "object" ? settings : {};
    const provider = hasOwn(providerLabels, candidate.provider)
      ? candidate.provider
      : defaults.provider;
    const memoryMode = hasOwn(memoryLabels, candidate.memoryMode)
      ? candidate.memoryMode
      : defaults.memoryMode;

    return {
      enabled: candidate.enabled !== false,
      hermesEnabled: candidate.hermesEnabled !== false,
      websocketSync: candidate.websocketSync === true,
      websocketUrl: normalizeText(candidate.websocketUrl, defaults.websocketUrl),
      provider,
      modelLabel: normalizeText(candidate.modelLabel, defaults.modelLabel),
      connectionLabel: normalizeText(candidate.connectionLabel, defaults.connectionLabel),
      allowAction: candidate.allowAction === true,
      allowExternalShare: candidate.allowExternalShare === true,
      memoryMode,
    };
  }

  function normalizeText(value, fallback) {
    const text = String(value ?? "").trim();
    return text || fallback;
  }

  function hasOwn(object, key) {
    return Object.prototype.hasOwnProperty.call(object, key);
  }

  global.BorderBuddiesProfiles = {
    createDefaultSettings,
    memoryLabels,
    normalizeSettings,
    profiles,
    providerLabels,
  };
})(globalThis);
