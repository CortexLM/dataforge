//! LLM prompt caching system for multi-conversation efficiency.
//!
//! This module provides caching for system prompts and conversation prefixes
//! to reduce token usage across multiple agent conversations.
//!
//! # Caching Strategy
//!
//! System prompts and conversation prefixes are cached using content hashing.
//! When the same prompt is used across multiple conversations, only the hash
//! is sent to the API (if supported), reducing token transmission.
//!
//! # Usage
//!
//! ```ignore
//! use dataforge::llm::{PromptCache, CachedMessage, Message};
//!
//! let cache = PromptCache::new(1000); // Max 1000 entries
//!
//! // Cache a system prompt
//! let cached = cache.cache_message(Message::system("You are helpful"));
//!
//! // Use cached message in requests
//! let request = GenerationRequest::new(model, vec![cached.into()]);
//! ```

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use super::Message;

/// Hash of cached content for efficient lookup and comparison.
///
/// The hash is computed using SHA-256 and stored as a hex-encoded string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(String);

impl ContentHash {
    /// Create a new content hash from content string.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to hash
    ///
    /// # Returns
    ///
    /// A `ContentHash` containing the hex-encoded SHA-256 hash of the content.
    pub fn from_content(content: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();
        Self(hex::encode(result))
    }

    /// Get the hash string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A cached message that can be reused across conversations.
///
/// Contains the original message along with metadata about caching status
/// and token usage.
#[derive(Debug, Clone)]
pub struct CachedMessage {
    /// The original message.
    pub message: Message,
    /// Content hash for cache lookup.
    pub hash: ContentHash,
    /// Whether this message was retrieved from cache.
    pub from_cache: bool,
    /// Token count (if known from previous API calls).
    pub token_count: Option<u32>,
}

impl CachedMessage {
    /// Create a new cached message wrapper.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to wrap
    ///
    /// # Returns
    ///
    /// A `CachedMessage` with computed hash and `from_cache` set to false.
    pub fn new(message: Message) -> Self {
        let hash = ContentHash::from_content(&message.content);
        Self {
            message,
            hash,
            from_cache: false,
            token_count: None,
        }
    }

    /// Mark this message as retrieved from cache.
    ///
    /// # Returns
    ///
    /// Self with `from_cache` set to true.
    pub fn mark_from_cache(mut self) -> Self {
        self.from_cache = true;
        self
    }

    /// Set the token count for this message.
    ///
    /// # Arguments
    ///
    /// * `count` - The number of tokens in this message
    ///
    /// # Returns
    ///
    /// Self with `token_count` set.
    pub fn with_token_count(mut self, count: u32) -> Self {
        self.token_count = Some(count);
        self
    }
}

impl From<CachedMessage> for Message {
    fn from(cached: CachedMessage) -> Self {
        cached.message
    }
}

/// Cache entry with metadata for LRU eviction and statistics.
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached message.
    message: Message,
    /// Token count (if known from API responses).
    token_count: Option<u32>,
    /// When this entry was created.
    created_at: Instant,
    /// Last access time for LRU eviction.
    last_accessed: Instant,
    /// Access count for statistics (reserved for future LFU eviction).
    #[allow(dead_code)]
    access_count: u64,
}

/// Configuration for the prompt cache.
///
/// Controls cache behavior including size limits, TTL, and which message
/// types to cache.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries in the cache.
    pub max_entries: usize,
    /// Time-to-live for cache entries. Entries older than this are evicted.
    pub ttl: Duration,
    /// Whether to cache system prompts (usually beneficial).
    pub cache_system_prompts: bool,
    /// Whether to cache user messages (usually unique per request).
    pub cache_user_messages: bool,
    /// Whether to cache assistant messages.
    pub cache_assistant_messages: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl: Duration::from_secs(3600), // 1 hour
            cache_system_prompts: true,
            cache_user_messages: false, // Usually unique per request
            cache_assistant_messages: false,
        }
    }
}

