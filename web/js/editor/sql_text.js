// The SQL text of a panel (10.3). Pure module: no DOM access.

// The text splits at each LF into lines. A CR LF becomes LF first (10.3).
export function sqlToLines(text) {
  return String(text).replace(/\r\n/g, "\n").split("\n");
}

export function linesToSql(lines) {
  return lines.join("\n");
}

// A column in bytes (UTF-8) of a line becomes a position in UTF-16 units (the rule of utf16_offset of ClickHouse Dash).
// A column that falls inside a character goes to the start of that character. A column past the end gives the length.
export function utf16Offset(lineText, byteOffset) {
  let bytes = 0;
  let units = 0;
  for (const ch of lineText) {
    const cp = ch.codePointAt(0);
    const size = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    if (bytes + size > byteOffset) return units;
    bytes += size;
    units += ch.length;
  }
  return units;
}

// The index in the text of the SQL for a line and a column of the server (7.5: from 1, the column in bytes).
export function indexOfPosition(lines, line, byteColumn) {
  const at = Math.max(0, Math.min(lines.length - 1, line - 1));
  let index = 0;
  for (let i = 0; i < at; i++) index += lines[i].length + 1;
  return index + utf16Offset(lines[at], Math.max(0, byteColumn - 1));
}
