from __future__ import annotations

import hashlib
import json
import math
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from .production import GraphExport, ProductionGraphBuilder

DEFAULT_EMBEDDING_DIMENSIONS = 384

class LadybugUnavailableError(RuntimeError):
    pass

class EmbeddingProvider(Protocol):
    dimensions: int

    def embed(self, text: str) -> list[float]:
        ...

class HashingEmbeddingProvider:
    def __init__(self, dimensions: int = DEFAULT_EMBEDDING_DIMENSIONS) -> None:
        self.dimensions = dimensions

    def embed(self, text: str) -> list[float]:
        vector = [0.0] * self.dimensions
        for token in text.lower().split():
            digest = hashlib.sha1(token.encode("utf-8")).digest()
            index = int.from_bytes(digest[:4], "big") % self.dimensions
            vector[index] += 1.0
        norm = math.sqrt(sum(value * value for value in vector)) or 1.0
        return [value / norm for value in vector]

@dataclass(slots=True)
class LadybugGraphExport:
    export: GraphExport
    embedding_dimensions: int = DEFAULT_EMBEDDING_DIMENSIONS

    def as_dict(self) -> dict[str, Any]:
        payload = self.export.as_dict()
        payload["embedding_dimensions"] = self.embedding_dimensions
        return payload

    def summary(self) -> dict[str, Any]:
        payload = self.export.summary()
        payload["embedding_dimensions"] = self.embedding_dimensions
        return payload

class LadybugGraphExporter:
    def __init__(self, repo_root: str | Path = ".", embedding_provider: EmbeddingProvider | None = None) -> None:
        self.repo_root = Path(repo_root)
        self.embedding_provider = embedding_provider or HashingEmbeddingProvider()

    def build_export(self) -> LadybugGraphExport:
        export = ProductionGraphBuilder(self.repo_root).build_export()
        return LadybugGraphExport(export, int(self.embedding_provider.dimensions))

class LadybugGraphStore:
    def __init__(self, db_path: str | Path) -> None:
        self.db_path = Path(db_path)

    def write_export(self, export: LadybugGraphExport) -> None:
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self.db_path.write_text(json.dumps(export.as_dict(), indent=2, sort_keys=True), encoding="utf-8")

    def read_export(self) -> dict[str, Any]:
        if not self.db_path.exists():
            return {"ontology": "", "metadata": {}, "nodes": [], "edges": []}
        return json.loads(self.db_path.read_text(encoding="utf-8"))

    def ensure_schema(self, embedding_dimensions: int = DEFAULT_EMBEDDING_DIMENSIONS) -> None:
        self.db_path.parent.mkdir(parents=True, exist_ok=True)

    def copy_from_staging(self, staging: Any) -> None:
        raise LadybugUnavailableError("Staging copy is not implemented for the JSON-backed base store.")
