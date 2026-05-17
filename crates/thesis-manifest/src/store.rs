//! @file store.rs
//! @description AuditLog：`.thesis/audit-log.jsonl` 追加写 + 按路径检索最近记录
//! @author Atlas.oi
//! @date 2026-05-17
//!
//! ## 并发安全策略
//!
//! JSONL 每行都是一个完整的 JSON 对象，以 `\n` 结尾。
//!
//! **POSIX `O_APPEND` 保证的范围**：
//! `O_APPEND` 只保证每次 `write(2)` 的**偏移量定位是原子的**（即多个进程不会写到同一位置），
//! 但 POSIX **不保证**普通文件（regular file）的小写入具有原子性。
//! PIPE_BUF（通常 4096 字节）的原子性承诺仅适用于**管道（pipe）和 FIFO**，
//! 不适用于普通文件，不要混用这两个概念。
//!
//! **实际行为（本地文件系统）**：
//! Linux ext4 / macOS APFS 在实现层面通过内核锁使小写入事实上是原子的，
//! 因此在本地存储上多进程/线程并发 append 通常不会交叉覆盖。
//! 这是**文件系统实现细节**，不是 POSIX 保证，不可跨文件系统依赖。
//!
//! **NFS / FUSE / 网络文件系统警告**：
//! 若 `.thesis/audit-log.jsonl` 存储在 NFS、FUSE 挂载或其他网络文件系统上，
//! 小写入的原子性**不再成立**，多写者可能交叉覆盖同一行。
//! 届时应在 `write(2)` 前后加 `flock(2)` 排他锁。
//!
//! **当前假设**：`.thesis/` 目录位于本地文件系统（ext4 / APFS），
//! 单条 Manifest 序列化后约 400-600 字节，远小于任何合理的块大小，
//! 因此暂不加锁。如迁移到网络存储，必须补充 `flock` 保护。

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use thesis_types::Manifest;

/// `.thesis/audit-log.jsonl` 的读写句柄。
pub struct AuditLog {
    /// `.thesis/` 目录的路径
    dir: PathBuf,
}

impl AuditLog {
    /// 以给定目录作为 `.thesis/` 目录构造 `AuditLog`。
    ///
    /// 通常传入 `docx_path.parent()/.thesis`，或测试用的 `TempDir`。
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// 返回 audit-log.jsonl 的完整路径。
    fn log_path(&self) -> PathBuf {
        self.dir.join("audit-log.jsonl")
    }

    /// 将 manifest 作为一行 JSON 追加到 audit-log.jsonl。
    ///
    /// 业务流程：
    /// 1. 确保 `.thesis/` 目录存在
    /// 2. 以 O_APPEND | O_CREAT 模式打开文件（POSIX 原子追加保证）
    /// 3. 序列化 manifest 为单行 JSON，末尾追加换行符
    ///
    /// # Errors
    /// 目录创建、文件打开、序列化失败时返回 `anyhow::Error`。
    pub fn append(&self, manifest: &Manifest) -> Result<(), anyhow::Error> {
        // 第一步：确保目录存在
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("无法创建目录: {}", self.dir.display()))?;

        // 第二步：以追加模式打开（O_APPEND | O_CREAT | O_WRONLY）
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_path())
            .with_context(|| format!("无法打开 audit-log: {}", self.log_path().display()))?;

        // 第三步：序列化为单行 JSON + 换行，一次 write_all 调用（本地文件系统单块写，见模块注释）
        let mut line = serde_json::to_vec(manifest).with_context(|| "序列化 manifest 失败")?;
        line.push(b'\n');

        file.write_all(&line)
            .with_context(|| "写入 audit-log 失败")?;

        Ok(())
    }

    /// 返回 audit-log.jsonl 中最后一条与 `docx_path` 匹配的 Manifest。
    ///
    /// 业务流程：
    /// 1. 读取全部行（文件较小，通常 < 100 条）
    /// 2. 过滤 docx_path 匹配的条目
    /// 3. 返回最后一条（时序最近）
    ///
    /// # Errors
    /// 文件不存在返回 `Ok(None)`，读取/反序列化失败返回 `Err`。
    pub fn latest_for(&self, docx_path: &Path) -> Result<Option<Manifest>, anyhow::Error> {
        let log_path = self.log_path();
        if !log_path.exists() {
            return Ok(None);
        }

        let file = std::fs::File::open(&log_path)
            .with_context(|| format!("无法打开 audit-log: {}", log_path.display()))?;

        let mut last: Option<Manifest> = None;

        for (line_no, line_result) in BufReader::new(file).lines().enumerate() {
            let line =
                line_result.with_context(|| format!("读取 audit-log 第 {} 行失败", line_no + 1))?;

            if line.trim().is_empty() {
                continue;
            }

            let manifest: Manifest = serde_json::from_str(&line).with_context(|| {
                format!("反序列化 audit-log 第 {} 行失败: {}", line_no + 1, &line)
            })?;

            if manifest.docx_path == docx_path {
                last = Some(manifest);
            }
        }

        Ok(last)
    }
}
