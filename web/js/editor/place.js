// The menu of a placed control in the draft grid (10.2, 10.6): "Edit the variable" and "Put back in the top bar".

import { setPlacement } from "./draft.js";

/**
 * The entries of the menu of the placed control of the variable `name`. `E` is the editor. "Edit the variable" opens the form of the
 * variable (the tab "Variables" of the settings). "Put back in the top bar" removes the placement.
 */
export function placedMenu(E, name) {
  return [
    {
      label: "Edit the variable",
      icon: "edit",
      testid: `placement-${name}-edit`,
      onSelect: () => {
        E.settingsTab = "variables";
        E.openVariable = name;
        E.dispatch("open-settings");
      },
    },
    { label: "Put back in the top bar", icon: "arrow-up", testid: `placement-${name}-remove`, onSelect: () => E.change(setPlacement(E.doc, name, null)) },
  ];
}
