from __future__ import annotations

import re
from typing import Any

FRONTMATTER_RE = re.compile(r"^---\s*\n(.*?)\n---\s*\n", re.DOTALL)
WIKI_LINK_RE = re.compile(r"\[\[([^\]#|]+)(?:#[^\]|]+)?(?:\|([^\]]+))?\]\]")

def normalize_slug(value: str) -> str:
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", value.strip().lower()).strip("-")
    return slug or "untitled"

def extract_wiki_links(markdown: str) -> list[str]:
    return [normalize_slug(match.group(1)) for match in WIKI_LINK_RE.finditer(markdown)]

def parse_markdown(content: str) -> tuple[dict[str, Any], str]:
    match = FRONTMATTER_RE.match(content)
    if not match:
        return {}, content
    frontmatter: dict[str, Any] = {}
    for line in match.group(1).splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        frontmatter[key.strip()] = value.strip().strip('"')
    return frontmatter, content[match.end():]

def plain_text(markdown: str) -> str:
    text = WIKI_LINK_RE.sub(lambda match: match.group(2) or match.group(1), markdown)
    text = re.sub(r"`{1,3}[^`]*`{1,3}", " ", text)
    text = re.sub(r"[#*_>\-]+", " ", text)
    return re.sub(r"\s+", " ", text).strip()
