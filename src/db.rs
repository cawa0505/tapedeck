//! SQLite 資產圖譜（OQ-04 / Pillar 2 / REQ-6.6）
//!
//! 三層關聯：`.roll（source_roll）➔ asset（DB row）➔ .md 引用（第三層）`
//! - `register()`：資產登錄（path/hash/source_roll/mtime/影格快取索引）
//! - `scan_md_references()`：掃描 .md 內資產引用
//! - `orphans()`：孤兒掃描（DB 有但 .md 無引用）

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use crate::paths;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS assets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL UNIQUE,
    hash        TEXT NOT NULL,
    source_roll TEXT,
    mtime       INTEGER NOT NULL,
    frame_cache TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);";

/// 資產記錄
// ponytail: hash/source_roll/mtime 消費端（Re-roll、frame cache、clean verbose）未實作，先保留欄位
#[allow(dead_code)]
pub struct Asset {
    pub path: PathBuf,
    pub hash: String,
    pub source_roll: Option<String>,
    pub mtime: i64,
}

impl Asset {
    /// 影格快取目錄（`~/.cache/tapedeck/frames/<sha256>/`，CONFIG #2881）
    // ponytail: 消費端（frame cache）未實作，先保留
    #[allow(dead_code)]
    pub fn frame_cache_dir(&self) -> PathBuf {
        paths::cache_dir().join("frames").join(&self.hash)
    }
}

/// 資產圖譜存取器
pub struct AssetTracker {
    conn: Connection,
}

impl AssetTracker {
    /// 預設 DB：`$XDG_STATE_HOME/tapedeck/tapedeck.db`（REQ-6.6）
    pub fn open() -> Result<Self> {
        Self::open_at(&paths::state_dir().join("tapedeck.db"))
    }

