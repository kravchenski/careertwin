"""Qdrant vector store — stores and queries Career Knowledge Graph."""

from __future__ import annotations

import hashlib
import os
from typing import Optional

from qdrant_client import QdrantClient
from qdrant_client.models import (
    Distance,
    FieldCondition,
    Filter,
    MatchValue,
    PointIdsList,
    PointStruct,
    VectorParams,
)

from careertwin.graph.schema import CareerGraph


COLLECTION_NAME = "career_graph"
VECTOR_DIM = 1536


def _stable_id(item_id: str) -> int:
    """Deterministic int ID from a string, stable across processes."""
    return int(hashlib.sha256(item_id.encode()).hexdigest(), 16) % (2**63)


class CareerVectorStore:
    """Vector store for career data using Qdrant."""

    def __init__(self, url: Optional[str] = None, api_key: Optional[str] = None):
        self.url = url or os.getenv("QDRANT_URL", "http://localhost:6333")
        self.api_key = api_key or os.getenv("QDRANT_API_KEY")
        self.client = QdrantClient(url=self.url, api_key=self.api_key)
        self._ensure_collection()

    def _ensure_collection(self):
        collections = [c.name for c in self.client.get_collections().collections]
        if COLLECTION_NAME not in collections:
            self.client.create_collection(
                collection_name=COLLECTION_NAME,
                vectors_config=VectorParams(size=VECTOR_DIM, distance=Distance.COSINE),
            )

    def upsert_graph(self, graph: CareerGraph, embeddings: dict[str, list[float]]):
        points = []

        for exp in graph.experience:
            text = f"{exp.position} at {exp.company}: {exp.description}"
            emb = embeddings.get(text)
            if emb:
                points.append(PointStruct(
                    id=_stable_id(exp.id),
                    vector=emb,
                    payload={
                        "type": "experience",
                        "id": exp.id,
                        "company": exp.company,
                        "position": exp.position,
                        "description": exp.description,
                        "text": text,
                    },
                ))

        for proj in graph.projects:
            text = f"{proj.name}: {proj.description}"
            emb = embeddings.get(text)
            if emb:
                points.append(PointStruct(
                    id=_stable_id(proj.id),
                    vector=emb,
                    payload={
                        "type": "project",
                        "id": proj.id,
                        "name": proj.name,
                        "description": proj.description,
                        "url": proj.url,
                        "text": text,
                    },
                ))

        for skill in graph.skills:
            text = f"Skill: {skill.name} ({skill.category.value})"
            emb = embeddings.get(text)
            if emb:
                points.append(PointStruct(
                    id=_stable_id(skill.id),
                    vector=emb,
                    payload={
                        "type": "skill",
                        "id": skill.id,
                        "name": skill.name,
                        "category": skill.category.value,
                        "proficiency": skill.proficiency,
                        "text": text,
                    },
                ))

        if points:
            self.client.upsert(collection_name=COLLECTION_NAME, points=points)

    def search(self, query_embedding: list[float], top_k: int = 10) -> list[dict]:
        results = self.client.search(
            collection_name=COLLECTION_NAME,
            query_vector=query_embedding,
            limit=top_k,
        )
        return [
            {
                "score": hit.score,
                "type": hit.payload.get("type"),
                "id": hit.payload.get("id"),
                "text": hit.payload.get("text"),
                "payload": hit.payload,
            }
            for hit in results
        ]

    def search_by_type(self, query_embedding: list[float], doc_type: str, top_k: int = 5) -> list[dict]:
        results = self.client.search(
            collection_name=COLLECTION_NAME,
            query_vector=query_embedding,
            query_filter=Filter(
                must=[FieldCondition(key="type", match=MatchValue(value=doc_type))]
            ),
            limit=top_k,
        )
        return [
            {
                "score": hit.score,
                "type": hit.payload.get("type"),
                "id": hit.payload.get("id"),
                "text": hit.payload.get("text"),
                "payload": hit.payload,
            }
            for hit in results
        ]

    def delete_all(self):
        self.client.delete(
            collection_name=COLLECTION_NAME,
            points_selector=PointIdsList(points=[]),
        )
