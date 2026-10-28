//! frpc / frps 管理 API 客户端。
//!
//! frpc 本地：GET /api/status、POST /api/reload、POST /api/stop
//! frps 看板：GET /api/serverinfo、GET /api/proxy/{type}
//! 两边都是 HTTP Basic Auth。

use serde_json::Value;
use std::time::Duration;

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

pub fn base(addr: &str, port: u16) -> String {
    let host = if addr.trim().is_empty() {
        "127.0.0.1"
    } else {
        addr.trim()
    };
    format!("http://{host}:{port}")
}

pub async fn get(
    client: &reqwest::Client,
    url: &str,
    user: &str,
    pass: &str,
) -> Result<Value, String> {
    let mut req = client.get(url).timeout(Duration::from_secs(8));
    if !user.is_empty() {
        req = req.basic_auth(user, Some(pass));
    }
    let resp = req.send().await.map_err(|e| format!("请求 {url} 失败：{e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "{url} 返回 HTTP {status}：{}",
            truncate(&text, 200)
        ));
    }
    serde_json::from_str(&text).map_err(|e| format!("{url} 返回的不是 JSON：{e}"))
}

pub async fn post_json(
    client: &reqwest::Client,
    url: &str,
    user: &str,
    pass: &str,
    body: &Value,
) -> Result<Value, String> {
    let mut req = client
        .post(url)
        .json(body)
        .timeout(Duration::from_secs(8));
    if !user.is_empty() {
        req = req.basic_auth(user, Some(pass));
    }
    let resp = req.send().await.map_err(|e| format!("请求 {url} 失败：{e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "{url} 返回 HTTP {status}：{}",
            truncate(&text, 200)
        ));
    }
    // reload 之类可能返回空 body
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| format!("{url} 返回的不是 JSON：{e}"))
}
