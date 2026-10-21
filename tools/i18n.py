#!/usr/bin/env python3
"""I18N-01: the shell's translatable texts and the catalogues' coverage.

The texts are the literal arguments of `t("...")` and `tf("...", ...)` in the
shell's sources. For each catalogue (crates/gnos-i18n/locales/<code>.json):
  missing   texts the code uses that the catalogue lacks;
  unused    entries no code uses any more.

usage: tools/i18n.py [--check] [--missing CODE]
  --check         exit 1 when any catalogue misses a text or keeps one no
                  code uses (CI gate: an unused entry is a text the scan lost)
  --missing CODE  print CODE's missing texts as a JSON object to fill in
"""
import json, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = [
    ROOT / "crates/gnos-ux-gpui/src",
    ROOT / "crates/gnos-lock/src",
    ROOT / "crates/gnos-utilities/src",
    ROOT / "crates/gnosix-application-catalog/src",
]
LOCALES = ROOT / "crates/gnos-i18n/locales"
CALL = re.compile(r'\btf?\(\s*"((?:[^"\\]|\\.)*)"')
# Texts translated through a variable (a const array, a label function)
# carry a `// i18n` marker on their line.
MARKED = re.compile(r'"((?:[^"\\]|\\.)*)"')
# A helper that translates its `label` (`let switch = |label: &'static str, ..|
# live_choice(t(label), ..)`, or a `fn` doing the same): a literal given to it
# is a text even when rustfmt puts it on a line of its own, away from any
# marker.
HELPER = re.compile(
    r"(?:let\s+(\w+)\s*=\s*\|\s*label\s*:|fn\s+(\w+)\s*\(\s*label\s*:)[^\n]*\n?(?:[^\n]*\n){0,3}?[^\n]*\bt\(label\)"
)


def helper_texts(text):
    names = {a or b for a, b in HELPER.findall(text)}
    if not names:
        return []
    call = re.compile(r"\b(?:%s)\(\s*\"((?:[^\"\\\\]|\\\\.)*)\"" % "|".join(sorted(names)))
    return [m.group(1) for m in call.finditer(text)]


def texts():
    found = set()
    for base in SOURCES:
        for path in base.rglob("*.rs"):
            text = path.read_text()
            literals = [m.group(1) for m in CALL.finditer(text)]
            literals += helper_texts(text)
            for line in text.splitlines():
                if line.rstrip().endswith("// i18n"):
                    literals += [m.group(1) for m in MARKED.finditer(line)]
            for lit in literals:
                found.add(lit.replace('\\"', '"'))
    return found


def main():
    used = texts()
    failed = False
    for cat in sorted(LOCALES.glob("*.json")):
        code = cat.stem
        have = json.loads(cat.read_text())
        missing = sorted(used - have.keys())
        unused = sorted(have.keys() - used)
        print(f"{code}: {len(used) - len(missing)}/{len(used)} translated, {len(missing)} missing, {len(unused)} unused")
        if "--missing" in sys.argv and sys.argv[sys.argv.index("--missing") + 1] == code:
            print(json.dumps({m: "" for m in missing}, ensure_ascii=False, indent=2))
        failed |= bool(missing) or bool(unused)
        if unused:
            print(f"{code}: unused: {unused}")
    if "--check" in sys.argv and failed:
        sys.exit(1)


if __name__ == "__main__":
    main()
