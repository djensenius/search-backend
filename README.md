[![Lint](https://github.com/djensenius/search-backend/actions/workflows/lint.yml/badge.svg)](https://github.com/djensenius/search-backend/actions/workflows/lint.yml)
[![Test](https://github.com/djensenius/search-backend/actions/workflows/test.yml/badge.svg)](https://github.com/djensenius/search-backend/actions/workflows/test.yml)
[![Build](https://github.com/djensenius/search-backend/actions/workflows/build.yml/badge.svg)](https://github.com/djensenius/search-backend/actions/workflows/build.yml)
[![Documentation](https://github.com/djensenius/search-backend/actions/workflows/rustdoc.yml/badge.svg)](https://github.com/djensenius/search-backend/actions/workflows/rustdoc.yml)

[![Deploy](https://github.com/djensenius/search-backend/actions/workflows/main_dachshund-api.yml/badge.svg)](https://github.com/djensenius/search-backend/actions/workflows/main_dachshund-api.yml)

# Search Engine Backend

A search engine backend built in Rust that uses Azure Cognitive Search and CosmosDB for indexing and storing web content. This application can crawl websites, index their content, and provide search capabilities.

## Features

- **Web Crawling**: Crawl websites with respect for robots.txt
- **Content Indexing**: Store and index web content using Azure Cognitive Search
- **Data Storage**: Use Azure CosmosDB for persistent storage
- **REST API**: HTTP endpoints for search and crawler management
- **Azure Deployment**: Ready for deployment to Azure Container Apps or similar services
- **Local Development**: Can run entirely locally with Azure services

## Architecture

The application consists of several main components:

- **Web Crawler**: Crawls websites and extracts content
- **Search Service**: Interfaces with Azure Cognitive Search
- **Storage Service**: Manages data in Azure CosmosDB
- **HTTP API**: REST endpoints for search and management

## Prerequisites

### For Local Development

1. **Rust** (latest stable version)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Azure Account** with:
   - Azure Cognitive Search service
   - Azure CosmosDB account (SQL API)

### For Azure Deployment

1. **Azure CLI**
   ```bash
   curl -sL https://aka.ms/InstallAzureCLIDeb | sudo bash
   ```

2. **Docker** (for containerization)
   ```bash
   sudo apt-get update
   sudo apt-get install docker.io
   ```

## Setup

### 1. Clone the Repository

```bash
git clone https://github.com/djensenius/search-backend.git
cd search-engine-backend
```

### 2. Configure Azure Services

#### Azure Cognitive Search

1. Create an Azure Cognitive Search service in the Azure portal
2. Note the service name and admin key
3. The application will automatically create the required search index

#### Azure CosmosDB

1. Create a CosmosDB account with SQL API
2. Note the account name, database name, and primary key
3. The application will automatically create the required database and containers

### 3. Environment Configuration

Copy the example environment file and configure it:

```bash
cp .env.example .env
```

Edit `.env` with your Azure service details:

```bash
# Azure Cognitive Search
AZURE_SEARCH_SERVICE_NAME=your-search-service-name
AZURE_SEARCH_ADMIN_KEY=your-search-admin-key
AZURE_SEARCH_INDEX_NAME=web-pages

# Azure CosmosDB
AZURE_COSMOS_ACCOUNT=your-cosmos-account-name
AZURE_COSMOS_KEY=your-cosmos-primary-key
AZURE_COSMOS_DATABASE_NAME=search-engine
AZURE_COSMOS_WEBPAGES_CONTAINER=webpages
AZURE_COSMOS_CRAWL_QUEUE_CONTAINER=crawl-queue

# Application Settings
MAX_CRAWL_DEPTH=3
CRAWL_BATCH_SIZE=10
USER_AGENT=SearchBot/1.0
```

## Running Locally

### 1. Build the Application

```bash
cargo build --release
```

### 2. Start the Server

```bash
cargo run -- --port 3000
```

The server will start on `http://localhost:3000` by default.

### 3. Test the API

#### Start Crawling a Domain

```bash
curl -X POST http://localhost:3000/crawl \
  -H "Content-Type: application/json" \
  -d '{"domain": "example.com"}'
```

#### Search Content

```bash
curl "http://localhost:3000/search?q=your+search+terms"
```

#### Check Health

```bash
curl http://localhost:3000/health
```

## API Endpoints

### GET /health
Health check endpoint

**Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

### POST /crawl
Start crawling a domain

**Request:**
```json
{
  "domain": "example.com"
}
```

**Response:**
```json
{
  "message": "Crawling started for domain: example.com"
}
```

### GET /search
Search indexed content

**Parameters:**
- `q` (required): Search query
- `limit` (optional): Number of results (default: 10, max: 50)

**Response:**
```json
{
  "results": [
    {
      "id": "unique-id",
      "title": "Page Title",
      "url": "https://example.com/page",
      "snippet": "Brief excerpt from the page...",
      "score": 0.95,
      "indexed_at": "2025-01-XX XX:XX:XX UTC"
    }
  ],
  "total": 1
}
```

## Azure Deployment

### 1. Create Container Image

Create a `Dockerfile`:

```dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/search-backend /usr/local/bin/search-backend

EXPOSE 3000

CMD ["search-backend", "--port", "3000"]
```

### 2. Build and Push Image

```bash
# Build the image
docker build -t search-backend-demo .

# Tag for Azure Container Registry (replace with your registry)
docker tag search-backend-demo your-registry.azurecr.io/search-backend-demo:latest

# Push to registry
docker push your-registry.azurecr.io/search-backend-demo:latest
```

### 3. Deploy to Azure Container Apps

Create a Container App:

```bash
az containerapp create \
  --name search-backend-demo \
  --resource-group your-resource-group \
  --environment your-container-env \
  --image your-registry.azurecr.io/search-backend-demo:latest \
  --target-port 3000 \
  --ingress external \
  --env-vars \
    AZURE_SEARCH_SERVICE_NAME="your-search-service" \
    AZURE_SEARCH_ADMIN_KEY="your-search-key" \
    AZURE_SEARCH_INDEX_NAME="web-pages" \
    AZURE_COSMOS_ACCOUNT="your-cosmos-account" \
    AZURE_COSMOS_KEY="your-cosmos-key" \
    AZURE_COSMOS_DATABASE_NAME="search-engine" \
    AZURE_COSMOS_WEBPAGES_CONTAINER="webpages" \
    AZURE_COSMOS_CRAWL_QUEUE_CONTAINER="crawl-queue" \
    MAX_CRAWL_DEPTH="3" \
    CRAWL_BATCH_SIZE="10" \
    USER_AGENT="SearchBot/1.0"
```

### 4. Alternative: Azure App Service

You can also deploy to Azure App Service using Web App for Containers:

```bash
az webapp create \
  --resource-group your-resource-group \
  --plan your-app-service-plan \
  --name search-backend-demo \
  --deployment-container-image-name your-registry.azurecr.io/search-backend-demo:latest
```

## Configuration

### Environment Variables

| Variable | Description | Required | Default |
|----------|-------------|----------|---------|
| `AZURE_SEARCH_SERVICE_NAME` | Azure Search service name | Yes | - |
| `AZURE_SEARCH_ADMIN_KEY` | Azure Search admin key | Yes | - |
| `AZURE_SEARCH_INDEX_NAME` | Search index name | No | `web-pages` |
| `AZURE_COSMOS_ACCOUNT` | CosmosDB account name | Yes | - |
| `AZURE_COSMOS_KEY` | CosmosDB primary key | Yes | - |
| `AZURE_COSMOS_DATABASE_NAME` | Database name | No | `search-engine` |
| `AZURE_COSMOS_WEBPAGES_CONTAINER` | Web pages container | No | `webpages` |
| `AZURE_COSMOS_CRAWL_QUEUE_CONTAINER` | Crawl queue container | No | `crawl-queue` |
| `MAX_CRAWL_DEPTH` | Maximum crawl depth | No | `3` |
| `CRAWL_BATCH_SIZE` | Concurrent crawl jobs | No | `10` |
| `USER_AGENT` | HTTP User-Agent string | No | `SearchBot/1.0` |

### Command Line Arguments

```bash
search-backend [OPTIONS]

Options:
  -p, --port <PORT>      Port to run the server on [default: 3000]
  -c, --config <CONFIG>  Path to configuration file
  -h, --help            Print help
```

## Development

### Project Structure

```
src/
├── main.rs          # Application entry point
├── lib.rs           # Library exports and API routes
├── config.rs        # Configuration management
├── search.rs        # Azure Cognitive Search integration
├── storage.rs       # Azure CosmosDB integration
└── indexer.rs       # Web crawling and indexing logic
```

### Running Tests

```bash
cargo test
```

### Linting

```bash
cargo clippy
```

### Formatting

```bash
cargo fmt
```

## Troubleshooting

### Common Issues

1. **Azure Service Connection Errors**
   - Verify your Azure credentials and service names
   - Check that your Azure services are in the same region
   - Ensure firewall rules allow connections

2. **Crawling Issues**
   - Check robots.txt compliance
   - Verify domain accessibility
   - Monitor rate limiting

3. **Search Results Empty**
   - Ensure crawling has completed
   - Check Azure Cognitive Search index status
   - Verify search query syntax

### Logs

The application uses structured logging. Set `RUST_LOG=debug` for detailed logs:

```bash
RUST_LOG=debug cargo run
```

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature-name`
3. Make your changes and add tests
4. Run tests: `cargo test`
5. Commit changes: `git commit -m "Add feature"`
6. Push to branch: `git push origin feature-name`
7. Submit a pull request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Security

- Never commit Azure keys or secrets to version control
- Use Azure Key Vault for production deployments
- Implement rate limiting for production use
- Monitor Azure service quotas and usage

## Documentation

### API Documentation

The Rust API documentation is automatically generated and deployed to GitHub Pages:

- **Live Documentation**: [https://djensenius.github.io/search-backend/search_engine_backend/](https://djensenius.github.io/search-backend/search_engine_backend/)

The documentation is automatically updated when changes are pushed to the `main` branch via GitHub Actions.

### Local Documentation

To generate and view documentation locally:

```bash
# Generate documentation
cargo doc --no-deps --all-features --document-private-items --open

# This will open the documentation in your default browser
```

## Support

For questions and issues:

1. Check the troubleshooting section above
2. Review Azure service documentation
3. Open an issue in this repository

## Roadmap

Future enhancements may include:

- [ ] Support for multiple search indices
- [ ] Advanced crawling features (sitemaps, pagination)
- [ ] Real-time indexing with webhooks
- [ ] Search analytics and metrics
- [ ] Authentication and authorization
- [ ] Content deduplication
- [ ] Support for additional Azure regions

---

**Note**: This is a demo application. For production use, implement additional security measures, error handling, monitoring, and performance optimizations.
