// Shim of ClickHouse Dash app_ui_state.js (origin "hune", 9.24). The chart engine calls only `announce`, so the shim exports only `announce`.

let liveRegion = null;

// announce(text): a short status sentence in one polite live region. The region is made at first use, not at import.
function announce(text) {
  try {
    if (!liveRegion) {
      liveRegion = document.createElement("div");
      liveRegion.setAttribute("role", "status");
      liveRegion.setAttribute("aria-live", "polite");
      liveRegion.className = "visually-hidden";
      liveRegion.style.cssText = "position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap";
      document.body.appendChild(liveRegion);
    }
    liveRegion.textContent = String(text || "");
  } catch { /* no page: nothing to announce */ }
}

// hune: E38. No empty, error, loading, block, banner, busy or HTML texts: the engine never calls them.
export const uiState = Object.freeze({ announce });
