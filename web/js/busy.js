// The page state hook of 9.29: data-hune-state on <html> is "busy" while a request is in flight or
// queued, or a panel draws. Else it is "idle". Each begin() has one end().

let count = 0;
let settleTimer = 0;

function apply() {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const next = count > 0 ? "busy" : "idle";
  if (root.getAttribute("data-hune-state") !== next) root.setAttribute("data-hune-state", next);
}

export function begin() {
  count += 1;
  if (settleTimer) {
    clearTimeout(settleTimer);
    settleTimer = 0;
  }
  apply();
}

export function end() {
  count = Math.max(0, count - 1);
  if (count > 0) return;
  // A short delay avoids an "idle" flash between two chained requests.
  if (settleTimer) clearTimeout(settleTimer);
  settleTimer = setTimeout(() => {
    settleTimer = 0;
    apply();
  }, 30);
}
