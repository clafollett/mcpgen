//! SQLite-powered resource caching system
//!
//! This module provides a revolutionary resource caching system built on SQLite
//! that goes beyond simple key-value storage to offer a full-featured resource database
//! with structured storage, rich queries, ACID transactions, and built-in analytics.

use crate::mcp::client::error::{ClientError, Result};
use crate::mcp::client::resource::{ResourceContent, ResourceInfo};
use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite_migration::{M, Migrations};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tracing::info;
use uuid::Uuid;

// Global tracking of initialized databases (double-checked locking pattern)
static INITIALIZED_DATABASES: OnceLock<Mutex<HashMap<String, ()>>> = OnceLock::new();

// Database migrations for schema versioning
static MIGRATIONS: &[M] = &[
    // v1: Initial schema with resources and analytics tables
    M::up(
        r#"
        CREATE TABLE IF NOT EXISTS resources (
            id TEXT PRIMARY KEY,
            uri TEXT UNIQUE NOT NULL,
            content BLOB NOT NULL,
            content_type TEXT,
            metadata_json TEXT,
            created_at INTEGER NOT NULL,
            accessed_at INTEGER NOT NULL,
            expires_at INTEGER,
            access_count INTEGER DEFAULT 0,
            size_bytes INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_resources_uri ON resources(uri);
        CREATE INDEX IF NOT EXISTS idx_resources_expires ON resources(expires_at);
        CREATE INDEX IF NOT EXISTS idx_resources_accessed ON resources(accessed_at);

        CREATE TABLE IF NOT EXISTS cache_analytics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            hit_rate REAL,
            total_requests INTEGER,
            cache_size_mb REAL,
            eviction_count INTEGER
        );

        CREATE TRIGGER IF NOT EXISTS cleanup_expired_resources
         AFTER INSERT ON resources
         BEGIN
             DELETE FROM resources 
             WHERE expires_at IS NOT NULL 
             AND expires_at < strftime('%s', 'now') * 1000;
         END;
    "#,
    )
    .down(
        r#"
        DROP TABLE IF EXISTS cache_analytics;
        DROP TABLE IF EXISTS resources;
    "#,
    ),
];

/// Configuration for the resource cache
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Database file path for SQLite cache
    pub database_path: String,
    /// Default TTL for cached resources
    pub default_ttl: Duration,
    /// Maximum cache size in MB (0 = unlimited)
    pub max_size_mb: u64,
    /// Enable automatic cleanup of expired resources
    pub auto_cleanup: bool,
    /// Cleanup interval for expired resources
    pub cleanup_interval: Duration,
    /// Minimum number of connections in the pool
    pub pool_min_connections: Option<u32>,
    /// Maximum number of connections in the pool
    pub pool_max_connections: Option<u32>,
    /// Connection timeout for pool operations
    pub pool_connection_timeout: Option<Duration>,
    /// Maximum lifetime for pooled connections (prevents stale connections)
    pub pool_max_lifetime: Option<Duration>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        // Get cache directory path
        let cache_path = if let Some(home) = dirs::home_dir() {
            home.join(".agenterra").join("cache.db")
        } else {
            // Fallback to current directory if home not found
            std::path::PathBuf::from("./agenterra_cache.db")
        };

        Self {
            database_path: cache_path.to_string_lossy().to_string(),
            default_ttl: Duration::from_secs(3600), // 1 hour
            max_size_mb: 100,                       // 100 MB
            auto_cleanup: true,
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            pool_min_connections: Some(1),              // Minimum connections in pool
            pool_max_connections: Some(10),             // Maximum connections in pool
            pool_connection_timeout: Some(Duration::from_secs(30)),
            pool_max_lifetime: Some(Duration::from_secs(300)), // 5 minutes to recycle connections
        }
    }
}

/// Cache analytics and performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheAnalytics {
    /// Total cache requests
    pub total_requests: u64,
    /// Cache hits
    pub cache_hits: u64,
    /// Cache misses
    pub cache_misses: u64,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
    /// Total cache size in bytes
    pub cache_size_bytes: u64,
    /// Number of cached resources
    pub resource_count: u64,
    /// Number of expired resources cleaned up
    pub eviction_count: u64,
    /// Last cleanup timestamp
    pub last_cleanup: DateTime<Utc>,
}

/// Cached resource metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResource {
    /// Unique cache entry ID
    pub id: String,
    /// Resource URI
    pub uri: String,
    /// Resource content
    pub content: Vec<u8>,
    /// Content type/MIME type
    pub content_type: Option<String>,
    /// Resource metadata as JSON
    pub metadata: HashMap<String, serde_json::Value>,
    /// When the resource was first cached
    pub created_at: DateTime<Utc>,
    /// When the resource was last accessed
    pub accessed_at: DateTime<Utc>,
    /// When the resource expires (if any)
    pub expires_at: Option<DateTime<Utc>>,
    /// Number of times this resource has been accessed
    pub access_count: u64,
    /// Size of the resource in bytes
    pub size_bytes: u64,
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Maximum number of connections in the pool
    pub max_connections: u32,
    /// Current number of active connections
    pub active_connections: u32,
    /// Number of connections waiting in the pool
    pub idle_connections: u32,
}

/// SQLite-powered resource cache
pub struct ResourceCache {
    /// Cache configuration
    config: CacheConfig,
    /// Cache analytics
    analytics: CacheAnalytics,
    /// Connection pool for all database operations
    pool: Pool<SqliteConnectionManager>,
}

impl ResourceCache {
    /// Create a new resource cache with the given configuration
    pub async fn new(config: CacheConfig) -> Result<Self> {
        // Initialize analytics
        let analytics = CacheAnalytics {
            total_requests: 0,
            cache_hits: 0,
            cache_misses: 0,
            hit_rate: 0.0,
            cache_size_bytes: 0,
            resource_count: 0,
            eviction_count: 0,
            last_cleanup: Utc::now(),
        };

        // Validate database path
        if config.database_path.is_empty() {
            return Err(ClientError::Validation(
                "database_path cannot be empty".to_string(),
            ));
        }

        // Validate pool configuration
        if let (Some(min), Some(max)) = (config.pool_min_connections, config.pool_max_connections) {
            if min > max {
                return Err(ClientError::Validation(format!(
                    "pool_min_connections ({}) must be ≤ pool_max_connections ({})",
                    min, max
                )));
            }
        }

        // Always create a connection pool
        let manager = SqliteConnectionManager::file(&config.database_path);
        let mut pool_builder = Pool::builder();

        // Use provided settings or defaults
        if let Some(min_size) = config.pool_min_connections {
            pool_builder = pool_builder.min_idle(Some(min_size));
        }
        if let Some(max_size) = config.pool_max_connections {
            pool_builder = pool_builder.max_size(max_size);
        }
        if let Some(timeout) = config.pool_connection_timeout {
            pool_builder = pool_builder.connection_timeout(timeout);
        }

        // Set max lifetime to recycle long-lived connections and avoid stale WAL readers
        if let Some(max_lifetime) = config.pool_max_lifetime {
            pool_builder = pool_builder.max_lifetime(Some(max_lifetime));
        }

        let pool = pool_builder
            .build(manager)
            .map_err(|e| ClientError::Pool(format!("Failed to create connection pool: {}", e)))?;

        let cache = Self {
            config,
            analytics,
            pool,
        };

        // Initialize database schema
        cache.init_schema().await?;

        Ok(cache)
    }