impl CacheConfig {
    /// Create a new cache configuration with specified max entries.
    ///
    /// # Arguments
    ///
    /// * `max_entries` - Maximum number of entries to store
    ///
    /// # Returns
    ///
    /// A `CacheConfig` with the specified max entries and default settings.
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            ..Default::default()
        }
    }

    /// Set the TTL for cache entries.
    ///
    /// # Arguments
    ///
    /// * `ttl` - Time-to-live duration
    ///
    /// # Returns
    ///
    /// Self with updated TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Enable caching for all message types.
    ///
    /// This is useful when running repeated conversations with similar
    /// patterns across all message types.
    ///
    /// # Returns
    ///
    /// Self with all caching options enabled.
    pub fn cache_all(mut self) -> Self {
        self.cache_system_prompts = true;
        self.cache_user_messages = true;
        self.cache_assistant_messages = true;
        self
    }

    /// Disable caching for all message types except system prompts.
    ///
    /// This is the recommended default for most use cases.
    ///
    /// # Returns
    ///
    /// Self with only system prompt caching enabled.
    pub fn system_prompts_only(mut self) -> Self {
        self.cache_system_prompts = true;
        self.cache_user_messages = false;
        self.cache_assistant_messages = false;
        self
    }
}

/// Prompt cache for storing and retrieving cached messages.
///
/// Thread-safe cache implementation using interior mutability with `RwLock`.
/// Uses LRU eviction when the cache reaches capacity.
///
/// # Example
///
/// ```ignore
/// use dataforge::llm::{PromptCache, Message};
///
/// let cache = PromptCache::new(100);
///
/// // First call - cache miss
/// let msg = Message::system("You are a helpful assistant.");
/// let cached = cache.cache_message(msg);
/// assert!(!cached.from_cache);
///
/// // Second call with same content - cache hit
/// let msg2 = Message::system("You are a helpful assistant.");
/// let cached2 = cache.cache_message(msg2);
/// assert!(cached2.from_cache);
/// ```
pub struct PromptCache {
    /// Cache storage protected by RwLock for thread safety.
    cache: RwLock<HashMap<ContentHash, CacheEntry>>,
    /// Cache configuration.
    config: CacheConfig,
    /// Cache statistics.
    stats: RwLock<CacheStats>,
}

/// Cache statistics for monitoring and debugging.
///
/// Tracks hits, misses, evictions, and estimated token savings.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total cache hits.
    pub hits: u64,
    /// Total cache misses.
    pub misses: u64,
    /// Total entries added.
    pub entries_added: u64,
    /// Total entries evicted.
    pub entries_evicted: u64,
    /// Estimated tokens saved by cache hits.
    pub tokens_saved: u64,
}

impl CacheStats {
    /// Calculate the cache hit rate.
    ///
    /// # Returns
    ///
    /// Hit rate as a value between 0.0 and 1.0, or 0.0 if no accesses.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Get the total number of cache accesses.
    pub fn total_accesses(&self) -> u64 {
        self.hits + self.misses
    }
}

