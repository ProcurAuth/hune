#!/usr/bin/env python3
"""Check Markdown files against the machine-checkable rules of ASD-STE100.

Use: python3 -I tools/ste_lint.py [--strict20] FILE...

The tool cannot check the whole standard. It checks these rules:
  S25   a sentence of more than 25 words (error)
  S20   a sentence of more than 20 words (warning; an error with --strict20)
  PARA  a paragraph of more than 6 sentences (error)
  ING   a word that ends in "ing" and is not on the list of permitted words (error)
  WORD  a word that the standard replaces: ensure, allow, provide, and the like (error)
  PHR   a phrasal verb of the list (error)

The tool skips code blocks, tables, headings, link targets and the text of inline code.
An inline code span counts as one word. Technical names are permitted: put them in code spans.
"""
import re
import sys

PERMITTED_ING = {
    # nouns and technical names that end in "ing"
    "string", "strings", "thing", "nothing", "something", "anything", "everything", "during", "bring", "ring", "spring",
    "morning", "evening", "king", "ping", "pings", "sibling", "siblings", "sibling's", "pudding", "wing", "sing", "swing",
    "railing", "ceiling", "ceilings", "building", "buildings", "heading", "headings", "padding", "mapping", "mappings",
    "routing", "caching", "logging", "pooling", "tracing", "sampling", "hashing", "encoding", "decoding", "scheduling",
    "polling", "listing", "listings", "setting", "settings", "timing", "binding", "bindings", "warning", "warnings",
    "filtering", "sorting", "parsing", "lexing", "shaping", "naming", "versioning", "proxying", "rendering",
    "querying", "testing", "tokenizing", "monitoring", "billing", "tooling", "trailing", "leading", "missing",
    "existing", "following", "remaining", "pending", "matching",
}
# Words that are technical names and end in "ing" are written in code spans. The list above is short on purpose:
# the writer MUST rewrite the sentence when the word is a verb form.
STRICT_ING = {  # of the permitted list, these are verb forms in most uses: flagged anyway
    "following", "remaining", "existing", "missing", "matching", "leading", "trailing", "pending",
}
PERMITTED_ING -= STRICT_ING

REPLACE = {
    "ensure": "make sure", "ensures": "make sure", "ensured": "made sure",
    "allow": "let", "allows": "let", "allowed": "permitted", "allowing": "let",
    "provide": "give", "provides": "give", "provided": "gave", "providing": "give",
    "utilize": "use", "utilise": "use", "utilizes": "use", "leverage": "use", "facilitate": "help",
    "in order to": "to", "prior to": "before", "subsequent": "next", "commence": "start", "terminate": "stop",
    "additionally": "also", "furthermore": "also", "however": "but", "therefore": "so", "approximately": "about",
}
PHRASAL = [
    "set up", "log in", "log out", "carry out", "turn on", "turn off", "start up", "shut down", "look up",
    "build up", "fill in", "clean up", "point out", "run out", "come back", "go back", "back up", "pick up",
    "hand over", "roll back", "write back", "read back", "fall back", "break down", "check out", "find out",
]


def strip_inline(text):
    text = re.sub(r"`[^`]*`", " CODE ", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"<[^>]+>", " ", text)
    text = re.sub(r"[*_]{1,3}", "", text)
    return text


def blocks(lines):
    """Yield (first_line_number, kind, text) for each paragraph or list item outside code and tables."""
    in_code = False
    buf, start = [], 0
    def flush():
        nonlocal buf, start
        if buf:
            yield (start, "para", " ".join(buf))
        buf = []
    for number, raw in enumerate(lines, 1):
        line = raw.rstrip("\n")
        if line.strip().startswith("```"):
            yield from flush()
            in_code = not in_code
            continue
        if in_code:
            continue
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or stripped.startswith("|") or re.match(r"^[-*_]{3,}$", stripped) \
                or stripped.startswith(">") and False:
            yield from flush()
            continue
        item = re.match(r"^(\s*)([-*+]|\d+[.)])\s+(.*)$", line)
        if item:
            yield from flush()
            buf, start = [item.group(3)], number
            continue
        if not buf:
            start = number
        buf.append(stripped)
    yield from flush()


def sentences(text):
    text = strip_inline(text)
    parts = re.split(r"(?<=[.!?:;])\s+(?=[A-Z0-9(])", text)
    return [p.strip() for p in parts if p.strip()]


def check(path, strict20):
    errors, warnings = [], []
    with open(path, encoding="utf-8") as handle:
        lines = handle.readlines()
    for number, kind, text in blocks(lines):
        sents = sentences(text)
        if len(sents) > 6:
            errors.append((number, "PARA", f"{len(sents)} sentences in one paragraph or item"))
        for sentence in sents:
            words = [w for w in re.split(r"\s+", sentence) if re.search(r"[A-Za-z0-9]", w)]
            count = len(words)
            if count > 25:
                errors.append((number, "S25", f"{count} words: {sentence[:90]}..."))
            elif count > 20:
                (errors if strict20 else warnings).append((number, "S20", f"{count} words: {sentence[:90]}..."))
            low = sentence.lower()
            for word in re.findall(r"[a-z][a-z'-]*", low):
                if word.endswith("ing") and len(word) > 4 and word not in PERMITTED_ING:
                    errors.append((number, "ING", word))
                if word in REPLACE and " " not in word:
                    errors.append((number, "WORD", f"{word} -> {REPLACE[word]}"))
            for phrase in [k for k in REPLACE if " " in k]:
                if phrase in low:
                    errors.append((number, "WORD", f"{phrase} -> {REPLACE[phrase]}"))
            for phrase in PHRASAL:
                if re.search(r"\b" + phrase + r"\b", low):
                    errors.append((number, "PHR", phrase))
    return errors, warnings


def main(argv):
    strict20 = "--strict20" in argv
    files = [a for a in argv if not a.startswith("--")]
    if not files:
        print(__doc__)
        return 2
    total_errors = 0
    for path in files:
        errors, warnings = check(path, strict20)
        for number, code, message in errors:
            print(f"{path}:{number}: {code} {message}")
        counts = {}
        for _, code, _ in errors:
            counts[code] = counts.get(code, 0) + 1
        print(f"{path}: {len(errors)} errors {counts}, {len(warnings)} warnings (S20)")
        total_errors += len(errors)
    return 1 if total_errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
