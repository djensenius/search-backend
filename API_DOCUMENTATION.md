# Search Engine Backend API Documentation

## Overview

The Search Engine Backend provides a RESTful API for searching indexed web content and managing indexing operations. The API is built with Rust using the Axum web framework and integrates with Azure Cognitive Search and CosmosDB.

## Base URL

```
http://localhost:3000
```

For production deployments, replace with your actual deployment URL.

## Authentication

Administrative endpoints (`/admin/*`) require authentication via the `X-Admin-Key` header. Set the `ADMIN_API_KEY` environment variable to configure the required key.

### Admin Authentication

For admin endpoints, include the admin key in your request headers:

```bash
curl -H "X-Admin-Key: your-admin-key-here" http://localhost:3000/admin/stats
```

## Content Type

All requests and responses use `application/json` content type unless otherwise specified.

## Response Format

All API responses follow a consistent JSON format:

### Success Response
```json
{
  "field1": "value1",
  "field2": "value2"
}
```

### Error Response
HTTP status codes indicate the type of error:
- `400 Bad Request` - Invalid request parameters
- `500 Internal Server Error` - Server-side processing error

## API Endpoints

### Health Check

Check the health status of the API service.

**Endpoint:** `GET /health`

**Parameters:** None

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2025-01-XX XX:XX:XX UTC",
  "service": "search-engine-backend"
}
```

**Example:**
```bash
curl http://localhost:3000/health
```

---

### Search Content

Search through indexed web content using full-text search capabilities.

**Endpoint:** `GET /search`

**Query Parameters:**
| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `q` | string | Yes | - | Search query string |
| `limit` | integer | No | 10 | Maximum number of results to return (1-50) |
| `offset` | integer | No | 0 | Number of results to skip for pagination |

**Response:**
```json
{
  "query": "search terms",
  "results": [
    {
      "id": "unique-document-id",
      "title": "Document Title",
      "url": "https://example.com/page",
      "snippet": "Relevant excerpt from the document...",
      "score": 0.95,
      "indexed_at": "2025-01-XX XX:XX:XX UTC"
    }
  ],
  "total_count": 25,
  "took_ms": 147
}
```

**Response Fields:**
- `query`: The original search query
- `results`: Array of search results
- `total_count`: Total number of results found
- `took_ms`: Time taken to process the search in milliseconds

**Search Result Fields:**
- `id`: Unique identifier for the document
- `title`: Page title
- `url`: Original URL of the page
- `snippet`: Relevant excerpt highlighting search terms
- `score`: Relevance score (0.0 to 1.0)
- `indexed_at`: When the document was indexed

**Examples:**

Simple search:
```bash
curl "http://localhost:3000/search?q=rust+programming"
```

Search with pagination:
```bash
curl "http://localhost:3000/search?q=azure&limit=5&offset=10"
```

Complex search query:
```bash
curl "http://localhost:3000/search?q=web+development+tutorial"
```

**Error Responses:**

Missing query parameter (400):
```bash
curl "http://localhost:3000/search"
```

Server error (500):
```json
{
  "error": "Internal server error"
}
```

---

### Submit Domains for Indexing

Queue one or more domains for web crawling and indexing.

**Endpoint:** `POST /index`

**Request Body:**
```json
{
  "domains": ["example.com", "another-site.org"]
}
```

**Request Fields:**
- `domains`: Array of domain names to crawl and index

**Response:**
```json
{
  "message": "Successfully queued 2 domains for indexing",
  "domains_queued": 2
}
```

**Response Fields:**
- `message`: Human-readable success message
- `domains_queued`: Number of domains successfully queued

**Examples:**

Index a single domain:
```bash
curl -X POST http://localhost:3000/index \
  -H "Content-Type: application/json" \
  -d '{"domains": ["example.com"]}'
```

Index multiple domains:
```bash
curl -X POST http://localhost:3000/index \
  -H "Content-Type: application/json" \
  -d '{"domains": ["site1.com", "site2.org", "blog.example.net"]}'
```

**Notes:**
- The indexing process runs asynchronously
- Domains are crawled according to robots.txt rules
- Only HTTP and HTTPS URLs are supported
- Duplicate domains in the same request are ignored

---

### Force Start Indexing (Admin)

Forces immediate indexing of all allowed domains, bypassing normal time-based checks. This endpoint requires admin authentication.

**Endpoint:** `POST /admin/force-index`

**Headers:**
- `X-Admin-Key`: Required admin API key for authentication

**Parameters:** None

**Response:**
```json
{
  "message": "Force indexing initiated: 25 domains queued for immediate processing",
  "domains_queued": 25,
  "timer_reset": true
}
```

**Response Fields:**
- `message`: Human-readable description of the operation
- `domains_queued`: Number of domains queued for immediate indexing
- `timer_reset`: Whether periodic indexing timer was reset

**Example Request:**
```bash
curl -X POST http://localhost:3000/admin/force-index \
  -H "X-Admin-Key: your-admin-key-here"