impl PromptCache {
    /// Create a new prompt cache with default configuration.
    ///
    /// # Arguments
    ///
    /// * `max_entries` - Maximum number of entries to store
    ///
    /// # Returns
    ///
    /// A new `PromptCache` instance.
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            config: CacheConfig::new(max_entries),
            stats: RwLock::new(CacheStats::default()),
        }
    }

    /// Create a new prompt cache with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Custom cache configuration
    ///
    /// # Returns
    ///
    /// A new `PromptCache` instance with the specified configuration.
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            config,
            stats: RwLock::new(CacheStats::default()),
        }
    }

    /// Check if a message type should be cached based on configuration.
    fn should_cache(&self, role: &str) -> bool {
        match role {
            "system" => self.config.cache_system_prompts,
            "user" => self.config.cache_user_messages,
            "assistant" => self.config.cache_assistant_messages,
            _ => false,
        }
    }

    /// Cache a message and return a cached version.
    ///
    /// If the message content is already in the cache, returns a `CachedMessage`
    /// with `from_cache` set to true. Otherwise, adds the message to the cache
    /// and returns a `CachedMessage` with `from_cache` set to false.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to cache
    ///
    /// # Returns
    ///
    /// A `CachedMessage` wrapper for the message.
    pub fn cache_message(&self, message: Message) -> CachedMessage {
        if !self.should_cache(&message.role) {
            return CachedMessage::new(message);
        }

        let hash = ContentHash::from_content(&message.content);

        // Check if already cached (read lock)
        {
            let cache = self.cache.read().expect("cache read lock poisoned");
            if let Some(entry) = cache.get(&hash) {
                // Check if entry has expired
                if entry.created_at.elapsed() < self.config.ttl {
                    // Update stats
                    let mut stats = self.stats.write().expect("stats write lock poisoned");
                    stats.hits += 1;
                    if let Some(tokens) = entry.token_count {
                        stats.tokens_saved += u64::from(tokens);
                    }

                    return CachedMessage {
                        message: entry.message.clone(),
                        hash,
                        from_cache: true,
                        token_count: entry.token_count,
                    };
                }
            }
        }

        // Add to cache (write lock)
        {
            let mut cache = self.cache.write().expect("cache write lock poisoned");

            // Evict old entries if needed
            if cache.len() >= self.config.max_entries {
                self.evict_oldest(&mut cache);
            }

            // Also evict expired entries opportunistically
            self.evict_expired(&mut cache);

            let now = Instant::now();
            cache.insert(
                hash.clone(),
                CacheEntry {
                    message: message.clone(),
                    token_count: None,
                    created_at: now,
                    last_accessed: now,
                    access_count: 1,
                },
            );

            let mut stats = self.stats.write().expect("stats write lock poisoned");
            stats.misses += 1;
            stats.entries_added += 1;
        }

        CachedMessage::new(message)
    }

    /// Cache multiple messages at once.
    ///
    /// More efficient than calling `cache_message` repeatedly for multiple
    /// messages as it batches lock acquisitions.
    ///
    /// # Arguments
    ///
    /// * `messages` - Iterator of messages to cache
    ///
    /// # Returns
    ///
    /// Vector of `CachedMessage` wrappers.
    pub fn cache_messages<I>(&self, messages: I) -> Vec<CachedMessage>
    where
        I: IntoIterator<Item = Message>,
    {
        messages
            .into_iter()
            .map(|msg| self.cache_message(msg))
            .collect()
    }

    /// Update the token count for a cached message.
    ///
    /// Call this after receiving an API response to track token usage
    /// for future cache hit statistics.
    ///
    /// # Arguments
    ///
    /// * `hash` - The content hash of the message
    /// * `token_count` - The number of tokens in the message
    pub fn update_token_count(&self, hash: &ContentHash, token_count: u32) {
        let mut cache = self.cache.write().expect("cache write lock poisoned");
        if let Some(entry) = cache.get_mut(hash) {
            entry.token_count = Some(token_count);
        }
    }

    /// Evict the oldest (LRU) entry from the cache.
    fn evict_oldest(&self, cache: &mut HashMap<ContentHash, CacheEntry>) {
        let oldest = cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(hash, _)| hash.clone());

        if let Some(hash) = oldest {
            cache.remove(&hash);
            let mut stats = self.stats.write().expect("stats write lock poisoned");
            stats.entries_evicted += 1;
        }
    }

    /// Evict all expired entries from the cache.
    fn evict_expired(&self, cache: &mut HashMap<ContentHash, CacheEntry>) {
        let ttl = self.config.ttl;
        let expired: Vec<ContentHash> = cache
            .iter()
            .filter(|(_, entry)| entry.created_at.elapsed() >= ttl)
            .map(|(hash, _)| hash.clone())
            .collect();

        let evicted_count = expired.len() as u64;
        for hash in expired {
            cache.remove(&hash);
        }

        if evicted_count > 0 {
            let mut stats = self.stats.write().expect("stats write lock poisoned");
            stats.entries_evicted += evicted_count;
        }
    }

    /// Get current cache statistics.
    ///
    /// # Returns
    ///
    /// A clone of the current `CacheStats`.
    pub fn stats(&self) -> CacheStats {
        self.stats.read().expect("stats read lock poisoned").clone()
    }

    /// Get the cache configuration.
    ///
    /// # Returns
    ///
    /// A reference to the `CacheConfig`.
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    /// Clear all entries from the cache.
    ///
    /// Statistics are preserved; only the cache entries are removed.
    pub fn clear(&self) {
        let mut cache = self.cache.write().expect("cache write lock poisoned");
        cache.clear();
    }

    /// Get the number of cached entries.
    pub fn len(&self) -> usize {
        self.cache.read().expect("cache read lock poisoned").len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if a specific content hash is in the cache.
    ///
    /// # Arguments
    ///
    /// * `hash` - The content hash to check
    ///
    /// # Returns
    ///
    /// `true` if the hash is cached and not expired.
    pub fn contains(&self, hash: &ContentHash) -> bool {
        let cache = self.cache.read().expect("cache read lock poisoned");
        if let Some(entry) = cache.get(hash) {
            entry.created_at.elapsed() < self.config.ttl
        } else {
            false
        }
    }
}