    /// 指定路徑開 DB（測試用）
    pub fn open_at(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("建立 DB 目錄失敗: {}", parent.display()))?;
        }
        let conn = Connection::open(db_path)
            .with_context(|| format!("開啟 DB 失敗: {}", db_path.display()))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// 登錄資產：SHA-256 hash + mtime（已存在則更新）
    pub fn register(&self, path: &Path, source_roll: Option<&str>) -> Result<()> {
        let hash =
            sha256_file(path).with_context(|| format!("計算 hash 失敗: {}", path.display()))?;
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let frame_cache = paths::cache_dir().join("frames").join(&hash);

        self.conn.execute(
            "INSERT INTO assets (path, hash, source_roll, mtime, frame_cache)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
               hash = excluded.hash,
               source_roll = excluded.source_roll,
               mtime = excluded.mtime,
               frame_cache = excluded.frame_cache",
            params![
                path.to_string_lossy(),
                hash,
                source_roll,
                mtime,
                frame_cache.to_string_lossy(),
            ],
        )?;
        Ok(())
    }

    /// 列出全部資產（依登錄序）
    pub fn assets(&self) -> Result<Vec<Asset>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, hash, source_roll, mtime FROM assets ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Asset {
                path: PathBuf::from(r.get::<_, String>(0)?),
                hash: r.get(1)?,
                source_roll: r.get(2)?,
                mtime: r.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 掃描目錄下所有 .md/.markdown 檔，回傳被引用的資產檔名集合
    /// Markdown 引用掃描：資產檔名 ➔ [(md 路徑, 行號)]（Pillar 2 三層關聯第三層）
    pub fn scan_md_references(
        &self,
        md_dir: &Path,
    ) -> Result<std::collections::HashMap<String, Vec<(std::path::PathBuf, usize)>>> {
        let mut refs: std::collections::HashMap<String, Vec<(std::path::PathBuf, usize)>> =
            std::collections::HashMap::new();
        for entry in walk_md(md_dir)? {
            let content = std::fs::read_to_string(&entry)?;
            for (i, line) in content.lines().enumerate() {
                if let Some(asset) = extract_asset_ref(line) {
                    refs.entry(asset).or_default().push((entry.clone(), i + 1));
                }
            }
        }
        Ok(refs)
    }

    /// 孤兒掃描：DB 中有登錄、但 .md 未引用的資產（依檔名比對）
    pub fn orphans(&self, md_dir: &Path) -> Result<Vec<Asset>> {
        let refs = self.scan_md_references(md_dir)?;
        Ok(self
            .assets()?
            .into_iter()
            .filter(|a| {
                let name = a
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                !refs.contains_key(&name)
            })
            .collect())
    }

    /// 刪除資產（檔案 + DB row）；`dry_run=true` 只回報不動手。
    /// 回傳動作描述字串（library 層零 stdout，由呼叫方決定輸出）。
    pub fn remove(&self, asset: &Asset, dry_run: bool) -> Result<String> {
        let path = &asset.path;
        if dry_run {
            return Ok(format!("[dry-run] 孤兒: {}", path.display()));
        }
        match std::fs::remove_file(path) {
            Ok(()) => {
                self.conn.execute(
                    "DELETE FROM assets WHERE path = ?1",
                    params![path.to_string_lossy()],
                )?;
                Ok(format!("已刪除孤兒: {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 檔案已不存在：僅清 DB row
                self.conn.execute(
                    "DELETE FROM assets WHERE path = ?1",
                    params![path.to_string_lossy()],
                )?;
                Ok(format!("清理失效 DB row: {}", path.display()))
            }
            Err(e) => Err(e).context(format!("刪除失敗: {}", path.display())),
        }
    }
}

/// SHA-256 檔案 hash（hex）
pub fn sha256_file(path: &Path) -> Result<String> {
    let data = std::fs::read(path)?;
    Ok(hex(&Sha256::digest(&data)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 遞迴蒐集 .md / .markdown 檔案
fn walk_md(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().map(|n| n != ".git").unwrap_or(false) {
                out.extend(walk_md(&path)?);
            }
        } else if path
            .extension()
            .map(|e| e == "md" || e == "markdown")
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(out)
}

/// 從一行 .md 文字擷取資產引用（`assets/xxx.webm` 或 `![alt](assets/xxx.webm)` 等）
fn extract_asset_ref(line: &str) -> Option<String> {
    // ponytail: 白名單比對容器副檔名（vhs/ffmpeg 輸出），避免誤抓非資產檔
    const MEDIA_EXTS: [&str; 4] = [".webm", ".mp4", ".gif", ".webp"];
    for token in line.split(['(', ')', '[', ']', ' ']) {
        if token.contains("assets/") && MEDIA_EXTS.iter().any(|e| token.ends_with(e)) {
            return Some(token.rsplit('/').next().unwrap_or(token).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_db(tag: &str) -> (PathBuf, AssetTracker) {
        let dir = std::env::temp_dir().join(format!(
            "tapedeck-db-test-{}-{}",
            tag,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("tapedeck.db");
        let tracker = AssetTracker::open_at(&db_path).unwrap();
        (dir, tracker)
    }

    fn touch(dir: &Path, rel: &str) -> PathBuf {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, format!("content-{rel}")).unwrap();
        p
    }

    #[test]
    fn register_and_query() {
        let (_dir, tracker) = tmp_db("rq");
        let asset = touch(&_dir, "assets/demo.webm");
        tracker.register(&asset, Some("demo.roll")).unwrap();

        let all = tracker.assets().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].path, asset);
        assert_eq!(all[0].source_roll.as_deref(), Some("demo.roll"));
        assert_eq!(all[0].hash.len(), 64); // sha256 hex
                                           // 影格快取索引指向 frames/<hash>
        assert!(all[0]
            .frame_cache_dir()
            .ends_with(format!("frames/{}", all[0].hash)));
    }

    #[test]
    fn register_replaces_duplicate() {
        let (_dir, tracker) = tmp_db("dup");
        let asset = touch(&_dir, "assets/a.webm");
        tracker.register(&asset, Some("a.roll")).unwrap();
        tracker.register(&asset, Some("b.roll")).unwrap();
        assert_eq!(tracker.assets().unwrap().len(), 1);
        assert_eq!(
            tracker.assets().unwrap()[0].source_roll.as_deref(),
            Some("b.roll")
        );
    }

    #[test]
    fn orphan_scan_finds_unreferenced() {
        let (dir, tracker) = tmp_db("orphan");
        let used = touch(&dir, "assets/used.webm");
        let orphan = touch(&dir, "assets/trash.webm");
        tracker.register(&used, Some("a.roll")).unwrap();
        tracker.register(&orphan, Some("b.roll")).unwrap();

        fs::create_dir_all(dir.join("docs")).unwrap();
        fs::write(dir.join("docs/README.md"), "![demo](assets/used.webm)\n").unwrap();

        let orphans = tracker.orphans(&dir).unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].path, orphan);

        // dry-run 不刪除
        assert!(!tracker.remove(&orphans[0], true).unwrap());
        assert!(orphan.exists());

        // 實際刪除
        assert!(tracker.remove(&orphans[0], false).unwrap());
        assert!(!orphan.exists());
        assert_eq!(tracker.assets().unwrap().len(), 1);
    }

    #[test]
    fn md_ref_extraction() {
        assert_eq!(
            extract_asset_ref("![demo](assets/used.webm)").as_deref(),
            Some("used.webm")
        );
        assert_eq!(
            extract_asset_ref("路徑: assets/sub/video.webm 說明").as_deref(),
            Some("video.webm")
        );
        assert_eq!(extract_asset_ref("無引用的一行"), None);
    }
}