```

**Notes:**
- Forces indexing of all configured allowed domains
- Bypasses the normal time-based re-indexing interval checks
- Requires admin authentication via X-Admin-Key header
- Effectively resets the periodic indexing timer by forcing immediate processing
- Returns 401 Unauthorized if admin key is invalid or missing

---

### Get Search Statistics (Admin)

Retrieve recent search statistics for analytics and monitoring. This endpoint requires admin authentication.

**Endpoint:** `GET /admin/stats`

**Headers:**
- `X-Admin-Key`: Required admin API key for authentication

**Parameters:** None

**Response:**
```json
{
  "recent_searches": [
    {
      "id": "uuid-v4",
      "query": "rust programming",
      "query_normalized": "rust programming",
      "result_count": 15,
      "search_time_ms": 147,
      "timestamp": "2025-01-XX XX:XX:XX UTC",
      "user_ip": null
    }
  ],
  "total_count": 50
}
```

**Response Fields:**
- `recent_searches`: Array of recent search operations (last 50)
- `total_count`: Number of search records returned

**Search Statistic Fields:**
- `id`: Unique identifier for the search operation
- `query`: Original search query
- `query_normalized`: Normalized query for aggregation
- `result_count`: Number of results returned
- `search_time_ms`: Time taken to process the search
- `timestamp`: When the search was performed
- `user_ip`: Client IP address (if available)

**Example:**
```bash
curl http://localhost:3000/admin/stats \
  -H "X-Admin-Key: your-admin-key-here"
```

**Use Cases:**
- Monitor search performance
- Analyze user search patterns
- Track system usage
- Identify slow queries

---

### Get Top Search Queries (Admin)

Retrieve the most frequently searched terms for analytics. This endpoint requires admin authentication.

**Endpoint:** `GET /admin/top-queries`

**Headers:**
- `X-Admin-Key`: Required admin API key for authentication

**Parameters:** None

**Response:**
```json
{
  "top_queries": [
    {
      "query": "rust programming",
      "frequency": 42
    },
    {
      "query": "web development",
      "frequency": 38
    },
    {
      "query": "azure tutorial",
      "frequency": 25
    }
  ]
}
```

**Response Fields:**
- `top_queries`: Array of top 20 most frequent queries

**Top Query Fields:**
- `query`: The search query (normalized)
- `frequency`: Number of times this query was searched

**Example:**
```bash
curl http://localhost:3000/admin/top-queries \
  -H "X-Admin-Key: your-admin-key-here"
```

**Use Cases:**
- Understand popular search topics
- Content optimization insights
- SEO analysis
- User behavior patterns

---

## Rate Limiting

Currently, no rate limiting is implemented. For production use, consider implementing:
- Rate limiting per IP address
- API key-based authentication
- Request throttling for expensive operations

## Error Handling

The API uses standard HTTP status codes:

| Status Code | Description |
|-------------|-------------|
| 200 | Success |
| 400 | Bad Request - Invalid parameters |
| 500 | Internal Server Error - Server-side issue |

## Data Storage

- **Search Index**: Azure Cognitive Search
- **Document Storage**: Azure CosmosDB
- **Statistics**: CosmosDB `search-stats` container
- **Crawl Queue**: CosmosDB `crawl-queue` container

## Performance Considerations

### Search Performance
- Searches typically complete in 50-200ms
- Response time depends on query complexity and index size
- Use pagination for large result sets

### Indexing Performance
- Indexing runs asynchronously in the background
- Crawling respects robots.txt and implements delays
- Large sites may take hours to fully index

### Statistics Storage
- Search statistics are stored asynchronously
- Statistics do not impact search response time
- Statistics are retained indefinitely

## Security Considerations

### Current Implementation
- No authentication required
- Administrative endpoints are publicly accessible
- No input validation beyond basic type checking

### Production Recommendations
- Implement API key authentication
- Secure administrative endpoints
- Add request validation and sanitization
- Enable HTTPS only
- Implement rate limiting
- Add request logging and monitoring

## SDKs and Client Libraries

Currently, no official SDKs are provided. The API can be consumed using any HTTP client library.

### Example Implementations

**JavaScript/Node.js:**
```javascript
const response = await fetch('http://localhost:3000/search?q=rust');
const data = await response.json();
console.log(data.results);
```

**Python:**
```python
import requests

response = requests.get('http://localhost:3000/search', 
                       params={'q': 'rust', 'limit': 5})
data = response.json()
print(data['results'])
```

**Rust:**
```rust
use reqwest;
use serde_json::Value;

let response = reqwest::get("http://localhost:3000/search?q=rust")
    .await?
    .json::<Value>()
    .await?;
```

## Monitoring and Observability

### Logging
- Structured logging using the `tracing` crate
- Request/response logging
- Error logging with context

### Metrics
- Search response times tracked in statistics
- Error rates can be monitored via logs
- Azure service metrics available through Azure Portal

### Health Monitoring
- Use the `/health` endpoint for liveness checks
- Monitor Azure service health through Azure Portal
- Check application logs for errors

## Changelog

### Version 0.1.0
- Initial API implementation
- Basic search functionality
- Domain indexing support
- Search statistics and analytics
- Administrative endpoints

## Support

For API support and questions:
1. Check this documentation
2. Review the main README.md
3. Check Azure service status
4. Open an issue in the GitHub repository

---

**Note**: This API is designed for demonstration purposes. Production deployments should implement additional security, monitoring, and performance optimizations.