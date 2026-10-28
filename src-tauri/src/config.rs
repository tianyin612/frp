//! frpc.toml 生成（0.52+ 新格式）与导入解析。
//!
//! 生成策略：手搓 `toml::Table` 而不是直接 serde 序列化模型，
//! 好处是能精确控制"该省略的字段就省略"（frpc 对空值很敏感），
//! 也方便把 `Profile.rawOverride` 深度合并进来。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use toml::map::Map;
use toml::Value;

use crate::model::{new_id, AppSettings, Profile, ProxyDef, ProxyKind, Server};

fn vs(s: impl Into<String>) -> Value {
    Value::String(s.into())
}

fn vi(n: u16) -> Value {
    Value::Integer(n as i64)
}

pub fn admin_port(p: &Profile, settings: &AppSettings) -> u16 {
    if p.admin_port == 0 {
        settings.default_admin_port
    } else {
        p.admin_port
    }
}

/// frpc 本地 webServer 的 Basic Auth 凭据。程序内部自己用，所以用确定性哈希即可。
pub fn admin_credentials(profile_id: &str, token: &str) -> (String, String) {
    let mut h = DefaultHasher::new();
    profile_id.hash(&mut h);
    token.hash(&mut h);
    ("admin".to_string(), format!("{:016x}", h.finish()))
}

fn proxy_value(p: &ProxyDef) -> Value {
    let mut t = Map::new();
    t.insert("name".into(), vs(p.name.clone()));
    t.insert("type".into(), vs(p.kind.as_str()));
    let ip = if p.local_ip.trim().is_empty() {
        "127.0.0.1"
    } else {
        p.local_ip.trim()
    };
    t.insert("localIP".into(), vs(ip));
    t.insert("localPort".into(), vi(p.local_port));

    if p.kind.needs_remote_port() {
        if let Some(rp) = p.remote_port {
            t.insert("remotePort".into(), vi(rp));
        }
    }
    if p.kind.needs_secret_key() {
        if let Some(k) = p.secret_key.as_ref().filter(|v| !v.trim().is_empty()) {
            t.insert("secretKey".into(), vs(k.trim()));
        }
    }
    if !p.custom_domains.is_empty() {
        t.insert(
            "customDomains".into(),
            Value::Array(
                p.custom_domains
                    .iter()
                    .map(|d| vs(d.trim()))
                    .filter(|v| !matches!(v, Value::String(s) if s.is_empty()))
                    .collect(),
            ),
        );
    }
    if let Some(sub) = p.subdomain.as_ref().filter(|v| !v.trim().is_empty()) {
        t.insert("subdomain".into(), vs(sub.trim()));
    }
    if !p.locations.is_empty() {
        t.insert(
            "locations".into(),
            Value::Array(p.locations.iter().map(|l| vs(l.trim())).collect()),
        );
    }
    if let Some(plugin) = p.plugin.as_ref().filter(|v| !v.trim().is_empty()) {
        t.insert("plugin".into(), vs(plugin.trim()));
    }
    Value::Table(t)
}

fn deep_merge(base: &mut Map<String, Value>, over: Map<String, Value>) {
    for (k, v) in over {
        // 两边都是表 → 递归合并；否则直接覆盖
        let v = match v {
            Value::Table(o) => {
                if let Some(Value::Table(b)) = base.get_mut(&k) {
                    deep_merge(b, o);
                    continue;
                }
                Value::Table(o)
            }
            other => other,
        };
        base.insert(k, v);
    }
}

/// 生成 frpc.toml 文本
pub fn build(p: &Profile, srv: &Server, settings: &AppSettings) -> Result<String, String> {
    if srv.addr.trim().is_empty() {
        return Err("服务器地址为空".into());
    }

    let mut root = Map::new();
    root.insert("serverAddr".into(), vs(srv.addr.trim()));
    root.insert("serverPort".into(), vi(srv.port));

    // transport
    let mut transport = Map::new();
    transport.insert("protocol".into(), vs(srv.protocol.clone()));
    let mut tls = Map::new();
    tls.insert("enable".into(), Value::Boolean(srv.tls_enable));
    if let Some(sn) = srv.tls_server_name.as_ref().filter(|v| !v.trim().is_empty()) {
        tls.insert("serverName".into(), vs(sn.trim()));
    }
    if srv.tls_insecure {
        tls.insert("insecureSkipVerify".into(), Value::Boolean(true));
    }
    transport.insert("tls".into(), Value::Table(tls));
    root.insert("transport".into(), Value::Table(transport));

    // auth
    if srv.auth_method != "none" {
        let mut auth = Map::new();
        auth.insert("method".into(), vs(srv.auth_method.clone()));
        if !srv.token.is_empty() {
            auth.insert("token".into(), vs(srv.token.clone()));
        }
        root.insert("auth".into(), Value::Table(auth));
    }

    // log：一律输出到 console，由父进程接管管道
    let mut log = Map::new();
    log.insert("to".into(), vs("console"));
    log.insert(
        "level".into(),
        vs(if p.log_level.trim().is_empty() {
            settings.log_level.clone()
        } else {
            p.log_level.clone()
        }),
    );
    log.insert("maxDays".into(), Value::Integer(3));
    root.insert("log".into(), Value::Table(log));

    // webServer：本地管理 API，只监听回环
    let mut web = Map::new();
    web.insert("addr".into(), vs("127.0.0.1"));
    web.insert("port".into(), vi(admin_port(p, settings)));
    let (user, pass) = admin_credentials(&p.id, &srv.token);
    web.insert("user".into(), vs(user));
    web.insert("password".into(), vs(pass));
    root.insert("webServer".into(), Value::Table(web));

    // proxies
    let proxies: Vec<Value> = p.proxies.iter().map(proxy_value).collect();
    if !proxies.is_empty() {
        root.insert("proxies".into(), Value::Array(proxies));
    }

    // 高级覆盖
    if let Some(raw) = p.raw_override.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        match toml::from_str::<Value>(raw) {
            Ok(Value::Table(over)) => deep_merge(&mut root, over),
            Ok(_) => return Err("高级覆盖必须是 TOML 表（键值对）".into()),
            Err(e) => return Err(format!("高级覆盖 TOML 语法错误: {e}")),
        }
    }

    let body = toml::to_string_pretty(&Value::Table(root)).map_err(|e| e.to_string())?;
    Ok(format!(
        "# generated by frpc-desktop, do not edit manually\n# profile: {}\n{}",
        p.name, body
    ))
}

