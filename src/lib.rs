use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hashbrown::HashMap;

#[derive(Default, Debug, Clone)]
pub struct Limiter<T>
where
    T: Hash + Eq + Send + 'static,
{
    requests: Arc<Mutex<HashMap<T, AssociatedEntity>>>,
}

#[derive(Debug, Clone, Hash)]
pub struct AssociatedEntity {
    bucket: usize, // How many requests are left in the bucket, 0 means the hard limit.
    bucket_init: Instant, // When was the last bucket refreshed
    bucket_max: usize, // set by user, this is the value the bucket will get refilled with.
    refresh_rate: Duration, // Every refresh_rate tick bucket gets filled with bucket_max
}

impl<T> Limiter<T>
where
    T: Hash + Eq + Send + 'static,
{
    pub fn new() -> Self {
        Limiter {
            requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Adds a entity to the limiter
    /// `entity` is something hashable like a IP, username, etc...
    ///
    /// `max_limit` is the max number of requests in the given timeframe that you allow for that specific entity
    ///
    /// `refresh_rate` is the timeframe after which the entity gets a renewed limit
    pub fn add_limited_entity(&self, entity: T, max_limit: usize, refresh_rate: Duration) {
        let mut requests = self.requests.lock().unwrap();
        requests.insert(
            entity,
            AssociatedEntity {
                bucket: max_limit,
                bucket_init: Instant::now(),
                bucket_max: max_limit,
                refresh_rate,
            },
        );
    }

    /// Removes a entity from the limiter
    ///
    /// Removes a key from the map, returning the value at the key if the key was previously in the map.
    /// Keeps the allocated memory for reuse.
    ///
    /// The key may be any borrowed form of the map's key type,
    /// but Hash and Eq on the borrowed form must match those for the key type.
    pub fn remove_limited_entity(&self, entity: T) -> Option<AssociatedEntity> {
        let mut requests = self.requests.lock().unwrap();
        requests.remove(&entity)
    }

    /// Checks whether a entity has requests left to consume.
    ///
    /// `entity` has been added by you previously with `add_limited_entity`
    ///
    /// ### returns:
    ///
    /// `None` -> entity was not found by the limiter, create one with `add_limited_entity`.
    ///
    /// `Some(false)` -> entity is rate limited, no requests to consume.
    ///
    /// `Some(true)` -> everything worked, entity had requests left.
    pub fn is_entity_limited(&mut self, entity: &T) -> Option<bool> {
        let mut requests = self.requests.lock().unwrap();
        let now = Instant::now();

        if let Some(entry) = requests.get_mut(entity) {
            if now.duration_since(entry.bucket_init) >= entry.refresh_rate {
                entry.bucket = entry.bucket_max;
                entry.bucket_init = now;
            }

            if entry.bucket > 0 {
                entry.bucket -= 1; // request allowed
                Some(true)
            } else {
                // entity is limited, request denied.
                Some(false)
            }
        } else {
            None
        }
    }

    /// Returns the current amount of requests left in the entity's bucket.
    ///
    /// `entity` has been added by you previously with `add_limited_entity`
    ///
    /// ### returns:
    ///
    /// `None` -> entity was not found by the limiter, create one with `add_limited_entity`.
    ///
    /// `Some(usize)` -> the current number of requests left in the entity's bucket.
    pub fn get_bucket_remaining(&self, entity: &T) -> Option<usize> {
        let requests = self.requests.lock().unwrap();
        requests.get(entity).map(|entry| entry.bucket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_add_limited_entity() {
        let limiter: Limiter<&str> = Limiter::new();
        limiter.add_limited_entity("user1", 5, Duration::from_secs(60));

        let requests = limiter.requests.lock().unwrap();
        assert!(requests.contains_key("user1"));
        assert_eq!(requests["user1"].bucket_max, 5);
        assert_eq!(requests["user1"].bucket, 5);
    }

    #[test]
    fn test_limiter_refresh_rate() {
        let mut limiter: Limiter<&str> = Limiter::new();
        let refresh_rate = Duration::from_millis(500);
        let max_requests = 3;

        limiter.add_limited_entity("user1", max_requests, refresh_rate);

        for _ in 0..max_requests {
            assert_eq!(
                limiter.is_entity_limited(&"user1"),
                Some(true),
                "Request should be allowed"
            );
        }

        assert_eq!(
            limiter.is_entity_limited(&"user1"),
            Some(false),
            "Request should be denied after limit is reached"
        );

        thread::sleep(refresh_rate + Duration::from_millis(50));

        // After refresh, we should be able to make max_requests again
        for i in 0..max_requests {
            assert_eq!(
                limiter.is_entity_limited(&"user1"),
                Some(true),
                "Request {} should be allowed after refresh",
                i + 1
            );
        }

        // The next request should be denied again
        assert_eq!(
            limiter.is_entity_limited(&"user1"),
            Some(false),
            "Request should be denied after refreshed limit is reached"
        );
    }

    #[test]
    fn test_is_entity_limited_allows_requests() {
        let mut limiter: Limiter<&str> = Limiter::new();
        limiter.add_limited_entity("user1", 2, Duration::from_secs(60));

        assert_eq!(limiter.is_entity_limited(&"user1"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user1"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user1"), Some(false));
    }

    #[test]
    fn test_is_entity_limited_refills_bucket() {
        let mut limiter: Limiter<&str> = Limiter::new();
        limiter.add_limited_entity("user1", 1, Duration::from_millis(10));

        assert_eq!(limiter.is_entity_limited(&"user1"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user1"), Some(false));
        thread::sleep(Duration::from_millis(25));
        assert_eq!(limiter.is_entity_limited(&"user1"), Some(true));
    }

    #[test]
    fn test_is_entity_limited_not_found() {
        let mut limiter: Limiter<&str> = Limiter::new();
        assert_eq!(limiter.is_entity_limited(&"unknown_user"), None);
    }

    #[test]
    fn test_multiple_entities() {
        let mut limiter: Limiter<&str> = Limiter::new();
        limiter.add_limited_entity("user1", 3, Duration::from_secs(60));
        limiter.add_limited_entity("user2", 5, Duration::from_secs(60));

        assert_eq!(limiter.is_entity_limited(&"user1"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user1"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user1"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user1"), Some(false)); // Now should be limited

        assert_eq!(limiter.is_entity_limited(&"user2"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user2"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user2"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user2"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user2"), Some(true));
        assert_eq!(limiter.is_entity_limited(&"user2"), Some(false)); // Now should be limited
    }

    #[test]
    fn test_limiter_with_multiple_threads() {
        let limiter = Arc::new(Mutex::new(Limiter::new()));
        limiter
            .lock()
            .unwrap()
            .add_limited_entity("user1", 5, Duration::from_secs(60));

        let limiter_clone1 = Arc::clone(&limiter);
        let limiter_clone2 = Arc::clone(&limiter);
        let limiter_clone3 = Arc::clone(&limiter);

        let thread1 = thread::spawn(move || {
            for _ in 0..2 {
                assert_eq!(
                    limiter_clone1.lock().unwrap().is_entity_limited(&"user1"),
                    Some(true)
                );
            }
        });

        let thread2 = thread::spawn(move || {
            for _ in 0..2 {
                assert_eq!(
                    limiter_clone2.lock().unwrap().is_entity_limited(&"user1"),
                    Some(true)
                );
            }
        });

        let thread3 = thread::spawn(move || {
            assert_eq!(
                limiter_clone3.lock().unwrap().is_entity_limited(&"user1"),
                Some(true)
            );
        });

        thread1.join().unwrap();
        thread2.join().unwrap();
        thread3.join().unwrap();

        assert_eq!(
            limiter.lock().unwrap().is_entity_limited(&"user1"),
            Some(false)
        );
    }

    #[test]
    fn test_remove_limited_entity() {
        let mut limiter: Limiter<&str> = Limiter::new();
        limiter.add_limited_entity("user1", 5, Duration::from_secs(60));

        {
            let requests = limiter.requests.lock().unwrap();
            assert!(requests.contains_key("user1"));
        }

        let removed_entity_exact = limiter.remove_limited_entity("user1");

        assert!(removed_entity_exact.is_some());
        assert_eq!(removed_entity_exact.unwrap().bucket_max, 5);

        {
            let requests = limiter.requests.lock().unwrap();
            assert!(!requests.contains_key("user1"));
        }

        let removed_non_existent = limiter.remove_limited_entity("unknown_user");
        assert!(removed_non_existent.is_none());

        assert_eq!(limiter.is_entity_limited(&"user1"), None);

        limiter.add_limited_entity("user2", 5, Duration::from_secs(60));
        let borrowed_key: &str = "user2";
        let removed_entity_borrowed = limiter.remove_limited_entity(borrowed_key);

        assert!(removed_entity_borrowed.is_some());
        assert_eq!(removed_entity_borrowed.unwrap().bucket_max, 5);

        {
            let requests = limiter.requests.lock().unwrap();
            assert!(!requests.contains_key("user2"));
        }
    }

    #[test]
    fn test_remove_limited_entity_memory_reuse() {
        let limiter: Limiter<&str> = Limiter::new();

        limiter.add_limited_entity("user1", 5, Duration::from_secs(60));

        {
            let requests = limiter.requests.lock().unwrap();
            assert!(requests.contains_key("user1"));
        }

        let removed_entity = limiter.remove_limited_entity("user1");
        assert!(removed_entity.is_some());

        {
            let requests = limiter.requests.lock().unwrap();
            assert!(!requests.contains_key("user1"));
        }

        limiter.add_limited_entity("user1", 10, Duration::from_secs(120));

        {
            let requests = limiter.requests.lock().unwrap();
            assert!(requests.contains_key("user1"));
            assert_eq!(requests["user1"].bucket_max, 10);
            assert_eq!(requests["user1"].bucket, 10); // Should reflect the new bucket max
        }

        let removed_entity_after_reuse = limiter.remove_limited_entity("user1");
        assert!(removed_entity_after_reuse.is_some());
        assert_eq!(removed_entity_after_reuse.unwrap().bucket_max, 10);

        {
            let requests = limiter.requests.lock().unwrap();
            assert!(!requests.contains_key("user1"));
        }
    }

    #[test]
    fn test_get_bucket_remaining() {
        let mut limiter: Limiter<&str> = Limiter::new();

        assert_eq!(limiter.get_bucket_remaining(&"user1"), None);

        limiter.add_limited_entity("user1", 5, Duration::from_secs(60));

        assert_eq!(limiter.get_bucket_remaining(&"user1"), Some(5));

        limiter.is_entity_limited(&"user1");

        assert_eq!(limiter.get_bucket_remaining(&"user1"), Some(4));

        limiter.is_entity_limited(&"user1");
        limiter.is_entity_limited(&"user1");

        assert_eq!(limiter.get_bucket_remaining(&"user1"), Some(2));
    }
}
