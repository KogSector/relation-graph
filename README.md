# Relation Graph Service

**Unified Knowledge Graph Service** - Combines Neo4j graph relationships with Zilliz vector embeddings for hybrid code and documentation search.

## Features

- **Dual-Layer Knowledge System**: 
  - Neo4j for explicit relationships (functions→classes→repos, doc sections→concepts)
  - Zilliz for vector similarity search across all content

- **Cross-Source Semantic Linking**: Automatically links code to documentation via:
  - Vector similarity
  - Explicit mention detection
  - Temporal proximity (docs written near commits)
  - Author overlap

- **Hybrid Search**: Queries combine vector search with graph traversal for comprehensive results

- **Entity Extraction**: 
  - Code: Functions, classes, modules, imports, relationships
  - Documents: Heading hierarchy, concepts, code references

## Quick Start

### Prerequisites

- Rust 1.70+
- Neo4j AuraDB or local Neo4j 5.x
- Zilliz Cloud account (or self-hosted Milvus)
- PostgreSQL (Neon or local)

### Running Locally

```powershell
# Copy environment template
cp .env.example .env

# Edit .env with your credentials
notepad .env

# Run the service
cargo run
```

The server starts on `http://localhost:3018`.

## API Endpoints

### Health & Status
- `GET /health` - Health check with component status

### Entity Operations
- `POST /api/graph/entities` - Create an entity
- `GET /api/graph/entities/:id` - Get entity by ID
- `GET /api/graph/entities/:id/neighbors` - Get related entities (n-hop traversal)

### Chunk Ingestion
- `POST /api/graph/chunks` - Ingest chunks from chunker service
  - Extracts entities automatically
  - Creates cross-source links
  - Stores vectors in Zilliz

### Cross-Source Linking
- `POST /api/graph/link` - Trigger cross-source linking between code and docs

### Search (Main Query API)
- `POST /api/search` - **Hybrid search** (vector + graph)
- `POST /api/search/vector` - Vector-only search
- `POST /api/search/graph` - Graph-only search

### Statistics
- `GET /api/graph/statistics` - Get graph and vector stats

## Example Requests

### Ingest Chunks
```bash
curl -X POST http://localhost:3018/api/graph/chunks \
  -H "Content-Type: application/json" \
  -d '{
    "chunks": [
      {
        "content": "pub fn authenticate(user: &str) -> Result<Token> { ... }",
        "source_kind": "code",
        "source_type": "github",
        "source_id": "repo/file.rs:10",
        "file_path": "src/auth.rs",
        "repo_name": "myapp",
        "language": "rust",
        "owner_id": "user123"
      },
      {
        "content": "# Authentication\n\nUse the `authenticate()` function to log in users.",
        "source_kind": "document",
        "source_type": "local_file",
        "source_id": "docs/auth.md",
        "heading_path": "# Authentication",
        "owner_id": "user123"
      }
    ],
    "extract_entities": true,
    "create_cross_links": true
  }'
```

### Hybrid Search
```bash
curl -X POST http://localhost:3018/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "how to authenticate users",
    "limit": 10,
    "graph_hops": 2,
    "include_cross_source": true
  }'
```

Response:
```json
{
  "chunks": [
    {
      "chunk_id": "...",
      "content": "...",
      "source_kind": "document",
      "similarity_score": 0.92
    }
  ],
  "related_entities": [
    {
      "id": "...",
      "entity_type": "function",
      "name": "authenticate"
    }
  ],
  "cross_source_links": [
    {
      "from_chunk_id": "...",
      "to_chunk_id": "...",
      "relationship_type": "EXPLAINS",
      "confidence": 0.87
    }
  ]
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Relation Graph Service                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │   Handlers   │    │   Services   │    │  Extractors  │       │
│  │              │    │              │    │              │       │
│  │ - search     │───▶│ - hybrid     │───▶│ - code       │       │
│  │ - ingest     │    │ - cross_link │    │ - document   │       │
│  │ - entities   │    │ - chunk_proc │    │              │       │
│  └──────────────┘    └──────────────┘    └──────────────┘       │
│         │                   │                                    │
│         ▼                   ▼                                    │
│  ┌──────────────────────────────────────────────────────┐       │
│  │                    Database Layer                     │       │
│  ├─────────────────┬─────────────────┬─────────────────┤       │
│  │    Neo4j        │    Zilliz       │   PostgreSQL    │       │
│  │   (Graph)       │   (Vectors)     │   (Evidence)    │       │
│  └─────────────────┴─────────────────┴─────────────────┘       │
└─────────────────────────────────────────────────────────────────┘
```

## Neo4j Graph Schema

### Node Types
- `REPOSITORY` - Git repositories
- `FILE` - Source files
- `FUNCTION` - Function definitions
- `CLASS` - Class/struct definitions
- `MODULE` - Modules/packages
- `DOCUMENT` - Documentation files
- `SECTION` - Document sections
- `CONCEPT` - Extracted concepts

### Relationship Types

**Code Structure:**
- `CONTAINS` - Repo→File, File→Class, Class→Function
- `IMPORTS` - File→Module
- `CALLS` - Function→Function
- `IMPLEMENTS` - Class→Trait

**Document Structure:**
- `PARENT_OF` - Section→Section (heading hierarchy)
- `REFERENCES` - Section→Concept
- `DEFINES` - Section→Concept

**Cross-Source (Unique Value!):**
- `EXPLAINS` - Document→Function
- `DOCUMENTS` - Document→Repository
- `SEMANTICALLY_SIMILAR` - Chunk→Chunk
- `MENTIONS_EXPLICITLY` - Document→Entity

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3018` | Server port |
| `NEO4J_URI` | `bolt://localhost:7687` | Neo4j connection URI |
| `NEO4J_USER` | `neo4j` | Neo4j username |
| `NEO4J_PASSWORD` | `password` | Neo4j password |
| `ZILLIZ_ENDPOINT` | - | Zilliz Cloud endpoint |
| `ZILLIZ_API_KEY` | - | Zilliz API key |
| `ZILLIZ_COLLECTION` | `knowledge_vectors` | Collection name |
| `DATABASE_URL` | - | PostgreSQL connection |
| `EMBEDDING_SERVICE_URL` | `http://localhost:8082` | Embeddings service |
| `SIMILARITY_THRESHOLD` | `0.75` | Min similarity for links |
| `MAX_GRAPH_HOPS` | `2` | Max graph traversal depth |

## Cross-Source Linking Algorithm

```
FOR each doc_chunk:
    similar_code = VectorSearch(doc_chunk.embedding, code_chunks, top_k=10)
    
    FOR each (code_chunk, similarity) in similar_code:
        IF similarity >= THRESHOLD:
            confidence = similarity
            
            # Boost 1: Explicit mention
            IF doc_chunk.text CONTAINS code_chunk.entity_name:
                confidence += 0.15
            
            # Boost 2: Temporal proximity
            IF |doc_date - commit_date| <= 7 days:
                confidence += 0.1 * (1 - days/7)
            
            # Boost 3: Author overlap
            IF doc_chunk.author == code_chunk.author:
                confidence += 0.1
            
            CREATE_RELATIONSHIP(doc → code, confidence)
```

## License

MIT