# RateGate

[![build & test](https://github.com/freergit/rate-limiter/actions/workflows/ci.yml/badge.svg)](https://github.com/freergit/rate-limiter/actions/workflows/ci.yml)
[![license](https://img.shields.io/github/license/freergit/rate-limiter)](https://github.com/freergit/rate-limiter/blob/main/LICENSE.txt)

A simple rate limiter using the bucket algorithm, it's thread safe and easy to use. For example usage look in `/examples`.

```rust
fn main() {
    // Create a new rate limiter
    let mut rate_limiter: Limiter<&str> = Limiter::new();

    // Add two users to the limiter with different limits
    rate_limiter.add_limited_entity("user1", 5, Duration::from_secs(5));
    rate_limiter.add_limited_entity("user2", 3, Duration::from_secs(5));

    // Simulate API requests for each user
    for i in 1..=6 {
        println!("Request {} from user1", i);
        match rate_limiter.is_entity_limited(&"user1") {
            Some(true) => println!("Request allowed for user1"),
            Some(false) => println!("Rate limit reached for user1"),
            None => println!("User1 not found in limiter"),
        }

        println!("Request {} from user2", i);
        match rate_limiter.is_entity_limited(&"user2") {
            Some(true) => println!("Request allowed for user2"),
            Some(false) => println!("Rate limit reached for user2"),
            None => println!("User2 not found in limiter"),
        }

        // Simulate a delay between requests
        thread::sleep(Duration::from_secs(1));
    }

    // Wait for user1's bucket to refill
    println!("\nWaiting for user1's limit to reset...\n");
    thread::sleep(Duration::from_secs(5));

    // Retry requests for user1 after the refresh time has passed
    for i in 1..=3 {
        println!("Request {} from user1 after refresh", i);
        match rate_limiter.is_entity_limited(&"user1") {
            Some(true) => println!("Request allowed for user1"),
            Some(false) => println!("Rate limit reached for user1"),
            None => println!("User1 not found in limiter"),
        }
    }
}

```