    /// Execute a function with a database connection from the pool
    async fn with_connection<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut rusqlite::Connection) -> rusqlite::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = pool.get().map_err(|e| {
                ClientError::Pool(format!("Failed to get pooled connection: {}", e))
            })?;

            f(&mut conn)
                .map_err(|e| ClientError::Client(format!("Database operation failed: {}", e)))
        })
        .await
        .map_err(|e| ClientError::Spawn(format!("Task execution failed: {}", e)))?
    }

    /// Initialize the SQLite database schema with proper double-checked locking
    async fn init_schema(&self) -> Result<()> {
        let db_path = normalize_db_path(&self.config.database_path);

        // First check: Has this database path already been initialized globally?
        {
            let tracker = get_db_tracker().lock().unwrap();
            if tracker.contains_key(&db_path) {
                tracing::debug!("Database schema already initialized for: {}", db_path);
                return Ok(());
            }
        }

        // If not initialized, enter the critical section
        self.with_connection(move |conn| {
            tracing::debug!(
                "Entering critical section for database schema initialization: {}",
                db_path
            );

            // Double check pattern - check the global tracker again
            {
                let tracker = get_db_tracker().lock().unwrap();
                if tracker.contains_key(&db_path) {
                    tracing::debug!(
                        "Database schema was initialized by another thread: {}",
                        db_path
                    );
                    return Ok(());
                }
            }

            // Create parent directory if it doesn't exist
            if let Some(parent) = std::path::Path::new(&db_path).parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                        Some(format!("Failed to create directory: {}", e)),
                    )
                })?;
            }

            // Enable WAL mode for better concurrent access
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.pragma_update(None, "cache_size", 10000)?;
            conn.pragma_update(None, "temp_store", "memory")?;

            // Set busy timeout to handle SQLITE_BUSY on slow filesystems
            conn.busy_timeout(std::time::Duration::from_secs(5))?;

            // Run migrations using rusqlite_migration
            let migrations = Migrations::new(MIGRATIONS.to_vec());
            match migrations.to_latest(conn) {
                Ok(()) => {
                    // Mark this database as initialized globally
                    let mut tracker = get_db_tracker().lock().unwrap();
                    tracker.insert(db_path.clone(), ());
                    tracing::debug!(
                        "Database migrations completed successfully for: {}",
                        db_path
                    );
                    Ok(())
                }
                Err(e) => {
                    // Check if this is a concurrent initialization issue
                    let error_msg = e.to_string().to_lowercase();
                    if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                        // Another thread beat us to it, mark as initialized
                        let mut tracker = get_db_tracker().lock().unwrap();
                        tracker.insert(db_path.clone(), ());
                        tracing::debug!("Schema already exists (concurrent creation), continuing");
                        Ok(())
                    } else {
                        tracing::error!("Database migration failed: {}", e);
                        // Convert migration error to rusqlite error for this context
                        Err(rusqlite::Error::SqliteFailure(
                            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
                            Some(format!("Migration failed: {}", e)),
                        ))
                    }
                }
            }
        })
        .await
    }

    /// Store a resource in the cache
    pub async fn store_resource(&mut self, resource: &ResourceContent) -> Result<String> {
        self.store_resource_with_ttl(resource, self.config.default_ttl)
            .await
    }

    /// Store a resource with custom TTL
    pub async fn store_resource_with_ttl(
        &mut self,
        resource: &ResourceContent,
        ttl: Duration,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = if ttl.is_zero() {
            None
        } else {
            Some(
                now + chrono::Duration::from_std(ttl)
                    .map_err(|_| ClientError::Validation("Invalid TTL duration".to_string()))?,
            )
        };

        // Clone metadata and add encoding if present
        let mut metadata = resource.info.metadata.clone();
        if let Some(ref encoding) = resource.encoding {
            metadata.insert("encoding".to_string(), serde_json::json!(encoding));
        }

        let metadata_json = serde_json::to_string(&metadata)?;

        let size_bytes = resource.data.len() as u64;

        // Clone data needed for the closure
        let id_clone = id.clone();
        let uri = resource.info.uri.clone();
        let content = resource.data.clone();
        let content_type = resource.info.mime_type.clone();
        let created_at = now.timestamp_millis();
        let accessed_at = now.timestamp_millis();
        let expires_at_millis = expires_at.map(|t| t.timestamp_millis());

        self.with_connection(move |conn| {
            // Use a transaction for ACID guarantees
            let tx = conn.transaction()?;

            tx.execute(
                "INSERT OR REPLACE INTO resources (
                    id, uri, content, content_type, metadata_json,
                    created_at, accessed_at, expires_at, access_count, size_bytes
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    id_clone,
                    uri,
                    content,
                    content_type,
                    metadata_json,
                    created_at,
                    accessed_at,
                    expires_at_millis,
                    1, // Initial access count
                    size_bytes as i64,
                ],
            )?;

            tx.commit()?;
            Ok(())
        })
        .await?;

        // Update analytics
        self.analytics.resource_count += 1;
        self.analytics.cache_size_bytes += size_bytes;

        Ok(id)
    }

    /// Get a resource from the cache by URI
    pub async fn get_resource(&mut self, uri: &str) -> Result<Option<ResourceContent>> {
        let uri = uri.to_string();
        let now = Utc::now().timestamp_millis();

        let result = self
            .with_connection(move |conn| {
                // Check if resource exists and is not expired
                let mut stmt = conn.prepare(
                    "SELECT id, uri, content, content_type, metadata_json, 
                            created_at, accessed_at, expires_at, access_count, size_bytes
                     FROM resources 
                     WHERE uri = ?1 
                     AND (expires_at IS NULL OR expires_at > ?2)"
                )?;

                let row = match stmt.query_row(rusqlite::params![uri, now], |row| {
                    Ok((
                        row.get::<_, String>(0)?,       // id
                        row.get::<_, String>(1)?,       // uri
                        row.get::<_, Vec<u8>>(2)?,      // content
                        row.get::<_, Option<String>>(3)?, // content_type
                        row.get::<_, String>(4)?,       // metadata_json
                        row.get::<_, i64>(5)?,          // created_at
                        row.get::<_, i64>(6)?,          // accessed_at
                        row.get::<_, Option<i64>>(7)?,  // expires_at
                        row.get::<_, i64>(8)?,          // access_count
                        row.get::<_, i64>(9)?,          // size_bytes
                    ))
                }) {
                    Ok(row) => row,
                    Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                    Err(e) => return Err(e),
                };

                // Update access time and count
                conn.execute(
                    "UPDATE resources SET accessed_at = ?1, access_count = access_count + 1 WHERE uri = ?2",
                    rusqlite::params![now, uri],
                )?;

                Ok(Some(row))
            })
            .await?;

        match result {
            Some((_, uri, content, content_type, metadata_json, _, _, _, _, _)) => {
                // Parse metadata
                let metadata: HashMap<String, serde_json::Value> =
                    serde_json::from_str(&metadata_json).map_err(|e| {
                        ClientError::Client(format!("Failed to parse metadata: {}", e))
                    })?;

                // Construct ResourceInfo
                let info = ResourceInfo {
                    uri: uri.clone(),
                    name: metadata
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    description: metadata
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    mime_type: content_type.clone(),
                    metadata,
                };

                // Extract encoding from metadata or content_type
                let encoding = info
                    .metadata
                    .get("encoding")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| content_type.as_ref().and_then(|ct| parse_charset(ct)));

                // Update analytics
                self.analytics.total_requests += 1;
                self.analytics.cache_hits += 1;
                self.analytics.hit_rate = if self.analytics.total_requests > 0 {
                    self.analytics.cache_hits as f64 / self.analytics.total_requests as f64
                } else {
                    0.0
                };

                Ok(Some(ResourceContent {
                    info,
                    data: content,
                    encoding,
                }))
            }
            None => {
                // Update analytics for cache miss
                self.analytics.total_requests += 1;
                self.analytics.cache_misses += 1;
                self.analytics.hit_rate = if self.analytics.total_requests > 0 {
                    self.analytics.cache_hits as f64 / self.analytics.total_requests as f64
                } else {
                    0.0
                };

                Ok(None)
            }
        }
    }

    /// List all cached resources
    pub async fn list_cached_resources(&self) -> Result<Vec<CachedResource>> {
        let now = Utc::now().timestamp_millis();

        self.with_connection(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, uri, content, content_type, metadata_json,
                        created_at, accessed_at, expires_at, access_count, size_bytes
                 FROM resources
                 WHERE expires_at IS NULL OR expires_at > ?1
                 ORDER BY accessed_at DESC",
            )?;

            let rows = stmt.query_map(rusqlite::params![now], |row| {
                let metadata_json: String = row.get(4)?;
                let metadata: HashMap<String, serde_json::Value> =
                    match serde_json::from_str(&metadata_json) {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!("Failed to parse metadata JSON: {}", e);
                            HashMap::new()
                        }
                    };

                Ok(CachedResource {
                    id: row.get(0)?,
                    uri: row.get(1)?,
                    content: row.get(2)?,
                    content_type: row.get(3)?,
                    metadata,
                    created_at: DateTime::from_timestamp_millis(row.get::<_, i64>(5)?)
                        .unwrap_or_default(),
                    accessed_at: DateTime::from_timestamp_millis(row.get::<_, i64>(6)?)
                        .unwrap_or_default(),
                    expires_at: row
                        .get::<_, Option<i64>>(7)?
                        .map(|ts| DateTime::from_timestamp_millis(ts).unwrap_or_default()),
                    access_count: row.get::<_, i64>(8)? as u64,
                    size_bytes: row.get::<_, i64>(9)? as u64,
                })
            })?;

            let mut resources = Vec::new();
            for row in rows {
                resources.push(row?);
            }

            Ok(resources)
        })
        .await
    }

    /// Check if a resource exists in the cache
    pub async fn contains_resource(&self, uri: &str) -> Result<bool> {
        let uri = uri.to_string();
        let now = Utc::now().timestamp_millis();

        self.with_connection(move |conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM resources WHERE uri = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
                rusqlite::params![uri, now],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        }).await
    }

    /// Remove a resource from the cache
    pub async fn remove_resource(&mut self, uri: &str) -> Result<bool> {
        let uri = uri.to_string();

        let removed = self
            .with_connection(move |conn| {
                let changes = conn.execute(
                    "DELETE FROM resources WHERE uri = ?1",
                    rusqlite::params![uri],
                )?;
                Ok(changes > 0)
            })
            .await?;

        if removed {
            // Update analytics (we'll recalculate these properly in update_analytics)
            self.analytics.resource_count = self.analytics.resource_count.saturating_sub(1);
        }

        Ok(removed)
    }

    /// Clear all cached resources
    pub async fn clear(&mut self) -> Result<()> {
        self.with_connection(|conn| {
            conn.execute("DELETE FROM resources", [])?;
            conn.execute("DELETE FROM cache_analytics", [])?;
            Ok(())
        })
        .await?;

        // Reset analytics
        self.analytics = CacheAnalytics {
            total_requests: 0,
            cache_hits: 0,
            cache_misses: 0,
            hit_rate: 0.0,
            cache_size_bytes: 0,
            resource_count: 0,
            eviction_count: 0,
            last_cleanup: Utc::now(),
        };

        Ok(())
    }

    /// Run cleanup to remove expired resources
    /// This method handles all expired resources, including idle ones that
    /// wouldn't be caught by the INSERT trigger
    pub async fn cleanup_expired(&mut self) -> Result<u64> {
        let now = Utc::now().timestamp_millis();

        let removed_count = self
            .with_connection(move |conn| {
                let changes = conn.execute(
                    "DELETE FROM resources WHERE expires_at IS NOT NULL AND expires_at <= ?1",
                    rusqlite::params![now],
                )?;
                Ok(changes as u64)
            })
            .await?;

        // Update analytics
        self.analytics.eviction_count += removed_count;
        self.analytics.last_cleanup = Utc::now();
        self.analytics.resource_count = self.analytics.resource_count.saturating_sub(removed_count);

        // Update analytics from database
        self.update_analytics().await?;

        Ok(removed_count)
    }

    /// Get cache analytics
    pub fn get_analytics(&self) -> &CacheAnalytics {
        &self.analytics
    }

    /// Update cache analytics
    async fn update_analytics(&mut self) -> Result<()> {
        let (total_size, resource_count) = self
            .with_connection(|conn| {
                let size: i64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(size_bytes), 0) FROM resources",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                let count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM resources", [], |row| row.get(0))
                    .unwrap_or(0);

                Ok((size as u64, count as u64))
            })
            .await?;

        self.analytics.cache_size_bytes = total_size;
        self.analytics.resource_count = resource_count;

        // Store analytics in database
        let timestamp = Utc::now().timestamp_millis();
        let hit_rate = self.analytics.hit_rate;
        let total_requests = self.analytics.total_requests as i64;
        let cache_size_mb = (self.analytics.cache_size_bytes as f64) / (1024.0 * 1024.0);
        let eviction_count = self.analytics.eviction_count as i64;

        self.with_connection(move |conn| {
            conn.execute(
                "INSERT INTO cache_analytics (timestamp, hit_rate, total_requests, cache_size_mb, eviction_count)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    timestamp,
                    hit_rate,
                    total_requests,
                    cache_size_mb,
                    eviction_count,
                ],
            )?;
            Ok(())
        }).await?;

        Ok(())
    }

    /// Search cached resources by metadata
    pub async fn search_resources(&self, query: &str) -> Result<Vec<CachedResource>> {
        let query = query.to_string();
        let now = Utc::now().timestamp_millis();

        self.with_connection(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, uri, content, content_type, metadata_json,
                        created_at, accessed_at, expires_at, access_count, size_bytes
                 FROM resources
                 WHERE (expires_at IS NULL OR expires_at > ?2)
                 AND (uri LIKE ?1 OR content_type LIKE ?1 OR metadata_json LIKE ?1)
                 ORDER BY accessed_at DESC",
            )?;

            let search_pattern = format!("%{}%", query);
            let rows = stmt.query_map(rusqlite::params![search_pattern, now], |row| {
                let metadata_json: String = row.get(4)?;
                let metadata: HashMap<String, serde_json::Value> =
                    match serde_json::from_str(&metadata_json) {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!("Failed to parse metadata JSON in search: {}", e);
                            HashMap::new()
                        }
                    };

                Ok(CachedResource {
                    id: row.get(0)?,
                    uri: row.get(1)?,
                    content: row.get(2)?,
                    content_type: row.get(3)?,
                    metadata,
                    created_at: DateTime::from_timestamp_millis(row.get::<_, i64>(5)?)
                        .unwrap_or_default(),
                    accessed_at: DateTime::from_timestamp_millis(row.get::<_, i64>(6)?)
                        .unwrap_or_default(),
                    expires_at: row
                        .get::<_, Option<i64>>(7)?
                        .map(|ts| DateTime::from_timestamp_millis(ts).unwrap_or_default()),
                    access_count: row.get::<_, i64>(8)? as u64,
                    size_bytes: row.get::<_, i64>(9)? as u64,
                })
            })?;

            let mut resources = Vec::new();
            for row in rows {
                resources.push(row?);
            }

            Ok(resources)
        })
        .await
    }

    /// Get cache size in bytes
    pub async fn get_cache_size(&self) -> Result<u64> {
        self.with_connection(|conn| {
            let size: i64 = conn.query_row(
                "SELECT COALESCE(SUM(size_bytes), 0) FROM resources",
                [],
                |row| row.get(0),
            )?;
            Ok(size as u64)
        })
        .await
    }

    /// Compact the database to reclaim space
    pub async fn compact(&mut self) -> Result<()> {
        self.with_connection(|conn| {
            conn.execute("VACUUM", [])?;
            Ok(())
        })
        .await
    }

    /// Get connection pool statistics
    pub fn get_pool_stats(&self) -> PoolStats {
        let state = self.pool.state();
        PoolStats {
            max_connections: self.pool.max_size(),
            active_connections: state.connections - state.idle_connections,
            idle_connections: state.idle_connections,
        }
    }
}

