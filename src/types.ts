// 与 src-tauri/src/model.rs 一一对应（Rust 侧统一 rename_all = "camelCase"）

export type ProxyKind =
  | "tcp"
  | "udp"
  | "http"
  | "https"
  | "stcp"
  | "sudp"
  | "xtcp"
  | "tcpmux";

export interface ProxyDef {
  id: string;
  name: string;
  kind: ProxyKind;
  localIp: string;
  localPort: number;
  remotePort?: number | null;
  customDomains: string[];
  subdomain?: string | null;
  locations: string[];
  secretKey?: string | null;
  plugin?: string | null;
}

export interface Server {
  id: string;
  name: string;
  addr: string;
  port: number;
  /** tcp | kcp | quic | websocket | wss */
  protocol: string;
  authMethod: string;
  token: string;
  tlsEnable: boolean;
  tlsServerName?: string | null;
  tlsInsecure: boolean;
  frpsAdminPort: number;
  frpsAdminUser: string;
  frpsAdminPassword: string;
  remark: string;
  subscriptionUrl?: string | null;
}

export interface Profile {
  id: string;
  name: string;
  serverId: string;
  autoStart: boolean;
  adminPort: number;
  logLevel: string;
  proxies: ProxyDef[];
  /** 高级逃生舱：一段 TOML，覆盖到生成结果最外层 */
  rawOverride?: string | null;
}

export interface AppSettings {
  logLevel: string;
  logMaxLines: number;
  frpcPath?: string | null;
  defaultAdminPort: number;
  launchAtLogin: boolean;
  minimizeToTray: boolean;
  theme: string;
}

export interface StoreData {
  version: number;
  servers: Server[];
  profiles: Profile[];
  settings: AppSettings;
}

export interface RunningInfo {
  profileId: string;
  pid: number;
  startedAt: number;
  configPath: string;
}

export interface LogLine {
  seq: number;
  ts: number;
  stream: string;
  line: string;
}

export interface FrpcInfo {
  path: string;
  version: string;
  builtin: boolean;
}
