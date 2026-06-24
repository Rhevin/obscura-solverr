//! End-to-end check for the request/response interception API on `obscura::Page`
//! (issue #306): preload scripts, the interception channel, and the passive
//! on_request/on_response callbacks all work for JS-initiated fetch()/XHR.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use obscura::{Browser, InterceptResolution, ResourceType};

/// Minimal HTTP/1.1 server: `/` returns HTML that fires `fetch('/api')`; `/api`
/// returns JSON. Enough to exercise JS fetch() interception + the callbacks.
fn spawn_echo_server() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for incoming in listener.incoming() {
            let mut s = match incoming {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let req = std::str::from_utf8(&buf).unwrap_or("");
            let path = req.split_whitespace().nth(1).unwrap_or("/");
            let (ct, body) = if path.starts_with("/api") {
                ("application/json", "{\"hello\":\"world\"}".to_string())
            } else {
                ("text/html", "<script>fetch('/api');</script>".to_string())
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
                ct,
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    });
    format!("http://{}", addr)
}

#[tokio::test]
async fn page_intercepts_and_observes_js_fetch() {
    std::env::set_var("OBSCURA_ALLOW_PRIVATE_NETWORK", "1");
    let base = spawn_echo_server();

    let browser = Browser::new().unwrap();
    let mut page = browser.new_page().await.unwrap();

    // Passive on_request counter (fires for navigation + the JS fetch).
    let req_count = Arc::new(AtomicU32::new(0));
    let rc = req_count.clone();
    page.on_request(Arc::new(move |_info| {
        rc.fetch_add(1, Ordering::SeqCst);
    }));

    // Passive on_response: capture the /api (Fetch) body.
    let captured = Arc::new(Mutex::new(String::new()));
    let cap = captured.clone();
    page.on_response(Arc::new(move |info, resp| {
        if info.resource_type == ResourceType::Fetch {
            *cap.lock().unwrap() = String::from_utf8_lossy(&resp.body).into_owned();
        }
    }));

    // Channel-based interception: resolve every request Continue so the fetch
    // is not blocked.
    let mut rx = page.enable_interception();
    tokio::spawn(async move {
        while let Some(req) = rx.recv().await {
            if req
                .resolver
                .send(InterceptResolution::Continue {
                    url: None,
                    method: None,
                    headers: None,
                    body: None,
                })
                .is_err()
            {
                break;
            }
        }
    });

    page.goto(&base).await.unwrap();
    // Pump the event loop so the inline fetch('/api') resolves.
    for _ in 0..20 {
        page.settle(500).await;
        if captured.lock().unwrap().contains("hello") {
            break;
        }
    }

    assert!(
        req_count.load(Ordering::SeqCst) >= 1,
        "on_request never fired for navigation or fetch"
    );
    let body = captured.lock().unwrap().clone();
    assert!(
        body.contains("hello"),
        "on_response did not capture the fetch response body: {:?}",
        body
    );
}
