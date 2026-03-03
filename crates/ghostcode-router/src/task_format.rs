/*!
 * @file task_format.rs
 * @description ---TASK---/---CONTENT--- 分隔符格式的解析和序列化
 *              实现多 Agent 并行任务格式的结构化解析，将文本格式转换为 TaskSpec 列表
 *              参考: ccg-workflow/codeagent-wrapper/config.go:113-185
 * @author Atlas.oi
 * @date 2026-03-02
 */

use thiserror::Error;

// ============================================
// 数据结构定义
// ============================================

/// 任务规格，包含 Agent 执行所需的所有配置
///
/// 对应 ---TASK--- 到下一个 ---TASK--- 或 EOF 之间的一个任务块
/// 参考: ccg-workflow/codeagent-wrapper/config.go:133-185
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSpec {
    /// 任务唯一标识符（必填）
    pub id: String,
    /// 任务内容文本，---CONTENT--- 之后的所有文本（必填）
    pub task_text: String,
    /// 工作目录，可选（未指定时由调用方决定）
    pub workdir: Option<String>,
    /// 目标后端名称，默认为 "codex"
    pub backend: String,
    /// 依赖的任务 id 列表，逗号分隔解析而来
    pub dependencies: Vec<String>,
    /// session_id 存在时表示恢复模式（resume），否则为新建模式
    pub session_id: Option<String>,
}

impl TaskSpec {
    /// 判断是否为 resume 模式（有 session_id）
    ///
    /// resume 模式下 Agent 会继承指定 session 的上下文继续工作
    pub fn is_resume(&self) -> bool {
        self.session_id.is_some()
    }
}

// ============================================
// 错误类型定义
// ============================================

/// 任务格式解析错误
#[derive(Error, Debug)]
pub enum TaskFormatError {
    /// 任务块缺少必填字段 id
    #[error("任务缺少必填字段 id")]
    MissingId,
    /// 通用解析错误，携带具体描述
    #[error("解析错误: {0}")]
    ParseError(String),
}

// ============================================
// 核心解析逻辑
// ============================================

/// 从格式化文本中解析出 TaskSpec 列表
///
/// 业务逻辑：
/// 1. 以 ---TASK--- 为分隔符将输入分割为多个任务块
/// 2. 每个任务块以 ---CONTENT--- 为界分为头部（元数据）和内容两部分
/// 3. 解析头部的 key: value 行，填充 TaskSpec 字段
/// 4. ---CONTENT--- 之后的文本即为 task_text
///
/// 格式定义（参考 ccg-workflow/codeagent-wrapper/config.go:113-185）：
/// ```text
/// ---TASK---
/// id: task1
/// workdir: /path/to/dir
/// backend: codex
/// dependencies: dep1,dep2
/// session_id: xxx123
/// ---CONTENT---
/// 实际任务内容（多行）
/// ```
///
/// @param input - 待解析的格式化文本
/// @returns 解析成功时返回 TaskSpec 列表；空输入返回空列表
pub fn parse_task_format(input: &str) -> Result<Vec<TaskSpec>, TaskFormatError> {
    // 空输入直接返回空列表
    if input.trim().is_empty() {
        return Ok(vec![]);
    }

    // ============================================
    // 第一步：以 ---TASK--- 分割所有任务块
    // 分割后第一个元素通常为空字符串（---TASK--- 之前的内容），跳过
    // ============================================
    let raw_blocks: Vec<&str> = input.split("---TASK---").collect();
    let mut specs = Vec::new();
    let mut task_index = 0usize;

    for block in &raw_blocks {
        let block = block.trim();
        // 跳过 ---TASK--- 之前的空内容
        if block.is_empty() {
            continue;
        }
        task_index += 1;

        // ============================================
        // 第二步：以 ---CONTENT--- 分割头部和内容
        // 头部包含 key: value 元数据，内容为实际任务文本
        // ============================================
        let parts: Vec<&str> = block.splitn(2, "---CONTENT---").collect();
        if parts.len() != 2 {
            return Err(TaskFormatError::ParseError(format!(
                "任务块 #{} 缺少 ---CONTENT--- 分隔符",
                task_index
            )));
        }

        let meta_section = parts[0].trim();
        // task_text 保留原始换行（仅去除首部单个换行），保持内容完整性
        let raw_content = parts[1];
        // 去除首部换行符（---CONTENT--- 后的换行），但保留内容中的所有换行
        let task_text = raw_content
            .strip_prefix('\n')
            .unwrap_or(raw_content)
            .to_string();

        // ============================================
        // 第三步：逐行解析头部元数据
        // 格式为 key: value，跳过空行和不符合格式的行
        // ============================================
        let mut id = String::new();
        let mut workdir: Option<String> = None;
        let mut backend = "codex".to_string(); // 默认后端
        let mut dependencies: Vec<String> = vec![];
        let mut session_id: Option<String> = None;

        for line in meta_section.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // 使用辅助函数解析 key: value 对
            if let Some((key, value)) = parse_header_field(line) {
                match key {
                    "id" => id = value.to_string(),
                    "workdir" => workdir = Some(value.to_string()),
                    "backend" => backend = value.to_string(),
                    "session_id" => session_id = Some(value.to_string()),
                    "dependencies" => {
                        // 依赖以逗号分隔，每项 trim 空白，过滤空字符串
                        dependencies = value
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    // 未知字段静默忽略，保持向后兼容
                    _ => {}
                }
            }
        }

        // ============================================
        // 第四步：验证必填字段
        // id 是唯一的必填字段
        // ============================================
        if id.is_empty() {
            return Err(TaskFormatError::MissingId);
        }

        specs.push(TaskSpec {
            id,
            task_text,
            workdir,
            backend,
            dependencies,
            session_id,
        });
    }

    Ok(specs)
}

