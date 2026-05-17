//! @file tools/init.rs
//! @description `init` 工具：在指定目录创建 .thesis/ 骨架文件
//! @author Atlas.oi
//! @date 2026-05-17

use std::path::{Path, PathBuf};

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `init` 工具输入参数。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct InitParams {
    /// 论文项目根目录的绝对路径（必须存在且可写）
    pub thesis_root: String,
}

/// `init` 工具输出。
#[derive(Debug, Serialize)]
pub struct InitOutput {
    /// 已创建的文件名列表（相对于 thesis_dir）
    pub created: Vec<String>,
    /// .thesis/ 目录绝对路径
    pub thesis_dir: String,
}

/// 执行初始化：在 `thesis_root/.thesis/` 下创建三个骨架文件。
///
/// 业务逻辑：
/// 1. 验证 thesis_root 存在
/// 2. 验证 docs/ 子目录存在（论文资源目录）
/// 3. 创建 .thesis/ 目录（已存在则跳过）
/// 4. 写入三个模板文件（已存在则跳过，不覆盖）
pub fn run_init(params: &InitParams) -> Result<InitOutput> {
    let root = PathBuf::from(&params.thesis_root);

    // 步骤 1：验证根目录
    if !root.exists() {
        anyhow::bail!("thesis_root 不存在: {}", root.display());
    }
    if !root.is_dir() {
        anyhow::bail!("thesis_root 不是目录: {}", root.display());
    }

    // 步骤 2：验证 docs/ 目录
    let docs_dir = root.join("docs");
    if !docs_dir.exists() {
        anyhow::bail!("docs/ 目录不存在，请先创建: {}", docs_dir.display());
    }

    // 步骤 3：创建 .thesis/ 目录
    let thesis_dir = root.join(".thesis");
    std::fs::create_dir_all(&thesis_dir)?;

    // 步骤 4：写入骨架文件（不覆盖已有文件）
    let mut created = Vec::new();
    write_if_absent(
        &thesis_dir.join("progress.md"),
        PROGRESS_TEMPLATE,
        &mut created,
    )?;
    write_if_absent(
        &thesis_dir.join("outline.md"),
        OUTLINE_TEMPLATE,
        &mut created,
    )?;
    write_if_absent(
        &thesis_dir.join("format-spec.md"),
        FORMAT_SPEC_TEMPLATE,
        &mut created,
    )?;

    Ok(InitOutput {
        created,
        thesis_dir: thesis_dir.to_string_lossy().into_owned(),
    })
}

/// 若目标文件不存在则写入内容，并将文件名加入 `created` 列表。
fn write_if_absent(path: &Path, content: &str, created: &mut Vec<String>) -> Result<()> {
    if path.exists() {
        tracing::info!("init: 跳过已存在文件 {:?}", path);
        return Ok(());
    }
    std::fs::write(path, content)?;
    // 只记录文件名（不含路径前缀）
    if let Some(name) = path.file_name() {
        created.push(name.to_string_lossy().into_owned());
    }
    tracing::info!("init: 创建文件 {:?}", path);
    Ok(())
}

// ─── 骨架文件模板 ────────────────────────────────────────────────────────────

/// 进度追踪模板：记录各章节撰写状态
const PROGRESS_TEMPLATE: &str = "\
# 论文撰写进度

<!-- thesis-mcp 自动生成，可按需修改 -->

| 章节 | 状态 | 最后更新 | 备注 |
|------|------|----------|------|
| 摘要 | 待写 | - | |
| 第一章 绪论 | 待写 | - | |
| 第二章 | 待写 | - | |
| 第三章 | 待写 | - | |
| 结论 | 待写 | - | |
| 参考文献 | 待写 | - | |

## 状态说明

- `待写`：尚未开始
- `草稿`：初稿完成，待修订
- `修订中`：正在修订
- `完成`：已定稿
";

/// 大纲模板：论文章节结构规划
const OUTLINE_TEMPLATE: &str = "\
# 论文大纲

<!-- thesis-mcp 自动生成，请根据实际情况修改 -->

## 题目

（填写论文题目）

## 摘要

- 研究背景
- 研究方法
- 主要结论

## 第一章 绪论

### 1.1 研究背景与意义
### 1.2 研究现状
### 1.3 研究内容与结构安排

## 第二章 （章节标题）

### 2.1
### 2.2

## 第三章 （章节标题）

### 3.1
### 3.2

## 结论

## 参考文献
";

/// 格式规范模板：记录从范文提取的排版要求
const FORMAT_SPEC_TEMPLATE: &str = "\
# 格式规范

<!-- thesis-mcp 自动生成 -->
<!-- 请将从范文 / 学校模板中提取的具体格式要求填入此处 -->
<!-- write_section 工具在写作时会参照此文件中的规范 -->

## 字体

- 正文字体：（待填写，例：宋体 12pt）
- 标题一字体：（待填写）
- 标题二字体：（待填写）

## 段落

- 行距：（待填写，例：1.5 倍行距）
- 段前距：（待填写）
- 段后距：（待填写）
- 首行缩进：（待填写，例：2 字符）

## 页面

- 纸张：（待填写，例：A4）
- 页边距：上 cm / 下 cm / 左 cm / 右 cm

## 编号

- 章节编号样式：（待填写，例：第一章 / 1.1 / 1.1.1）
- 图表编号样式：（待填写，例：图 1-1）

## 参考文献

- 引用格式：（待填写，例：GB/T 7714-2015）
";

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_root() -> TempDir {
        let tmp = TempDir::new().expect("tempdir");
        // init 要求 docs/ 存在
        std::fs::create_dir_all(tmp.path().join("docs")).expect("create docs");
        tmp
    }

    #[test]
    fn init_creates_three_files() {
        let tmp = setup_root();
        let output = run_init(&InitParams {
            thesis_root: tmp.path().to_string_lossy().into_owned(),
        })
        .expect("run_init");

        // 三个文件都应创建
        assert_eq!(
            output.created.len(),
            3,
            "应创建 3 个文件: {:?}",
            output.created
        );

        let thesis_dir = tmp.path().join(".thesis");
        assert!(thesis_dir.join("progress.md").exists());
        assert!(thesis_dir.join("outline.md").exists());
        assert!(thesis_dir.join("format-spec.md").exists());
    }

    #[test]
    fn init_does_not_overwrite_existing() {
        let tmp = setup_root();
        let thesis_dir = tmp.path().join(".thesis");
        std::fs::create_dir_all(&thesis_dir).expect("mkdir");

        // 预先写入 progress.md
        let existing = "# existing content";
        std::fs::write(thesis_dir.join("progress.md"), existing).expect("prewrite");

        let output = run_init(&InitParams {
            thesis_root: tmp.path().to_string_lossy().into_owned(),
        })
        .expect("run_init");

        // 只创建了 2 个新文件（progress.md 已存在，跳过）
        assert_eq!(output.created.len(), 2);
        // 已有内容不被覆盖
        let content = std::fs::read_to_string(thesis_dir.join("progress.md")).expect("read");
        assert_eq!(content, existing);
    }

    #[test]
    fn init_fails_if_docs_missing() {
        let tmp = TempDir::new().expect("tempdir");
        // 不创建 docs/ 目录
        let result = run_init(&InitParams {
            thesis_root: tmp.path().to_string_lossy().into_owned(),
        });
        assert!(result.is_err(), "缺少 docs/ 时应返回错误");
    }

    #[test]
    fn init_fails_if_root_missing() {
        let result = run_init(&InitParams {
            thesis_root: "/nonexistent/path/12345".to_string(),
        });
        assert!(result.is_err());
    }
}
