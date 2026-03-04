//! MCP 工具注册表测试
//!
//! 验证模块化注册表模式的正确性：
//! 1. 注册表包含全部 16 个工具
//! 2. 未知工具名返回 None
//! 3. 每个工具的 schema 包含必要字段
//! 4. 工具名唯一性保证
//!
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_mcp::tools;

/// 测试 1: 注册表包含全部 16 个工具（原有 11 + 新增 5）
#[test]
fn registry_contains_all_16_tools() {
    let reg = tools::registry();

    // 验证数量 >= 16
    assert!(
        reg.len() >= 16,
        "注册表必须包含 >= 16 个工具，实际数量: {}",
        reg.len()
    );

    // 提取所有工具名
    let names: Vec<&str> = reg.iter().map(|d| d.name).collect();

    // 验证新增的 5 个工具名存在
    let new_tools = [
        "ghostcode_group_list",
        "ghostcode_dashboard_snapshot",
        "ghostcode_verification_status",
        "ghostcode_skill_list",
        "ghostcode_team_skill_list",
    ];

    for tool_name in &new_tools {
        assert!(
            names.contains(tool_name),
            "新工具 '{}' 必须在注册表中",
            tool_name
        );
    }
}

/// 测试 2: 未知工具名通过 find_tool 返回 None
#[test]
fn unknown_tool_name_returns_none() {
    let result = tools::find_tool("nonexistent_tool");
    assert!(
        result.is_none(),
        "查找不存在的工具必须返回 None"
    );
}

/// 测试 3: 每个工具的 schema 返回非空 JSON Schema，且包含 name 和 description 字段
#[test]
fn each_tool_has_valid_schema() {
    let reg = tools::registry();

    for descriptor in reg {
        let schema = (descriptor.schema)();

        // 验证 schema 非空
        assert!(
            !schema.is_null(),
            "工具 '{}' 的 schema 不能为 null",
            descriptor.name
        );

        // 验证包含 name 字段
        assert!(
            schema.get("name").is_some(),
            "工具 '{}' 的 schema 必须包含 name 字段",
            descriptor.name
        );

        // 验证包含 description 字段
        assert!(
            schema.get("description").is_some(),
            "工具 '{}' 的 schema 必须包含 description 字段",
            descriptor.name
        );

        // 验证 name 字段内容与 descriptor.name 一致
        let schema_name = schema["name"].as_str().unwrap_or("");
        assert_eq!(
            schema_name, descriptor.name,
            "工具 schema 中的 name 必须与 descriptor.name 一致"
        );
    }
}

/// 测试 4: 注册表中所有工具名唯一，无重复
#[test]
fn tool_names_are_unique() {
    let reg = tools::registry();
    let names: Vec<&str> = reg.iter().map(|d| d.name).collect();

    // 检查每个名字是否只出现一次
    for (i, name) in names.iter().enumerate() {
        for (j, other) in names.iter().enumerate() {
            if i != j {
                assert_ne!(
                    name, other,
                    "工具名 '{}' 在注册表中重复出现（索引 {} 和 {}）",
                    name, i, j
                );
            }
        }
    }
}