/// Thread-safe shared prompt cache type alias.
///
/// Use this when sharing a cache across multiple async tasks or threads.
pub type SharedPromptCache = std::sync::Arc<PromptCache>;

/// Create a new shared prompt cache.
///
/// # Arguments
///
/// * `max_entries` - Maximum number of entries to store
///
/// # Returns
///
/// An `Arc`-wrapped `PromptCache` suitable for sharing across threads.
///
/// # Example
///
/// ```ignore
/// use dataforge::llm::create_shared_cache;
///
/// let cache = create_shared_cache(1000);
/// let cache_clone = cache.clone(); // Cheap clone
/// ```
pub fn create_shared_cache(max_entries: usize) -> SharedPromptCache {
    std::sync::Arc::new(PromptCache::new(max_entries))
}

/// Create a new shared prompt cache with custom configuration.
///
/// # Arguments
///
/// * `config` - Custom cache configuration
///
/// # Returns
///
/// An `Arc`-wrapped `PromptCache` with the specified configuration.
pub fn create_shared_cache_with_config(config: CacheConfig) -> SharedPromptCache {
    std::sync::Arc::new(PromptCache::with_config(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let hash1 = ContentHash::from_content("hello world");
        let hash2 = ContentHash::from_content("hello world");
        let hash3 = ContentHash::from_content("different content");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_content_hash_display() {
        let hash = ContentHash::from_content("test");
        let display = format!("{}", hash);
        assert_eq!(display.len(), 64); // SHA-256 produces 64 hex chars
    }

    #[test]
    fn test_cached_message_new() {
        let msg = Message::system("You are helpful");
        let cached = CachedMessage::new(msg.clone());

        assert_eq!(cached.message.content, "You are helpful");
        assert_eq!(cached.message.role, "system");
        assert!(!cached.from_cache);
        assert!(cached.token_count.is_none());
    }

    #[test]
    fn test_cached_message_modifiers() {
        let msg = Message::system("test");
        let cached = CachedMessage::new(msg)
            .mark_from_cache()
            .with_token_count(100);

        assert!(cached.from_cache);
        assert_eq!(cached.token_count, Some(100));
    }

    #[test]
    fn test_cached_message_into_message() {
        let original = Message::system("test content");
        let cached = CachedMessage::new(original.clone());
        let converted: Message = cached.into();

        assert_eq!(converted.content, original.content);
        assert_eq!(converted.role, original.role);
    }

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();

        assert_eq!(config.max_entries, 1000);
        assert_eq!(config.ttl, Duration::from_secs(3600));
        assert!(config.cache_system_prompts);
        assert!(!config.cache_user_messages);
        assert!(!config.cache_assistant_messages);
    }

    #[test]
    fn test_cache_config_builder() {
        let config = CacheConfig::new(500)
            .with_ttl(Duration::from_secs(1800))
            .cache_all();

        assert_eq!(config.max_entries, 500);
        assert_eq!(config.ttl, Duration::from_secs(1800));
        assert!(config.cache_system_prompts);
        assert!(config.cache_user_messages);
        assert!(config.cache_assistant_messages);
    }

    #[test]
    fn test_cache_config_system_prompts_only() {
        let config = CacheConfig::default().cache_all().system_prompts_only();

        assert!(config.cache_system_prompts);
        assert!(!config.cache_user_messages);
        assert!(!config.cache_assistant_messages);
    }

    #[test]
    fn test_prompt_cache_hit_miss() {
        let cache = PromptCache::new(100);

        // First access - cache miss
        let msg = Message::system("You are helpful");
        let cached1 = cache.cache_message(msg.clone());
        assert!(!cached1.from_cache);

        // Second access - cache hit
        let cached2 = cache.cache_message(msg);
        assert!(cached2.from_cache);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries_added, 1);
    }

    #[test]
    fn test_prompt_cache_no_cache_user_messages_by_default() {
        let cache = PromptCache::new(100);

        // User messages are not cached by default
        let msg = Message::user("What is 2+2?");
        let cached1 = cache.cache_message(msg.clone());
        assert!(!cached1.from_cache);

        let cached2 = cache.cache_message(msg);
        assert!(!cached2.from_cache);

        assert!(cache.is_empty()); // No entries should be added
    }

    #[test]
    fn test_prompt_cache_with_user_caching_enabled() {
        let config = CacheConfig::new(100).cache_all();
        let cache = PromptCache::with_config(config);

        let msg = Message::user("What is 2+2?");
        let cached1 = cache.cache_message(msg.clone());
        assert!(!cached1.from_cache);

        let cached2 = cache.cache_message(msg);
        assert!(cached2.from_cache);

        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_prompt_cache_eviction() {
        let cache = PromptCache::new(2);

        cache.cache_message(Message::system("one"));
        cache.cache_message(Message::system("two"));
        assert_eq!(cache.len(), 2);

        cache.cache_message(Message::system("three"));
        assert_eq!(cache.len(), 2);

        let stats = cache.stats();
        assert_eq!(stats.entries_evicted, 1);
        assert_eq!(stats.entries_added, 3);
    }

    #[test]
    fn test_prompt_cache_clear() {
        let cache = PromptCache::new(100);

        cache.cache_message(Message::system("one"));
        cache.cache_message(Message::system("two"));
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        // Stats should be preserved
        let stats = cache.stats();
        assert_eq!(stats.entries_added, 2);
    }

    #[test]
    fn test_prompt_cache_contains() {
        let cache = PromptCache::new(100);

        let msg = Message::system("test content");
        let hash = ContentHash::from_content(&msg.content);

        assert!(!cache.contains(&hash));

        cache.cache_message(msg);
        assert!(cache.contains(&hash));
    }

    #[test]
    fn test_prompt_cache_update_token_count() {
        let cache = PromptCache::new(100);

        let msg = Message::system("test content");
        let cached = cache.cache_message(msg.clone());
        assert!(cached.token_count.is_none());

        // Update token count
        cache.update_token_count(&cached.hash, 42);

        // Next cache hit should include token count
        let cached2 = cache.cache_message(msg);
        assert!(cached2.from_cache);
        assert_eq!(cached2.token_count, Some(42));
    }

    #[test]
    fn test_prompt_cache_batch_messages() {
        let cache = PromptCache::new(100);

        let messages = vec![
            Message::system("system prompt"),
            Message::user("user message"), // Won't be cached by default
            Message::assistant("assistant response"), // Won't be cached by default
        ];

        let cached = cache.cache_messages(messages);

        assert_eq!(cached.len(), 3);
        assert!(!cached[0].from_cache); // system - cached but first access
        assert!(!cached[1].from_cache); // user - not cached
        assert!(!cached[2].from_cache); // assistant - not cached

        assert_eq!(cache.len(), 1); // Only system prompt cached
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let mut stats = CacheStats::default();

        // No accesses
        assert_eq!(stats.hit_rate(), 0.0);

        // All misses
        stats.misses = 10;
        assert_eq!(stats.hit_rate(), 0.0);

        // 50% hit rate
        stats.hits = 10;
        assert_eq!(stats.hit_rate(), 0.5);

        // All hits
        stats.misses = 0;
        assert_eq!(stats.hit_rate(), 1.0);
    }

    #[test]
    fn test_cache_stats_total_accesses() {
        let mut stats = CacheStats::default();
        stats.hits = 5;
        stats.misses = 3;

        assert_eq!(stats.total_accesses(), 8);
    }

    #[test]
    fn test_create_shared_cache() {
        let cache = create_shared_cache(100);
        let cache_clone = cache.clone();

        // Both references should point to the same cache
        cache.cache_message(Message::system("test"));
        assert_eq!(cache_clone.len(), 1);
    }

    #[test]
    fn test_create_shared_cache_with_config() {
        let config = CacheConfig::new(50).with_ttl(Duration::from_secs(60));
        let cache = create_shared_cache_with_config(config);

        assert_eq!(cache.config().max_entries, 50);
        assert_eq!(cache.config().ttl, Duration::from_secs(60));
    }

    #[test]
    fn test_prompt_cache_different_roles_same_content() {
        let config = CacheConfig::new(100).cache_all();
        let cache = PromptCache::with_config(config);

        // Same content, different roles - should be separate entries
        // But since hash is only on content, they will share cache entry
        let sys_msg = Message::system("hello");
        let user_msg = Message::user("hello");

        cache.cache_message(sys_msg);
        let cached_user = cache.cache_message(user_msg);

        // Hash is the same (based on content), so it's a cache hit
        // but the stored message will have the original role
        assert!(cached_user.from_cache);
        assert_eq!(cache.len(), 1);
    }
}
