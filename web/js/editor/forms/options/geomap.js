// The option table of the type `geomap` (10.3h; 4.33g, 9.19i; decision ID-K328). The form has no row for a tile source: the tile source comes
// only from the block `geomap` of the configuration (13.12c), and the document holds none. The hooks are `geo-*`.

import { displayNameRow, row } from "./rows.js";

const G = (id, hook, member, kind, label, group, rest = {}) => row(id, member, kind, label, group, { hook, ...rest });

/** The value of "Latitude" and "Longitude" of the view: a number in the range of 4.33g. */
export function parseCoordinate(limit) {
  return (text) => {
    const t = text.trim();
    const n = /^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?$/.test(t) ? Number(t) : NaN;
    return Number.isFinite(n) && n >= -limit && n <= limit ? { value: n } : { error: `Use a number from -${limit} to ${limit}.` };
  };
}

// The size range of 4.33g: the largest size is not below the smallest size (rule 2).
const sizeMin = (text, o) => {
  const t = text.trim();
  const n = /^[1-9][0-9]*$/.test(t) ? Number(t) : NaN;
  if (!(n >= 1 && n <= 100)) return { error: "Use a whole number from 1 to 100." };
  return n > o.layer.size_max ? { error: `Use a number of ${o.layer.size_max} or less: the largest size.` } : { value: n };
};
const sizeMax = (text, o) => {
  const t = text.trim();
  const n = /^[1-9][0-9]*$/.test(t) ? Number(t) : NaN;
  if (!(n >= 1 && n <= 100)) return { error: "Use a whole number from 1 to 100." };
  return n < o.layer.size_min ? { error: `Use a number of ${o.layer.size_min} or more: the smallest size.` } : { value: n };
};

export default {
  type: "geomap",
  groups: [["view", "Map view"], ["layer", "Markers"], ["controls", "Map controls"], ["tooltip", "Tooltip"]],
  rows: [
    G("view-mode", "geo-view-mode", "options.view.mode", "radio", "View", "view", { items: [["fixed", "Coordinates"], ["fit", "Fit to data"]], default: "fixed", hint: "With a tile source, the map always opens on the coordinates." }),
    G("view-lat", "geo-view-lat", "options.view.lat", "number", "Latitude", "view", { min: -90, max: 90, default: 0, parse: parseCoordinate(90) }),
    G("view-lon", "geo-view-lon", "options.view.lon", "number", "Longitude", "view", { min: -180, max: 180, default: 0, parse: parseCoordinate(180) }),
    G("view-zoom", "geo-view-zoom", "options.view.zoom", "slider", "Zoom", "view", { min: 0, max: 22, default: 1 }),
    G("lat-field", "geo-lat-field", "options.layer.lat_field", "field", "Latitude field", "layer", { nullText: "Field named lat or latitude", default: null, nullable: true }),
    G("lon-field", "geo-lon-field", "options.layer.lon_field", "field", "Longitude field", "layer", { nullText: "Field named lon, lng or longitude", default: null, nullable: true }),
    G("size-field", "geo-size-field", "options.layer.size_field", "field", "Size field", "layer", { nullText: "Fixed size", default: null, nullable: true }),
    G("size", "geo-size", "options.layer.size", "slider", "Size", "layer", { min: 1, max: 100, default: 5, hint: "The radius when no size field is set." }),
    G("size-min", "geo-size-min", "options.layer.size_min", "number", "Min size", "layer", { min: 1, max: 100, integer: true, default: 2, parse: sizeMin, hint: "The radius of the lowest value of the size field." }),
    G("size-max", "geo-size-max", "options.layer.size_max", "number", "Max size", "layer", { min: 1, max: 100, integer: true, default: 15, parse: sizeMax, hint: "The radius of the highest value of the size field." }),
    G("color-field", "geo-color-field", "options.layer.color_field", "field", "Color field", "layer", { nullText: "Fixed color", default: null, nullable: true }),
    G("color", "geo-color", "options.layer.color", "custom", "Color", "layer", { default: "dark-green", colour: true, hint: "The color when no color field is set." }),
    G("opacity", "geo-opacity", "options.layer.opacity", "slider", "Fill opacity", "layer", { min: 0, max: 100, default: 40 }),
    G("show-zoom", "geo-show-zoom", "options.show_zoom", "switch", "Show zoom control", "controls", { default: true }),
    G("wheel-zoom", "geo-wheel-zoom", "options.mouse_wheel_zoom", "switch", "Mouse wheel zoom", "controls", { default: true }),
    G("tooltip", "geo-tooltip", "options.tooltip", "radio", "Tooltip", "tooltip", { items: [["details", "Details"], ["none", "None"]], default: "details", hint: "None: the card shows after a click." }),
    displayNameRow(),
  ],
};
