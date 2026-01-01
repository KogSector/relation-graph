# Relation Graph Documentation

## Overview

The relation-graph service is ConFuse's **knowledge graph** — a unified system combining Neo4j for explicit relationships and Zilliz for vector similarity. It enables hybrid search that leverages both semantic similarity and structural relationships.

## Role in ConFuse

```
┌─────────────────────────────────────────────────────────────────────┐
│                         EMBEDDINGS                                   │
│                    Chunk embeddings (vectors)                        │
└───────────────────────────────┬─────────────────────────────────────┘
                                │
┌───────────────────────────────┼─────────────────────────────────────┐
│                               ▼                                      │
│               RELATION-GRAPH (This Service)                          │
│                         Port: 3018                                   │
│                                                                      │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │                      Hybrid Search                           │   │
│   │     Vector Similarity (Zilliz) + Graph Traversal (Neo4j)    │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
│   ┌─────────────┐   ┌─────────────┐   ┌─────────────┐              │
│   │  Neo4j      │   │   Zilliz    │   │ Cross-Link  │              │
│   │  (Graph)    │   │  (Vectors)  │   │   Engine    │              │
│   │             │   │             │   │             │              │
│   │ • Entities  │   │ • Chunks    │   │ • Code↔Doc  │              │
│   │ • Relations │   │ • Embeddings│   │ • Semantic  │              │
│   │ • Traversal │   │ • ANN search│   │   linking   │              │
│   └─────────────┘   └─────────────┘   └─────────────┘              │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          MCP-SERVER                                  │
│                 Exposes graph via MCP tools                          │
└─────────────────────────────────────────────────────────────────────┘
```

## Dual-Layer Architecture

### 1. Neo4j (Explicit Relationships)

Stores:
- **Entities**: Functions, classes, files, documents, concepts
- **Relationships**: CONTAINS, CALLS, IMPORTS, EXPLAINS, DOCUMENTS

Use cases:
- "What functions call `authenticate`?"
- "What classes are in this file?"
- "What documentation mentions this function?"

### 2. Zilliz (Vector Similarity)

Stores:
- **Chunk embeddings**: Vector representations of content
- **Metadata**: Source, type, language, path

Use cases:
- "Find code similar to this query"
- "What documentation is semantically related?"

### 3. Hybrid Search (The Magic)

Combines both for comprehensive results:

```
Query: "authentication flow"
           │
           ├───> Vector Search (Zilliz)
           │     └── Semantically similar chunks
           │
           └───> Graph Traversal (Neo4j)
                 └── Related entities (functions that call auth, docs that explain)
           │
           ▼
    Combined, deduplicated, reranked results
```

## Neo4j Schema

### Node Types

| Type | Description | Properties |
|------|-------------|------------|
| `Repository` | Git repository | name, owner, url |
| `File` | Source file | path, language, last_modified |
| `Function` | Function/method | name, signature, start_line, end_line |
| `Class` | Class/struct | name, base_classes |
| `Module` | Module/package | name, path |
| `Document` | Documentation | title, path, source_type |
| `Section` | Doc section | heading, level, path |
| `Concept` | Extracted concept | name, description |

### Relationship Types

**Code Structure:**
```cypher
(Repository)-[:CONTAINS]->(File)
(File)-[:CONTAINS]->(Class)
(Class)-[:CONTAINS]->(Function)
(File)-[:IMPORTS]->(Module)
(Function)-[:CALLS]->(Function)
(Class)-[:IMPLEMENTS]->(Trait)
```

**Document Structure:**
```cypher
(Document)-[:PARENT_OF]->(Section)
(Section)-[:PARENT_OF]->(Section)
(Section)-[:REFERENCES]->(Concept)
```

**Cross-Source (Unique Value!):**
```cypher
(Document)-[:EXPLAINS]->(Function)
(Document)-[:DOCUMENTS]->(Repository)
(Chunk)-[:SEMANTICALLY_SIMILAR {confidence: 0.87}]->(Chunk)
(Section)-[:MENTIONS_EXPLICITLY]->(Entity)
```

## API Endpoints

### POST /api/search

Hybrid search (vector + graph).

**Request:**
```json
{
  "query": "authentication flow",
  "limit": 10,
  "graph_hops": 2,
  "include_cross_source": true,
  "filters": {
    "source_types": ["code", "document"],
    "languages": ["python", "typescript"]
  }
}
```

