//! @file layering_test.rs
//! @description ghostcode-config 四层配置分层集成测试
//!              验证 runtime > project > global > default 优先级覆盖
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_config::{load_effective_config, EffectiveConfig};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// 创建临时目录并写入指定内容的 TOML 文件
fn write_toml(dir: &Path, filename: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(filename);
    fs::write(&path, content).expect("写入 TOML 文件失败");
    path
}

/// 测试用例 1: runtime > project > global > default 四层优先级覆盖
///
/// 验证更高优先级层的配置会覆盖低优先级层
#[test]
fn runtime_overrides_project_overrides_global_overrides_default() {
    let base_dir = TempDir::new().expect("创建临时 base_dir 失败");
    let project_dir = TempDir::new().expect("创建临时 project_dir 失败");
    let runtime_dir = TempDir::new().expect("创建临时 runtime_dir 失败");

    // global 层：设置 log_level 为 debug（覆盖默认 info）
    write_toml(
        base_dir.path(),
        "config.toml",
        r#"
[observability]
log_level = "debug"
"#,
    );

    // project 层：设置 max_actors = 5（覆盖 global 未设置）
    let ghostcode_dir = project_dir.path().join(".ghostcode");
    fs::create_dir_all(&ghostcode_dir).expect("创建 .ghostcode 目录失败");
    write_toml(
        &ghostcode_dir,
        "config.toml",
        r#"
[runtime]
max_actors = 5
"#,
    );

    // runtime 层：覆盖 log_level 为 warn（最高优先级）
    let runtime_config = write_toml(
        runtime_dir.path(),
        "runtime.toml",
        r#"
[observability]
log_level = "warn"
"#,
    );

    let config = load_effective_config(
        base_dir.path(),
        Some(project_dir.path()),
        Some(&runtime_config),
    )
    .expect("加载四层配置失败");

    // runtime 层 log_level = "warn" 覆盖 global 层 "debug"
    assert_eq!(config.observability.log_level, "warn");
    // project 层 max_actors = 5 覆盖默认值
    assert_eq!(config.runtime.max_actors, 5);
}

/// 测试用例 2: 缺失某层 TOML 文件时仍成功生成 EffectiveConfig
///
/// global 和 project 层文件不存在，仅 default 层生效
#[test]
fn missing_layers_still_produce_effective_config() {
    let base_dir = TempDir::new().expect("创建临时 base_dir 失败");
    // 不创建 global config.toml，不提供 project_dir 和 runtime_path

    let config = load_effective_config(base_dir.path(), None, None)
        .expect("缺失可选层时加载配置应成功");

    // 默认配置应该生效
    assert!(!config.distribution.socket_path.is_empty());
    assert!(config.runtime.request_timeout_secs > 0);
}

/// 测试用例 3: 未知字段按严格模式报错，禁止静默吞掉拼写错误
///
/// 确保 deny_unknown_fields 生效，防止配置项拼写错误被忽略
#[test]
fn unknown_fields_cause_error() {
    let base_dir = TempDir::new().expect("创建临时 base_dir 失败");
    let project_dir = TempDir::new().expect("创建临时 project_dir 失败");

    // 设置一个未知字段（拼写错误 typo）
    write_toml(
        base_dir.path(),
        "config.toml",
        r#"
[runtime]
typo_unknown_field = "should_fail"
"#,
    );

    let result = load_effective_config(base_dir.path(), Some(project_dir.path()), None);

    // 必须返回错误，不能静默忽略
    assert!(
        result.is_err(),
        "未知字段应导致错误，但实际返回了: {:?}",
        result
    );
}

/// 测试用例 4: 完全空配置使用默认值
///
/// 所有层均为空或不存在时，使用内嵌默认配置
#[test]
fn empty_config_uses_defaults() {
    let base_dir = TempDir::new().expect("创建临时 base_dir 失败");

    // base_dir 下没有任何配置文件
    let config: EffectiveConfig = load_effective_config(base_dir.path(), None, None)
        .expect("空配置应使用默认值成功");

    // 验证各 domain 默认值存在且合理
    assert!(!config.distribution.socket_path.is_empty(), "socket_path 不应为空");
    assert!(!config.distribution.pid_file.is_empty(), "pid_file 不应为空");
    assert!(
        config.runtime.request_timeout_secs > 0,
        "request_timeout_secs 应大于 0"
    );
    assert!(
        config.runtime.shutdown_timeout_secs > 0,
        "shutdown_timeout_secs 应大于 0"
    );
    assert!(
        config.runtime.max_actors > 0,
        "max_actors 应大于 0"
    );
    assert!(
        !config.security.sovereignty_mode.is_empty(),
        "sovereignty_mode 不应为空"
    );
    assert!(
        !config.observability.log_level.is_empty(),
        "log_level 不应为空"
    );
}