/* ------------------------------------------------------------ TOML 导入 */

fn kind_from_str(s: &str) -> ProxyKind {
    match s.to_ascii_lowercase().as_str() {
        "udp" => ProxyKind::Udp,
        "http" => ProxyKind::Http,
        "https" => ProxyKind::Https,
        "stcp" => ProxyKind::Stcp,
        "sudp" => ProxyKind::Sudp,
        "xtcp" => ProxyKind::Xtcp,
        "tcpmux" => ProxyKind::Tcpmux,
        _ => ProxyKind::Tcp,
    }
}

/// 把一份现成的 frpc.toml 解析成 (Server, Profile)
pub fn parse_frpc_toml(text: &str) -> Result<(Server, Profile), String> {
    let value: Value = toml::from_str(text).map_err(|e| format!("TOML 解析失败: {e}"))?;
    let t = value.as_table().ok_or("配置文件顶层必须是表")?;

    let mut srv = Server::default();
    srv.name = "导入的服务器".into();
    if let Some(v) = t.get("serverAddr").and_then(|v| v.as_str()) {
        srv.addr = v.to_string();
    }
    if let Some(v) = t.get("serverPort").and_then(|v| v.as_integer()) {
        srv.port = v as u16;
    }
    if let Some(auth) = t.get("auth").and_then(|v| v.as_table()) {
        if let Some(v) = auth.get("method").and_then(|v| v.as_str()) {
            srv.auth_method = v.to_string();
        }
        if let Some(v) = auth.get("token").and_then(|v| v.as_str()) {
            srv.token = v.to_string();
        }
    }
    if let Some(tr) = t.get("transport").and_then(|v| v.as_table()) {
        if let Some(v) = tr.get("protocol").and_then(|v| v.as_str()) {
            srv.protocol = v.to_string();
        }
        if let Some(tls) = tr.get("tls").and_then(|v| v.as_table()) {
            if let Some(v) = tls.get("enable").and_then(|v| v.as_bool()) {
                srv.tls_enable = v;
            }
            if let Some(v) = tls.get("serverName").and_then(|v| v.as_str()) {
                srv.tls_server_name = Some(v.to_string());
            }
        }
    }

    let mut prof = Profile::default();
    prof.name = "导入的配置".into();
    prof.server_id = srv.id.clone();
    prof.proxies.clear();

    if let Some(arr) = t.get("proxies").and_then(|v| v.as_array()) {
        for (idx, item) in arr.iter().enumerate() {
            let Some(pt) = item.as_table() else { continue };
            let mut p = ProxyDef::default();
            p.id = new_id();
            p.name = pt
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("proxy{idx}"));
            p.kind = kind_from_str(pt.get("type").and_then(|v| v.as_str()).unwrap_or("tcp"));
            if let Some(v) = pt.get("localIP").and_then(|v| v.as_str()) {
                p.local_ip = v.to_string();
            }
            if let Some(v) = pt.get("localPort").and_then(|v| v.as_integer()) {
                p.local_port = v as u16;
            }
            p.remote_port = pt.get("remotePort").and_then(|v| v.as_integer()).map(|v| v as u16);
            if let Some(v) = pt.get("subdomain").and_then(|v| v.as_str()) {
                p.subdomain = Some(v.to_string());
            }
            if let Some(v) = pt.get("secretKey").and_then(|v| v.as_str()) {
                p.secret_key = Some(v.to_string());
            }
            if let Some(v) = pt.get("plugin").and_then(|v| v.as_str()) {
                p.plugin = Some(v.to_string());
            }
            if let Some(arr) = pt.get("customDomains").and_then(|v| v.as_array()) {
                p.custom_domains = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
            }
            prof.proxies.push(p);
        }
    }

    Ok((srv, prof))
}
