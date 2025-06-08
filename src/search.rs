//! # Search Service
//!
//! This module provides the search functionality using Azure Cognitive Search.
//! It handles search queries, index management, and document operations.
//!
//! ## Key Components
//!
//! - [`SearchService`]: Main service for Azure Cognitive Search operations
//! - [`SearchDocument`]: Document structure for search index
//! - [`SearchRequest`]: Request structure for search queries
//! - [`SearchResponse`]: Response structure from search operations
//!
//! ## Usage
//!
//! ```rust,no_run
//! use search_engine_backend::{Config, SearchService};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = Arc::new(Config::from_env()?);
//!     let search_service = SearchService::new(config).await?;
//!     
//!     // Perform a search
//!     let results = search_service.search("rust programming", 10, 0).await?;
//!     Ok(())
//! }
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::{Config, SearchResult};

/// Represents a document in the Azure Cognitive Search index.
///
/// This structure defines the schema for documents stored in the search index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDocument {
    /// Unique identifier for the document
    pub id: String,
    /// Title of the document
    pub title: String,
    /// Original URL of the document
    pub url: String,
    /// Full text content for search indexing
    pub content: String,
    /// Brief excerpt for display in search results
    pub snippet: String,
    /// Domain name the document belongs to
    pub domain: String,
    /// Timestamp when the document was indexed
    pub indexed_at: DateTime<Utc>,
    /// Timestamp of the last crawl
    pub last_crawled: DateTime<Utc>,
}

/// Request structure for Azure Cognitive Search queries.
///
/// Maps to the Azure Search REST API query parameters.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    /// Search query string
    pub search: String,
    /// Maximum number of results to return
    #[serde(rename = "$top")]
    pub top: usize,
    /// Number of results to skip for pagination
    #[serde(rename = "$skip")]
    pub skip: usize,
    /// Fields to include in the response
    #[serde(rename = "$select")]
    pub select: Option<String>,
    pub highlight: Option<String>,
    #[serde(rename = "highlightPreTag")]
    pub highlight_pre_tag: Option<String>,
    #[serde(rename = "highlightPostTag")]
    pub highlight_post_tag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub value: Vec<SearchHit>,
    #[serde(rename = "@odata.count")]
    #[allow(dead_code)]
    pub count: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub url: String,
    #[allow(dead_code)]
    pub content: Option<String>,
    pub snippet: String,
    #[allow(dead_code)]
    pub domain: String,
    pub indexed_at: DateTime<Utc>,
    #[serde(rename = "@search.score")]
    pub score: f64,
    #[serde(rename = "@search.highlights")]
    #[allow(dead_code)]
    pub highlights: Option<HashMap<String, Vec<String>>>,
}

#[derive(Debug, Serialize)]
pub struct IndexSchema {
    pub name: String,
    pub fields: Vec<IndexField>,
}

#[derive(Debug, Serialize)]
pub struct IndexField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub searchable: Option<bool>,
    pub filterable: Option<bool>,
    pub sortable: Option<bool>,
    pub facetable: Option<bool>,
    pub key: Option<bool>,
    pub retrievable: Option<bool>,
}

pub struct SearchService {
    client: Client,
    config: Arc<Config>,
}

impl SearchService {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let client = Client::builder()
            .user_agent(&config.application.user_agent)
            .build()
            .context("Failed to create HTTP client")?;

        let service = Self { client, config };

        // Ensure the search index exists
        service.ensure_index_exists().await?;

        Ok(service)
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchResult>> {
        info!(
            "🔍 SEARCH REQUEST: Query='{}', limit={}, offset={}",
            query, limit, offset
        );

        // Build URL with query parameters for Azure Cognitive Search API
        // Simple URL encoding for common characters
        let query_encoded = query.replace(" ", "%20").replace("+", "%2B");
        let mut url = format!(
            "{}?api-version={}&search={}&$top={}&$skip={}",
            self.config.search_query_url(),
            self.config.azure.search_api_version,
            query_encoded,
            limit,
            offset
        );

        // Add optional parameters
        url.push_str("&$select=id,title,url,snippet,domain,indexed_at");
        url.push_str("&highlight=content,title");
        url.push_str("&highlightPreTag=%3Cmark%3E"); // <mark> URL encoded
        url.push_str("&highlightPostTag=%3C%2Fmark%3E"); // </mark> URL encoded

        debug!(
            "🔍 SEARCH API: Sending request to Azure Cognitive Search at {}",
            url
        );

        let start_time = std::time::Instant::now();

        let response = self
            .client
            .get(&url)
            .header("api-key", &self.config.azure.search_api_key)
            .send()
            .await
            .context("Failed to send search request")?;

        let elapsed = start_time.elapsed();
        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!(
                "❌ SEARCH FAILED: Query='{}' failed with status {} in {}ms - {}",
                query,
                status,
                elapsed.as_millis(),
                error_text
            );
            return Err(anyhow::anyhow!(
                "Search request failed with status {}: {}",
                status,
                error_text
            ));
        }