**Response:**
```json
{
  "chunks": [
    {
      "chunk_id": "uuid",
      "content": "def authenticate(user, password): ...",
      "source_kind": "code",
      "similarity_score": 0.92,
      "metadata": {
        "path": "src/auth/handler.py",
        "language": "python"
      }
    }
  ],
  "related_entities": [
    {
      "id": "entity-uuid",
      "entity_type": "function",
      "name": "authenticate",
      "file": "src/auth/handler.py"
    }
  ],
  "cross_source_links": [
    {
      "from_chunk_id": "doc-chunk",
      "to_chunk_id": "code-chunk",
      "relationship_type": "EXPLAINS",
      "confidence": 0.87
    }
  ],
  "stats": {
    "vector_results": 15,
    "graph_results": 8,
    "cross_links": 3,
    "search_time_ms": 45
  }
}
```

### POST /api/graph/chunks

Ingest chunks from chunker.

**Request:**
```json
{
  "chunks": [
    {
      "id": "chunk-uuid",
      "content": "def authenticate()...",
      "embedding": [0.1, 0.2, ...],
      "source_kind": "code",
      "source_type": "github",
      "source_id": "repo/file.py:10",
      "metadata": {
        "path": "src/auth.py",
        "language": "python",
        "entity_name": "authenticate",
        "entity_type": "function"
      }
    }
  ],
  "extract_entities": true,
  "create_cross_links": true
}
```

### POST /api/graph/link

Trigger cross-source linking.

### GET /api/graph/entities/:id

Get entity with neighbors.

### GET /api/graph/statistics

Graph statistics.

## Cross-Source Linking Algorithm

The unique value of ConFuse: automatically linking code to its documentation.

```python
FOR each doc_chunk:
    similar_code = VectorSearch(doc_chunk.embedding, code_chunks, top_k=10)
    
    FOR each (code_chunk, similarity) in similar_code:
        IF similarity >= THRESHOLD (0.75):
            confidence = similarity
            
            # Boost 1: Explicit mention
            IF doc_chunk.text CONTAINS code_chunk.entity_name:
                confidence += 0.15
            
            # Boost 2: Temporal proximity (doc written near commit)
            IF |doc_date - commit_date| <= 7 days:
                confidence += 0.1 * (1 - days/7)
            
            # Boost 3: Author overlap
            IF doc_chunk.author == code_chunk.author:
                confidence += 0.1
            
            CREATE_RELATIONSHIP(doc → code, "EXPLAINS", confidence)
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `PORT` | Server port | `3018` |
| `NEO4J_URI` | Neo4j connection | `bolt://localhost:7687` |
| `NEO4J_USER` | Neo4j username | `neo4j` |
| `NEO4J_PASSWORD` | Neo4j password | Required |
| `ZILLIZ_ENDPOINT` | Zilliz endpoint | Required |
| `ZILLIZ_API_KEY` | Zilliz API key | Required |
| `ZILLIZ_COLLECTION` | Collection name | `knowledge_vectors` |
| `EMBEDDING_SERVICE_URL` | Embeddings service | `http://localhost:3005` |
| `SIMILARITY_THRESHOLD` | Min similarity for links | `0.75` |
| `MAX_GRAPH_HOPS` | Max traversal depth | `2` |

## Zilliz Collection Schema

```json
{
  "collection_name": "knowledge_vectors",
  "fields": [
    {"name": "id", "type": "VARCHAR", "is_primary": true},
    {"name": "embedding", "type": "FLOAT_VECTOR", "dim": 1024},
    {"name": "content", "type": "VARCHAR"},
    {"name": "source_kind", "type": "VARCHAR"},
    {"name": "source_type", "type": "VARCHAR"},
    {"name": "source_id", "type": "VARCHAR"},
    {"name": "owner_id", "type": "VARCHAR"},
    {"name": "language", "type": "VARCHAR"},
    {"name": "path", "type": "VARCHAR"}
  ],
  "index": {
    "index_type": "IVF_FLAT",
    "metric_type": "COSINE"
  }
}
```

## Related Services

| Service | Relationship |
|---------|--------------|
| embeddings | Provides vectors for chunks |
| chunker | Sends chunks for storage |
| mcp-server | Queries graph via tools |

## Performance

| Operation | Latency |
|-----------|---------|
| Vector search (10 results) | 15ms |
| Graph traversal (2 hops) | 25ms |
| Hybrid search | 35ms |
| Chunk ingestion | 5ms/chunk |
| Cross-link creation | 50ms/doc |
