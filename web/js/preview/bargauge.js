// The loading preview of the panel type `bargauge` (9.11).
// The rules are in the council file `bargauge.md` (scratchpad/council). The loaded panel is drawn by web/js/panels/bargauge.js, items.js.
//
// No member of the document fixes the number of items of a bar gauge, and one item is not certain either. Every shape inside the box (an item, a
// gap, a track, a name, a value, a fill, a cell) needs at least one item. So the preview draws nothing inside the box: it keeps the box at its
// exact place, and the warning line of a bad override pattern. This is the same form for every orientation, display mode and option.
// The parts that this module claims:
//   holder   the box of the loaded `.bargauge` (exact)
//   warning, label (the lines of the warning, see stat.js)

import { exact, resultOf } from "./common.js";
import { holderNode, warningLayout } from "./stat.js";

// `box` is the content box of the body: the holder of the bar gauge is under the warning line, if any.
export function build(panel, box, doc, ctx) {
  const warn = warningLayout(panel, 0, 0, box.w);
  return resultOf([...warn.nodes, holderNode(warn.height)], { holder: exact(Math.max(0, box.h - warn.height)), warning: exact(warn.height) }, [warn.height ? "box warning" : "box"]);
}
