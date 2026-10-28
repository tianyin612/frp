import { invoke } from "@tauri-apps/api/core";
import type {
  FrpcInfo,
  LogLine,
  Profile,
  RunningInfo,
  Server,
  AppSettings,
  StoreData,
} from "./types";

const call = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> =>
  invoke<T>(cmd, args);

export const api = {
  // 全局状态
  getState: () => call<StoreData>("get_app_state"),
  saveSettings: (settings: AppSettings) =>
    call<StoreData>("save_settings", { settings }),

  // 服务器
  saveServer: (server: Server) => call<StoreData>("save_server", { server }),
  deleteServer: (serverId: string) =>
    call<StoreData>("delete_server", { serverId }),

  // 配置
  saveProfile: (profile: Profile) =>
    call<StoreData>("save_profile", { profile }),
  deleteProfile: (profileId: string) =>
    call<StoreData>("delete_profile", { profileId }),
  duplicateProfile: (profileId: string) =>
    call<StoreData>("duplicate_profile", { profileId }),

  // 配置生成 / 校验
  previewConfig: (profileId: string) =>
    call<string>("preview_config", { profileId }),
  verifyProfile: (profileId: string) =>
    call<string>("verify_profile", { profileId }),

  // 生命周期
  startProfile: (profileId: string) =>
    call<void>("start_profile", { profileId }),
  stopProfile: (profileId: string) => call<void>("stop_profile", { profileId }),
  restartProfile: (profileId: string) =>
    call<void>("restart_profile", { profileId }),
  getRuntime: () => call<RunningInfo[]>("get_runtime"),

  // 日志
  getLogs: (profileId: string, sinceSeq = 0) =>
    call<LogLine[]>("get_logs", { profileId, sinceSeq }),
  clearLogs: (profileId: string) => call<void>("clear_logs", { profileId }),

  // 管理 API
  adminStatus: (profileId: string) =>
    call<unknown>("admin_status", { profileId }),
  adminReload: (profileId: string) =>
    call<string>("admin_reload", { profileId }),
  frpsDashboard: (serverId: string) =>
    call<unknown>("frps_dashboard", { serverId }),

  // 内核
  detectFrpc: () => call<FrpcInfo>("detect_frpc"),

  // 导入导出 / 订阅
  exportBundle: () => call<string>("export_bundle"),
  importBundle: (json: string) => call<string>("import_bundle", { json }),
  fetchSubscription: (url: string) =>
    call<string>("fetch_subscription", { url }),
};
