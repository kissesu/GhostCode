//! @file layers.rs
//! @description TOML 配置分层加载与合并逻辑
//!              实现 default > global > project > runtime 四层合并
//! @author Atlas.oi
//! @date 2026-03-04

use crate::{ConfigError, EffectiveConfig};
use std::path::Path;

/// 内嵌默认配置文件内容
const DEFAULT_CONFIG: &str = include_str!("../../../config/default.toml");

/// 加载单层 TOML 文件
///
/// 文件不存在时返回空 toml::Value（不报错，允许层缺失）
///
/// @param path - TOML 文件路径
/// @returns 解析后的 toml::Value，或错误
fn load_toml_layer(path: &Path) -> Result<toml::Value, ConfigError> {
    if !path.exists() {
        // 层不存在时返回空表，允许可选层缺失
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    let content = std::fs::read_to_string(path)?;
    let value: toml::Value = toml::from_str(&content)?;
    Ok(value)
}

/// 合并两个 TOML Value，高优先级覆盖低优先级
///
/// 合并规则：
/// - 两个 Table 递归合并
/// - 非 Table 值直接以高优先级覆盖
///
/// @param base - 低优先级基础值
/// @param overlay - 高优先级覆盖值
/// @returns 合并结果
fn merge_two(base: toml::Value, overlay: toml::Value) -> toml::Value {
    match (base, overlay) {
        (toml::Value::Table(mut base_map), toml::Value::Table(overlay_map)) => {
            // 递归合并 Table
            for (key, overlay_val) in overlay_map {
                let merged = if let Some(base_val) = base_map.remove(&key) {
                    merge_two(base_val, overlay_val)
                } else {
                    overlay_val
                };
                base_map.insert(key, merged);
            }
            toml::Value::Table(base_map)
        }
        // 非 Table 直接以覆盖层为准
        (_, overlay) => overlay,
    }
}

/// 合并多层 TOML Value（按优先级从低到高排列）
///
/// @param layers - 配置层数组，从低优先级到高优先级排列
/// @returns 合并后的 toml::Value
pub fn merge_layers(layers: &[toml::Value]) -> toml::Value {
    layers.iter().cloned().fold(
        toml::Value::Table(toml::map::Map::new()),
        merge_two,
    )
}

/// 加载四层配置并合并为 EffectiveConfig
///
/// 层优先级（从低到高）：
/// 1. default（内嵌 config/default.toml）
/// 2. global（base_dir/config.toml）
/// 3. project（project_dir/.ghostcode/config.toml）
/// 4. runtime（显式传入路径，最高优先级）
///
/// @param base_dir - 全局配置目录
/// @param project_dir - 项目根目录（可选）
/// @param runtime_path - 运行时配置文件路径（可选）
/// @returns 合并后的 EffectiveConfig
pub fn load_layered_config(
    base_dir: &Path,
    project_dir: Option<&Path>,
    runtime_path: Option<&Path>,
) -> Result<EffectiveConfig, ConfigError> {
    // ============================================
    // 第一层：内嵌默认配置（始终存在）
    // ============================================
    let default_layer: toml::Value = toml::from_str(DEFAULT_CONFIG)?;

    // ============================================
    // 第二层：全局配置（base_dir/config.toml）
    // ============================================
    let global_path = base_dir.join("config.toml");
    let global_layer = load_toml_layer(&global_path)?;

    // ============================================
    // 第三层：项目配置（project_dir/.ghostcode/config.toml）
    // ============================================
    let project_layer = if let Some(proj_dir) = project_dir {
        let project_config_path = proj_dir.join(".ghostcode").join("config.toml");
        load_toml_layer(&project_config_path)?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    // ============================================
    // 第四层：运行时配置（显式传入，最高优先级）
    // ============================================
    let runtime_layer = if let Some(rt_path) = runtime_path {
        load_toml_layer(rt_path)?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    // ============================================
    // 合并四层（低优先级 → 高优先级）
    // ============================================
    let merged = merge_layers(&[default_layer, global_layer, project_layer, runtime_layer]);

    // ============================================
    // 将合并后的 toml::Value 反序列化为 EffectiveConfig
    // deny_unknown_fields 在 EffectiveConfig 结构体上声明，此处自动生效
    // ============================================
    let config: EffectiveConfig = merged
        .try_into()
        .map_err(|e: toml::de::Error| ConfigError::TomlParse(e))?;

    Ok(config)
}
