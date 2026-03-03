// @file rolefile_test.rs
// @description ghostcode-router rolefile 模块集成测试
//              测试 ROLE_FILE 注入功能的各种场景：
//              单文件注入、多文件注入、无引用不变、文件不存在报错、
//              文件过大报错、路径空白修剪
// @author Atlas.oi
// @date 2026-03-02

use ghostcode_router::rolefile::*;
use proptest::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;

// 测试单个 ROLE_FILE 引用注入
#[test]
fn inject_single_rolefile() {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "你是一个专业的分析师").unwrap();
    let path = f.path().to_str().unwrap();
    let input = format!("ROLE_FILE: {}\n其他内容", path);
    let result = inject_role_files(&input).unwrap();
    assert!(result.contains("你是一个专业的分析师"));
    assert!(!result.contains("ROLE_FILE:"));
    assert!(result.contains("其他内容"));
}

// 测试多个 ROLE_FILE 引用同时注入
#[test]
fn inject_multiple_rolefiles() {
    let mut f1 = NamedTempFile::new().unwrap();
    write!(f1, "角色1内容").unwrap();
    let mut f2 = NamedTempFile::new().unwrap();
    write!(f2, "角色2内容").unwrap();
    let input = format!(
        "ROLE_FILE: {}\n中间文本\nROLE_FILE: {}",
        f1.path().to_str().unwrap(),
        f2.path().to_str().unwrap()
    );
    let result = inject_role_files(&input).unwrap();
    assert!(result.contains("角色1内容"));
    assert!(result.contains("角色2内容"));
    assert!(result.contains("中间文本"));
    assert!(!result.contains("ROLE_FILE:"));
}

// 测试不含 ROLE_FILE 的文本注入后保持不变
#[test]
fn no_rolefile_unchanged() {
    let input = "普通文本\n没有引用";
    let result = inject_role_files(input).unwrap();
    assert_eq!(result, input);
}

// 测试文件不存在时返回错误
#[test]
fn missing_file_returns_error() {
    let input = "ROLE_FILE: /nonexistent/path/to/file.md";
    let result = inject_role_files(input);
    assert!(result.is_err());
}

// 测试文件过大时返回错误（写入 2MB 数据，超过 1MB 限制）
#[test]
fn oversized_file_returns_error() {
    let mut f = NamedTempFile::new().unwrap();
    // 写入 2MB 数据，超过 1MB 限制
    let data = vec![b'x'; 2 * 1024 * 1024];
    f.write_all(&data).unwrap();
    let input = format!("ROLE_FILE: {}", f.path().to_str().unwrap());
    let result = inject_role_files(&input);
    assert!(result.is_err());
}

// 测试路径两端空白被正确修剪
#[test]
fn whitespace_trimmed_from_path() {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "trimmed content").unwrap();
    let input = format!("ROLE_FILE:   {}   ", f.path().to_str().unwrap());
    let result = inject_role_files(&input).unwrap();
    assert!(result.contains("trimmed content"));
}

// Property-based 测试：不含 ROLE_FILE 的任意文本注入后保持不变
proptest! {
    #[test]
    fn proptest_no_rolefile_unchanged(s in "[^R]*") {
        // 生成不包含 'R' 开头的字符串，确保不含 "ROLE_FILE:"
        let result = inject_role_files(&s).unwrap();
        prop_assert_eq!(result, s);
    }
}
