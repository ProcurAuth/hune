// The id of a new folder (10.7). The person gives a title only: the page makes the id from it. The id follows the rule of the API: 1 to 40
// characters, lower-case letters, digits and "-", the first one a letter or a digit. It is a pure function, so a test can run it without a browser.

export const ID_RULE = /^[a-z0-9][a-z0-9-]{0,39}$/;

// Four characters of lower-case letters and digits. `fill` fills a Uint8Array with random bytes (crypto.getRandomValues in the page).
function shortSuffix(fill) {
  const bytes = fill(new Uint8Array(4));
  return [...bytes].map((b) => (b % 36).toString(36)).join("");
}

// The id for a title: accents are removed, each run of other characters becomes one "-", the ends have no "-". A title with no letter and no
// digit (only symbols) gives "folder-" and a short suffix, so the id is never empty.
export function folderIdFromTitle(title, fill) {
  const base = String(title)
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+/, "")
    .slice(0, 40)
    .replace(/-+$/, "");
  return base || `folder-${shortSuffix(fill)}`;
}

// A new id for the same title, when the first one exists already: the id with "-" and a short suffix, cut so that it keeps 40 characters at most.
export function folderIdWithSuffix(id, fill) {
  return `${id.slice(0, 35).replace(/-+$/, "")}-${shortSuffix(fill)}`;
}