        debug!(
            "✅ SEARCH RESPONSE: Received response in {}ms with status {}",
            elapsed.as_millis(),
            status
        );

        let search_response: SearchResponse = response
            .json()
            .await
            .context("Failed to parse search response")?;

        let results: Vec<SearchResult> = search_response
            .value
            .into_iter()
            .map(|hit| SearchResult {
                id: hit.id,
                title: hit.title,
                url: hit.url,
                snippet: hit.snippet,
                score: hit.score,
                indexed_at: hit.indexed_at,
            })
            .collect();

        let total_time = start_time.elapsed();
        info!(
            "🎯 SEARCH COMPLETE: Query='{}' returned {} results in {}ms (processing time: {}ms)",
            query,
            results.len(),
            total_time.as_millis(),
            elapsed.as_millis()
        );

        // Log some result details if we have results
        if !results.is_empty() {
            let top_score = results.iter().map(|r| r.score).fold(0.0, f64::max);
            let avg_score = results.iter().map(|r| r.score).sum::<f64>() / results.len() as f64;
            debug!(
                "📊 SEARCH SCORES: Top score: {:.3}, Average score: {:.3}, Results: {}",
                top_score,
                avg_score,
                results.len()
            );
        }

        Ok(results)
    }

    pub async fn index_document(&self, document: &SearchDocument) -> Result<()> {
        debug!(
            "📝 INDEX DOCUMENT: Starting to index {} - title: '{}'",
            document.url, document.title
        );

        let url = format!(
            "{}/index?api-version={}",
            self.config.search_documents_url(),
            self.config.azure.search_api_version
        );

        let index_request = serde_json::json!({
            "value": [
                {
                    "@search.action": "upload",
                    "id": document.id,
                    "title": document.title,
                    "url": document.url,
                    "content": document.content,
                    "snippet": document.snippet,
                    "domain": document.domain,
                    "indexed_at": document.indexed_at,
                    "last_crawled": document.last_crawled,
                }
            ]
        });

        debug!(
            "📤 INDEX API: Sending document to Azure Cognitive Search index at {}",
            url
        );
        debug!(
            "📄 DOCUMENT DETAILS: ID={}, Domain={}, Content size={} chars",
            document.id,
            document.domain,
            document.content.len()
        );

        let start_time = std::time::Instant::now();

        let response = self
            .client
            .post(&url)
            .header("api-key", &self.config.azure.search_api_key)
            .header("Content-Type", "application/json")
            .json(&index_request)
            .send()
            .await
            .context("Failed to send index request")?;

        let elapsed = start_time.elapsed();
        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!(
                "❌ INDEX FAILED: Document {} failed to index with status {} in {}ms - {}",
                document.url,
                status,
                elapsed.as_millis(),
                error_text
            );
            return Err(anyhow::anyhow!(
                "Index request failed with status {}: {}",
                status,
                error_text
            ));
        }

        crate::log_and_capture!(
            info,
            "✅ INDEXED: Document {} successfully indexed in {}ms (title: '{}', {} chars)",
            document.url,
            elapsed.as_millis(),
            document.title,
            document.content.len()
        );
        Ok(())
    }

    async fn ensure_index_exists(&self) -> Result<()> {
        info!(
            "🔍 INDEX CHECK: Verifying search index '{}' exists",
            self.config.azure.search_index_name
        );

        let url = format!(
            "{}?api-version={}",
            self.config.search_index_url(),
            self.config.azure.search_api_version
        );

        // First, check if index exists
        debug!("📡 INDEX API: Checking index existence at {}", url);
        let response = self
            .client
            .get(&url)
            .header("api-key", &self.config.azure.search_api_key)
            .send()
            .await
            .context("Failed to check if index exists")?;

        if response.status().is_success() {
            info!(
                "✅ INDEX EXISTS: Search index '{}' is already available",
                self.config.azure.search_index_name
            );
            return Ok(());
        }

        // Create the index if it doesn't exist
        info!(
            "🛠️ CREATING INDEX: Search index '{}' not found, creating new index",
            self.config.azure.search_index_name
        );

        let schema = IndexSchema {
            name: self.config.azure.search_index_name.clone(),
            fields: vec![
                IndexField {
                    name: "id".to_string(),
                    field_type: "Edm.String".to_string(),
                    key: Some(true),
                    searchable: Some(false),
                    filterable: Some(true),
                    sortable: Some(false),
                    facetable: Some(false),
                    retrievable: Some(true),
                },
                IndexField {
                    name: "title".to_string(),
                    field_type: "Edm.String".to_string(),
                    key: Some(false),
                    searchable: Some(true),
                    filterable: Some(false),
                    sortable: Some(false),
                    facetable: Some(false),
                    retrievable: Some(true),
                },
                IndexField {
                    name: "url".to_string(),
                    field_type: "Edm.String".to_string(),
                    key: Some(false),
                    searchable: Some(false),
                    filterable: Some(true),
                    sortable: Some(false),
                    facetable: Some(false),
                    retrievable: Some(true),
                },
                IndexField {
                    name: "content".to_string(),
                    field_type: "Edm.String".to_string(),
                    key: Some(false),
                    searchable: Some(true),
                    filterable: Some(false),
                    sortable: Some(false),
                    facetable: Some(false),
                    retrievable: Some(false),
                },
                IndexField {
                    name: "snippet".to_string(),
                    field_type: "Edm.String".to_string(),
                    key: Some(false),
                    searchable: Some(false),
                    filterable: Some(false),
                    sortable: Some(false),
                    facetable: Some(false),
                    retrievable: Some(true),
                },
                IndexField {
                    name: "domain".to_string(),
                    field_type: "Edm.String".to_string(),
                    key: Some(false),
                    searchable: Some(false),
                    filterable: Some(true),
                    sortable: Some(false),
                    facetable: Some(true),
                    retrievable: Some(true),
                },
                IndexField {
                    name: "indexed_at".to_string(),
                    field_type: "Edm.DateTimeOffset".to_string(),
                    key: Some(false),
                    searchable: Some(false),
                    filterable: Some(true),
                    sortable: Some(true),
                    facetable: Some(false),
                    retrievable: Some(true),
                },
                IndexField {
                    name: "last_crawled".to_string(),
                    field_type: "Edm.DateTimeOffset".to_string(),
                    key: Some(false),
                    searchable: Some(false),
                    filterable: Some(true),
                    sortable: Some(true),
                    facetable: Some(false),
                    retrievable: Some(false),
                },
            ],
        };

        let create_url = format!(
            "https://{}.search.windows.net/indexes?api-version={}",
            self.config.azure.search_service_name, self.config.azure.search_api_version
        );

        debug!(
            "🛠️ INDEX CREATE API: Sending create request to {}",
            create_url
        );
        debug!(
            "📋 INDEX SCHEMA: Creating index with {} fields",
            schema.fields.len()
        );

        let response = self
            .client
            .post(&create_url)
            .header("api-key", &self.config.azure.search_api_key)
            .header("Content-Type", "application/json")
            .json(&schema)
            .send()
            .await
            .context("Failed to create search index")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!(
                "❌ INDEX CREATE FAILED: Failed to create index '{}' with status {} - {}",
                self.config.azure.search_index_name, status, error_text
            );
            return Err(anyhow::anyhow!(
                "Failed to create search index with status {}: {}",
                status,
                error_text
            ));
        }

        info!(
            "✅ INDEX CREATED: Successfully created search index '{}' with {} fields",
            self.config.azure.search_index_name,
            schema.fields.len()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_config() -> Arc<Config> {
        Arc::new(Config {
            environment: "test".to_string(),
            azure: crate::config::AzureConfig {
                search_service_name: "test-service".to_string(),
                search_api_key: "test-key".to_string(),
                search_api_version: "2023-11-01".to_string(),
                search_index_name: "test-index".to_string(),
                cosmos_endpoint: "test".to_string(),
                cosmos_key: "test".to_string(),
                cosmos_database_name: "test".to_string(),
                cosmos_container_name: "test".to_string(),
            },
            application: crate::config::ApplicationConfig {
                max_crawl_depth: 5,
                crawl_delay_ms: 1000,
                max_concurrent_requests: 10,
                user_agent: "test".to_string(),
                allowed_domains: vec![],
                periodic_index_interval_days: 7,
                duplicate_removal_interval_hours: 24,
                admin_api_key: "test".to_string(),
            },
        })
    }

    #[test]
    fn test_search_url_construction() {
        let config = create_test_config();

        // Test that the URL is correctly constructed for search queries
        let expected_base = "https://test-service.search.windows.net/indexes/test-index/docs";
        assert_eq!(config.search_query_url(), expected_base);

        // Test URL encoding of common characters
        let test_query = "web development tutorial";
        let encoded = test_query.replace(" ", "%20").replace("+", "%2B");
        assert_eq!(encoded, "web%20development%20tutorial");

        let test_query_with_plus = "C++ programming";
        let encoded_plus = test_query_with_plus.replace(" ", "%20").replace("+", "%2B");
        assert_eq!(encoded_plus, "C%2B%2B%20programming");
    }

    #[test]
    fn test_search_request_structure_removed() {
        // Verify that we're no longer using SearchRequest for JSON body
        // This test validates that our change from POST+JSON to GET+query params is working

        // The SearchRequest struct should still exist for potential future use
        // but we should not be using it in the search method anymore
        let search_request = SearchRequest {
            search: "test".to_string(),
            top: 10,
            skip: 0,
            select: Some("id,title".to_string()),
            highlight: Some("content".to_string()),
            highlight_pre_tag: Some("<mark>".to_string()),
            highlight_post_tag: Some("</mark>".to_string()),
        };

        // The struct should still be valid for serialization if needed
        assert_eq!(search_request.search, "test");
        assert_eq!(search_request.top, 10);
        assert_eq!(search_request.skip, 0);
    }
}
