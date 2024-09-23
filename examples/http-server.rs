// Hello!
//
// to run this example simply do:
// cargo run --example http-server
//
// After which you can open a new terminal and do:
// curl http://127.0.0.1:3000
//
// Send a bunch of requests through curl and follow what happens!

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use rate_gate::Limiter;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

async fn handle_request(
    _: Request<Body>,
    limiter: Arc<Mutex<Limiter<String>>>,
) -> Result<Response<Body>, Infallible> {
    // Get the IP address of the request (for simplicity, use "127.0.0.1" as a mock)
    let entity_ip = "127.0.0.1".to_string();

    let mut limiter_lock = limiter.lock().unwrap();

    // Check if the entity is rate-limited
    match limiter_lock.is_entity_limited(&entity_ip) {
        Some(true) => Ok(Response::new(Body::from("Request allowed\n"))),
        Some(false) => Ok(Response::new(Body::from("Rate limit exceeded\n"))),
        None => {
            // Add a new entity if it's not already tracked
            limiter_lock.add_limited_entity(entity_ip.clone(), 5, Duration::from_secs(10));
            Ok(Response::new(Body::from("Request allowed (first time)\n")))
        }
    }
}

#[tokio::main]
async fn main() {
    let limiter = Arc::new(Mutex::new(Limiter::new()));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // Create a service that wraps the limiter with the HTTP request handler
    let make_svc = make_service_fn(move |_conn| {
        let limiter = Arc::clone(&limiter);
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                handle_request(req, Arc::clone(&limiter))
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    println!("Listening on http://{}", addr);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}