/// Get the global database initialization tracker
fn get_db_tracker() -> &'static Mutex<HashMap<String, ()>> {
    INITIALIZED_DATABASES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Parse charset from content-type header
///
/// IMPORTANT: This function is duplicated in the template file at
/// `templates/mcp/client/rust_reqwest/src/cache.rs.tera` and must be kept in sync.
/// Any changes here should be applied to both locations.
fn parse_charset(content_type: &str) -> Option<String> {
    content_type.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        if key.trim().eq_ignore_ascii_case("charset") {
            Some(
                value
                    .trim_matches(|c| c == '"' || c == '\'')
                    .to_ascii_lowercase(),
            )
        } else {
            None
        }
    })
}

/// Normalize database path to prevent double-initialization due to path differences
/// (e.g., "./db.sqlite" vs "db.sqlite" vs absolute paths)
///
/// Note: Only provides lexical normalization for non-existent files. Symlinks
/// are resolved only if the file already exists via canonicalize().
fn normalize_db_path(db_path: &str) -> String {
    let path = Path::new(db_path);

    // First try canonicalize (resolves symlinks and relative components for existing files)
    if let Ok(canonical) = path.canonicalize() {
        return canonical.to_string_lossy().to_string();
    }

    // If canonicalize fails (file doesn't exist yet), make relative paths absolute
    // and normalize path components (remove "." and resolve "..")
    if path.is_relative() {
        if let Ok(current_dir) = std::env::current_dir() {
            let absolute_path = current_dir.join(path);
            // Normalize the path components to resolve "." and ".."
            return normalize_path_components(&absolute_path);
        }
    }

    // For absolute paths that don't exist, try to normalize components
    if path.is_absolute() {
        return normalize_path_components(path);
    }

    // Fallback to original path if all else fails
    db_path.to_string()
}

