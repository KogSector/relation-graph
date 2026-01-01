# ConFuse Relation Graph

Knowledge graph service for the ConFuse Knowledge Intelligence Platform. Combines Neo4j and Zilliz for hybrid search across code and documentation.

## Overview

This service is the **knowledge graph** that:
- Stores entity relationships in Neo4j
- Stores vector embeddings in Zilliz
- Enables hybrid search (semantic + structural)
- Links code to documentation automatically

## Architecture

See [docs/README.md](docs/README.md) for complete documentation.

## Quick Start

```bash
# Build
cargo build --release

# Configure
cp .env.example .env
# Configure Neo4j and Zilliz credentials

# Run
cargo run
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/search` | POST | Hybrid search |
| `/api/graph/chunks` | POST | Ingest chunks |
| `/api/graph/entities/:id` | GET | Get entity |
| `/api/graph/link` | POST | Create cross-links |
| `/health` | GET | Health check |

## Hybrid Search

Combines:
- **Vector search**: Semantic similarity via Zilliz
- **Graph traversal**: Relationship-based via Neo4j
- **Cross-source links**: Code↔documentation

## Environment Variables

| Variable | Description |
|----------|-------------|
| `NEO4J_URI` | Neo4j connection |
| `ZILLIZ_ENDPOINT` | Zilliz endpoint |
| `SIMILARITY_THRESHOLD` | Link threshold |

## Documentation

See [docs/](docs/) folder for complete documentation.

## License

MIT - ConFuse Team