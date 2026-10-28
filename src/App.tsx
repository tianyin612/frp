import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { api } from "./api";
import type {
  FrpcInfo,
  LogLine,
  Profile,
  ProxyDef,
  ProxyKind,
  RunningInfo,
  Server,
  StoreData,
} from "./types";

/* ---------------------------------------------------------------- helpers */

const PROXY_KINDS: ProxyKind[] = [
  "tcp",
  "udp",
  "http",
  "https",
  "stcp",
  "sudp",
  "xtcp",
  "tcpmux",
];

const uid = () =>
  `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

const blankServer = (): Server => ({
  id: uid(),
  name: "新服务器",
  addr: "127.0.0.1",
  port: 7000,
  protocol: "tcp",
  authMethod: "token",
  token: "",
  tlsEnable: true,
  tlsServerName: "",
  tlsInsecure: false,
  frpsAdminPort: 7500,
  frpsAdminUser: "admin",
  frpsAdminPassword: "",
  remark: "",
  subscriptionUrl: "",
});

const blankProxy = (): ProxyDef => ({
  id: uid(),
  name: "ssh",
  kind: "tcp",
  localIp: "127.0.0.1",
  localPort: 22,
  remotePort: 6000,
  customDomains: [],
  subdomain: "",
  locations: [],
  secretKey: "",
  plugin: "",
});

const blankProfile = (serverId: string): Profile => ({
  id: uid(),
  name: "新配置",
  serverId,
  autoStart: false,
  adminPort: 7400,
  logLevel: "info",
  proxies: [blankProxy()],
  rawOverride: "",
});

const fmtTime = (sec: number) =>
  new Date(sec * 1000).toLocaleTimeString("zh-CN", { hour12: false });

function useToast() {
  const [msg, setMsg] = useState<{ text: string; kind: string } | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const show = useCallback((text: string, kind: "ok" | "err" = "ok") => {
    setMsg({ text, kind });
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMsg(null), 5000);
  }, []);
  return { msg, show };
}

/* ------------------------------------------------------------------ fields */

function Field(props: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="field">
      <label>{props.label}</label>
      {props.children}
      {props.hint ? <span className="hint">{props.hint}</span> : null}
    </div>
  );
}

/* ------------------------------------------------------------------- proxy */

function ProxyRow(props: {
  proxy: ProxyDef;
  onChange: (p: ProxyDef) => void;
  onRemove: () => void;
}) {
  const { proxy, onChange, onRemove } = props;
  const set = (patch: Partial<ProxyDef>) => onChange({ ...proxy, ...patch });
  const needsRemotePort = ["tcp", "udp"].includes(proxy.kind);
  const needsDomain = ["http", "https", "tcpmux"].includes(proxy.kind);
  const needsSecret = ["stcp", "sudp", "xtcp"].includes(proxy.kind);

  return (
    <div className="card" style={{ marginBottom: 10, padding: 12 }}>
      <div className="grid three">
        <Field label="名称">
          <input value={proxy.name} onChange={(e) => set({ name: e.target.value })} />
        </Field>
        <Field label="类型">
          <select
            value={proxy.kind}
            onChange={(e) => set({ kind: e.target.value as ProxyKind })}
          >
            {PROXY_KINDS.map((k) => (
              <option key={k} value={k}>
                {k}
              </option>
            ))}
          </select>
        </Field>
        <Field label="本地端口">
          <input
            type="number"
            value={proxy.localPort}
            onChange={(e) => set({ localPort: Number(e.target.value) || 0 })}
          />
        </Field>

        <Field label="本地地址">
          <input
            value={proxy.localIp}
            onChange={(e) => set({ localIp: e.target.value })}
          />
        </Field>

        {needsRemotePort ? (
          <Field label="远程端口" hint="frps 上对外开放的端口">
            <input
              type="number"
              value={proxy.remotePort ?? 0}
              onChange={(e) => set({ remotePort: Number(e.target.value) || 0 })}
            />
          </Field>
        ) : (
          <Field label="子域名" hint="需 frps 配置 subDomainHost">
            <input
              value={proxy.subdomain ?? ""}
              onChange={(e) => set({ subdomain: e.target.value })}
            />
          </Field>
        )}

        {needsDomain ? (
          <Field label="自定义域名" hint="多个用逗号分隔">
            <input
              value={proxy.customDomains.join(",")}
              onChange={(e) =>
                set({
                  customDomains: e.target.value
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean),
                })
              }
            />
          </Field>
        ) : null}

        {needsSecret ? (
          <Field label="secretKey">
            <input
              value={proxy.secretKey ?? ""}
              onChange={(e) => set({ secretKey: e.target.value })}
            />
          </Field>
        ) : null}

        <Field label="插件" hint="可选：socks5 / http_proxy / static_file">
          <input
            value={proxy.plugin ?? ""}
            onChange={(e) => set({ plugin: e.target.value })}
          />
        </Field>
      </div>
      <div className="row end" style={{ marginTop: 10 }}>
        <button className="ghost danger" onClick={onRemove}>
          删除此映射
        </button>
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- profile */

function ProfileEditor(props: {
  profile: Profile;
  servers: Server[];
  running: boolean;
  info?: RunningInfo;
  onSave: (p: Profile) => void;
  onStart: () => void;
  onStop: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
  show: (t: string, k?: "ok" | "err") => void;
}) {
  const { profile, servers, running, info, onSave, show } = props;
  const [draft, setDraft] = useState<Profile>(profile);
  const [preview, setPreview] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => setDraft(profile), [profile.id]);

  const set = (patch: Partial<Profile>) => setDraft({ ...draft, ...patch });
  const dirty = JSON.stringify(draft) !== JSON.stringify(profile);

  const refreshPreview = async () => {
    if (dirty) {
      setPreview("（先保存再预览）");
      return;
    }
    try {
      setPreview(await api.previewConfig(profile.id));
    } catch (e) {
      setPreview(String(e));
    }
  };

  useEffect(() => {
    void refreshPreview();
  }, [profile, running]);

  const verify = async () => {
    setBusy(true);
    try {
      const out = await api.verifyProfile(profile.id);
      show(`配置校验通过${out ? `：${out}` : ""}`, "ok");
    } catch (e) {
      show(String(e), "err");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="card">
        <h3>
          基本
          <small>
            {running ? `运行中 · pid ${info?.pid ?? "-"} · ${fmtTime(info?.startedAt ?? 0)} 启动` : "未运行"}
          </small>
        </h3>
        <div className="grid">
          <Field label="配置名称">
            <input value={draft.name} onChange={(e) => set({ name: e.target.value })} />
          </Field>
          <Field label="归属服务器">
            <select
              value={draft.serverId}
              onChange={(e) => set({ serverId: e.target.value })}
            >
              <option value="">— 未选择 —</option>
              {servers.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name} ({s.addr}:{s.port})
                </option>
              ))}
            </select>
          </Field>
          <Field label="frpc 管理端口" hint="本地 webServer，用于状态查询/热加载">
            <input
              type="number"
              value={draft.adminPort}
              onChange={(e) => set({ adminPort: Number(e.target.value) || 0 })}
            />
          </Field>
          <Field label="日志级别">
            <select
              value={draft.logLevel}
              onChange={(e) => set({ logLevel: e.target.value })}
            >
              {["trace", "debug", "info", "warn", "error"].map((l) => (
                <option key={l} value={l}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <div className="row" style={{ marginTop: 12 }}>
          <label className="check">
            <input
              type="checkbox"
              checked={draft.autoStart}
              onChange={(e) => set({ autoStart: e.target.checked })}
            />
            应用启动时自动运行
          </label>
        </div>
      </div>

      <div className="card">
        <h3>
          端口映射
          <small>{draft.proxies.length} 条</small>
        </h3>
        {draft.proxies.map((p, i) => (
          <ProxyRow
            key={p.id || i}
            proxy={p}
            onChange={(next) =>
              set({ proxies: draft.proxies.map((x, j) => (j === i ? next : x)) })
            }
            onRemove={() =>
              set({ proxies: draft.proxies.filter((_, j) => j !== i) })
            }
          />
        ))}
        <button onClick={() => set({ proxies: [...draft.proxies, blankProxy()] })}>
          + 添加映射
        </button>
      </div>

      <div className="card">
        <h3>高级覆盖（TOML 片段）</h3>
        <Field
          label="会合并到生成配置的最外层"
          hint="例如 transport.poolCount = 5 / [[proxies]] 扩展字段；留空即可"
        >
          <textarea
            rows={4}
            value={draft.rawOverride ?? ""}
            onChange={(e) => set({ rawOverride: e.target.value })}
          />
        </Field>
      </div>

      <div className="row" style={{ marginBottom: 18 }}>
        <button
          className="primary"
          disabled={!dirty}
          onClick={() => onSave(draft)}
        >
          保存配置
        </button>
        <button disabled={busy} onClick={verify}>
          校验配置
        </button>
        {running ? (
          <button className="danger" onClick={props.onStop}>
            停止
          </button>
        ) : (
          <button
            onClick={props.onStart}
            disabled={!profile.serverId}
            title={!profile.serverId ? "先选择服务器" : ""}
          >
            启动
          </button>
        )}
        <button onClick={props.onDuplicate}>复制</button>
        <button className="ghost danger" onClick={props.onDelete}>
          删除
        </button>
      </div>

      <div className="card">
        <h3>生成的 frpc.toml</h3>
        <pre className="code">{preview}</pre>
      </div>
    </>
  );
}

/* ---------------------------------------------------------------- servers */

function ServersPanel(props: {
  servers: Server[];
  onSave: (s: Server) => void;
  onDelete: (id: string) => void;
  show: (t: string, k?: "ok" | "err") => void;
}) {
  const [draft, setDraft] = useState<Server>(blankServer());
  const [dashboard, setDashboard] = useState("");

  const set = (patch: Partial<Server>) => setDraft({ ...draft, ...patch });

  return (
    <>
      <div className="card">
        <h3>服务器列表</h3>
        <table>
          <thead>
            <tr>
              <th>名称</th>
              <th>地址</th>
              <th>协议</th>
              <th>TLS</th>
              <th>备注</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {props.servers.map((s) => (
              <tr key={s.id}>
                <td>{s.name}</td>
                <td className="mono">
                  {s.addr}:{s.port}
                </td>
                <td>{s.protocol}</td>
                <td>{s.tlsEnable ? "开" : "关"}</td>
                <td style={{ color: "var(--muted)" }}>{s.remark}</td>
                <td>
                  <div className="row">
                    <button className="icon" onClick={() => setDraft(s)}>
                      编辑
                    </button>
                    <button
                      className="icon"
                      onClick={async () => {
                        try {
                          const info = await api.frpsDashboard(s.id);
                          setDashboard(JSON.stringify(info, null, 2));
                        } catch (e) {
                          props.show(String(e), "err");
                        }
                      }}
                    >
                      看板
                    </button>
                    <button
                      className="icon danger"
                      onClick={() => props.onDelete(s.id)}
                    >
                      删除
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {props.servers.length === 0 && (
              <tr>
                <td colSpan={6} className="empty">
                  还没有服务器，先在下面添加一台
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="card">
        <h3>{props.servers.some((s) => s.id === draft.id) ? "编辑服务器" : "新增服务器"}</h3>
        <div className="grid">
          <Field label="名称">
            <input value={draft.name} onChange={(e) => set({ name: e.target.value })} />
          </Field>
          <Field label="地址">
            <input value={draft.addr} onChange={(e) => set({ addr: e.target.value })} />
          </Field>
          <Field label="serverPort">
            <input
              type="number"
              value={draft.port}
              onChange={(e) => set({ port: Number(e.target.value) || 0 })}
            />
          </Field>
          <Field label="传输协议">
            <select
              value={draft.protocol}
              onChange={(e) => set({ protocol: e.target.value })}
            >
              {["tcp", "kcp", "quic", "websocket", "wss"].map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          </Field>
          <Field label="鉴权方式">
            <select
              value={draft.authMethod}
              onChange={(e) => set({ authMethod: e.target.value })}
            >
              <option value="token">token</option>
              <option value="oidc">oidc</option>
            </select>
          </Field>
          <Field label="token">
            <input
              type="password"
              value={draft.token}
              onChange={(e) => set({ token: e.target.value })}
            />
          </Field>
          <Field label="TLS serverName" hint="留空则用 serverAddr">
            <input
              value={draft.tlsServerName ?? ""}
              onChange={(e) => set({ tlsServerName: e.target.value })}
            />
          </Field>
          <Field label="frps 看板端口" hint="frps.toml 的 webServer.port">
            <input
              type="number"
              value={draft.frpsAdminPort}
              onChange={(e) => set({ frpsAdminPort: Number(e.target.value) || 0 })}
            />
          </Field>
          <Field label="frps 看板账号">
            <input
              value={draft.frpsAdminUser}
              onChange={(e) => set({ frpsAdminUser: e.target.value })}
            />
          </Field>
          <Field label="frps 看板密码">
            <input
              type="password"
              value={draft.frpsAdminPassword}
              onChange={(e) => set({ frpsAdminPassword: e.target.value })}
            />
          </Field>
        </div>
        <div className="row" style={{ marginTop: 12 }}>
          <label className="check">
            <input
              type="checkbox"
              checked={draft.tlsEnable}
              onChange={(e) => set({ tlsEnable: e.target.checked })}
            />
            启用 TLS（强烈建议）
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={draft.tlsInsecure}
              onChange={(e) => set({ tlsInsecure: e.target.checked })}
            />
            跳过证书校验（不安全）
          </label>
        </div>
        <div className="row" style={{ marginTop: 12 }}>
          <button
            className="primary"
            onClick={() => {
              props.onSave(draft);
              setDraft(blankServer());
            }}
          >
            保存
          </button>
          <button className="ghost" onClick={() => setDraft(blankServer())}>
            清空 / 新建
          </button>
        </div>
      </div>

      {dashboard && (
        <div className="card">
          <h3>frps 看板原始返回</h3>
          <pre className="code">{dashboard}</pre>
        </div>
      )}
    </>
  );
}

/* ------------------------------------------------------------------- logs */

function LogsPanel(props: { profiles: Profile[]; running: string[] }) {
  const [profileId, setProfileId] = useState(props.profiles[0]?.id ?? "");
  const [lines, setLines] = useState<LogLine[]>([]);
  const boxRef = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    let alive = true;
    const tick = async () => {
      if (!profileId) return;
      try {
        const next = await api.getLogs(profileId, 0);
        if (alive) setLines(next);
      } catch {
        /* 忽略轮询错误 */
      }
    };
    void tick();
    const id = window.setInterval(tick, 1000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [profileId]);

  useEffect(() => {
    const el = boxRef.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [lines]);

  return (
    <div className="card">
      <h3>
        运行日志
        <small>{lines.length} 行（环形缓冲，只在内存里）</small>
      </h3>
      <div className="row" style={{ marginBottom: 10 }}>
        <select
          value={profileId}
          onChange={(e) => setProfileId(e.target.value)}
          style={{ width: 260 }}
        >
          {props.profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
              {props.running.includes(p.id) ? " · 运行中" : ""}
            </option>
          ))}
        </select>
        <button
          onClick={async () => {
            if (!profileId) return;
            await api.clearLogs(profileId);
            setLines([]);
          }}
        >
          清空
        </button>
        <span className="spacer" />
      </div>
      <div
        className="logs"
        ref={boxRef}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
      >
        {lines.length === 0 && <div className="l sys">（暂无日志）</div>}
        {lines.map((l) => (
          <div key={l.seq} className={`l ${l.stream === "stderr" ? "err" : ""}`}>
            {fmtTime(l.ts)} {l.line}
          </div>
        ))}
      </div>
    </div>
  );
}

/* ----------------------------------------------------------------- 设置 */

function SettingsPanel(props: {
  state: StoreData;
  frpc: FrpcInfo | null;
  onSave: (patch: Partial<StoreData["settings"]>) => void;
  onDetect: () => void;
  show: (t: string, k?: "ok" | "err") => void;
}) {
  const s = props.state.settings;
  return (
    <>
      <div className="card">
        <h3>frpc 内核</h3>
        <div className="row" style={{ marginBottom: 10 }}>
          <span className="badge">
            {props.frpc
              ? `${props.frpc.builtin ? "内置" : "自定义"} · ${props.frpc.version} · ${props.frpc.path}`
              : "未检测"}
          </span>
          <button onClick={props.onDetect}>重新检测</button>
        </div>
        <Field
          label="自定义 frpc 路径（留空使用内置）"
          hint="设置后优先使用该文件，方便自行升级 frpc 版本"
        >
          <input
            value={s.frpcPath ?? ""}
            placeholder="/usr/local/bin/frpc 或 C:\\frp\\frpc.exe"
            onChange={(e) => props.onSave({ frpcPath: e.target.value })}
          />
        </Field>
      </div>

      <div className="card">
        <h3>行为</h3>
        <div className="grid">
          <Field label="日志级别">
            <select
              value={s.logLevel}
              onChange={(e) => props.onSave({ logLevel: e.target.value })}
            >
              {["debug", "info", "warn", "error"].map((l) => (
                <option key={l} value={l}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
          <Field label="日志最大行数">
            <input
              type="number"
              value={s.logMaxLines}
              onChange={(e) =>
                props.onSave({ logMaxLines: Number(e.target.value) || 1000 })
              }
            />
          </Field>
          <Field label="默认管理端口">
            <input
              type="number"
              value={s.defaultAdminPort}
              onChange={(e) =>
                props.onSave({ defaultAdminPort: Number(e.target.value) || 7400 })
              }
            />
          </Field>
        </div>
        <div className="row" style={{ marginTop: 12 }}>
          <label className="check">
            <input
              type="checkbox"
              checked={s.launchAtLogin}
              onChange={(e) => props.onSave({ launchAtLogin: e.target.checked })}
            />
            开机自启（登录时带 --hidden 静默进托盘）
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={s.minimizeToTray}
              onChange={(e) => props.onSave({ minimizeToTray: e.target.checked })}
            />
            关闭窗口时最小化到托盘
          </label>
        </div>
      </div>

      <div className="card">
        <h3>导入 / 导出 / 订阅</h3>
        <ImportExport show={props.show} />
      </div>
    </>
  );
}

function ImportExport(props: { show: (t: string, k?: "ok" | "err") => void }) {
  const [text, setText] = useState("");
  const [url, setUrl] = useState("");

  return (
    <>
      <div className="row" style={{ marginBottom: 10 }}>
        <button
          onClick={async () => {
            try {
              setText(await api.exportBundle());
              props.show("已导出到下方文本框，可复制保存", "ok");
            } catch (e) {
              props.show(String(e), "err");
            }
          }}
        >
          导出全部配置
        </button>
        <button
          onClick={async () => {
            try {
              // 浏览器下载（WebView2 支持；WKWebView 若被拦，可手动复制）
              const blob = new Blob([text], { type: "application/json" });
              const a = document.createElement("a");
              a.href = URL.createObjectURL(blob);
              a.download = `frpc-desktop-${Date.now()}.json`;
              a.click();
              URL.revokeObjectURL(a.href);
            } catch (e) {
              props.show(String(e), "err");
            }
          }}
        >
          下载为文件
        </button>
        <button
          className="primary"
          onClick={async () => {
            try {
              props.show(await api.importBundle(text), "ok");
            } catch (e) {
              props.show(String(e), "err");
            }
          }}
        >
          导入下方内容
        </button>
      </div>

      <textarea
        rows={7}
        value={text}
        placeholder="粘贴 JSON 配置包（也支持单个 frpc.toml 文本）"
        onChange={(e) => setText(e.target.value)}
      />

      <div className="row" style={{ marginTop: 12 }}>
        <input
          value={url}
          placeholder="订阅地址（返回 JSON 配置包 / frpc.toml / base64 JSON）"
          onChange={(e) => setUrl(e.target.value)}
        />
        <button
          onClick={async () => {
            try {
              props.show(await api.fetchSubscription(url), "ok");
            } catch (e) {
              props.show(String(e), "err");
            }
          }}
        >
          拉取订阅
        </button>
      </div>
    </>
  );
}

/* ------------------------------------------------------------------- app */

export default function App() {
  const [state, setState] = useState<StoreData | null>(null);
  const [running, setRunning] = useState<RunningInfo[]>([]);
  const [selected, setSelected] = useState("");
  const [tab, setTab] = useState<"profiles" | "servers" | "logs" | "settings">(
    "profiles"
  );
  const [frpc, setFrpc] = useState<FrpcInfo | null>(null);
  const { msg, show } = useToast();

  const refresh = useCallback(async () => {
    try {
      setState(await api.getState());
    } catch (e) {
      show(String(e), "err");
    }
  }, [show]);

  useEffect(() => {
    void refresh();
    void api
      .detectFrpc()
      .then(setFrpc)
      .catch((e) => show(String(e), "err"));
  }, [refresh, show]);

  useEffect(() => {
    const id = window.setInterval(async () => {
      try {
        setRunning(await api.getRuntime());
      } catch {
        /* 忽略 */
      }
    }, 1200);
    return () => window.clearInterval(id);
  }, []);

  const profile = useMemo(
    () => state?.profiles.find((p) => p.id === selected) ?? state?.profiles[0],
    [state, selected]
  );
  const isRunning = (id: string) => running.some((r) => r.profileId === id);

  const guard = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      await refresh();
    } catch (e) {
      show(String(e), "err");
    }
  };

  if (!state) {
    return <div className="empty">加载中…</div>;
  }

  const patchState = (next: StoreData) => setState(next);

  return (
    <div className="app">
      <div className="topbar">
        <div className="brand">
          frpc-desktop<span>{running.length} 个运行中</span>
        </div>
        <div className="nav">
          {(
            [
              ["profiles", "配置"],
              ["servers", "服务器"],
              ["logs", "日志"],
              ["settings", "设置"],
            ] as const
          ).map(([k, label]) => (
            <button
              key={k}
              className={tab === k ? "active" : ""}
              onClick={() => setTab(k)}
            >
              {label}
            </button>
          ))}
        </div>
        <span className="spacer" />
        <span className="badge" title={frpc?.path}>
          {frpc ? `frpc ${frpc.version} · ${frpc.builtin ? "内置" : "自定义"}` : "frpc 未就绪"}
        </span>
      </div>

      <div className="body">
        <div className="sidebar">
          <div className="sidebar-head">
            <span>配置列表</span>
            <button
              className="icon"
              onClick={() =>
                guard(async () => {
                  const s = state.servers[0];
                  if (!s) throw new Error("请先添加服务器");
                  const p = blankProfile(s.id);
                  const next = await api.saveProfile(p);
                  patchState(next);
                  setSelected(p.id);
                  setTab("profiles");
                })
              }
            >
              + 新建
            </button>
          </div>
          <div className="list">
            {state.profiles.map((p) => (
              <div
                key={p.id}
                className={`item ${profile?.id === p.id ? "active" : ""}`}
                onClick={() => {
                  setSelected(p.id);
                  setTab("profiles");
                }}
              >
                <span className={`dot ${isRunning(p.id) ? "on" : ""}`} />
                <div className="item-main">
                  <div className="item-title">{p.name}</div>
                  <div className="item-sub">
                    {p.proxies.length} 个映射 ·{" "}
                    {state.servers.find((s) => s.id === p.serverId)?.addr ?? "未绑定"}
                  </div>
                </div>
                {isRunning(p.id) ? (
                  <button
                    className="icon"
                    title="停止"
                    onClick={(e) => {
                      e.stopPropagation();
                      void guard(() => api.stopProfile(p.id));
                    }}
                  >
                    ■
                  </button>
                ) : (
                  <button
                    className="icon"
                    title="启动"
                    onClick={(e) => {
                      e.stopPropagation();
                      void guard(() => api.startProfile(p.id));
                    }}
                  >
                    ▶
                  </button>
                )}
              </div>
            ))}
            {state.profiles.length === 0 && (
              <div className="empty">还没有配置</div>
            )}
          </div>
        </div>

        <div className="content">
          {tab === "profiles" &&
            (profile ? (
              <ProfileEditor
                key={profile.id}
                profile={profile}
                servers={state.servers}
                running={isRunning(profile.id)}
                info={running.find((r) => r.profileId === profile.id)}
                show={show}
                onSave={(p) => guard(async () => patchState(await api.saveProfile(p)))}
                onStart={() => guard(() => api.startProfile(profile.id))}
                onStop={() => guard(() => api.stopProfile(profile.id))}
                onDuplicate={() => guard(() => api.duplicateProfile(profile.id))}
                onDelete={() =>
                  guard(async () => {
                    patchState(await api.deleteProfile(profile.id));
                    setSelected("");
                  })
                }
              />
            ) : (
              <div className="empty">左侧新建一个配置开始</div>
            ))}

          {tab === "servers" && (
            <ServersPanel
              servers={state.servers}
              show={show}
              onSave={(s) => guard(async () => patchState(await api.saveServer(s)))}
              onDelete={(id) =>
                guard(async () => patchState(await api.deleteServer(id)))
              }
            />
          )}

          {tab === "logs" && (
            <LogsPanel
              profiles={state.profiles}
              running={running.map((r) => r.profileId)}
            />
          )}

          {tab === "settings" && (
            <SettingsPanel
              state={state}
              frpc={frpc}
              show={show}
              onDetect={() =>
                void api
                  .detectFrpc()
                  .then((f) => {
                    setFrpc(f);
                    show(`已检测到 frpc ${f.version}`, "ok");
                  })
                  .catch((e) => show(String(e), "err"))
              }
              onSave={(patch) =>
                guard(async () =>
                  patchState(
                    await api.saveSettings({ ...state.settings, ...patch })
                  )
                )
              }
            />
          )}
        </div>
      </div>

      {msg && <div className={`toast ${msg.kind}`}>{msg.text}</div>}
    </div>
  );
}
