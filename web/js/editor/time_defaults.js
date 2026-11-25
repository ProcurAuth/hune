// The default time range and the default refresh of a dashboard, from the Save dialog (10.9). The header controls of the page set a current
// range and a current refresh. When one of them differs from the default of the draft document (`time.from`, `time.to`, `time.refresh`), the
// dialog offers to save it as the default. The functions here are pure, so a test can run them without a browser.

import { checkTimeOrder, checkTimeText } from "./forms/rules.js";

// What differs. `time` is the `time` member of the draft document. `current` is { from, to, refresh }: two time texts and a refresh that is
// one of the allowed values or null (off). The range counts as one thing: a change of one end is a change of the range.
export function timeDifferences(time, current) {
  return {
    range: current.from !== time.from || current.to !== time.to,
    refresh: current.refresh !== time.refresh,
  };
}

// The text of the first problem of the range that would be saved, or "" when it is valid. It uses the checks of the settings form (4.4, 4.6).
export function rangeProblem(current) {
  return checkTimeText(current.from) || checkTimeText(current.to) || checkTimeOrder(current.from, current.to);
}

// A copy of the draft document with the chosen defaults. `choose` is { range, refresh } of booleans. It changes only `time.from` and `time.to`
// for the range, and only `time.refresh` for the refresh. No other member changes.
export function withTimeDefaults(doc, current, choose) {
  const next = structuredClone(doc);
  if (choose.range) {
    next.time.from = current.from;
    next.time.to = current.to;
  }
  if (choose.refresh) next.time.refresh = current.refresh;
  return next;
}

// The text of a refresh value for a label: the value, or "Off" for null.
export const refreshText = (value) => (value === null || value === undefined ? "Off" : value);