/// 将 TaskSpec 列表序列化为 ---TASK---/---CONTENT--- 格式文本
///
/// 业务逻辑：
/// 1. 每个 TaskSpec 生成一个 ---TASK--- 块
/// 2. 头部按固定顺序输出各字段
/// 3. 可选字段仅在有值时输出
/// 4. 依赖列表以逗号连接输出
///
/// @param tasks - 待序列化的 TaskSpec 列表
/// @returns 格式化文本字符串
pub fn serialize_task_format(tasks: &[TaskSpec]) -> String {
    let mut output = String::new();

    for task in tasks {
        // 输出 ---TASK--- 块标记
        output.push_str("---TASK---\n");

        // id 是必填字段，始终输出
        output.push_str(&format!("id: {}\n", task.id));

        // 可选字段：workdir
        if let Some(ref dir) = task.workdir {
            output.push_str(&format!("workdir: {}\n", dir));
        }

        // backend（即使是默认值也显式输出，保持格式一致）
        output.push_str(&format!("backend: {}\n", task.backend));

        // 可选字段：dependencies（逗号连接）
        if !task.dependencies.is_empty() {
            output.push_str(&format!("dependencies: {}\n", task.dependencies.join(",")));
        }

        // 可选字段：session_id
        if let Some(ref sid) = task.session_id {
            output.push_str(&format!("session_id: {}\n", sid));
        }

        // 输出 ---CONTENT--- 分隔符和任务内容
        output.push_str("---CONTENT---\n");
        output.push_str(&task.task_text);

        // 确保内容末尾有换行，以便下一个 ---TASK--- 块正确分割
        if !task.task_text.ends_with('\n') {
            output.push('\n');
        }
    }

    output
}

// ============================================
// 辅助函数
// ============================================

/// 解析单行头部字段，格式为 "key: value"
///
/// 使用 splitn(2, ':') 处理 value 中可能包含冒号的情况（如路径）
///
/// @param line - 待解析的行（已 trim）
/// @returns Some((key, value)) 成功解析时返回键值对；格式不符时返回 None
fn parse_header_field(line: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = line.splitn(2, ':').collect();
    if parts.len() != 2 {
        return None;
    }
    let key = parts[0].trim();
    let value = parts[1].trim();
    // key 不能为空
    if key.is_empty() {
        return None;
    }
    Some((key, value))
}
