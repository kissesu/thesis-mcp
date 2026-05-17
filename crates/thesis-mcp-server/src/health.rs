//! @file health.rs
//! @description 启动时健康检查：验证 ooxmlsdk 可加载、.thesis/ 目录可写
//! @author Atlas.oi
//! @date 2026-05-17

use std::path::Path;

/// 健康检查报告。
#[derive(Debug)]
pub struct HealthReport {
    /// ooxmlsdk 可通过编译期引用加载
    pub ooxmlsdk_ok: bool,
    /// 当前工作目录存在且可写（.thesis/ 可创建）
    pub workdir_writable: bool,
    /// 所有检查项全部通过
    pub all_ok: bool,
}

impl HealthReport {
    /// 在指定根目录下执行健康检查。
    ///
    /// 业务逻辑：
    /// 1. 用编译期 `use ooxmlsdk` 验证 crate 可链接（编译期通过即运行期通过）
    /// 2. 尝试在 `root/.thesis/` 写入探针文件，验证目录可创建 + 文件可写
    /// 3. 所有项通过则 all_ok = true
    pub fn check(root: &Path) -> Self {
        // 步骤 1：ooxmlsdk 编译期引用检查
        // 只要此 crate 能编译链接，ooxmlsdk 就可用；
        // 通过引用 SdkError 触发链接，不执行任何 I/O
        let ooxmlsdk_ok = {
            let _ = std::any::TypeId::of::<ooxmlsdk::common::SdkError>();
            true
        };

        // 步骤 2：.thesis/ 目录可写检查
        let thesis_dir = root.join(".thesis");
        let workdir_writable = check_writable(&thesis_dir);

        let all_ok = ooxmlsdk_ok && workdir_writable;

        HealthReport {
            ooxmlsdk_ok,
            workdir_writable,
            all_ok,
        }
    }
}

/// 检查目录是否可写：目录不存在则尝试创建，存在则尝试写入探针文件。
fn check_writable(dir: &Path) -> bool {
    // 若目录不存在，先尝试创建；合并 if 条件避免嵌套
    if !dir.exists()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        tracing::warn!("health: 无法创建目录 {:?}: {}", dir, e);
        return false;
    }

    // 写入临时探针文件验证写权限
    let probe = dir.join(".health_probe");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            // 清理探针文件，忽略删除失败
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(e) => {
            tracing::warn!("health: 写权限探针失败 {:?}: {}", probe, e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn health_report_ok_with_writable_temp_dir() {
        let tmp = TempDir::new().expect("tempdir");
        let report = HealthReport::check(tmp.path());
        // ooxmlsdk 编译期可用，临时目录可写 → all_ok
        assert!(report.workdir_writable, "tempdir 应可写");
        assert!(report.all_ok, "所有检查应通过");
    }

    #[test]
    fn health_report_creates_thesis_dir() {
        let tmp = TempDir::new().expect("tempdir");
        let thesis_dir = tmp.path().join(".thesis");
        assert!(!thesis_dir.exists(), "前提：.thesis/ 不存在");
        let report = HealthReport::check(tmp.path());
        assert!(report.workdir_writable);
        // 健康检查应已创建 .thesis/ 目录
        assert!(thesis_dir.exists(), ".thesis/ 应被创建");
    }
}
