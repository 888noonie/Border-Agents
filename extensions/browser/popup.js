const SETTINGS_KEY = "border-buddies:settings:v1";
const profileRuntime = window.BorderBuddiesProfiles;
const profile = profileRuntime.profiles.hermes;
const defaults = profileRuntime.createDefaultSettings(profile);

const fields = {
  allowAction: document.getElementById("allowAction"),
  allowExternalShare: document.getElementById("allowExternalShare"),
  enabled: document.getElementById("enabled"),
  hermesEnabled: document.getElementById("hermesEnabled"),
  memoryMode: document.getElementById("memoryMode"),
  modelLabel: document.getElementById("modelLabel"),
  provider: document.getElementById("provider"),
  websocketSync: document.getElementById("websocketSync"),
  websocketUrl: document.getElementById("websocketUrl"),
  showHermes: document.getElementById("showHermes"),
  status: document.getElementById("status"),
};

populateSelect(fields.provider, profileRuntime.providerLabels);
populateSelect(fields.memoryMode, profileRuntime.memoryLabels);

chrome.storage.local.get([SETTINGS_KEY], (stored) => {
  const settings = profileRuntime.normalizeSettings(profile, stored[SETTINGS_KEY] || defaults);
  fields.enabled.checked = settings.enabled;
  fields.hermesEnabled.checked = settings.hermesEnabled;
  fields.allowAction.checked = settings.allowAction;
  fields.allowExternalShare.checked = settings.allowExternalShare;
  fields.memoryMode.value = settings.memoryMode;
  fields.modelLabel.value = settings.modelLabel;
  fields.provider.value = settings.provider;
  fields.websocketSync.checked = settings.websocketSync;
  fields.websocketUrl.value = settings.websocketUrl;
});

for (const key of ["allowAction", "allowExternalShare", "enabled", "hermesEnabled", "websocketSync"]) {
  fields[key].addEventListener("change", saveSettings);
}

for (const key of ["memoryMode", "modelLabel", "provider"]) {
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
    allowAction: fields.allowAction.checked,
    allowExternalShare: fields.allowExternalShare.checked,
    enabled: fields.enabled.checked,
    hermesEnabled: fields.hermesEnabled.checked,
    memoryMode: fields.memoryMode.value,
    modelLabel: fields.modelLabel.value.trim() || defaults.modelLabel,
    provider: fields.provider.value,
    websocketSync: fields.websocketSync.checked,
    websocketUrl: fields.websocketUrl.value.trim() || defaults.websocketUrl,
  };
  const normalizedSettings = profileRuntime.normalizeSettings(profile, settings);

  chrome.storage.local.set({ [SETTINGS_KEY]: normalizedSettings }, () => {
    chrome.runtime.sendMessage({ type: "BB_SETTINGS_UPDATED", settings: normalizedSettings });
    chrome.tabs.query({}, (tabs) => {
      for (const tab of tabs) {
        if (tab.id) {
          chrome.tabs.sendMessage(tab.id, { type: "BB_SETTINGS_UPDATED", settings: normalizedSettings }).catch?.(() => {});
        }
      }
    });
    fields.status.textContent = "Settings saved.";
    setTimeout(() => {
      fields.status.textContent = "";
    }, 1400);
  });
}

function populateSelect(select, labels) {
  select.innerHTML = Object.entries(labels)
    .map(([value, label]) => `<option value="${value}">${label}</option>`)
    .join("");
}
