(function bootBorderBuddies() {
  "use strict";

  if (window.__borderBuddiesLoaded) {
    return;
  }

  window.__borderBuddiesLoaded = true;

  const STORAGE_KEY = "border-buddies:placements:v2";
  const SETTINGS_KEY = "border-buddies:settings:v1";
  const SNAP_DISTANCE = 96;
  const FREE_SIZE = 118;
  const TUCKED_SIZE = 54;
  const MARGIN = 12;
  const GATEWAY_PROTOCOL_VERSION = 1;

  const hermes = window.BorderBuddiesHermes;
  const profileRuntime = window.BorderBuddiesProfiles;
  const profile = profileRuntime?.profiles?.hermes;

  if (!hermes || !profileRuntime || !profile) {
    return;
  }

  const state = {
    settings: profileRuntime.createDefaultSettings(profile),
    placement: loadPlacement(),
    dragging: null,
    bubbleVisible: true,
    settingsOpen: false,
    composerDraft: "",
    gatewayBusy: false,
    gatewayStatus: "",
    pendingChat: "",
    websocket: null,
    suppressClickUntil: 0,
  };

  chrome.storage?.local?.get([SETTINGS_KEY, STORAGE_KEY], (stored) => {
    if (stored?.[SETTINGS_KEY]) {
      state.settings = profileRuntime.normalizeSettings(profile, stored[SETTINGS_KEY]);
    }

    if (stored?.[STORAGE_KEY]?.hermes) {
      state.placement = stored[STORAGE_KEY].hermes;
    }

    render();
    connectWebSocket();
  });

  window.addEventListener("storage", (event) => {
    if (event.key === STORAGE_KEY && event.newValue) {
      const placements = safeJson(event.newValue);
      if (placements?.hermes) {
        state.placement = normalizePlacement(placements.hermes);
        render();
      }
    }
  });

  chrome.runtime?.onMessage?.addListener((message, _sender, sendResponse) => {
    if (message?.type === "BB_SETTINGS_UPDATED") {
      state.settings = profileRuntime.normalizeSettings(profile, message.settings);
      render();
      connectWebSocket();
      sendResponse?.({ ok: true });
    }

    if (message?.type === "BB_SHOW_HERMES") {
      state.bubbleVisible = true;
      render();
      sendResponse?.({ ok: true });
    }
  });

  function ensureRoot() {
    let root = document.getElementById("bb-browser-root");

    if (!root) {
      root = document.createElement("div");
      root.id = "bb-browser-root";
      document.documentElement.appendChild(root);
    }

    return root;
  }

  function render() {
    const root = ensureRoot();
    root.hidden = !state.settings.enabled || !state.settings.hermesEnabled;

    if (root.hidden) {
      return;
    }

    let buddy = root.querySelector("[data-buddy='hermes']");
    let bubble = root.querySelector(".bb-speech-bubble");

    if (!buddy) {
      buddy = document.createElement("section");
      buddy.className = "bb-buddy";
      buddy.dataset.buddy = "hermes";
      buddy.innerHTML = `<button class="bb-buddy__button" type="button" aria-label="Hermes, Grok buddy"></button>`;
      root.appendChild(buddy);
      wireBuddy(buddy);
    }

    if (!bubble) {
      bubble = document.createElement("aside");
      bubble.className = "bb-speech-bubble";
      bubble.setAttribute("role", "region");
      bubble.setAttribute("aria-label", "Hermes buddy controls");
      root.appendChild(bubble);
    }

    const button = buddy.querySelector(".bb-buddy__button");
    buddy.dataset.state = state.placement.state;
    buddy.dataset.edge = state.placement.edge;
    buddy.dataset.dragging = state.dragging ? "true" : "false";
    buddy.style.setProperty("--bb-color", hermes.config.color);
    buddy.style.setProperty("--bb-accent", hermes.config.accent);
    buddy.style.setProperty("--bb-bob-duration", `${hermes.config.bobDuration}ms`);

    if (state.placement.state === "free") {
      buddy.style.setProperty("--bb-x", `${state.placement.x}px`);
      buddy.style.setProperty("--bb-y", `${state.placement.y}px`);
    } else {
      buddy.style.removeProperty("--bb-x");
      buddy.style.removeProperty("--bb-y");
    }

    button.innerHTML = hermes.render(state.placement.state);

    bubble.hidden = !state.bubbleVisible || state.placement.state !== "tucked";
    bubble.dataset.edge = state.placement.edge;
    bubble.dataset.settingsOpen = state.settingsOpen ? "true" : "false";
    bubble.innerHTML = `
      <div class="bb-speech-bubble__header">
        <strong>Hermes</strong>
        <span class="bb-speech-bubble__owner">${escapeHtml(profileRuntime.providerLabels[state.settings.provider])}</span>
        <button class="bb-speech-bubble__icon-button" type="button" data-bb-action="settings" aria-expanded="${state.settingsOpen}" title="Hermes settings">Set</button>
      </div>
      <p class="bb-speech-bubble__message">${escapeHtml(hermes.config.message)}</p>
      <form class="bb-speech-bubble__composer" data-bb-composer>
        <input type="text" value="${escapeHtml(state.composerDraft)}" placeholder="Ask Hermes" aria-label="Ask Hermes" ${state.gatewayBusy ? "disabled" : ""}>
        <button type="submit" title="Ask Hermes" ${state.gatewayBusy ? "disabled" : ""}>${state.gatewayBusy ? "..." : "Go"}</button>
      </form>
      <div class="bb-speech-bubble__meta">
        <button type="button" data-bb-action="settings">${escapeHtml(state.settings.modelLabel)}</button>
        <span>${escapeHtml(profileRuntime.memoryLabels[state.settings.memoryMode])}</span>
      </div>
      ${state.settingsOpen ? renderSettingsPanel() : ""}
      <p class="bb-speech-bubble__status">${escapeHtml(statusText())}</p>
    `;
    wireBubble(bubble);

    positionBubble(buddy, bubble);
  }

  function renderSettingsPanel() {
    return `
      <div class="bb-speech-bubble__settings" aria-label="Hermes settings">
        <label>
          <span>Platform</span>
          <select data-bb-setting="provider">
            ${renderOptions(profileRuntime.providerLabels, state.settings.provider)}
          </select>
        </label>
        <label>
          <span>Model</span>
          <input type="text" data-bb-setting="modelLabel" value="${escapeHtml(state.settings.modelLabel)}">
        </label>
        <label>
          <span>Memory</span>
          <select data-bb-setting="memoryMode">
            ${renderOptions(profileRuntime.memoryLabels, state.settings.memoryMode)}
          </select>
        </label>
        <label class="bb-speech-bubble__check">
          <span>Agent action</span>
          <input type="checkbox" data-bb-setting="allowAction" ${state.settings.allowAction ? "checked" : ""}>
        </label>
        <label class="bb-speech-bubble__check">
          <span>External share</span>
          <input type="checkbox" data-bb-setting="allowExternalShare" ${state.settings.allowExternalShare ? "checked" : ""}>
        </label>
      </div>
    `;
  }

  function renderOptions(labels, selectedValue) {
    return Object.entries(labels)
      .map(([value, label]) => `<option value="${escapeHtml(value)}" ${value === selectedValue ? "selected" : ""}>${escapeHtml(label)}</option>`)
      .join("");
  }

  function wireBubble(bubble) {
    bubble.onclick = (event) => {
      const target = event.target;
      if (target?.dataset?.bbAction === "settings") {
        state.settingsOpen = !state.settingsOpen;
        render();
      }
    };

    bubble.oninput = (event) => {
      const target = event.target;
      if (target?.closest?.("[data-bb-composer]")) {
        state.composerDraft = target.value || "";
      }
    };

    bubble.onchange = (event) => {
      const target = event.target;
      const key = target?.dataset?.bbSetting;

      if (!key) {
        return;
      }

      updateSettings({
        [key]: target.type === "checkbox" ? target.checked : target.value,
      });
    };

    bubble.onsubmit = (event) => {
      event.preventDefault();
      sendChat();
    };
  }

  function wireBuddy(buddy) {
    const button = buddy.querySelector(".bb-buddy__button");

    button.addEventListener("pointerdown", (event) => {
      const pointer = getPointer(event);
      const freePlacement =
        state.placement.state === "free"
          ? state.placement
          : {
              state: "free",
              edge: state.placement.edge,
              x: clamp(pointer.x - FREE_SIZE / 2, MARGIN, window.innerWidth - FREE_SIZE - MARGIN),
              y: clamp(pointer.y - FREE_SIZE / 2, MARGIN, window.innerHeight - FREE_SIZE - MARGIN),
            };

      event.preventDefault();
      button.setPointerCapture(event.pointerId);
      state.dragging = {
        offsetX: pointer.x - freePlacement.x,
        offsetY: pointer.y - freePlacement.y,
      };
      state.placement = freePlacement;
      state.bubbleVisible = false;
      render();
    });

    button.addEventListener("pointermove", (event) => {
      if (!state.dragging) {
        return;
      }

      const pointer = getPointer(event);
      state.placement = {
        state: "free",
        edge: state.placement.edge,
        x: clamp(pointer.x - state.dragging.offsetX, MARGIN, window.innerWidth - FREE_SIZE - MARGIN),
        y: clamp(pointer.y - state.dragging.offsetY, MARGIN, window.innerHeight - FREE_SIZE - MARGIN),
      };
      render();
    });

    button.addEventListener("pointerup", finishDrag);
    button.addEventListener("pointercancel", finishDrag);

    button.addEventListener("click", () => {
      if (Date.now() < state.suppressClickUntil) {
        return;
      }

      if (!state.dragging) {
        state.bubbleVisible = !state.bubbleVisible;
        hermes.config.message = hermes.nextLine();
        render();
      }
    });

    button.addEventListener("dblclick", () => {
      tuckBuddy(state.placement.edge);
    });
  }

  function finishDrag() {
    if (!state.dragging) {
      return;
    }

    state.dragging = null;
    state.suppressClickUntil = Date.now() + 250;

    if (state.placement.state === "free") {
      const edge = getSnapEdge(state.placement);

      if (edge) {
        tuckBuddy(edge);
        return;
      }
    }

    persistPlacement();
    render();
  }

  function tuckBuddy(edge) {
    state.placement = { state: "tucked", edge };
    state.bubbleVisible = true;
    persistPlacement();
    render();
  }

  function positionBubble(buddy, bubble) {
    if (bubble.hidden) {
      return;
    }

    const rect = buddy.getBoundingClientRect();
    const bubbleWidth = Math.min(380, window.innerWidth - 56);
    const top = clamp(rect.top + rect.height / 2 - 20, 12, window.innerHeight - 58);
    let left;

    if (state.placement.edge === "left") {
      left = rect.right + 14;
    } else {
      left = rect.left - bubbleWidth - 14;
    }

    bubble.style.width = `${bubbleWidth}px`;
    bubble.style.left = `${clamp(left, 12, window.innerWidth - bubbleWidth - 12)}px`;
    bubble.style.top = `${top}px`;
  }

  function connectWebSocket() {
    if (state.websocket) {
      state.websocket.close();
      state.websocket = null;
    }

    if (!state.settings.websocketSync) {
      state.gatewayStatus = "Desktop sync is off.";
      state.gatewayBusy = false;
      return;
    }

    try {
      const socket = new WebSocket(state.settings.websocketUrl);
      state.websocket = socket;
      state.gatewayStatus = "Connecting to desktop gateway...";
      render();

      socket.addEventListener("open", () => {
        socket.send(
          JSON.stringify({
            type: "hello",
            source: "browser-extension",
            version: GATEWAY_PROTOCOL_VERSION,
            buddy: "hermes",
          }),
        );
        socket.send(JSON.stringify({ type: "placement", buddy: "hermes", placement: state.placement }));
        state.gatewayStatus = "Desktop gateway connected.";
        if (state.pendingChat) {
          const pending = state.pendingChat;
          state.pendingChat = "";
          sendChat(pending);
        } else {
          render();
        }
      });

      socket.addEventListener("close", () => {
        if (state.websocket === socket) {
          state.gatewayStatus = "Desktop gateway offline.";
          state.gatewayBusy = false;
          render();
        }
      });

      socket.addEventListener("error", () => {
        if (state.websocket === socket) {
          state.gatewayStatus = "Desktop gateway unreachable.";
          state.gatewayBusy = false;
          render();
        }
      });

      socket.addEventListener("message", (event) => {
        const message = safeJson(event.data);

        if (message?.type === "status") {
          state.gatewayStatus = message.message || "Desktop gateway connected.";
          render();
        }

        if (message?.type === "placement" && message.buddy === "hermes" && message.placement) {
          state.placement = normalizePlacement(message.placement);
          persistPlacement(false);
          render();
        }

        if (message?.type === "bubble" && message.buddy === "hermes" && message.text) {
          hermes.config.message = String(message.text);
          state.bubbleVisible = true;
          render();
        }

        if (message?.type === "chat_reply" && message.buddy === "hermes" && message.text) {
          hermes.config.message = String(message.text);
          state.gatewayBusy = false;
          state.gatewayStatus = "Reply from desktop gateway.";
          state.bubbleVisible = true;
          render();
        }

        if (message?.type === "error") {
          state.gatewayBusy = false;
          state.gatewayStatus = message.message || "Desktop gateway error.";
          state.bubbleVisible = true;
          render();
        }
      });
    } catch {
      state.websocket = null;
      state.gatewayStatus = "Desktop gateway unreachable.";
      state.gatewayBusy = false;
      render();
    }
  }

  function sendChat(textOverride) {
    const text = (textOverride ?? state.composerDraft).trim();

    if (!text) {
      return;
    }

    if (!state.settings.websocketSync) {
      state.gatewayStatus = "Turn on Desktop sync to chat through Hermes.";
      render();
      return;
    }

    if (!state.websocket || state.websocket.readyState !== WebSocket.OPEN) {
      state.pendingChat = text;
      state.gatewayStatus = "Connecting to desktop gateway...";
      connectWebSocket();
      render();
      return;
    }

    const requestId = `browser-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
    state.composerDraft = "";
    state.gatewayBusy = true;
    state.gatewayStatus = "Hermes is thinking...";
    hermes.config.message = text;
    state.websocket.send(
      JSON.stringify({
        type: "chat",
        buddy: "hermes",
        text,
        requestId,
      }),
    );
    render();
  }

  function persistPlacement(sendSocket = true) {
    const current = safeJson(localStorage.getItem(STORAGE_KEY)) || {};
    current.hermes = state.placement;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(current));
    chrome.storage?.local?.set({ [STORAGE_KEY]: current });

    if (sendSocket && state.websocket?.readyState === WebSocket.OPEN) {
      state.websocket.send(JSON.stringify({ type: "placement", buddy: "hermes", placement: state.placement }));
    }
  }

  function updateSettings(patch) {
    state.settings = profileRuntime.normalizeSettings(profile, {
      ...state.settings,
      ...patch,
    });
    chrome.storage?.local?.set({ [SETTINGS_KEY]: state.settings });
    chrome.runtime?.sendMessage?.({ type: "BB_SETTINGS_UPDATED", settings: state.settings });
    render();
    connectWebSocket();
  }

  function statusText() {
    if (state.gatewayStatus) {
      return state.gatewayStatus;
    }

    return state.settings.allowAction ? "Action requests require receipts." : "Action authority is off.";
  }

  function loadPlacement() {
    const stored = safeJson(localStorage.getItem(STORAGE_KEY));
    return normalizePlacement(stored?.hermes);
  }

  function normalizePlacement(placement) {
    if (placement?.state === "free" && typeof placement.x === "number" && typeof placement.y === "number") {
      return {
        state: "free",
        edge: normalizeEdge(placement.edge),
        x: clamp(placement.x, MARGIN, window.innerWidth - FREE_SIZE - MARGIN),
        y: clamp(placement.y, MARGIN, window.innerHeight - FREE_SIZE - MARGIN),
      };
    }

    return {
      state: "tucked",
      edge: normalizeEdge(placement?.edge),
    };
  }

  function normalizeEdge(edge) {
    return ["top", "right", "bottom", "left"].includes(edge) ? edge : "right";
  }

  function getSnapEdge(placement) {
    const distances = [
      ["left", placement.x],
      ["right", window.innerWidth - (placement.x + FREE_SIZE)],
      ["top", placement.y],
      ["bottom", window.innerHeight - (placement.y + FREE_SIZE)],
    ].sort((a, b) => a[1] - b[1]);

    return distances[0][1] <= SNAP_DISTANCE ? distances[0][0] : null;
  }

  function getPointer(event) {
    return {
      x: event.clientX,
      y: event.clientY,
    };
  }

  function clamp(value, min, max) {
    return Math.min(Math.max(value, min), max);
  }

  function safeJson(value) {
    try {
      return value ? JSON.parse(value) : null;
    } catch {
      return null;
    }
  }

  function escapeHtml(value) {
    return String(value)
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;");
  }
})();
