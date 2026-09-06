//! Temporary live probe: replicate DeepSeek /users/current via reqwest.
//! Run: cargo test -p cipherroute --test live_deepseek_probe -- --ignored --nocapture
//! DELETE THIS FILE after diagnosis.

const TOKEN: &str = "olUA3kbKmEqrSpix1msiDQRK/IVir+dEULPfFurs/4+OCujgCKYaAKW7+ToTN5uT";

fn fake_headers() -> Vec<(&'static str, String)> {
    vec![
        ("Accept", "*/*".to_string()),
        ("Accept-Encoding", "identity".to_string()),
        ("Accept-Language", "en-US,en;q=0.9".to_string()),
        ("Origin", "https://chat.deepseek.com".to_string()),
        ("Referer", "https://chat.deepseek.com/".to_string()),
        (
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36".to_string(),
        ),
        ("X-Client-Bundle-Id", "com.deepseek.chat".to_string()),
        ("X-Client-Locale", "en-US".to_string()),
        ("X-Client-Platform", "web".to_string()),
        ("X-Client-Version", "2.0.0".to_string()),
    ]
}

#[tokio::test]
#[ignore]
async fn probe_http1_full_headers() {
    let client = reqwest::Client::builder()
        .http1_only()
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap();
    let mut req = client
        .get("https://chat.deepseek.com/api/v0/users/current")
        .header("Authorization", format!("Bearer {TOKEN}"));
    for (k, v) in fake_headers() {
        req = req.header(k, v);
    }
    let resp = req.send().await.unwrap();
    println!(
        "HTTP1+FULL: http={:?} status={}",
        resp.version(),
        resp.status()
    );
    println!(
        "HTTP1+FULL: body={}",
        &resp.text().await.unwrap()[..200.min(usize::MAX).min(100000)]
    );
}

#[tokio::test]
#[ignore]
async fn probe_http1_minimal_headers() {
    let client = reqwest::Client::builder()
        .http1_only()
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap();
    let resp = client
        .get("https://chat.deepseek.com/api/v0/users/current")
        .header("Authorization", format!("Bearer {TOKEN}"))
        .header("Accept", "*/*")
        .header("X-Client-Bundle-Id", "com.deepseek.chat")
        .header("X-Client-Version", "2.0.0")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36")
        .send()
        .await
        .unwrap();
    println!(
        "HTTP1+MIN: http={:?} status={}",
        resp.version(),
        resp.status()
    );
    let body = resp.text().await.unwrap();
    println!("HTTP1+MIN: body={}", &body[..body.len().min(200)]);
}

#[tokio::test]
#[ignore]
async fn probe_default_client_full_headers() {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap();
    let mut req = client
        .get("https://chat.deepseek.com/api/v0/users/current")
        .header("Authorization", format!("Bearer {TOKEN}"));
    for (k, v) in fake_headers() {
        req = req.header(k, v);
    }
    let resp = req.send().await.unwrap();
    println!(
        "DEFAULT+H2?: http={:?} status={}",
        resp.version(),
        resp.status()
    );
    let body = resp.text().await.unwrap();
    println!("DEFAULT: body={}", &body[..body.len().min(200)]);
}
