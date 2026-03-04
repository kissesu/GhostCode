//! @file lib.rs
//! @description ghostcode-config 四层 TOML 配置分层系统公共接口
//! @author Atlas.oi
//! @date 2026-03-04

pub mod layers;

use serde::Deserialize;
use std::path::Path;
use thiserror::Error;

/// 配置加载错误类型
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("读取配置文件失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML 解析失败: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("配置合并失败: {0}")]
    Merge(String),
    #[error("配置序列化失败: {0}")]
    Serialize(String),
}

/// 分发配置 domain
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributionConfig {
    /// Unix Socket 路径，用于 daemon 通信
    pub socket_path: String,
    /// PID 文件路径，用于单实例锁
    pub pid_file: String,
}

/// 运行时配置 domain
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    /// 请求超时时间（秒）
    pub request_timeout_secs: u64,
    /// 关闭超时时间（秒）
    pub shutdown_timeout_secs: u64,
    /// 最大 Actor 并发数
    pub max_actors: usize,
}

/// 安全配置 domain
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityConfig {
    /// 主权模式：strict（严格）或 permissive（宽松）
    pub sovereignty_mode: String,
    /// 允许的后端列表
    pub allowed_backends: Vec<String>,
}

/// 可观测性配置 domain
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservabilityConfig {
    /// 日志级别：trace/debug/info/warn/error
    pub log_level: String,
    /// 日志文件路径（可选）
    pub log_file: Option<String>,
}

/// 合并后的有效配置（最终生效配置）
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveConfig {
    /// 分发配置
    pub distribution: DistributionConfig,
    /// 运行时配置
    pub runtime: RuntimeConfig,
    /// 安全配置
    pub security: SecurityConfig,
    /// 可观测性配置
    pub observability: ObservabilityConfig,
}

/// 加载四层配置并返回有效配置
///
/// 优先级：runtime > project > global > default
///
/// 业务逻辑：
/// 1. 内嵌 default.toml 作为基础层
/// 2. base_dir/config.toml 作为 global 层（可选）
/// 3. project_dir/.ghostcode/config.toml 作为 project 层（可选）
/// 4. runtime_path 作为 runtime 层（可选，最高优先级）
///
/// @param base_dir - 全局配置目录（通常为 ~/.ghostcode）
/// @param project_dir - 项目根目录（可选）
/// @param runtime_path - 运行时配置文件路径（可选，最高优先级）
/// @returns 合并后的有效配置
pub fn load_effective_config(
    base_dir: &Path,
    project_dir: Option<&Path>,
    runtime_path: Option<&Path>,
) -> Result<EffectiveConfig, ConfigError> {
    layers::load_layered_config(base_dir, project_dir, runtime_path)
}
