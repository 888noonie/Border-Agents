const SETTINGS_KEY = "border-buddies:settings:v1";

const defaults = {
  enabled: true,
  hermesEnabled: true,
  websocketSync: false,
  websocketUrl: "ws://127.0.0.1:17387/border-buddies",
};

const fields = {
  enabled: document.getElementById("enabled"),
  hermesEnabled: document.getElementById("hermesEnabled"),
  websocketSync: document.getElementById("websocketSync"),
  websocketUrl: document.getElementById("websocketUrl"),
  showHermes: document.getElementById("showHermes"),
  status: document.getElementById("status"),
};

chrome.storage.local.get([SETTINGS_KEY], (stored) => {
  const settings = { ...defaults, ...(stored[SETTINGS_KEY] || {}) };
  fields.enabled.checked = settings.enabled;
  fields.hermesEnabled.checked = settings.hermesEnabled;
  fields.websocketSync.checked = settings.websocketSync;
  fields.websocketUrl.value = settings.websocketUrl;
});

for (const key of ["enabled", "hermesEnabled", "websocketSync"]) {
  fields[key].addEventListener("change", saveSettings);
}

fields.websocketUrl.addEventListener("change", saveSettings);

fields.showHermes.addEventListener("click", () => {
  chrome.tabs.query({ active: true, currentWindow: true }, ([tab]) => {
    if (!tab?.id) {
      return;
    }

    chrome.tabs.sendMessage(tab.id, { type: "BB_SHOW_HERMES" }, () => {
      fields.status.textContent = "Hermes has been summoned.";
      setTimeout(() => {
        fields.status.textContent = "";
      }, 1800);
    });
  });
});

function saveSettings() {
  const settings = {
    enabled: fields.enabled.checked,
    hermesEnabled: fields.hermesEnabled.checked,
    websocketSync: fields.websocketSync.checked,
    websocketUrl: fields.websocketUrl.value.trim() || defaults.websocketUrl,
  };

  chrome.storage.local.set({ [SETTINGS_KEY]: settings }, () => {
    chrome.runtime.sendMessage({ type: "BB_SETTINGS_UPDATED", settings });
    chrome.tabs.query({}, (tabs) => {
      for (const tab of tabs) {
        if (tab.id) {
          chrome.tabs.sendMessage(tab.id, { type: "BB_SETTINGS_UPDATED", settings }).catch?.(() => {});
        }
      }
    });
    fields.status.textContent = "Settings saved.";
    setTimeout(() => {
      fields.status.textContent = "";
    }, 1400);
  });
}
