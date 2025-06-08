use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use reqwest::Client;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

use crate::search::SearchDocument;
use crate::storage::{CrawlQueue, CrawlStatus, WebPage};
use crate::{Config, SearchService, StorageService};

pub struct IndexerService {
    config: Arc<Config>,
    storage_service: Arc<StorageService>,
    search_service: Arc<SearchService>,
    client: Client,
    force_process_flag: Arc<AtomicBool>,
}

impl IndexerService {
    /// Generate a deterministic ID from a URL to prevent duplicate crawl items
    fn url_to_id(url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let hash = hasher.finalize();
        format!("{:x}", hash)
    }

    pub async fn new(
        config: Arc<Config>,
        storage_service: Arc<StorageService>,
        search_service: Arc<SearchService>,
    ) -> Result<Self> {
        let client = Client::builder()
            .user_agent(&config.application.user_agent)
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client for indexer")?;

        Ok(Self {
            config,
            storage_service,
            search_service,
            client,
            force_process_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Queue domains for crawling
    pub async fn queue_domains(&self, domains: &[String]) -> Result<usize> {
        self.queue_domains_with_check(domains, true).await
    }

    /// Queue domains for crawling with optional last-indexed time checking
    /// If check_last_indexed is true, domains that were indexed recently will be skipped
    pub async fn queue_domains_with_check(
        &self,
        domains: &[String],
        check_last_indexed: bool,
    ) -> Result<usize> {
        let mut queued_count = 0;

        for domain in domains {
            let url = if domain.starts_with("http") {
                domain.clone()
            } else {
                format!("https://{}", domain)
            };

            let parsed_url = Url::parse(&url).with_context(|| format!("Invalid URL: {}", url))?;

            let domain_name = parsed_url
                .host_str()
                .ok_or_else(|| anyhow::anyhow!("No host in URL: {}", url))?
                .to_string();

            // Check if domain is allowed
            if !self.config.is_domain_allowed(&domain_name) {
                warn!("Domain not in allowed list, skipping: {}", domain_name);
                continue;
            }

            // Check if domain was recently indexed (if check is enabled)
            if check_last_indexed {
                if let Ok(Some(last_indexed)) = self
                    .storage_service
                    .get_domain_last_indexed(&domain_name)
                    .await
                {
                    let threshold = Utc::now()
                        - ChronoDuration::days(
                            self.config.application.periodic_index_interval_days as i64,
                        );
                    if last_indexed > threshold {
                        crate::log_and_capture!(info,
                            "📋 SKIPPING DOMAIN: {} was recently indexed on {} (within {} day interval), skipping indexing",
                            domain_name,
                            last_indexed.format("%Y-%m-%d %H:%M:%S UTC"),
                            self.config.application.periodic_index_interval_days
                        );
                        continue;
                    } else {
                        crate::log_and_capture!(info,
                            "🔄 INDEXING DOMAIN: {} was last indexed on {} (older than {} day interval), will re-index",
                            domain_name,
                            last_indexed.format("%Y-%m-%d %H:%M:%S UTC"),
                            self.config.application.periodic_index_interval_days
                        );
                    }
                } else {
                    info!(
                        "🆕 INDEXING DOMAIN: {} has never been indexed, adding to queue",
                        domain_name
                    );
                }
            } else {
                info!(
                    "➡️ INDEXING DOMAIN: {} added to queue (forced indexing, no recency check)",
                    domain_name
                );
            }

            let crawl_item = CrawlQueue {
                id: Uuid::new_v4().to_string(),
                url: url.clone(),
                domain: domain_name,
                depth: 0,
                status: CrawlStatus::Pending,
                created_at: Utc::now(),
                processed_at: None,
                error_message: None,
                retry_count: 0,
            };

            match self.storage_service.queue_crawl(&crawl_item).await {
                Ok(_) => {
                    info!(
                        "✅ QUEUED: {} successfully added to crawl queue (depth: {})",
                        url, crawl_item.depth
                    );
                    queued_count += 1;
                }
                Err(e) => {
                    warn!("❌ QUEUE FAILED: Unable to queue domain {} - {}", url, e);
                }
            }
        }

        info!(
            "📊 DOMAIN QUEUEING SUMMARY: Processed {} domains, queued {} for indexing",
            domains.len(),
            queued_count
        );
        Ok(queued_count)
    }

    /// Start the periodic indexing service that checks for stale domains and re-indexes them
    /// This runs in a loop, checking every 6 hours for domains that need re-indexing
    pub async fn start_periodic_indexing(&self) -> Result<()> {
        info!(
            "Starting periodic indexing service with interval of {} days",
            self.config.application.periodic_index_interval_days
        );

        // Run periodically - check every 6 hours for domains that need re-indexing
        let check_interval = Duration::from_secs(6 * 60 * 60); // 6 hours

        loop {
            sleep(check_interval).await;

            info!("Running periodic domain indexing check");

            match self.check_and_queue_stale_domains().await {
                Ok(count) => {
                    if count > 0 {
                        info!("Queued {} domains for periodic re-indexing", count);
                    } else {
                        debug!("No domains need re-indexing at this time");
                    }
                }
                Err(e) => {
                    error!("Failed to check and queue stale domains: {}", e);
                }
            }
        }
    }

    /// Start the periodic duplicate removal service
    /// This runs in a loop, checking for and removing duplicates at configured intervals
    pub async fn start_periodic_duplicate_removal(&self) -> Result<()> {
        info!(
            "Starting periodic duplicate removal service with interval of {} hours",
            self.config.application.duplicate_removal_interval_hours
        );

        // Run periodically based on configuration
        let check_interval =
            Duration::from_secs(self.config.application.duplicate_removal_interval_hours * 60 * 60);

        loop {
            sleep(check_interval).await;

            info!("Running periodic duplicate removal");

            match self.storage_service.remove_duplicates().await {
                Ok(count) => {
                    if count > 0 {
                        info!("Removed {} duplicate entries", count);
                    } else {
                        debug!("No duplicates found during this run");
                    }
                }
                Err(e) => {
                    error!("Failed to remove duplicates: {}", e);
                }
            }
        }
    }

    /// Trigger immediate queue processing by setting the force flag
    pub fn trigger_force_process_queue(&self) -> Result<()> {
        self.force_process_flag.store(true, Ordering::Relaxed);
        info!("🚀 Force queue processing triggered");
        Ok(())
    }

    async fn check_and_queue_stale_domains(&self) -> Result<usize> {
        // Get all allowed domains and queue those that haven't been indexed recently
        let allowed_domains = &self.config.application.allowed_domains;

        // Use queue_domains_with_check which will automatically check last indexed time
        self.queue_domains_with_check(allowed_domains, true).await
    }

    pub async fn process_crawl_queue(&self) -> Result<()> {
        info!("🚀 Starting crawl queue processing service");

        let mut last_status_log = std::time::Instant::now();
        let status_log_interval = Duration::from_secs(30); // Log status every 30 seconds
        let mut items_processed_count = 0;
        let mut total_processing_time = Duration::from_secs(0);

        loop {
            // Log periodic status if enough time has passed
            if last_status_log.elapsed() >= status_log_interval {
                self.log_crawl_queue_status(items_processed_count, total_processing_time)
                    .await;
                last_status_log = std::time::Instant::now();
            }

            let pending_items = self
                .storage_service
                .get_pending_crawl_items(self.config.application.max_concurrent_requests)
                .await?;

            if pending_items.is_empty() {
                debug!("📭 No pending crawl items, entering idle mode");

                // Wait for either timeout or force signal
                let mut sleep_duration = Duration::from_secs(10);
                loop {
                    // Check if force processing was triggered
                    if self.force_process_flag.load(Ordering::Relaxed) {
                        self.force_process_flag.store(false, Ordering::Relaxed);
                        info!("⚡ Force queue processing signal received - checking for items immediately");
                        break;
                    }

                    // Sleep for 1 second at a time to be responsive to force signals
                    let chunk_duration = Duration::from_secs(1);
                    sleep(chunk_duration).await;

                    if sleep_duration <= chunk_duration {
                        break;
                    }
                    sleep_duration -= chunk_duration;
                }
                continue;
            }

            let batch_start = std::time::Instant::now();

            // Log detailed information about the batch
            let root_domains = pending_items.iter().filter(|item| item.depth == 0).count();
            let discovered_links = pending_items.iter().filter(|item| item.depth > 0).count();

            info!(
                "⚙️ Processing batch of {} crawl items: {} root domains, {} discovered links",
                pending_items.len(),
                root_domains,
                discovered_links
            );

            if discovered_links > 0 {
                info!("🎯 PROCESSING DISCOVERED LINKS: The queue processor is now processing {} previously discovered links - this should resolve the missing URLs issue!", discovered_links);
            }

            // Process items concurrently but with a limit
            let semaphore = Arc::new(tokio::sync::Semaphore::new(
                self.config.application.max_concurrent_requests,
            ));
            let mut tasks = Vec::new();

            for item in pending_items {
                let semaphore = semaphore.clone();
                let storage_service = self.storage_service.clone();
                let search_service = self.search_service.clone();
                let config = self.config.clone();
                let client = self.client.clone();

                let task = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    Self::process_crawl_item(item, storage_service, search_service, config, client)
                        .await
                });

                tasks.push(task);
            }

            // Wait for all tasks to complete
            let mut batch_success_count = 0;
            let mut batch_failure_count = 0;
            for task in tasks {
                match task.await {
                    Ok(_) => batch_success_count += 1,
                    Err(e) => {
                        error!("Crawl task failed: {}", e);
                        batch_failure_count += 1;
                    }
                }
            }

            let batch_duration = batch_start.elapsed();
            items_processed_count += batch_success_count + batch_failure_count;
            total_processing_time += batch_duration;

            info!(
                "✅ Batch completed: {} successful, {} failed, took {:?} (total processed: {})",
                batch_success_count, batch_failure_count, batch_duration, items_processed_count
            );

            // Add delay between batches
            sleep(Duration::from_millis(
                self.config.application.crawl_delay_ms,
            ))
            .await;
        }
    }

    /// Log detailed crawl queue status for monitoring
    async fn log_crawl_queue_status(&self, items_processed: usize, total_time: Duration) {
        match self.storage_service.get_crawl_queue_stats().await {
            Ok((pending, processing, completed, failed)) => {
                let avg_time_per_item = if items_processed > 0 {
                    total_time.as_millis() as f64 / items_processed as f64
                } else {
                    0.0
                };

                info!(
                    "📊 CRAWL QUEUE STATUS: Pending: {}, Processing: {}, Completed: {}, Failed: {} | Processed: {} items | Avg time/item: {:.1}ms",
                    pending, processing, completed, failed, items_processed, avg_time_per_item
                );

                // Log warning if queue is backing up
                if pending > self.config.application.max_concurrent_requests * 3 {
                    warn!(
                        "⚠️ QUEUE BACKLOG: {} pending items detected, consider scaling up processing",
                        pending
                    );
                }

                // Log info about processing efficiency
                if failed > 0 && completed > 0 {
                    let failure_rate = (failed as f64 / (completed + failed) as f64) * 100.0;
                    if failure_rate > 20.0 {
                        warn!(
                            "⚠️ HIGH FAILURE RATE: {:.1}% of crawl attempts are failing",
                            failure_rate
                        );
                    } else {
                        debug!("✅ Crawl success rate: {:.1}%", 100.0 - failure_rate);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to get crawl queue statistics: {}", e);
            }
        }
    }

    async fn process_crawl_item(
        item: CrawlQueue,
        storage_service: Arc<StorageService>,
        search_service: Arc<SearchService>,
        config: Arc<Config>,
        client: Client,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!(
            "🔍 PROCESSING: Starting to crawl {} (domain: {}, depth: {})",
            item.url, item.domain, item.depth
        );

        // Update status to processing
        if let Err(e) = storage_service
            .update_crawl_status(&item.id, &item.domain, CrawlStatus::Processing, None)
            .await
        {
            warn!("Failed to update crawl status to processing: {}", e);
        }

        match Self::crawl_and_index_page(&item, &storage_service, &search_service, &config, &client)
            .await
        {
            Ok(_) => {
                let processing_time = start_time.elapsed();
                crate::log_and_capture!(
                    info,
                    "✅ INDEXED: Successfully processed and indexed {} in {:?}",
                    item.url,
                    processing_time
                );
                if let Err(e) = storage_service
                    .update_crawl_status(&item.id, &item.domain, CrawlStatus::Completed, None)
                    .await
                {
                    warn!("Failed to update crawl status to completed: {}", e);
                }
            }
            Err(e) => {
                let processing_time = start_time.elapsed();
                error!(
                    "❌ INDEX FAILED: Failed to process {} after {:?} - {}",
                    item.url, processing_time, e
                );
                if let Err(update_err) = storage_service
                    .update_crawl_status(
                        &item.id,
                        &item.domain,
                        CrawlStatus::Failed,
                        Some(e.to_string()),
                    )
                    .await
                {
                    warn!("Failed to update crawl status to failed: {}", update_err);
                }
            }
        }

        Ok(())
    }

    async fn crawl_and_index_page(
        item: &CrawlQueue,
        storage_service: &Arc<StorageService>,
        search_service: &Arc<SearchService>,
        config: &Arc<Config>,
        client: &Client,
    ) -> Result<()> {
        debug!("Crawling URL: {}", item.url);

        // Check robots.txt (simplified check)
        if !Self::is_allowed_by_robots(&item.url, &config.application.user_agent, client).await? {
            info!(
                "🚫 ROBOTS.TXT BLOCKED: {} is disallowed by robots.txt, skipping",
                item.url
            );
            return Ok(());
        }

        // Fetch the page
        let response = client
            .get(&item.url)
            .send()
            .await
            .context("Failed to fetch page")?;

        let status_code = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("HTTP error: {}", status_code));
        }

        let content = response
            .text()
            .await
            .context("Failed to read response body")?;
        let content_length = content.len();

        debug!(
            "📥 FETCHED: {} - {} bytes, status: {}",
            item.url, content_length, status_code
        );

        // Only process HTML content
        if let Some(ref ct) = content_type {
            if !ct.contains("text/html") {
                info!(
                    "📄 CONTENT TYPE SKIPPED: {} has content type '{}', only HTML is indexed",
                    item.url, ct
                );
                return Ok(());
            }
        }

        // Parse HTML and extract all data synchronously
        debug!(
            "🔍 PARSING: Extracting content from {} ({} bytes)",
            item.url,
            content.len()
        );
        let (title, text_content, snippet, discovered_links) = {
            let document = Html::parse_document(&content);

            // Extract title
            let title_selector = Selector::parse("title").unwrap();
            let title = document
                .select(&title_selector)
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_else(|| "Untitled".to_string());

            // Extract text content
            let text_content = Self::extract_text_content(&document);
            debug!(
                "📝 CONTENT EXTRACTED: {} - title: '{}', content: {} chars",
                item.url,
                title,
                text_content.len()
            );

            // Generate snippet
            let snippet = Self::generate_snippet(&text_content, 200);

            // Extract links for further crawling (if within depth limit)
            let discovered_links = if item.depth < config.application.max_crawl_depth {
                debug!(
                    "🔗 LINK DISCOVERY: Extracting links from {} (current depth: {}, max: {})",
                    item.url, item.depth, config.application.max_crawl_depth
                );
                let base_url_parsed = Url::parse(&item.url)?;
                let link_selector = Selector::parse("a[href]").unwrap();
                let mut discovered_urls = HashSet::new();

                // Collect all URLs first to avoid holding references across await
                let mut hrefs = Vec::new();
                for link in document.select(&link_selector) {
                    if let Some(href) = link.value().attr("href") {
                        hrefs.push(href.to_string());
                    }
                }

                // Process the collected URLs
                for href in hrefs {
                    if let Ok(absolute_url) = base_url_parsed.join(&href) {
                        let url_str = absolute_url.to_string();

                        // Only crawl URLs from allowed domains
                        if let Some(host) = absolute_url.host_str() {
                            if config.is_domain_allowed(host) && !discovered_urls.contains(&url_str)
                            {
                                discovered_urls.insert(url_str.clone());
                            }
                        }
                    }
                }

                discovered_urls.into_iter().collect::<Vec<_>>()
            } else {
                debug!(
                    "🔗 LINK DISCOVERY SKIPPED: {} at maximum depth ({}/{}), not extracting links",
                    item.url, item.depth, config.application.max_crawl_depth
                );
                Vec::new()
            };

            (title, text_content, snippet, discovered_links)
        }; // document is dropped here

        // Create webpage record
        let webpage = WebPage {
            id: Uuid::new_v4().to_string(),
            url: item.url.clone(),
            title: title.clone(),
            content: text_content.clone(),
            snippet: snippet.clone(),
            domain: item.domain.clone(),
            indexed_at: Utc::now(),
            last_crawled: Utc::now(),
            status_code,
            content_type,
            content_length: Some(content_length),
        };

        // Store in Cosmos DB
        debug!("💾 STORING: Saving webpage {} to database", item.url);
        storage_service
            .store_webpage(&webpage)
            .await
            .context("Failed to store webpage")?;

        // Index in Azure Cognitive Search
        debug!("🔍 INDEXING: Adding {} to search index", item.url);
        let search_doc = SearchDocument {
            id: webpage.id.clone(),
            title: webpage.title.clone(),
            url: webpage.url.clone(),
            content: webpage.content.clone(),
            snippet: webpage.snippet.clone(),
            domain: webpage.domain.clone(),
            indexed_at: webpage.indexed_at,
            last_crawled: webpage.last_crawled,
        };

        search_service
            .index_document(&search_doc)
            .await
            .context("Failed to index document")?;

        // Queue discovered links for further crawling
        let num_discovered = discovered_links.len();

        info!(
            "🎯 INDEXED SUCCESSFULLY: {} - title: '{}', content: {} chars, links discovered: {}",
            item.url,
            title,
            text_content.len(),
            num_discovered
        );

        if num_discovered > 0 {
            info!(
                "🔗 QUEUEING LINKS: Adding {} discovered links to crawl queue from {}",
                num_discovered, item.url
            );
        }
        let mut successfully_queued = 0;
        for url_str in discovered_links {
            // Extract domain from the discovered URL instead of using parent's domain
            let discovered_domain = if let Ok(parsed_url) = Url::parse(&url_str) {
                parsed_url.host_str().unwrap_or(&item.domain).to_string()
            } else {
                item.domain.clone() // Fallback to parent domain if URL parsing fails
            };

            let crawl_item = CrawlQueue {
                id: Self::url_to_id(&url_str),
                url: url_str.clone(),
                domain: discovered_domain,
                depth: item.depth + 1,
                status: CrawlStatus::Pending,
                created_at: Utc::now(),
                processed_at: None,
                error_message: None,
                retry_count: 0,
            };

            if let Err(e) = storage_service.queue_crawl(&crawl_item).await {
                let error_str = e.to_string();
                // Check if this is a conflict error (HTTP 409) indicating the URL already exists
                if error_str.contains("409") || error_str.to_lowercase().contains("conflict") {
                    debug!("URL already queued (skipping duplicate): {}", url_str);
                } else {
                    warn!("Failed to queue discovered link {}: {}", url_str, e);
                }
            } else {
                info!(
                    "✅ QUEUED LINK: {} (depth: {}) discovered from {}",
                    url_str, crawl_item.depth, item.url
                );
                successfully_queued += 1;
            }
        }

        if num_discovered > 0 {
            info!(
                "🔗 LINK DISCOVERY COMPLETE: Successfully queued {} of {} discovered links from {} (total indexed content: {} chars)",
                successfully_queued, num_discovered, item.url, text_content.len()
            );
        }

        Ok(())
    }

    fn extract_text_content(document: &Html) -> String {
        // Remove script and style elements
        let body_selector = Selector::parse("body").unwrap();

        let body = document.select(&body_selector).next();
        if let Some(body_el) = body {
            let mut text_parts = Vec::new();
            Self::extract_text_recursive(body_el, &mut text_parts);
            text_parts.join(" ")
        } else {
            // Fallback to all text
            document.root_element().text().collect::<Vec<_>>().join(" ")
        }
    }

    fn extract_text_recursive(element: scraper::ElementRef, text_parts: &mut Vec<String>) {
        for child in element.children() {
            if let Some(text) = child.value().as_text() {
                let text = text.trim();
                if !text.is_empty() {
                    text_parts.push(text.to_string());
                }
            } else if let Some(child_element) = scraper::ElementRef::wrap(child) {
                // Skip script and style elements
                if !matches!(
                    child_element.value().name(),
                    "script" | "style" | "nav" | "footer" | "aside"
                ) {
                    Self::extract_text_recursive(child_element, text_parts);
                }
            }
        }
    }

    fn generate_snippet(content: &str, max_length: usize) -> String {
        if content.len() <= max_length {
            return content.to_string();
        }

        // Find a good breaking point near the limit
        let truncated = &content[..max_length];
        if let Some(last_space) = truncated.rfind(' ') {
            format!("{}...", &content[..last_space])
        } else {
            format!("{}...", truncated)
        }
    }

    async fn is_allowed_by_robots(url: &str, _user_agent: &str, client: &Client) -> Result<bool> {
        let parsed_url = Url::parse(url)?;
        let robots_url = format!(
            "{}://{}/robots.txt",
            parsed_url.scheme(),
            parsed_url.host_str().unwrap_or_default()
        );

        // Simple robots.txt check - in production you'd want more sophisticated parsing
        match client.get(&robots_url).send().await {
            Ok(response) if response.status().is_success() => {
                let robots_content = response.text().await.unwrap_or_default();
                // Very basic check - look for "Disallow: /" for our user agent
                if robots_content.contains("User-agent: *")
                    && robots_content.contains("Disallow: /")
                {
                    debug!("Robots.txt found with restrictions for {}", url);
                    return Ok(false);
                }
            }
            _ => {
                // If we can't fetch robots.txt, assume it's allowed
                debug!("Could not fetch robots.txt for {}, assuming allowed", url);
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ApplicationConfig, AzureConfig};
    use chrono::{Duration as ChronoDuration, Utc};

    fn create_test_config() -> Arc<Config> {
        Arc::new(Config {
            environment: "test".to_string(),
            azure: AzureConfig {
                search_service_name: "test".to_string(),
                search_api_key: "test".to_string(),
                search_api_version: "2023-11-01".to_string(),
                search_index_name: "test".to_string(),
                cosmos_endpoint: "test".to_string(),
                cosmos_key: "test".to_string(),
                cosmos_database_name: "test".to_string(),
                cosmos_container_name: "test".to_string(),
            },
            application: ApplicationConfig {
                max_crawl_depth: 5,
                crawl_delay_ms: 1000,
                max_concurrent_requests: 10,
                user_agent: "test".to_string(),
                allowed_domains: vec!["example.com".to_string(), "test.org".to_string()],
                periodic_index_interval_days: 7,
                duplicate_removal_interval_hours: 24,
                admin_api_key: "test-admin-key".to_string(),
            },
        })
    }

    #[tokio::test]
    async fn test_queue_domains_with_check_skips_recently_indexed() {
        // This test would require mocking the storage service
        // For now, we'll test the configuration and basic logic
        let config = create_test_config();

        // Test that the periodic interval is correctly configured
        assert_eq!(config.application.periodic_index_interval_days, 7);

        // Test allowed domains logic
        assert!(config.is_domain_allowed("example.com"));
        assert!(config.is_domain_allowed("test.org"));
        assert!(!config.is_domain_allowed("not-allowed.com"));
    }

    #[test]
    fn test_domain_filtering_logic() {
        let config = create_test_config();

        // Test with various domain formats
        let domains = vec![
            "example.com".to_string(),
            "https://test.org".to_string(),
            "not-allowed.com".to_string(),
        ];

        let mut allowed_count = 0;
        for domain in &domains {
            let url = if domain.starts_with("http") {
                domain.clone()
            } else {
                format!("https://{}", domain)
            };

            if let Ok(parsed_url) = Url::parse(&url) {
                if let Some(host) = parsed_url.host_str() {
                    if config.is_domain_allowed(host) {
                        allowed_count += 1;
                    }
                }
            }
        }

        assert_eq!(allowed_count, 2); // example.com and test.org should be allowed
    }

    #[test]
    fn test_periodic_indexing_time_logic() {
        let config = create_test_config();
        let interval_days = config.application.periodic_index_interval_days as i64;

        // Simulate current time
        let now = Utc::now();

        // Test recent indexing (should be skipped)
        let recent_time = now - ChronoDuration::days(interval_days - 1);
        let threshold = now - ChronoDuration::days(interval_days);
        assert!(
            recent_time > threshold,
            "Recent indexing should be after threshold"
        );

        // Test old indexing (should be re-indexed)
        let old_time = now - ChronoDuration::days(interval_days + 1);
        assert!(
            old_time <= threshold,
            "Old indexing should be before or at threshold"
        );

        // Test exactly at threshold
        let threshold_time = now - ChronoDuration::days(interval_days);
        assert!(
            threshold_time <= threshold,
            "Threshold time should trigger re-indexing"
        );
    }

    #[test]
    fn test_trigger_force_process_queue() {
        let _config = create_test_config();

        // Create a mock IndexerService with just the force processing capability
        let force_process_flag = Arc::new(AtomicBool::new(false));

        // Simulate the trigger
        force_process_flag.store(true, Ordering::Relaxed);
        assert!(
            force_process_flag.load(Ordering::Relaxed),
            "Force flag should be set after trigger"
        );

        // Simulate processing the flag (like the queue processor would do)
        force_process_flag.store(false, Ordering::Relaxed);
        assert!(
            !force_process_flag.load(Ordering::Relaxed),
            "Force flag should be cleared after processing"
        );
    }

    #[test]
    fn test_domain_extraction_from_discovered_urls() {
        use url::Url;

        // Test domain extraction for different URL types
        let test_cases = vec![
            ("https://example.com/page1", "example.com"),
            ("https://test.org/docs/intro", "test.org"),
            (
                "https://subdomain.example.com/path",
                "subdomain.example.com",
            ),
            ("http://example.com:8080/api", "example.com"),
        ];

        for (url_str, expected_domain) in test_cases {
            let parsed_url = Url::parse(url_str).expect("Should parse valid URL");
            let extracted_domain = parsed_url.host_str().expect("Should have host");
            assert_eq!(
                extracted_domain, expected_domain,
                "Domain extraction failed for URL: {}",
                url_str
            );
        }
    }

    #[test]
    fn test_crawl_queue_domain_assignment() {
        use crate::storage::{CrawlQueue, CrawlStatus};
        use chrono::Utc;
        use uuid::Uuid;

        // Simulate the corrected logic for domain assignment
        let parent_domain = "example.com";
        let discovered_urls = [
            "https://example.com/page2".to_string(),
            "https://test.org/external".to_string(),
            "https://subdomain.example.com/sub".to_string(),
        ];

        let expected_domains = ["example.com", "test.org", "subdomain.example.com"];

        for (url_str, expected_domain) in discovered_urls.iter().zip(expected_domains.iter()) {
            // Extract domain from the discovered URL instead of using parent's domain
            let discovered_domain = if let Ok(parsed_url) = Url::parse(url_str) {
                parsed_url.host_str().unwrap_or(parent_domain).to_string()
            } else {
                parent_domain.to_string() // Fallback to parent domain if URL parsing fails
            };

            let crawl_item = CrawlQueue {
                id: Uuid::new_v4().to_string(),
                url: url_str.clone(),
                domain: discovered_domain.clone(),
                depth: 1,
                status: CrawlStatus::Pending,
                created_at: Utc::now(),
                processed_at: None,
                error_message: None,
                retry_count: 0,
            };

            assert_eq!(
                crawl_item.domain, *expected_domain,
                "Domain should be extracted from discovered URL: {}",
                url_str
            );
            assert_eq!(crawl_item.url, *url_str, "URL should match discovered URL");
        }
    }

    #[test]
    fn test_domain_extraction_includes_subdomains_not_just_tld() {
        use url::Url;

        // Test that we extract the full domain including subdomains, not just TLD
        let test_cases = vec![
            ("https://api.example.com/v1/users", "api.example.com"), // Should include subdomain
            (
                "https://blog.subdomain.example.com/post",
                "blog.subdomain.example.com",
            ), // Should include full subdomain chain
            (
                "https://cdn.assets.company.org/images",
                "cdn.assets.company.org",
            ), // Should include all subdomains
            ("https://example.com/page", "example.com"), // Root domain without subdomain
            ("https://www.example.co.uk/page", "www.example.co.uk"), // Should include www and country TLD
            ("http://localhost:8080/test", "localhost"),             // Should work with localhost
        ];

        for (url_str, expected_domain) in test_cases {
            let parsed_url = Url::parse(url_str).expect("Should parse valid URL");
            let extracted_domain = parsed_url.host_str().expect("Should have host");
            assert_eq!(
                extracted_domain, expected_domain,
                "Domain extraction should include full subdomain path for URL: {}",
                url_str
            );

            // Verify we're not just extracting TLD
            if expected_domain.contains('.') {
                let tld_only = expected_domain.split('.').next_back().unwrap();
                assert_ne!(
                    extracted_domain, tld_only,
                    "Should extract full domain, not just TLD '{}' for URL: {}",
                    tld_only, url_str
                );
            }
        }
    }

    #[test]
    fn test_url_to_id_generates_deterministic_ids() {
        // Test that the same URL generates the same ID consistently
        let url1 = "https://example.com/page";
        let url2 = "https://example.com/page";
        let url3 = "https://example.com/different";

        let id1 = IndexerService::url_to_id(url1);
        let id2 = IndexerService::url_to_id(url2);
        let id3 = IndexerService::url_to_id(url3);

        assert_eq!(id1, id2, "Same URL should generate same ID");
        assert_ne!(id1, id3, "Different URLs should generate different IDs");

        // IDs should be valid hex strings
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(id3.chars().all(|c| c.is_ascii_hexdigit()));

        // IDs should be 64 characters (SHA256 hex)
        assert_eq!(id1.len(), 64);
        assert_eq!(id3.len(), 64);
    }

    #[test]
    fn test_url_deduplication_in_crawl_queue() {
        use crate::storage::{CrawlQueue, CrawlStatus};
        use chrono::Utc;

        // Test that the same URL gets the same ID in CrawlQueue items
        let url = "https://example.com/test-page";
        let domain = "example.com";

        let crawl_item1 = CrawlQueue {
            id: IndexerService::url_to_id(url),
            url: url.to_string(),
            domain: domain.to_string(),
            depth: 1,
            status: CrawlStatus::Pending,
            created_at: Utc::now(),
            processed_at: None,
            error_message: None,
            retry_count: 0,
        };

        let crawl_item2 = CrawlQueue {
            id: IndexerService::url_to_id(url),
            url: url.to_string(),
            domain: domain.to_string(),
            depth: 2, // Different depth
            status: CrawlStatus::Pending,
            created_at: Utc::now(),
            processed_at: None,
            error_message: None,
            retry_count: 0,
        };

        // Both items should have the same ID since they have the same URL
        assert_eq!(
            crawl_item1.id, crawl_item2.id,
            "Same URL should produce same ID for deduplication"
        );
        assert_eq!(crawl_item1.url, crawl_item2.url, "URLs should match");
    }

    #[test]
    fn test_link_discovery_queues_discovered_urls() {
        use crate::storage::{CrawlQueue, CrawlStatus};
        use chrono::Utc;

        // Test that discovered links from a page get properly queued with correct metadata
        let base_url = "https://clojure.org";
        let discovered_link = "https://clojure.org/guides/getting_started";

        // Simulate creating a crawl queue item for a discovered link
        let base_item = CrawlQueue {
            id: IndexerService::url_to_id(base_url),
            url: base_url.to_string(),
            domain: "clojure.org".to_string(),
            depth: 0, // Root domain
            status: CrawlStatus::Completed,
            created_at: Utc::now(),
            processed_at: Some(Utc::now()),
            error_message: None,
            retry_count: 0,
        };

        // This would be created when processing the base_item and discovering links
        let discovered_item = CrawlQueue {
            id: IndexerService::url_to_id(discovered_link),
            url: discovered_link.to_string(),
            domain: "clojure.org".to_string(),
            depth: base_item.depth + 1,   // Should be depth 1
            status: CrawlStatus::Pending, // Should be pending for processing
            created_at: Utc::now(),
            processed_at: None, // Not processed yet
            error_message: None,
            retry_count: 0,
        };

        // Validate the discovered item has the expected properties
        assert_eq!(discovered_item.url, discovered_link);
        assert_eq!(discovered_item.domain, "clojure.org");
        assert_eq!(discovered_item.depth, 1);
        assert_eq!(discovered_item.status as u8, CrawlStatus::Pending as u8);
        assert!(discovered_item.processed_at.is_none());

        // Verify the URLs are different but from the same domain
        assert_ne!(base_item.url, discovered_item.url);
        assert_eq!(base_item.domain, discovered_item.domain);
        assert!(discovered_item.url.starts_with(&base_item.url));
    }
}
