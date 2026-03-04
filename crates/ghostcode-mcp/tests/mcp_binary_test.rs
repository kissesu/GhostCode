// mcp_binary_test.rs - 验证 ghostcode-mcp crate 的 bin target 配置
//
// 解析 cargo metadata JSON，确认 ghostcode-mcp package 自身
// 具有名为 "ghostcode-mcp" 的可执行 bin target
//
// @author Atlas.oi
// @date 2026-03-04

use std::process::Command;

/// 从 cargo metadata JSON 中解析 ghostcode-mcp package 的 target kinds
///
/// 返回该 package 所有 target 的 (name, kind_list) 列表
fn get_mcp_target_kinds() -> Vec<(String, Vec<String>)> {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
            manifest,
        ])
        .output()
        .expect("cargo metadata 命令执行失败");

    assert!(
        output.status.success(),
        "cargo metadata 失败: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 手工解析 JSON，找 ghostcode-mcp package 的 targets
    // 格式: {"packages":[{"name":"ghostcode-mcp","targets":[{"name":"...","kind":["bin"]},...]},...]}
    let json_str = String::from_utf8_lossy(&output.stdout);

    // 找到 ghostcode-mcp package 的 targets 数组
    // 通过定位 "name":"ghostcode-mcp" 及其后的 "targets" 字段
    let mut result = Vec::new();

    // 定位 ghostcode-mcp package 块
    let pkg_marker = r#""name":"ghostcode-mcp""#;
    let alt_marker = r#""name": "ghostcode-mcp""#;

    let pkg_pos = json_str
        .find(pkg_marker)
        .or_else(|| json_str.find(alt_marker))
        .expect("未找到 ghostcode-mcp package");

    // 从 package 起始位置向后截取一段（避免跨 package 污染）
    // 截取 8000 字符足够覆盖一个 package 的完整定义
    let pkg_section = &json_str[pkg_pos..];
    let section_end = pkg_section.len().min(8000);
    let pkg_section = &pkg_section[..section_end];

    // 查找每个 target：解析 targets 数组中的 name 和 kind
    // cargo metadata JSON 格式中，target 对象包含 "name" 和 "kind" 字段
    // 注意：target name 字段紧跟在 target 对象开头，需要在 kind 之后向前查找
    let mut search_pos = 0;
    while let Some(kind_pos) = pkg_section[search_pos..].find(r#""kind":"#) {
        let abs_pos = search_pos + kind_pos;

        // 提取 kind 数组内容：格式为 ["bin"] 或 ["lib"] 等
        let after_kind = &pkg_section[abs_pos + 7..]; // 跳过 "kind":
        if let Some(arr_start) = after_kind.find('[') {
            if let Some(arr_end) = after_kind[arr_start..].find(']') {
                let kinds_str = &after_kind[arr_start + 1..arr_start + arr_end];
                let kinds: Vec<String> = kinds_str
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                // 向后找最近的 "name" 字段（target name 在 kind 之后）
                // cargo metadata 格式：targets 数组中 kind 通常在 name 之前
                // 但也有可能在 name 之后，需要向后搜索
                let after_arr = &after_kind[arr_start + arr_end + 1..];
                if let Some(name_pos) = after_arr.find(r#""name":"#) {
                    let after_name = &after_arr[name_pos + 7..];
                    if let Some(val_start) = after_name.find('"') {
                        if let Some(val_end) = after_name[val_start + 1..].find('"') {
                            let name =
                                after_name[val_start + 1..val_start + 1 + val_end].to_string();
                            result.push((name, kinds));
                        }
                    }
                }
            }
        }

        search_pos = abs_pos + 1;
    }

    result
}

/// 验证 ghostcode-mcp package 包含 bin target
///
/// 当 Cargo.toml 中缺少 [[bin]] section 时，
/// targets 只有 lib，此测试应失败（Red 阶段）
#[test]
fn ghostcode_mcp_has_bin_target() {
    let targets = get_mcp_target_kinds();

    // 检查是否有任何 target 的 kind 包含 "bin"
    let has_bin = targets
        .iter()
        .any(|(_, kinds)| kinds.iter().any(|k| k == "bin"));

    assert!(
        has_bin,
        "ghostcode-mcp package 缺少 bin target！\n\
         当前 targets: {:?}\n\
         修复方法：在 Cargo.toml 中添加：\n\
         [[bin]]\n\
         name = \"ghostcode-mcp\"\n\
         path = \"src/main.rs\"\n\
         并创建 src/main.rs 文件",
        targets
    );
}

/// 验证 ghostcode-mcp bin target 名称正确为 "ghostcode-mcp"
#[test]
fn ghostcode_mcp_bin_name_is_correct() {
    let targets = get_mcp_target_kinds();

    // 找到所有 bin target
    let bin_targets: Vec<&String> = targets
        .iter()
        .filter(|(_, kinds)| kinds.iter().any(|k| k == "bin"))
        .map(|(name, _)| name)
        .collect();

    assert!(
        bin_targets.contains(&&"ghostcode-mcp".to_string()),
        "ghostcode-mcp 的 bin target 名称应为 'ghostcode-mcp'，\
         当前 bin targets: {:?}",
        bin_targets
    );
}
