//! The updater against a local "release": a signed file and its latest.json, as the release workflow
//! writes them, with a key made for the test (lider_core::update).

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use lider_core::http::HttpClient;
use lider_core::tools::Progress;
use lider_core::update;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// Serve these paths (anything else is a 404).
async fn serve(files: HashMap<String, Vec<u8>>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let files = Arc::new(files);
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = l.accept().await else { return };
            let files = files.clone();
            tokio::spawn(async move {
                let (r, mut w) = s.into_split();
                let mut r = BufReader::new(r);
                let mut line = String::new();
                r.read_line(&mut line).await.unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).await.unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                }
                let resp = match files.get(&path) {
                    Some(body) => {
                        let mut v = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        v.extend_from_slice(body);
                        v
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = w.write_all(&resp).await;
            });
        }
    });
    base
}

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

/// A key pair: (public key as LID stores it, a signer for data).
fn key() -> (String, impl Fn(&[u8]) -> String) {
    let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let pk = b64(&kp.pk.to_box().unwrap().to_string());
    let sk = kp.sk;
    let sign = move |data: &[u8]| {
        let sig = minisign::sign(None, &sk, std::io::Cursor::new(data), None, None).unwrap();
        b64(&sig.to_string())
    };
    (pk, sign)
}

fn release(base: &str, file: &[u8], signature: &str) -> HashMap<String, Vec<u8>> {
    let manifest = serde_json::json!({
        "version": "9.0.0",
        "notes": "",
        "platforms": {
            update::platform_key(): {"url": format!("{base}/LID-9.0.0.AppImage"), "signature": signature}
        }
    });
    HashMap::from([
        (
            "/latest.json".to_string(),
            manifest.to_string().into_bytes(),
        ),
        ("/LID-9.0.0.AppImage".to_string(), file.to_vec()),
    ])
}

#[tokio::test]
async fn a_signed_update_downloads_and_a_wrong_signature_is_refused() {
    let (pk, sign) = key();
    let data = b"\x7fELF the new LID".repeat(1000);
    let sig = sign(&data);
    let base = serve_release(&data, &sig).await;
    let client = HttpClient::new("lider-test", "", 5.0).unwrap();
    let progress = Progress::default();
    let (version, got) = update::download(&client, &format!("{base}/latest.json"), &pk, &progress)
        .await
        .unwrap();
    assert_eq!(version, "9.0.0");
    assert_eq!(got, data);
    assert_eq!(
        progress.done.load(std::sync::atomic::Ordering::Relaxed),
        data.len() as i64
    );

    // Signed by another key: refused.
    let (_, other) = key();
    let base = serve_release(&data, &other(&data)).await;
    let err = update::download(&client, &format!("{base}/latest.json"), &pk, &progress)
        .await
        .unwrap_err();
    assert_eq!(err, "the update is not signed with LID's key");

    // The file changed after signing: refused.
    let base = serve_release(b"\x7fELF something else", &sig).await;
    assert!(
        update::download(&client, &format!("{base}/latest.json"), &pk, &progress)
            .await
            .is_err()
    );
}

/// A release whose latest.json points back at the same server.
async fn serve_release(file: &[u8], signature: &str) -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let base = format!("http://{addr}");
    let files = Arc::new(release(&base, file, signature));
    let l = TcpListener::from_std({
        l.set_nonblocking(true).unwrap();
        l
    })
    .unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = l.accept().await else { return };
            let files = files.clone();
            tokio::spawn(async move {
                let (r, mut w) = s.into_split();
                let mut r = BufReader::new(r);
                let mut line = String::new();
                r.read_line(&mut line).await.unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).await.unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                }
                let body = files.get(&path).cloned();
                let mut resp = match &body {
                    Some(b) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        b.len()
                    ),
                    None => {
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .into()
                    }
                }
                .into_bytes();
                if let Some(b) = body {
                    resp.extend_from_slice(&b);
                }
                let _ = w.write_all(&resp).await;
            });
        }
    });
    base
}

#[tokio::test]
async fn the_releases_list_gives_the_newer_version() {
    let list = serde_json::json!([
        {"tag_name": "v99.0.0", "prerelease": false, "draft": false, "html_url": "https://x/v99", "body": "big",
         "assets": [{"name": "latest.json", "browser_download_url": "https://x/latest.json"}]},
        {"tag_name": "v1.0.0", "prerelease": false, "draft": false, "html_url": "https://x/v1", "body": "", "assets": []}
    ]);
    let base = serve(HashMap::from([(
        "/releases".to_string(),
        list.to_string().into_bytes(),
    )]))
    .await;
    let client = HttpClient::new("lider-test", "", 5.0).unwrap();
    let f = update::find(&client, &format!("{base}/releases"), "stable", true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f.version, "99.0.0");
    assert!(f.can_install);
    let f = update::find(&client, &format!("{base}/releases"), "stable", false)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !f.can_install,
        "a package manager's copy only gets the page"
    );
    assert!(
        update::find(&client, &format!("{base}/nothing"), "stable", true)
            .await
            .is_err()
    );
}
