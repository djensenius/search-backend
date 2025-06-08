# GitHub Workflows

This repository includes four separate GitHub workflows for continuous integration and deployment:

## Workflows Overview

### 1. Lint Workflow (`.github/workflows/lint.yml`)
- **Triggers**: Push and pull requests to `main` and `develop` branches
- **Purpose**: Code quality enforcement
- **Actions**:
  - Check code formatting with `cargo fmt --check`
  - Run linting with `cargo clippy --all-targets --all-features -- -D warnings`
- **Dependencies**: Rust stable toolchain with clippy and rustfmt components

### 2. Build Workflow (`.github/workflows/build.yml`)
- **Triggers**: Push and pull requests to `main` and `develop` branches  
- **Purpose**: Compile the application
- **Actions**:
  - Build release binary with `cargo build --release`
  - Upload binary artifact (1-day retention)
- **Dependencies**: Rust stable toolchain

### 3. Test Workflow (`.github/workflows/test.yml`)
- **Triggers**: Push and pull requests to `main` and `develop` branches
- **Purpose**: Run automated tests
- **Actions**:
  - Execute tests with `cargo test --all-features`
  - Set debug logging environment
- **Dependencies**: Rust stable toolchain

### 4. Deploy Workflow (`.github/workflows/deploy.yml`)
- **Triggers**: Push to `main` branch only (on merge)
- **Purpose**: Deploy to Azure Container Apps
- **Actions**:
  - Build and push Docker image to Azure Container Registry
  - Deploy to Azure Container Apps with production configuration
- **Dependencies**: Azure credentials and container registry access

## Required Secrets for Deployment

To use the deployment workflow, configure these repository secrets:

### Azure Authentication
- `AZURE_CREDENTIALS`: Service principal credentials in JSON format
- `AZURE_RESOURCE_GROUP`: Azure resource group name
- `AZURE_CONTAINER_REGISTRY`: Container registry URL (e.g., `myregistry.azurecr.io`)
- `AZURE_REGISTRY_USERNAME`: Container registry username
- `AZURE_REGISTRY_PASSWORD`: Container registry password
- `AZURE_CONTAINER_APP_ENVIRONMENT`: Container Apps environment name

### Azure Services Configuration
- `AZURE_SEARCH_SERVICE_NAME`: Azure Cognitive Search service name
- `AZURE_SEARCH_API_KEY`: Azure Cognitive Search admin key
- `AZURE_COSMOS_ENDPOINT`: CosmosDB endpoint URL
- `AZURE_COSMOS_KEY`: CosmosDB primary key

## Workflow Features

- **Efficient Caching**: All workflows use Cargo registry caching to speed up builds
- **Security**: Deploy workflow only runs on main branch merges
- **Modern Actions**: Uses latest GitHub Actions versions (v4/v5)
- **Production Ready**: Deploy workflow includes all necessary environment variables
- **Separate Concerns**: Each workflow has a single responsibility

## Usage

The workflows will automatically run based on their triggers. For deployment:

1. Ensure all required secrets are configured in repository settings
2. Merge changes into the `main` branch
3. The deploy workflow will automatically build and deploy to Azure

## Troubleshooting

- **Lint failures**: Run `cargo fmt` and `cargo clippy` locally to fix issues
- **Build failures**: Check Rust version compatibility and dependencies
- **Test failures**: Ensure tests pass locally with `cargo test`
- **Deploy failures**: Verify Azure credentials and service configuration