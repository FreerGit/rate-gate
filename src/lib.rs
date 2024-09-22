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

    /// Checks whether a entity has requests left to consume.
    ///
    /// `entity` has been added by you previously with `add_limited_entity`
    ///
    /// ### returns:
    ///
    /// `None` -> entity was not found by the limiter, create one.
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

        assert_eq!(limiter.is_entity_limited(&"user1"), Some(true)); // 1st request
        assert_eq!(limiter.is_entity_limited(&"user1"), Some(true)); // 2nd request
        assert_eq!(limiter.is_entity_limited(&"user1"), Some(false)); // 3rd request, should be limited
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

        // Wait for all threads to finish
        thread1.join().unwrap();
        thread2.join().unwrap();
        thread3.join().unwrap();

        // After 5 requests, the next request should be limited
        assert_eq!(
            limiter.lock().unwrap().is_entity_limited(&"user1"),
            Some(false)
        );
    }
}
