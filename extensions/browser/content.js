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

  const hermes = window.BorderBuddiesHermes;

  if (!hermes) {
    return;
  }

  const state = {
    settings: {
      enabled: true,
      hermesEnabled: true,
      websocketSync: false,
      websocketUrl: "ws://127.0.0.1:17387/border-buddies",
    },
    placement: loadPlacement(),
    dragging: null,
    bubbleVisible: true,
    websocket: null,
    suppressClickUntil: 0,
  };

  chrome.storage?.local?.get([SETTINGS_KEY, STORAGE_KEY], (stored) => {
    if (stored?.[SETTINGS_KEY]) {
      state.settings = { ...state.settings, ...stored[SETTINGS_KEY] };
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
      state.settings = { ...state.settings, ...message.settings };
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
      bubble.setAttribute("role", "status");
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
    bubble.innerHTML = `
      <strong>Hermes</strong>
      <span class="bb-speech-bubble__owner">Grok</span>
      <span>${escapeHtml(hermes.config.message)}</span>
    `;

    positionBubble(buddy, bubble);
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
      return;
    }

    try {
      const socket = new WebSocket(state.settings.websocketUrl);
      state.websocket = socket;

      socket.addEventListener("open", () => {
        socket.send(JSON.stringify({ type: "hello", source: "browser-extension" }));
        socket.send(JSON.stringify({ type: "placement", buddy: "hermes", placement: state.placement }));
      });

      socket.addEventListener("message", (event) => {
        const message = safeJson(event.data);

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
      });
    } catch {
      state.websocket = null;
    }
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
