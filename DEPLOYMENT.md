# Azure Container Apps Deployment

This directory contains Azure deployment templates for the Search Engine Backend.

## Quick Deploy

### Prerequisites

1. Azure CLI installed and logged in
2. Docker image built and pushed to a container registry
3. Azure Cognitive Search and CosmosDB services created

### Deploy using Azure CLI

1. Set environment variables:

```bash
export RESOURCE_GROUP="rg-search-backend"
export LOCATION="East US"
export CONTAINER_APP_ENV="env-search-backend"
export APP_NAME="search-backend-demo"
export CONTAINER_IMAGE="your-registry.azurecr.io/search-backend-demo:latest"

# Azure service configurations
export AZURE_SEARCH_SERVICE_NAME="your-search-service"
export AZURE_SEARCH_ADMIN_KEY="your-search-key"
export AZURE_COSMOS_ACCOUNT="your-cosmos-account"
export AZURE_COSMOS_KEY="your-cosmos-key"
```

2. Create resource group (if it doesn't exist):

```bash
az group create --name $RESOURCE_GROUP --location "$LOCATION"
```

3. Create Container Apps environment:

```bash
az containerapp env create \
  --name $CONTAINER_APP_ENV \
  --resource-group $RESOURCE_GROUP \
  --location "$LOCATION"
```

4. Deploy the application:

```bash
az containerapp create \
  --name $APP_NAME \
  --resource-group $RESOURCE_GROUP \
  --environment $CONTAINER_APP_ENV \
  --image $CONTAINER_IMAGE \
  --target-port 3000 \
  --ingress external \
  --min-replicas 1 \
  --max-replicas 3 \
  --cpu 0.5 \
  --memory 1Gi \
  --env-vars \
    AZURE_SEARCH_SERVICE_NAME="$AZURE_SEARCH_SERVICE_NAME" \
    AZURE_SEARCH_ADMIN_KEY="$AZURE_SEARCH_ADMIN_KEY" \
    AZURE_SEARCH_INDEX_NAME="web-pages" \
    AZURE_COSMOS_ACCOUNT="$AZURE_COSMOS_ACCOUNT" \
    AZURE_COSMOS_KEY="$AZURE_COSMOS_KEY" \
    AZURE_COSMOS_DATABASE_NAME="search-engine" \
    AZURE_COSMOS_WEBPAGES_CONTAINER="webpages" \
    AZURE_COSMOS_CRAWL_QUEUE_CONTAINER="crawl-queue" \
    MAX_CRAWL_DEPTH="3" \
    CRAWL_BATCH_SIZE="10" \
    USER_AGENT="SearchBot/1.0"
```

5. Get the application URL:

```bash
az containerapp show \
  --name $APP_NAME \
  --resource-group $RESOURCE_GROUP \
  --query properties.configuration.ingress.fqdn \
  --out tsv
```

## Azure App Service Deployment

Alternatively, deploy to Azure App Service:

```bash
# Create App Service Plan
az appservice plan create \
  --name plan-search-backend \
  --resource-group $RESOURCE_GROUP \
  --location "$LOCATION" \
  --is-linux \
  --sku B1

# Create Web App
az webapp create \
  --resource-group $RESOURCE_GROUP \
  --plan plan-search-backend \
  --name $APP_NAME \
  --deployment-container-image-name $CONTAINER_IMAGE

# Configure environment variables
az webapp config appsettings set \
  --name $APP_NAME \
  --resource-group $RESOURCE_GROUP \
  --settings \
    AZURE_SEARCH_SERVICE_NAME="$AZURE_SEARCH_SERVICE_NAME" \
    AZURE_SEARCH_ADMIN_KEY="$AZURE_SEARCH_ADMIN_KEY" \
    AZURE_SEARCH_INDEX_NAME="web-pages" \
    AZURE_COSMOS_ACCOUNT="$AZURE_COSMOS_ACCOUNT" \
    AZURE_COSMOS_KEY="$AZURE_COSMOS_KEY" \
    AZURE_COSMOS_DATABASE_NAME="search-engine" \
    AZURE_COSMOS_WEBPAGES_CONTAINER="webpages" \
    AZURE_COSMOS_CRAWL_QUEUE_CONTAINER="crawl-queue" \
    MAX_CRAWL_DEPTH="3" \
    CRAWL_BATCH_SIZE="10" \
    USER_AGENT="SearchBot/1.0"
```

## Monitoring and Scaling

### Enable Application Insights

```bash
# Create Application Insights
az monitor app-insights component create \
  --app search-backend-insights \
  --location "$LOCATION" \
  --resource-group $RESOURCE_GROUP

# Get instrumentation key
APPINSIGHTS_KEY=$(az monitor app-insights component show \
  --app search-backend-insights \
  --resource-group $RESOURCE_GROUP \
  --query instrumentationKey \
  --out tsv)

# Update Container App with Application Insights
az containerapp update \
  --name $APP_NAME \
  --resource-group $RESOURCE_GROUP \
  --set-env-vars APPINSIGHTS_INSTRUMENTATIONKEY="$APPINSIGHTS_KEY"
```

### Scale Configuration

The deployment above is configured with:
- **Min replicas**: 1 (always have at least one instance)
- **Max replicas**: 3 (scale up to 3 instances under load)
- **CPU**: 0.5 cores per instance
- **Memory**: 1GB per instance

Adjust these values based on your expected load and budget.

## Security Considerations

### Use Key Vault for Secrets

For production deployments, store secrets in Azure Key Vault:

```bash
# Create Key Vault
az keyvault create \
  --name kv-search-backend \
  --resource-group $RESOURCE_GROUP \
  --location "$LOCATION"

# Store secrets
az keyvault secret set \
  --vault-name kv-search-backend \
  --name azure-search-key \
  --value "$AZURE_SEARCH_ADMIN_KEY"

az keyvault secret set \
  --vault-name kv-search-backend \
  --name azure-cosmos-key \
  --value "$AZURE_COSMOS_KEY"
```

Then reference the secrets in your Container App configuration.

## Cleanup

To remove all resources:

```bash
az group delete --name $RESOURCE_GROUP --yes --no-wait
```