from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from codebase_graph.extract import ParseBundle

HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$")


@dataclass(frozen=True, slots=True)
class MarkdownDocumentParser:
    """Represent markdown document parser data used by source scanning and graph materialization.
    """
    language: str = "markdown"
    parser_version: str = "markdown-docs-v1"

    def parse_file(
        self,
        path: Path,
        *,
        relative_path: str,
        source_root: Path,
        repository_label: str,
        content_hash: str,
    ) -> ParseBundle:
        """Parse file for source scanning and graph materialization.

        Args:
            path: Filesystem path read from or written by this operation.
            relative_path: Repository-relative path stored in graph and manifest metadata.
            source_root: Root directory scanned for source files.
            repository_label: Repository label used by the source scanning and graph
            materialization workflow.
            content_hash: Content hash used by the source scanning and graph
            materialization workflow.

        Returns:
            ParseBundle instance populated with data from the source scanning and graph
            materialization workflow.
        """
        source_text = path.read_text(encoding="utf-8")
        return ParseBundle(
            language=self.language,
            path=relative_path,
            source_text=source_text,
            captures=_document_captures(relative_path, source_text),
            repository_label=repository_label,
            source_root=source_root.as_posix(),
            content_hash=content_hash,
        )


def _document_captures(path: str, source_text: str) -> tuple[dict[str, Any], ...]:
    """Manage captures within source scanning and graph materialization.

    Args:
        path: Filesystem path read from or written by this operation.
        source_text: Original source text used for labels, summaries, and byte-range
        extraction.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    lines = source_text.splitlines()
    total_lines = max(len(lines), 1)
    captures: list[dict[str, Any]] = [
        {
            "capture_name": "doc.source",
            "node": {
                "type": "DocumentationSource",
                "name": path,
                "line_start": 1,
                "line_end": total_lines,
                "text": _summary(source_text),
            },
        }
    ]
    for index, section in enumerate(_sections(lines), start=1):
        label = section.heading or f"{path} section {index}"
        captures.append(
            {
                "capture_name": "doc.chunk",
                "node": {
                    "type": "DocumentationChunk",
                    "name": label,
                    "heading": section.heading,
                    "level": section.level,
                    "line_start": section.line_start,
                    "line_end": section.line_end,
                    "text": _summary(section.text),
                },
            }
        )
    return tuple(captures)


@dataclass(frozen=True, slots=True)
class _Section:
    """Represent section data used by source scanning and graph materialization."""
    heading: str
    level: int
    line_start: int
    line_end: int
    text: str


def _sections(lines: list[str]) -> tuple[_Section, ...]:
    """Manage source scanning and graph materialization state.

    Args:
        lines: Lines used by the source scanning and graph materialization workflow.

    Returns:
        Tuple of stable results returned to the source scanning and graph materialization
        caller.
    """
    headings: list[tuple[int, int, str]] = []
    for line_number, line in enumerate(lines, start=1):
        match = HEADING_RE.match(line)
        if match is None:
            continue
        headings.append((line_number, len(match.group(1)), match.group(2).strip()))

    if not headings:
        text = "\n".join(lines).strip()
        return (_Section("", 0, 1, max(len(lines), 1), text),) if text else ()

    sections: list[_Section] = []
    for index, (line_start, level, heading) in enumerate(headings):
        line_end = headings[index + 1][0] - 1 if index + 1 < len(headings) else len(lines)
        text = "\n".join(lines[line_start - 1 : line_end]).strip()
        if text:
            sections.append(_Section(heading, level, line_start, line_end, text))
    return tuple(sections)


def _summary(text: str) -> str:
    """Summarize source scanning and graph materialization for source scanning and graph materialization.

    Args:
        text: Text being parsed, formatted, or written.

    Returns:
        Formatted text returned to the caller.
    """
    return text.strip()[:2000]