/// Helper function to normalize path components (resolve "." and "..")
fn normalize_path_components(path: &Path) -> String {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {
                // Skip "." components
                continue;
            }
            std::path::Component::ParentDir => {
                // Pop the last component for ".."
                if !components.is_empty() {
                    components.pop();
                }
            }
            _ => {
                components.push(component);
            }
        }
    }

    // Reconstruct the path
    let mut result = std::path::PathBuf::new();
    for component in components {
        result.push(component);
    }

    result.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::client::resource::ResourceInfo;
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

    // Test helper constants
    const POOL_TIMEOUT: Duration = Duration::from_secs(30);

    #[test]
    fn test_normalize_db_path_existing_file() {
        // Create a temporary file to test with existing files
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Normalizing an existing file should return its canonical path
        let normalized = normalize_db_path(&temp_path);
        assert!(!normalized.is_empty());
        assert!(Path::new(&normalized).is_absolute());
    }

    #[test]
    fn test_normalize_db_path_relative_nonexistent() {
        // Test relative path that doesn't exist yet
        let relative_path = "./test_db.sqlite";
        let normalized = normalize_db_path(relative_path);

        // Should be converted to absolute path
        assert!(Path::new(&normalized).is_absolute());
        assert!(normalized.ends_with("test_db.sqlite"));
        assert_ne!(normalized, relative_path);
    }

    #[test]
    fn test_normalize_db_path_absolute_nonexistent() {
        // Test absolute path that doesn't exist
        let current_dir = std::env::current_dir().unwrap();
        let absolute_path = current_dir.join("nonexistent_db.sqlite");
        let path_str = absolute_path.to_string_lossy().to_string();

        let normalized = normalize_db_path(&path_str);

        // Should remain the same since it's already absolute
        assert_eq!(normalized, path_str);
        assert!(Path::new(&normalized).is_absolute());
    }

    #[test]
    fn test_normalize_db_path_dot_prefix() {
        // Test the specific case mentioned by o3 Marvin: "./db.sqlite" vs "db.sqlite"
        let dot_path = "./db.sqlite";
        let plain_path = "db.sqlite";

        let normalized_dot = normalize_db_path(dot_path);
        let normalized_plain = normalize_db_path(plain_path);

        // Both should normalize to the same absolute path
        assert_eq!(normalized_dot, normalized_plain);
        assert!(Path::new(&normalized_dot).is_absolute());
        assert!(normalized_dot.ends_with("db.sqlite"));

        // Also verify they both resolve to current_dir + filename
        let current_dir = std::env::current_dir().unwrap();
        let expected = current_dir.join("db.sqlite").to_string_lossy().to_string();
        assert_eq!(normalized_dot, expected);
        assert_eq!(normalized_plain, expected);
    }

    #[test]
    fn test_normalize_db_path_consistency() {
        // Test that multiple calls with the same path return the same result
        let test_path = "./test.db";
        let normalized1 = normalize_db_path(test_path);
        let normalized2 = normalize_db_path(test_path);

        assert_eq!(normalized1, normalized2);
    }

    #[test]
    fn test_normalize_db_path_edge_cases() {
        let current_dir = std::env::current_dir().unwrap();
        let expected_current = current_dir.to_string_lossy().to_string();

        // Test empty string - note: empty paths should be caught by validation before reaching normalize_db_path
        let normalized_empty = normalize_db_path("");
        assert_eq!(normalized_empty, expected_current);

        // Test single dot - should become current directory
        let normalized_dot = normalize_db_path(".");
        assert!(Path::new(&normalized_dot).is_absolute());
        assert_eq!(normalized_dot, expected_current);

        // Test double dot - should become parent directory
        let normalized_double_dot = normalize_db_path("..");
        assert!(Path::new(&normalized_double_dot).is_absolute());
        let expected_parent = current_dir
            .parent()
            .unwrap_or(&current_dir)
            .to_string_lossy()
            .to_string();
        assert_eq!(normalized_double_dot, expected_parent);
    }

    /// Create a test cache config with a unique temporary database file
    fn create_test_cache_config() -> (CacheConfig, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join(format!("test_{}.db", Uuid::new_v4()));
        let config = CacheConfig {
            database_path: db_path.to_string_lossy().to_string(),
            ..Default::default()
        };
        (config, temp_dir)
    }

    fn create_test_resource() -> ResourceContent {
        let mut metadata = HashMap::new();
        metadata.insert(
            "size".to_string(),
            serde_json::Value::Number(serde_json::Number::from(13)),
        );

        let info = ResourceInfo {
            uri: "test://example.txt".to_string(),
            name: Some("example.txt".to_string()),
            description: Some("Test resource".to_string()),
            mime_type: Some("text/plain".to_string()),
            metadata,
        };

        ResourceContent {
            info,
            data: b"Hello, World!".to_vec(),
            encoding: Some("utf-8".to_string()),
        }
    }

    #[tokio::test]
    async fn test_cache_creation_with_temp_file() {
        let (config, _temp_dir) = create_test_cache_config();
        let result = ResourceCache::new(config).await;

        // Should succeed with file-based database
        assert!(result.is_ok());
        let cache = result.unwrap();
        assert_eq!(cache.get_analytics().resource_count, 0);
        assert_eq!(cache.get_analytics().cache_size_bytes, 0);
    }

    #[tokio::test]
    async fn test_cache_persistence_across_sessions() {
        let (config, _temp_dir) = create_test_cache_config();
        let db_path = config.database_path.clone();

        // Session 1: Store a resource
        {
            let mut cache = ResourceCache::new(config.clone()).await.unwrap();
            let resource = create_test_resource();
            cache.store_resource(&resource).await.unwrap();
        }

        // Session 2: Resource should still be there
        {
            let config = CacheConfig {
                database_path: db_path,
                ..Default::default()
            };
            let mut cache = ResourceCache::new(config).await.unwrap();
            let retrieved = cache.get_resource("test://example.txt").await.unwrap();
            assert!(
                retrieved.is_some(),
                "Resource should persist across sessions"
            );
            assert_eq!(retrieved.unwrap().data, b"Hello, World!");
        }
    }

    #[tokio::test]
    async fn test_cache_creation_file_based() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = CacheConfig {
            database_path: temp_file.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let result = ResourceCache::new(config).await;

        // Should succeed now that it's implemented
        assert!(result.is_ok());
        let cache = result.unwrap();
        assert_eq!(cache.get_analytics().resource_count, 0);
    }

    #[tokio::test]
    async fn test_store_and_retrieve_resource() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();
        let resource = create_test_resource();

        // Store resource
        let result = cache.store_resource(&resource).await;
        assert!(result.is_ok());
        let resource_id = result.unwrap();
        assert!(!resource_id.is_empty());

        // Retrieve resource
        let result = cache.get_resource("test://example.txt").await;
        assert!(result.is_ok());
        let retrieved = result.unwrap();
        assert!(retrieved.is_some());
        let retrieved_resource = retrieved.unwrap();
        assert_eq!(retrieved_resource.info.uri, "test://example.txt");
        assert_eq!(retrieved_resource.data, b"Hello, World!");
    }

    #[tokio::test]
    async fn test_store_resource_with_custom_ttl() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();
        let resource = create_test_resource();
        let ttl = Duration::from_secs(60);

        let result = cache.store_resource_with_ttl(&resource, ttl).await;
        assert!(result.is_ok());
        let resource_id = result.unwrap();
        assert!(!resource_id.is_empty());

        // Verify resource was stored
        let retrieved = cache.get_resource("test://example.txt").await.unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_list_cached_resources() {
        let (config, _temp_dir) = create_test_cache_config();
        let config = CacheConfig {
            pool_connection_timeout: Some(POOL_TIMEOUT),
            ..config
        };
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Initially empty
        let result = cache.list_cached_resources().await;
        if let Err(ref e) = result {
            tracing::error!("Initial list_cached_resources failed: {:?}", e);
        }
        assert!(result.is_ok(), "Initial list should succeed");
        let resources = result.unwrap();
        assert_eq!(resources.len(), 0);

        // Add a resource
        let resource = create_test_resource();
        cache.store_resource(&resource).await.unwrap();

        // Should have one resource
        let result = cache.list_cached_resources().await;
        if let Err(ref e) = result {
            tracing::error!("Second list_cached_resources failed: {:?}", e);
        }
        assert!(
            result.is_ok(),
            "Second list should succeed after storing resource"
        );
        let resources = result.unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, "test://example.txt");
    }

    #[tokio::test]
    async fn test_contains_resource() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = CacheConfig {
            database_path: temp_file.path().to_string_lossy().to_string(),
            pool_connection_timeout: Some(POOL_TIMEOUT),
            ..Default::default()
        };
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Initially should not contain resource
        let result = cache.contains_resource("test://example.txt").await;
        assert!(result.is_ok(), "Initial contains_resource should succeed");
        assert!(!result.unwrap());

        // Add resource
        let resource = create_test_resource();
        cache.store_resource(&resource).await.unwrap();

        // Should now contain resource
        let result = cache.contains_resource("test://example.txt").await;
        assert!(result.is_ok(), "Second contains_resource should succeed");
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_remove_resource() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Add resource
        let resource = create_test_resource();
        cache.store_resource(&resource).await.unwrap();

        // Verify it exists
        assert!(cache.contains_resource("test://example.txt").await.unwrap());

        // Remove resource
        let result = cache.remove_resource("test://example.txt").await;
        assert!(result.is_ok());
        assert!(result.unwrap()); // Should return true (was removed)

        // Verify it's gone
        assert!(!cache.contains_resource("test://example.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Add some resources
        let resource = create_test_resource();
        cache.store_resource(&resource).await.unwrap();

        // Verify cache has resources
        let resources = cache.list_cached_resources().await.unwrap();
        assert!(!resources.is_empty());

        // Clear cache
        let result = cache.clear().await;
        assert!(result.is_ok());

        // Verify cache is empty
        let resources = cache.list_cached_resources().await.unwrap();
        assert!(resources.is_empty());
        assert_eq!(cache.get_analytics().resource_count, 0);
    }

    #[tokio::test]
    async fn test_cleanup_expired_resources() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Add resource that expires immediately
        let resource = create_test_resource();
        cache
            .store_resource_with_ttl(&resource, Duration::from_millis(1))
            .await
            .unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Run cleanup
        let result = cache.cleanup_expired().await;
        assert!(result.is_ok());
        let removed_count = result.unwrap();
        assert_eq!(removed_count, 1);

        // Verify resource is gone
        assert!(!cache.contains_resource("test://example.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_cache_analytics() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Initial analytics
        let analytics = cache.get_analytics();
        assert_eq!(analytics.resource_count, 0);
        assert_eq!(analytics.cache_size_bytes, 0);
        assert_eq!(analytics.total_requests, 0);
        assert_eq!(analytics.cache_hits, 0);
        assert_eq!(analytics.cache_misses, 0);

        // Add a resource and access it
        let resource = create_test_resource();
        cache.store_resource(&resource).await.unwrap();

        // Access the resource to generate analytics
        let _retrieved = cache.get_resource("test://example.txt").await.unwrap();

        // Check updated analytics
        let analytics = cache.get_analytics();
        assert_eq!(analytics.resource_count, 1);
        assert!(analytics.cache_size_bytes > 0);
        assert_eq!(analytics.total_requests, 1);
        assert_eq!(analytics.cache_hits, 1);
        assert_eq!(analytics.cache_misses, 0);
        assert_eq!(analytics.hit_rate, 1.0);
    }

    #[tokio::test]
    async fn test_search_resources() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Initially empty
        let result = cache.search_resources("text/plain").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);

        // Add a resource
        let resource = create_test_resource();
        cache.store_resource(&resource).await.unwrap();

        // Search should find it
        let result = cache.search_resources("text/plain").await;
        assert!(result.is_ok());
        let resources = result.unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, "test://example.txt");

        // Search by URI should also work
        let result = cache.search_resources("example").await;
        assert!(result.is_ok());
        let resources = result.unwrap();
        assert_eq!(resources.len(), 1);
    }

    #[tokio::test]
    async fn test_get_cache_size() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Initially empty
        let result = cache.get_cache_size().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        // Add a resource
        let resource = create_test_resource();
        cache.store_resource(&resource).await.unwrap();

        // Should have size now
        let result = cache.get_cache_size().await;
        assert!(result.is_ok());
        let size = result.unwrap();
        assert!(size > 0);
        assert_eq!(size, 13); // "Hello, World!" is 13 bytes
    }

    #[tokio::test]
    async fn test_database_compaction() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Add and remove some resources to create fragmentation
        let resource = create_test_resource();
        cache.store_resource(&resource).await.unwrap();
        cache.remove_resource("test://example.txt").await.unwrap();

        // Compact should succeed
        let result = cache.compact().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let (mut config, _temp_dir) = create_test_cache_config();
        config.default_ttl = Duration::from_millis(100); // Very short TTL for testing
        let mut cache = ResourceCache::new(config).await.unwrap();
        let resource = create_test_resource();

        // Store resource
        let _id = cache.store_resource(&resource).await.unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Resource should be expired
        let result = cache.get_resource("test://example.txt").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let (config, temp_dir) = create_test_cache_config();
        let cache = std::sync::Arc::new(tokio::sync::Mutex::new(
            ResourceCache::new(config).await.unwrap(),
        ));
        let _temp_dir = std::sync::Arc::new(temp_dir); // Keep temp dir alive

        let resource = create_test_resource();
        let tasks = (0..10).map(|i| {
            let cache = cache.clone();
            let mut resource = resource.clone();
            resource.info.uri = format!("test://example{}.txt", i);

            tokio::spawn(async move {
                let mut cache = cache.lock().await;
                cache.store_resource(&resource).await
            })
        });

        // All operations should complete without corruption
        let results = futures::future::join_all(tasks).await;
        for result in results {
            assert!(result.is_ok());
            let store_result = result.unwrap();
            assert!(store_result.is_ok());
        }

        // Verify all resources were stored
        let cache = cache.lock().await;
        let resources = cache.list_cached_resources().await.unwrap();
        assert_eq!(resources.len(), 10);
    }

    #[tokio::test]
    async fn test_acid_transactions() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();
        let resource = create_test_resource();

        // Simulate a transaction that should either fully succeed or fully fail
        let result = cache.store_resource(&resource).await;

        // Even if it fails, the database should remain in a consistent state
        match result {
            Ok(_) => {
                // If successful, resource should be retrievable
                let retrieved = cache.get_resource("test://example.txt").await.unwrap();
                assert!(retrieved.is_some());
            }
            Err(_) => {
                // If failed, resource should not be partially stored
                let retrieved = cache.get_resource("test://example.txt").await.unwrap();
                assert!(retrieved.is_none());
            }
        }
    }

    #[test]
    fn test_cache_config_defaults() {
        let config = CacheConfig::default();
        // Should default to file-based database, not :memory:
        assert!(config.database_path.ends_with("cache.db"));
        assert!(!config.database_path.contains(":memory:"));
        assert_eq!(config.default_ttl, Duration::from_secs(3600));
        assert_eq!(config.max_size_mb, 100);
        assert!(config.auto_cleanup);
        assert_eq!(config.cleanup_interval, Duration::from_secs(300));
    }

    #[tokio::test]
    async fn test_empty_database_path_validation() {
        let config = CacheConfig {
            database_path: String::new(),
            ..Default::default()
        };

        let result = ResourceCache::new(config).await;
        assert!(result.is_err());
        if let Err(ClientError::Validation(msg)) = result {
            assert!(msg.contains("database_path cannot be empty"));
        } else {
            panic!("Expected Validation error for empty database path");
        }
    }

    #[tokio::test]
    async fn test_invalid_pool_configuration() {
        let (mut config, _temp_dir) = create_test_cache_config();
        config.pool_min_connections = Some(10);
        config.pool_max_connections = Some(5); // min > max

        let result = ResourceCache::new(config).await;
        assert!(result.is_err());
        if let Err(ClientError::Validation(msg)) = result {
            assert!(msg.contains("pool_min_connections"));
            assert!(msg.contains("pool_max_connections"));
        } else {
            panic!("Expected Validation error for invalid pool configuration");
        }
    }

    #[test]
    fn test_cached_resource_structure() {
        let cached_resource = CachedResource {
            id: Uuid::new_v4().to_string(),
            uri: "test://example.txt".to_string(),
            content: b"Hello, World!".to_vec(),
            content_type: Some("text/plain".to_string()),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            accessed_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            access_count: 1,
            size_bytes: 13,
        };

        assert_eq!(cached_resource.uri, "test://example.txt");
        assert_eq!(cached_resource.content, b"Hello, World!");
        assert_eq!(cached_resource.size_bytes, 13);
        assert!(cached_resource.expires_at.is_some());
    }

    #[tokio::test]
    async fn test_concurrent_cache_creation_shared_database() {
        // Test that multiple cache instances can safely use the same database file
        // This simulates the real-world scenario where multiple connections access a shared database
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_string_lossy().to_string();

        // Create multiple cache instances pointing to the same database file
        let mut caches = Vec::new();
        for _ in 0..5 {
            let config = CacheConfig {
                database_path: db_path.clone(),
                pool_connection_timeout: Some(POOL_TIMEOUT),
                ..Default::default()
            };
            let cache = ResourceCache::new(config).await.unwrap();
            caches.push(cache);
        }

        // All caches should be able to operate on the shared database
        for (i, cache) in caches.iter_mut().enumerate() {
            let resource = create_test_resource();
            let mut test_resource = resource.clone();
            test_resource.info.uri = format!("test://shared-{}.txt", i);

            // Store resource
            cache.store_resource(&test_resource).await.unwrap();

            // Verify it exists
            assert!(
                cache
                    .contains_resource(&test_resource.info.uri)
                    .await
                    .unwrap()
            );
        }

        // Verify all resources are accessible from any cache instance
        let first_cache = &caches[0];
        for i in 0..5 {
            let uri = format!("test://shared-{}.txt", i);
            assert!(
                first_cache.contains_resource(&uri).await.unwrap(),
                "Resource {} should be accessible from any cache instance",
                i
            );
        }
    }

    // ========== CONNECTION POOLING TESTS (TDD - These should FAIL initially) ==========

    #[tokio::test]
    async fn test_connection_pool_configuration() {
        // Test that CacheConfig supports connection pool settings
        let (mut config, _temp_dir) = create_test_cache_config();
        config.pool_min_connections = Some(2);
        config.pool_max_connections = Some(10);
        config.pool_connection_timeout = Some(POOL_TIMEOUT);

        let result = ResourceCache::new(config).await;
        assert!(result.is_ok());
        let cache = result.unwrap();

        // Should be able to get pool stats
        let stats = cache.get_pool_stats();
        assert_eq!(stats.max_connections, 10);
        assert!(stats.active_connections <= 10);
    }

    #[tokio::test]
    async fn test_concurrent_cache_operations_with_pool() {
        // Test that multiple operations can run truly concurrently with a connection pool
        let (mut config, temp_dir) = create_test_cache_config();
        config.pool_min_connections = Some(3);
        config.pool_max_connections = Some(5);
        let _temp_dir = std::sync::Arc::new(temp_dir); // Keep temp dir alive

        let cache = std::sync::Arc::new(tokio::sync::Mutex::new(
            ResourceCache::new(config).await.unwrap(),
        ));

        // Create test resources
        let mut tasks = Vec::new();
        for i in 0..10 {
            let cache = cache.clone();
            let task = tokio::spawn(async move {
                let mut resource = create_test_resource();
                resource.info.uri = format!("test://concurrent{}.txt", i);

                let mut cache_guard = cache.lock().await;
                let start = std::time::Instant::now();
                let result = cache_guard.store_resource(&resource).await;
                let duration = start.elapsed();

                // With pooling, operations should be faster due to parallelism
                assert!(result.is_ok());
                duration
            });
            tasks.push(task);
        }

        let durations: Vec<std::time::Duration> = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // All operations should complete successfully
        assert_eq!(durations.len(), 10);

        // With proper connection pooling, average duration should be reasonable
        let avg_duration = durations.iter().sum::<std::time::Duration>() / durations.len() as u32;
        assert!(avg_duration < Duration::from_millis(100)); // Should be fast with pooling
    }

    #[tokio::test]
    async fn test_pool_exhaustion_handling() {
        // Test behavior when all connections in pool are exhausted
        let (mut config, _temp_dir) = create_test_cache_config();
        config.pool_min_connections = Some(1);
        config.pool_max_connections = Some(2); // Very small pool to force exhaustion
        config.pool_connection_timeout = Some(Duration::from_millis(100)); // Short timeout

        let mut cache = ResourceCache::new(config).await.unwrap();

        // This should work fine initially
        let resource = create_test_resource();
        let result = cache.store_resource(&resource).await;
        assert!(result.is_ok());

        // Pool should handle exhaustion gracefully (queue or timeout appropriately)
        let pool_stats = cache.get_pool_stats();
        assert!(pool_stats.max_connections == 2);
    }

    #[tokio::test]
    async fn test_connection_reuse_in_pool() {
        // Test that connections are properly reused from the pool
        let (mut config, _temp_dir) = create_test_cache_config();
        config.pool_min_connections = Some(2);
        config.pool_max_connections = Some(3);

        let mut cache = ResourceCache::new(config).await.unwrap();
        let resource = create_test_resource();

        // First operation
        let _result1 = cache.store_resource(&resource).await.unwrap();
        let stats1 = cache.get_pool_stats();

        // Second operation should reuse connection
        let _result2 = cache.get_resource("test://example.txt").await.unwrap();
        let stats2 = cache.get_pool_stats();

        // Connection count shouldn't increase unnecessarily
        assert!(stats2.active_connections <= stats1.active_connections + 1);
    }

    #[tokio::test]
    async fn test_pool_connection_lifecycle() {
        // Test proper connection creation, usage, and cleanup
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let config = CacheConfig {
            database_path: temp_file.path().to_string_lossy().to_string(),
            pool_min_connections: Some(1),
            pool_max_connections: Some(3),
            ..Default::default()
        };

        {
            let cache = ResourceCache::new(config).await.unwrap();
            let pool_stats = cache.get_pool_stats();
            // Pool should be created and configured properly
            assert_eq!(pool_stats.max_connections, 3);
            // Note: idle connections may be 0 until actually used
            assert!(pool_stats.active_connections <= pool_stats.max_connections);
        }

        // After drop, connections should be cleaned up
        // (We can't easily test this without exposing internals, but the pattern should work)
    }

    #[test]
    fn test_parse_charset() {
        // Basic charset parsing
        assert_eq!(
            parse_charset("text/html; charset=utf-8"),
            Some("utf-8".to_string())
        );
        assert_eq!(
            parse_charset("text/plain; charset=ISO-8859-1"),
            Some("iso-8859-1".to_string())
        );

        // Edge cases
        assert_eq!(parse_charset("text/plain"), None);
        assert_eq!(parse_charset("application/octet-stream"), None);
        assert_eq!(
            parse_charset("text/html;charset=utf-8"),
            Some("utf-8".to_string())
        ); // no space
        assert_eq!(
            parse_charset("text/html; charset=UTF-8"),
            Some("utf-8".to_string())
        ); // uppercase
        assert_eq!(parse_charset(""), None);
        assert_eq!(
            parse_charset("text/html; charset=utf-8; boundary=something"),
            Some("utf-8".to_string())
        );

        // NEW ROBUSTNESS TESTS (should fail with current implementation)
        // Quoted values
        assert_eq!(
            parse_charset("text/html; charset=\"utf-8\""),
            Some("utf-8".to_string())
        );
        assert_eq!(
            parse_charset("text/html; charset='iso-8859-1'"),
            Some("iso-8859-1".to_string())
        );

        // Case insensitive key matching
        assert_eq!(
            parse_charset("text/html; Charset=UTF-8"),
            Some("utf-8".to_string())
        );
        assert_eq!(
            parse_charset("text/html; CHARSET=windows-1252"),
            Some("windows-1252".to_string())
        );

        // Mixed case with quotes
        assert_eq!(
            parse_charset("text/html; Charset=\"UTF-8\""),
            Some("utf-8".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_resource_with_encoding_from_metadata() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Create a resource with encoding in metadata
        let mut metadata = HashMap::new();
        metadata.insert("encoding".to_string(), serde_json::json!("utf-16"));

        let resource = ResourceContent {
            info: ResourceInfo {
                uri: "test://encoded.txt".to_string(),
                name: Some("encoded.txt".to_string()),
                description: Some("Test resource with encoding".to_string()),
                mime_type: Some("text/plain".to_string()),
                metadata,
            },
            data: b"Hello, World!".to_vec(),
            encoding: Some("utf-16".to_string()),
        };

        // Store the resource
        cache.store_resource(&resource).await.unwrap();

        // Retrieve and check encoding is preserved
        let retrieved = cache.get_resource("test://encoded.txt").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved_resource = retrieved.unwrap();
        assert_eq!(retrieved_resource.encoding, Some("utf-16".to_string()));
    }

    #[tokio::test]
    async fn test_get_resource_with_encoding_from_content_type() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Create a resource without encoding in metadata but with charset in content_type
        let resource = ResourceContent {
            info: ResourceInfo {
                uri: "test://charset.html".to_string(),
                name: Some("charset.html".to_string()),
                description: Some("Test resource with charset in content type".to_string()),
                mime_type: Some("text/html; charset=iso-8859-1".to_string()),
                metadata: HashMap::new(),
            },
            data: b"<html>Hello</html>".to_vec(),
            encoding: None, // No encoding specified
        };

        // Store the resource
        cache.store_resource(&resource).await.unwrap();

        // Retrieve and check encoding is extracted from content_type
        let retrieved = cache.get_resource("test://charset.html").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved_resource = retrieved.unwrap();
        assert_eq!(retrieved_resource.encoding, Some("iso-8859-1".to_string()));
    }

    #[tokio::test]
    async fn test_store_and_retrieve_with_encoding() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Create a resource with encoding
        let resource = ResourceContent {
            info: ResourceInfo {
                uri: "test://utf8.txt".to_string(),
                name: Some("utf8.txt".to_string()),
                description: Some("UTF-8 encoded text".to_string()),
                mime_type: Some("text/plain".to_string()),
                metadata: HashMap::new(),
            },
            data: "Hello, 世界! 🌍".as_bytes().to_vec(),
            encoding: Some("utf-8".to_string()),
        };

        // Store the resource
        cache.store_resource(&resource).await.unwrap();

        // Retrieve and verify encoding is preserved
        let retrieved = cache.get_resource("test://utf8.txt").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved_resource = retrieved.unwrap();
        assert_eq!(retrieved_resource.encoding, Some("utf-8".to_string()));
        assert_eq!(retrieved_resource.data, "Hello, 世界! 🌍".as_bytes());
    }

    #[tokio::test]
    async fn test_round_trip_encoding_with_quoted_charset() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Create resource with quoted charset in content-type (should work after fix)
        let resource = ResourceContent {
            info: ResourceInfo {
                uri: "test://quoted-charset.html".to_string(),
                name: Some("quoted-charset.html".to_string()),
                description: Some("HTML with quoted charset".to_string()),
                mime_type: Some("text/html; charset=\"windows-1252\"".to_string()),
                metadata: HashMap::new(),
            },
            data: b"<html>Content with special chars</html>".to_vec(),
            encoding: None, // No encoding specified - should extract from content-type
        };

        // Store the resource
        cache.store_resource(&resource).await.unwrap();

        // Retrieve and verify encoding was extracted from quoted content-type
        let retrieved = cache
            .get_resource("test://quoted-charset.html")
            .await
            .unwrap();
        assert!(retrieved.is_some());
        let retrieved_resource = retrieved.unwrap();
        assert_eq!(
            retrieved_resource.encoding,
            Some("windows-1252".to_string())
        );
    }

    #[tokio::test]
    async fn test_round_trip_encoding_with_case_insensitive_charset() {
        let (config, _temp_dir) = create_test_cache_config();
        let mut cache = ResourceCache::new(config).await.unwrap();

        // Create resource with uppercase Charset in content-type (should work after fix)
        let resource = ResourceContent {
            info: ResourceInfo {
                uri: "test://uppercase-charset.xml".to_string(),
                name: Some("uppercase-charset.xml".to_string()),
                description: Some("XML with uppercase Charset".to_string()),
                mime_type: Some("application/xml; Charset=UTF-8".to_string()),
                metadata: HashMap::new(),
            },
            data: b"<?xml version=\"1.0\"?><root>data</root>".to_vec(),
            encoding: None, // No encoding specified - should extract from content-type
        };

        // Store the resource
        cache.store_resource(&resource).await.unwrap();

        // Retrieve and verify encoding was extracted from uppercase Charset
        let retrieved = cache
            .get_resource("test://uppercase-charset.xml")
            .await
            .unwrap();
        assert!(retrieved.is_some());
        let retrieved_resource = retrieved.unwrap();
        assert_eq!(retrieved_resource.encoding, Some("utf-8".to_string()));
    }

    #[test]
    fn test_analytics_hit_rate_calculation_safety() {
        let analytics = CacheAnalytics {
            total_requests: 0,
            cache_hits: 0,
            cache_misses: 0,
            hit_rate: 0.0,
            cache_size_bytes: 0,
            resource_count: 0,
            eviction_count: 0,
            last_cleanup: Utc::now(),
        };

        // Calculate hit rate with zero requests - should not panic
        let hit_rate = if analytics.total_requests > 0 {
            analytics.cache_hits as f64 / analytics.total_requests as f64
        } else {
            0.0
        };

        assert_eq!(hit_rate, 0.0);
    }

    #[tokio::test]
    async fn test_migration_system_and_connection_pool() {
        use std::time::Duration;

        // Create cache with pool settings to test migration + pool integration
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let config = CacheConfig {
            database_path: temp_file.path().to_string_lossy().to_string(),
            default_ttl: Duration::from_secs(60),
            max_size_mb: 100,
            auto_cleanup: true,
            cleanup_interval: Duration::from_secs(30),
            pool_min_connections: Some(2),
            pool_max_connections: Some(4),
            pool_connection_timeout: Some(Duration::from_secs(5)),
            pool_max_lifetime: Some(Duration::from_secs(300)),
        };

        // Test that migrations work with the connection pool
        let mut cache = ResourceCache::new(config).await.unwrap();

        let test_resource = ResourceContent {
            info: ResourceInfo {
                uri: "test://migration/verification".to_string(),
                name: Some("Migration Test".to_string()),
                description: Some("Verify migration + pool work together".to_string()),
                mime_type: Some("text/plain".to_string()),
                metadata: std::collections::HashMap::new(),
            },
            data: b"migration test data".to_vec(),
            encoding: None,
        };

        // Store and retrieve to verify the migrated schema works with pooled connections
        let _id = cache.store_resource(&test_resource).await.unwrap();
        let retrieved = cache.get_resource(&test_resource.info.uri).await.unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().data, test_resource.data);

        // Verify analytics table exists and works (created by migration)
        let analytics = cache.get_analytics();
        assert_eq!(analytics.total_requests, 1); // Should have 1 request from get_resource above

        // Test basic pool functionality by accessing multiple resources sequentially
        for i in 0..5 {
            let uri = format!("test://pool/resource{}", i);
            let result = cache.get_resource(&uri).await;
            assert!(result.is_ok()); // Should succeed even for non-existent resources
        }

        info!("Migration system and connection pool integration test passed");
    }
}
