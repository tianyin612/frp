//! frpc 子进程托管。
//!
//! 关键点：
//! 1. 用 `std::process::Command` 直接起，不用 shell 插件 —— 才能拿到 pid、控制管道。
//! 2. stdout/stderr 各一个读线程，写进每个配置独立的环形缓冲（不落盘，避免长跑撑爆磁盘）。
//! 3. 每次启动把 {profileId: pid} 写进 running.json；下次启动 `cleanup_orphans` 校验进程名后清理，
//!    解决"GUI 退出了 frpc 还在后台跑"这个经典问题，同时避免 pid 复用误杀。

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub seq: u64,
    pub ts: i64,
    /// stdout | stderr | sys
    pub stream: String,
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningInfo {
    pub profile_id: String,
    pub pid: u32,
    pub started_at: i64,
    pub config_path: String,
}

struct Running {
    child: Child,
    pid: u32,
    started_at: i64,
    config: PathBuf,
}

struct Inner {
    procs: HashMap<String, Running>,
    logs: HashMap<String, VecDeque<LogLine>>,
    seq: u64,
    max_lines: usize,
    pid_file: Option<PathBuf>,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            procs: HashMap::new(),
            logs: HashMap::new(),
            seq: 0,
            max_lines: 2000,
            pid_file: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct Supervisor {
    inner: Arc<Mutex<Inner>>,
}

fn push_log(inner: &mut Inner, id: &str, stream: &str, line: String) {
    inner.seq += 1;
    let entry = LogLine {
        seq: inner.seq,
        ts: now_secs(),
        stream: stream.to_string(),
        line,
    };
    let buf = inner.logs.entry(id.to_string()).or_default();
    buf.push_back(entry);
    let max = inner.max_lines;
    while buf.len() > max {
        buf.pop_front();
    }
}

/// 为一个输出管道起读线程，逐行写进环形缓冲。
/// 泛型是为了同时吃下 ChildStdout 和 ChildStderr 两种类型。
fn spawn_reader<R>(inner: Arc<Mutex<Inner>>, id: String, stream: &'static str, pipe: R)
where
    R: std::io::Read + Send + 'static,
{
    std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            match line {
                Ok(line) => {
                    if let Ok(mut g) = inner.lock() {
                        push_log(&mut g, &id, stream, line);
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn persist(inner: &Inner) {    let Some(path) = inner.pid_file.as_ref() else { return };
    let map: HashMap<&String, u32> = inner.procs.iter().map(|(k, v)| (k, v.pid)).collect();
    if let Ok(text) = serde_json::to_string(&map) {
        let _ = std::fs::write(path, text);
    }
}

impl Supervisor {
    pub fn set_max_lines(&self, n: usize) {
        if let Ok(mut g) = self.inner.lock() {
            g.max_lines = n.max(200);
        }
    }

    pub fn set_pid_file(&self, path: PathBuf) {
        if let Ok(mut g) = self.inner.lock() {
            g.pid_file = Some(path);
        }
    }

    pub fn is_running(&self, profile_id: &str) -> bool {
        self.inner
            .lock()
            .map(|g| g.procs.contains_key(profile_id))
            .unwrap_or(false)
    }

    /// 启动；返回 pid。若同名已在跑，先杀掉旧的。
    pub fn spawn(&self, profile_id: &str, mut cmd: Command, config: PathBuf) -> Result<u32, String> {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let existing = {
            let mut g = self.inner.lock().map_err(|e| e.to_string())?;
            g.procs.remove(profile_id)
        };
        if let Some(mut old) = existing {
            let _ = old.child.kill();
            let _ = old.child.wait();
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("无法启动 frpc（{e}）。请检查二进制是否存在并且有执行权限"))?;

        let pid = child.id();
        let started_at = now_secs();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        {
            let mut g = self.inner.lock().map_err(|e| e.to_string())?;
            g.procs.insert(
                profile_id.to_string(),
                Running {
                    child,
                    pid,
                    started_at,
                    config: config.clone(),
                },
            );
            push_log(&mut g, profile_id, "sys", format!("已启动 frpc，pid {pid}"));
            persist(&g);
        }

        // stdout 和 stderr 是两种不同的类型（ChildStdout / ChildStderr），
        // 不能塞进同一个数组，所以用泛型函数分别起读线程。
        if let Some(pipe) = stdout {
            spawn_reader(Arc::clone(&self.inner), profile_id.to_string(), "stdout", pipe);
        }
        if let Some(pipe) = stderr {
            spawn_reader(Arc::clone(&self.inner), profile_id.to_string(), "stderr", pipe);
        }

        Ok(pid)
    }

    pub fn stop(&self, profile_id: &str) -> Result<(), String> {
        let mut g = self.inner.lock().map_err(|e| e.to_string())?;
        if let Some(mut r) = g.procs.remove(profile_id) {
            let _ = r.child.kill();
            let _ = r.child.wait();
            push_log(&mut g, profile_id, "sys", format!("已停止 frpc，pid {}", r.pid));
            persist(&g);
        }
        Ok(())
    }

    pub fn stop_all(&self) {
        let Ok(mut g) = self.inner.lock() else { return };
        let ids: Vec<String> = g.procs.keys().cloned().collect();
        for id in ids {
            if let Some(mut r) = g.procs.remove(&id) {
                let _ = r.child.kill();
                let _ = r.child.wait();
            }
        }
        persist(&g);
    }

    /// 当前存活进程；顺带回手已经自己退出的（配置错误、被 frps 拒绝等）。
    pub fn running(&self) -> Vec<RunningInfo> {
        let Ok(mut g) = self.inner.lock() else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let mut dead: Vec<(String, String, u32)> = Vec::new();

        for (id, r) in g.procs.iter_mut() {
            match r.child.try_wait() {
                Ok(Some(status)) => dead.push((id.clone(), status.to_string(), r.pid)),
                _ => out.push(RunningInfo {
                    profile_id: id.clone(),
                    pid: r.pid,
                    started_at: r.started_at,
                    config_path: r.config.to_string_lossy().to_string(),
                }),
            }
        }

        for (id, status, pid) in dead {
            g.procs.remove(&id);
            push_log(
                &mut g,
                &id,
                "sys",
                format!("frpc 进程已退出：{status}（pid {pid}）"),
            );
        }
        if !g.procs.is_empty() || out.is_empty() {
            persist(&g);
        }

        out
    }

    pub fn logs(&self, profile_id: &str, since_seq: u64) -> Vec<LogLine> {
        self.inner
            .lock()
            .map(|g| {
                g.logs
                    .get(profile_id)
                    .map(|b| {
                        b.iter()
                            .filter(|l| l.seq > since_seq)
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    pub fn clear_logs(&self, profile_id: &str) {
        if let Ok(mut g) = self.inner.lock() {
            if let Some(b) = g.logs.get_mut(profile_id) {
                b.clear();
            }
        }
    }
}

/* --------------------------------------------------------- 孤儿进程清理 */

#[cfg(windows)]
fn process_matches_frpc(pid: u32) -> bool {
    // 注意：args 数组元素必须是同一类型，不能把 &String 和 &str 混在一起
    let filter = format!("PID eq {pid}");
    let out = Command::new("tasklist")
        .args(["/FI", filter.as_str(), "/FO", "CSV", "/NH"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("frpc"),
        Err(_) => false,
    }
}

#[cfg(unix)]
fn process_matches_frpc(pid: u32) -> bool {
    let pid = pid.to_string();
    let out = Command::new("ps")
        .args(["-p", pid.as_str(), "-o", "comm="])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .to_ascii_lowercase()
            .contains("frpc"),
        Err(_) => false,
    }
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let pid = pid.to_string();
    let _ = Command::new("taskkill")
        .args(["/PID", pid.as_str(), "/F"])
        .status();
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    let pid = pid.to_string();
    let _ = Command::new("kill").args(["-9", pid.as_str()]).status();
}

/// 应用启动时调用：清掉上次崩溃/强杀留下的 frpc。
pub fn cleanup_orphans(pid_file: &Path) {
    let Ok(text) = std::fs::read_to_string(pid_file) else {
        return;
    };
    let map: HashMap<String, u32> = serde_json::from_str(&text).unwrap_or_default();
    for (profile_id, pid) in map {
        if pid == 0 {
            continue;
        }
        if process_matches_frpc(pid) {
            eprintln!("[supervisor] 清理残留 frpc 进程 pid={pid} profile={profile_id}");
            kill_pid(pid);
        }
    }
    let _ = std::fs::remove_file(pid_file);
}
