from __future__ import annotations

from dataclasses import dataclass
import re

@dataclass(slots=True)
class LogicalChunk:
    id: str
    heading: str
    text: str
    ordinal: int

class LogicalChunker:
    def __init__(self, max_chars: int = 1600) -> None:
        self.max_chars = max_chars

    def chunk(self, text: str) -> list[LogicalChunk]:
        sections = _split_sections(text)
        chunks: list[LogicalChunk] = []
        for index, (heading, body) in enumerate(sections):
            body = body.strip()
            if not body:
                continue
            for part_index, part in enumerate(_split_body(body, self.max_chars)):
                suffix = f"-{part_index}" if part_index else ""
                chunks.append(LogicalChunk(id=f"chunk-{index}{suffix}", heading=heading, text=part, ordinal=len(chunks)))
        if not chunks and text.strip():
            chunks.append(LogicalChunk(id="chunk-0", heading="Document", text=text.strip()[: self.max_chars], ordinal=0))
        return chunks

def _split_sections(text: str) -> list[tuple[str, str]]:
    matches = list(re.finditer(r"^(#{1,6})\s+(.+)$", text, flags=re.MULTILINE))
    if not matches:
        return [("Document", text)]
    sections: list[tuple[str, str]] = []
    for index, match in enumerate(matches):
        start = match.end()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        sections.append((match.group(2).strip(), text[start:end]))
    return sections

def _split_body(body: str, max_chars: int) -> list[str]:
    if len(body) <= max_chars:
        return [body]
    paragraphs = [part.strip() for part in body.split("\n\n") if part.strip()]
    chunks: list[str] = []
    current = ""
    for paragraph in paragraphs:
        if current and len(current) + len(paragraph) + 2 > max_chars:
            chunks.append(current)
            current = paragraph
        else:
            current = f"{current}\n\n{paragraph}".strip()
    if current:
        chunks.append(current)
    return chunks or [body[:max_chars]]
