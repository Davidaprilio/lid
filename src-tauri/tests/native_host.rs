//! The real program in browser-helper mode (1.x test_native_host.py `test_host_process_speaks_native_messaging`
//! and `test_native_host_does_not_load_qt`: here, the helper runs without any display, so it starts no GUI).

use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn frame(v: &serde_json::Value) -> Vec<u8> {
    let body = v.to_string().into_bytes();
    let mut out = (body.len() as u32).to_le_bytes().to_vec();
    out.extend(body);
    out
}

fn replies(mut data: &[u8]) -> Vec<serde_json::Value> {
    let mut out = vec![];
    while data.len() >= 4 {
        let n = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
        out.push(serde_json::from_slice(&data[4..4 + n]).unwrap());
        data = &data[4 + n..];
    }
    out
}

#[test]
fn host_process_speaks_native_messaging_without_a_display() {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lider"))
        .arg("--native-host")
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env(
            "LIDER_SOCKET_NAME",
            format!("lider-host-test-{}", std::process::id()),
        )
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(&frame(&serde_json::json!({"type": "hello"})))
            .unwrap();
        stdin
            .write_all(&frame(&serde_json::json!({"type": "format-disk"})))
            .unwrap();
    } // EOF ends the host
    let mut out = vec![];
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    let status = child.wait().unwrap();
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let r = replies(&out);
    assert_eq!(r.len(), 2, "two replies; stderr: {err}");
    assert_eq!(r[0]["ok"], true);
    assert!(r[0]["types"].as_array().unwrap().iter().any(|t| t == "ZIP"));
    assert_eq!(r[1]["ok"], false);
    assert!(status.success(), "exit code 0 at end of input");
    assert!(
        !err.to_lowercase().contains("gtk"),
        "no GUI toolkit is touched: {err}"
    );
}
