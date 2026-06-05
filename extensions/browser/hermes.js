(function attachHermesBuddy(global) {
  "use strict";

  const HERMES = {
    id: "hermes",
    name: "Hermes",
    owner: "Grok",
    edge: "right",
    color: "#2f7dff",
    accent: "#7df9ff",
    message: "Signal caught. Want the sharp version?",
    bobDuration: 3600,
    snapDistance: 96,
  };

  function renderHermes(state) {
    return state === "free" ? renderFullBody() : renderTuckedHead();
  }

  function renderDefs() {
    return `
      <defs>
        <linearGradient id="bb-hermes-shell" x1="30" y1="10" x2="110" y2="130" gradientUnits="userSpaceOnUse">
          <stop offset="0" stop-color="#2f7dff"/>
          <stop offset="0.52" stop-color="#111b34"/>
          <stop offset="1" stop-color="#050913"/>
        </linearGradient>
      </defs>
    `;
  }

  function renderTuckedHead() {
    return `
      <svg class="bb-buddy__svg" viewBox="0 0 128 128" aria-hidden="true">
        ${renderDefs()}
        <circle cx="66" cy="64" r="55" fill="#7df9ff" opacity="0.12"/>
        <path class="bb-hermes-accent" d="M55 20c10-18 28-18 39 0-14-4-27-4-39 0Z" opacity="0.9"/>
        <path class="bb-hermes-shell" d="M25 72c0-31 22-55 51-55 25 0 43 18 43 43 0 33-27 54-58 49-22-3-36-17-36-37Z"/>
        <path class="bb-hermes-line" d="M38 50c18-18 45-22 68-6" stroke-width="4" opacity="0.62"/>
        <ellipse class="bb-hermes-eye" cx="56" cy="61" rx="13" ry="16"/>
        <ellipse class="bb-hermes-eye" cx="88" cy="59" rx="13" ry="16"/>
        <circle class="bb-hermes-pupil" cx="60" cy="62" r="5"/>
        <circle class="bb-hermes-pupil" cx="92" cy="60" r="5"/>
        <circle class="bb-hermes-shine" cx="63" cy="57" r="2"/>
        <circle class="bb-hermes-shine" cx="95" cy="55" r="2"/>
        <path class="bb-hermes-line" d="M58 82c13 11 29 11 43-1" stroke-width="4"/>
        <path class="bb-hermes-cape" d="M108 61c13 6 18 15 17 27-10-4-18-10-24-19Z"/>
        <path class="bb-hermes-star" d="M31 30l3 7 7 3-7 3-3 7-3-7-7-3 7-3z"/>
      </svg>
    `;
  }

  function renderFullBody() {
    return `
      <svg class="bb-buddy__svg" viewBox="0 0 128 160" aria-hidden="true">
        ${renderDefs()}
        <ellipse cx="64" cy="78" rx="60" ry="72" fill="#7df9ff" opacity="0.1"/>
        <path class="bb-hermes-accent" d="M54 22c10-18 29-18 40 0-15-4-28-4-40 0Z" opacity="0.9"/>
        <path class="bb-hermes-cape" d="M85 74c22 10 36 34 31 70-20-7-35-22-42-45Z"/>
        <ellipse class="bb-hermes-shell" cx="64" cy="64" rx="44" ry="43"/>
        <ellipse class="bb-hermes-shell" cx="66" cy="109" rx="29" ry="33"/>
        <path class="bb-hermes-line" d="M28 123c10-7 21-10 33-6" stroke-width="7"/>
        <path class="bb-hermes-line" d="M85 108c12-9 22-22 27-38" stroke-width="4"/>
        <circle class="bb-hermes-accent" cx="90" cy="66" r="4"/>
        <ellipse class="bb-hermes-eye" cx="49" cy="56" rx="12" ry="15"/>
        <ellipse class="bb-hermes-eye" cx="78" cy="54" rx="12" ry="15"/>
        <circle class="bb-hermes-pupil" cx="53" cy="56" r="5"/>
        <circle class="bb-hermes-pupil" cx="82" cy="54" r="5"/>
        <circle class="bb-hermes-shine" cx="56" cy="52" r="2"/>
        <circle class="bb-hermes-shine" cx="85" cy="50" r="2"/>
        <path class="bb-hermes-line" d="M50 75c13 12 29 12 43-1" stroke-width="4"/>
        <circle class="bb-hermes-accent" cx="66" cy="100" r="10" opacity="0.86"/>
        <circle cx="66" cy="100" r="4" fill="#f2fbff"/>
        <path d="M48 142c6 4 13 4 20 0" fill="none" stroke="#071020" stroke-width="6" stroke-linecap="round"/>
        <path d="M73 142c6 4 13 4 20 0" fill="none" stroke="#071020" stroke-width="6" stroke-linecap="round"/>
        <path class="bb-hermes-star" d="M23 30l3 7 7 3-7 3-3 7-3-7-7-3 7-3z"/>
      </svg>
    `;
  }

  function nextLine() {
    const lines = [
      "Signal caught. Want the sharp version?",
      "Grok has a fast read for you.",
      "Thread shimmer detected.",
      "I found the spicy useful bit.",
      "Tiny insight, neatly wrapped.",
    ];

    return lines[Math.floor(Math.random() * lines.length)];
  }

  global.BorderBuddiesHermes = {
    config: HERMES,
    render: renderHermes,
    nextLine,
  };
})(globalThis);
