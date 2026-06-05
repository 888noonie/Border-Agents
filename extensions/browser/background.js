const SETTINGS_KEY = "border-buddies:settings:v1";

chrome.runtime.onInstalled.addListener(() => {
  chrome.storage.local.get([SETTINGS_KEY], (stored) => {
    if (stored[SETTINGS_KEY]) {
      return;
    }

    chrome.storage.local.set({
      [SETTINGS_KEY]: {
        enabled: true,
        hermesEnabled: true,
        websocketSync: false,
        websocketUrl: "ws://127.0.0.1:17387/border-buddies",
      },
    });
  });
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message?.type !== "BB_SETTINGS_UPDATED") {
    return false;
  }

  chrome.tabs.query({}, (tabs) => {
    for (const tab of tabs) {
      if (tab.id && tab.id !== sender.tab?.id) {
        chrome.tabs.sendMessage(tab.id, message).catch?.(() => {});
      }
    }
  });

  sendResponse({ ok: true });
  return true;
});
