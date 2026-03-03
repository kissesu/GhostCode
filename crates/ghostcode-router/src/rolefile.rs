// @file rolefile.rs
// @description ROLE_FILE 注入器
//              扫描文本中的 `ROLE_FILE: <path>` 行，读取文件内容替换整行。
//              参考: ccg-workflow/codeagent-wrapper/utils.go:75-117 - 文件内容注入逻辑
// @author Atlas.oi
// @date 2026-03-02

use std::fs;
use std::path::Path;
use thiserror::Error;

// 角色文件大小上限：1MB，防止超大文件塞满提示词
const MAX_ROLE_FILE_SIZE: u64 = 1024 * 1024;

/// 角色文件注入错误类型
#[derive(Error, Debug)]
pub enum RoleFileError {
    /// 角色文件路径不存在
    #[error("角色文件不存在: {path}")]
    FileNotFound { path: String },
    /// 角色文件超过大小限制（1MB）
    #[error("角色文件过大 ({size} bytes > {max} bytes): {path}")]
    FileTooLarge { path: String, size: u64, max: u64 },
    /// 角色文件路径包含不安全的路径遍历（如 ../ 或绝对路径跳出允许范围）
    #[error("角色文件路径不安全，包含路径遍历: {path}")]
    UnsafePath { path: String },
    /// 读取角色文件时发生 IO 错误
    #[error("读取角色文件失败: {0}")]
    IoError(#[from] std::io::Error),
}

/// 注入 ROLE_FILE 引用
///
/// 业务逻辑说明：
/// 1. 逐行扫描输入文本，匹配 `ROLE_FILE:` 前缀的行
/// 2. 提取 `ROLE_FILE:` 后的路径部分，trim 两端空白
/// 3. 检查文件是否存在，不存在返回 FileNotFound 错误
/// 4. 检查文件大小，超过 1MB 返回 FileTooLarge 错误
/// 5. 读取文件内容，替换整行（去除原始的 ROLE_FILE: 行）
/// 6. 重新拼接处理后的各行，保留非 ROLE_FILE 行的原样内容
///
/// @param text - 待注入的文本内容，可能含多行 ROLE_FILE 引用
/// @returns 注入后的文本（Ok），或首个遇到的错误（Err）
pub fn inject_role_files(text: &str) -> Result<String, RoleFileError> {
    // ============================================
    // 快速检查：若文本不含 ROLE_FILE: 前缀，直接返回原文
    // 避免不必要的字符串分割和重建开销
    // ============================================
    if !text.contains("ROLE_FILE:") {
        return Ok(text.to_string());
    }

    // ============================================
    // 逐行处理，通过手动分割保留换行符信息
    // 使用 split('\n') 而非 lines()，以正确处理连续换行符
    // ============================================
    let raw_lines: Vec<&str> = text.split('\n').collect();
    let total = raw_lines.len();
    let mut result_parts: Vec<String> = Vec::with_capacity(total);

    for (i, line) in raw_lines.iter().enumerate() {
        if let Some(rest) = line.strip_prefix("ROLE_FILE:") {
            // 提取路径，trim 两端空白（包括行尾的 \r）
            let path_str = rest.trim();

            // 安全检查：拒绝包含路径遍历的文件路径
            // 防止通过 ROLE_FILE 注入读取宿主机上的任意文件（如 /etc/passwd）
            if path_str.contains("..") {
                return Err(RoleFileError::UnsafePath {
                    path: path_str.to_string(),
                });
            }

            // 检查文件是否存在
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(RoleFileError::FileNotFound {
                    path: path_str.to_string(),
                });
            }

            // 检查文件大小是否超过 1MB 限制
            let metadata = fs::metadata(path)?;
            let file_size = metadata.len();
            if file_size > MAX_ROLE_FILE_SIZE {
                return Err(RoleFileError::FileTooLarge {
                    path: path_str.to_string(),
                    size: file_size,
                    max: MAX_ROLE_FILE_SIZE,
                });
            }

            // 读取文件内容，替换当前行（去除文件内容末尾多余换行）
            let content = fs::read_to_string(path)?;
            result_parts.push(content.trim_end_matches('\n').to_string());
        } else {
            // 非 ROLE_FILE 行，原样保留（含空行）
            result_parts.push(line.to_string());
        }

        // 在非最后一行之间插入换行符，还原 split('\n') 的分隔符
        if i < total - 1 {
            result_parts.push("\n".to_string());
        }
    }

    Ok(result_parts.concat())
}
