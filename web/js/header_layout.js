// The size steps of the header bar (9.6). The header is always one row. A step is a window width. The css of the page uses the same widths
// in its media queries (base.css, components.css, list.css). A test compares the two lists.

// The widths, in CSS pixels. A step applies at this width and below, except `versionMin`, which applies above `labelsMax`.
export const STEPS = Object.freeze({
  // At this width or less the version line hides, the labels "Edit" and "Save" hide, and the range button is narrower. The version line
  // shows next to the name above this width.
  versionMin: 1281,
  labelsMax: 1100,
  // The list page: the filters "tag" and "sort" show their icon only.
  listFiltersMax: 1180,
  // The list page: the button "New" with its menu replaces the three actions.
  listMenuMax: 900,
  // The range button shows its clock and chevron only, and each control of the time group is an icon.
  iconsMax: 768,
  // The time shift and the zoom out move into the menu "More actions".
  menuTimeMax: 900,
  // "Add" shows its icon only (the edit buttons take room from the crumbs).
  addIconMax: 900,
  // "Revert" moves into the menu.
  revertMenuMax: 640,
  // The time control, the switch and "Add" get their small sizes (the interval shows its chevron only).
  compactMax: 560,
  // The product name hides.
  nameMax: 640,
  // A phone: the gaps and the controls are smaller, the crumb of the folder hides, and the list keeps the search and its menu only.
  tightMax: 480,
});

// The media query text for a "this width or less" step.
export const maxQuery = (px) => `(max-width: ${px}px)`;

// The steps that apply at a width. A pure function: the tests and the menus use it.
export function stepsAt(width) {
  return {
    version: width >= STEPS.versionMin,
    labels: width > STEPS.labelsMax,
    listFilterText: width > STEPS.listFiltersMax,
    listMenu: width <= STEPS.listMenuMax,
    icons: width <= STEPS.iconsMax,
    timeInMenu: width <= STEPS.menuTimeMax,
    tight: width <= STEPS.tightMax,
  };
}

// True when the time shift and the zoom out are in the menu "More actions" (the window is narrow).
export function timeInMenu() {
  try {
    return window.matchMedia(maxQuery(STEPS.menuTimeMax)).matches;
  } catch (error) {
    return false;
  }
}
