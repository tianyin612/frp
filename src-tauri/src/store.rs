//! 持久化：单个 store.json，原子写入（tmp + rename），避免半截文件把配置写坏。

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::StoreData;

pub struct Store {
    pub path: PathBuf,
    pub data: StoreData,
}

impl Store {
    pub fn load(path: PathBuf) -> Self {
        let data = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<StoreData>(&text) {
                Ok(mut d) => {
                    if d.version == 0 {
                        d.version = 1;
                    }
                    // 版本迁移位：以后加字段在这里按 d.version 升。
                    d
                }
                Err(e) => {
                    eprintln!("[store] {} 解析失败，已备份并用空配置启动: {e}", path.display());
                    let _ = fs::copy(&path, path.with_extension("json.broken"));
                    StoreData::default()
                }
            },
            Err(_) => StoreData::default(),
        };
        Self { path, data }
    }

    pub fn save(&self) -> Result<(), String> {
        atomic_write(&self.path, &serde_json::to_vec_pretty(&self.data).map_err(|e| e.to_string())?)
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败 {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("写入失败 {}: {e}", tmp.display()))?;
    // Windows 上 rename 到已存在文件会失败，先删目标
    #[cfg(windows)]
    {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
    fs::rename(&tmp, path).map_err(|e| format!("替换失败 {}: {e}", path.display()))?;
    Ok(())
}
