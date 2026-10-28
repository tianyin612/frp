//! 数据模型：Server / Profile / ProxyDef / AppSettings
//!
//! 所有结构体统一 `rename_all = "camelCase"`，与前端 TS 类型一一对应。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyKind {
    Tcp,
    Udp,
    Http,
    Https,
    Stcp,
    Sudp,
    Xtcp,
    Tcpmux,
}

impl ProxyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProxyKind::Tcp => "tcp",
            ProxyKind::Udp => "udp",
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
            ProxyKind::Stcp => "stcp",
            ProxyKind::Sudp => "sudp",
            ProxyKind::Xtcp => "xtcp",
            ProxyKind::Tcpmux => "tcpmux",
        }
    }

    /// 需要 remotePort 的类型
    pub fn needs_remote_port(self) -> bool {
        matches!(self, ProxyKind::Tcp | ProxyKind::Udp)
    }

    /// 需要 secretKey 的类型
    pub fn needs_secret_key(self) -> bool {
        matches!(self, ProxyKind::Stcp | ProxyKind::Sudp | ProxyKind::Xtcp)
    }
}

impl Default for ProxyKind {
    fn default() -> Self {
        ProxyKind::Tcp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxyDef {
    pub id: String,
    pub name: String,
    pub kind: ProxyKind,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_port: Option<u16>,
    pub custom_domains: Vec<String>,
    pub subdomain: Option<String>,
    pub locations: Vec<String>,
    pub secret_key: Option<String>,
    pub plugin: Option<String>,
}

impl Default for ProxyDef {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: "proxy".into(),
            kind: ProxyKind::Tcp,
            local_ip: "127.0.0.1".into(),
            local_port: 80,
            remote_port: Some(6000),
            custom_domains: Vec::new(),
            subdomain: None,
            locations: Vec::new(),
            secret_key: None,
            plugin: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub port: u16,
    /// tcp | kcp | quic | websocket | wss
    pub protocol: String,
    /// token | oidc | none
    pub auth_method: String,
    pub token: String,
    pub tls_enable: bool,
    pub tls_server_name: Option<String>,
    pub tls_insecure: bool,
    pub frps_admin_port: u16,
    pub frps_admin_user: String,
    pub frps_admin_password: String,
    pub remark: String,
    pub subscription_url: Option<String>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: "新服务器".into(),
            addr: "127.0.0.1".into(),
            port: 7000,
            protocol: "tcp".into(),
            auth_method: "token".into(),
            token: String::new(),
            tls_enable: true,
            tls_server_name: None,
            tls_insecure: false,
            frps_admin_port: 7500,
            frps_admin_user: "admin".into(),
            frps_admin_password: String::new(),
            remark: String::new(),
            subscription_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub server_id: String,
    pub auto_start: bool,
    pub admin_port: u16,
    pub log_level: String,
    pub proxies: Vec<ProxyDef>,
    /// 高级逃生舱：一段 TOML，深度合并到生成结果上
    pub raw_override: Option<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: "新配置".into(),
            server_id: String::new(),
            auto_start: false,
            admin_port: 7400,
            log_level: "info".into(),
            proxies: vec![ProxyDef::default()],
            raw_override: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub log_level: String,
    pub log_max_lines: usize,
    pub frpc_path: Option<String>,
    pub default_admin_port: u16,
    pub launch_at_login: bool,
    pub minimize_to_tray: bool,
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            log_level: "info".into(),
            log_max_lines: 2000,
            frpc_path: None,
            default_admin_port: 7400,
            launch_at_login: false,
            minimize_to_tray: false,
            theme: "dark".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoreData {
    pub version: u32,
    pub servers: Vec<Server>,
    pub profiles: Vec<Profile>,
    pub settings: AppSettings,
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            version: 1,
            servers: Vec::new(),
            profiles: Vec::new(),
            settings: AppSettings::default(),
        }
    }
}

impl StoreData {
    pub fn server(&self, id: &str) -> Option<&Server> {
        self.servers.iter().find(|s| s.id == id)
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn upsert_server(&mut self, mut server: Server) -> String {
        if server.id.is_empty() {
            server.id = new_id();
        }
        match self.servers.iter_mut().find(|s| s.id == server.id) {
            Some(slot) => *slot = server.clone(),
            None => self.servers.push(server.clone()),
        }
        server.id
    }

    pub fn upsert_profile(&mut self, mut profile: Profile) -> String {
        if profile.id.is_empty() {
            profile.id = new_id();
        }
        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(slot) => *slot = profile.clone(),
            None => self.profiles.push(profile.clone()),
        }
        profile.id
    }

    pub fn remove_server(&mut self, id: &str) {
        self.servers.retain(|s| s.id != id);
    }

    pub fn remove_profile(&mut self, id: &str) {
        self.profiles.retain(|p| p.id != id);
    }

    /// 复制一份配置，返回新 id
    pub fn duplicate_profile(&mut self, id: &str) -> Option<String> {
        let src = self.profile(id)?.clone();
        let mut copy = src;
        copy.id = new_id();
        copy.name = format!("{} 副本", copy.name);
        copy.auto_start = false;
        for p in copy.proxies.iter_mut() {
            p.id = new_id();
            p.name = format!("{}_copy", p.name);
        }
        let new_id = copy.id.clone();
        self.profiles.push(copy);
        Some(new_id)
    }
}
