/**
 * @file dag_test.rs
 * @description DAG 拓扑排序算法集成测试
 *              覆盖空输入、单节点、独立节点、线性链、菱形依赖、环检测、缺失依赖等场景
 *              以及基于 proptest 的属性测试（完备性 + 拓扑序有效性）
 * @author Atlas.oi
 * @date 2026-03-02
 */

use ghostcode_router::dag::*;
use proptest::prelude::*;

// ============================================
// 基础功能测试
// ============================================

#[test]
fn empty_input_empty_output() {
    let result = topological_sort(vec![]).unwrap();
    assert!(result.is_empty());
}

#[test]
fn single_task_one_layer() {
    let tasks = vec![TaskNode { id: "a".into(), dependencies: vec![] }];
    let layers = topological_sort(tasks).unwrap();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].len(), 1);
    assert_eq!(layers[0][0].id, "a");
}

#[test]
fn independent_tasks_all_first_layer() {
    let tasks = vec![
        TaskNode { id: "a".into(), dependencies: vec![] },
        TaskNode { id: "b".into(), dependencies: vec![] },
        TaskNode { id: "c".into(), dependencies: vec![] },
    ];
    let layers = topological_sort(tasks).unwrap();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].len(), 3);
}

#[test]
fn linear_chain_n_layers() {
    // A -> B -> C
    let tasks = vec![
        TaskNode { id: "a".into(), dependencies: vec![] },
        TaskNode { id: "b".into(), dependencies: vec!["a".into()] },
        TaskNode { id: "c".into(), dependencies: vec!["b".into()] },
    ];
    let layers = topological_sort(tasks).unwrap();
    assert_eq!(layers.len(), 3);
    assert_eq!(layers[0][0].id, "a");
    assert_eq!(layers[1][0].id, "b");
    assert_eq!(layers[2][0].id, "c");
}

#[test]
fn diamond_dependency() {
    // A -> B, A -> C, B -> D, C -> D
    let tasks = vec![
        TaskNode { id: "a".into(), dependencies: vec![] },
        TaskNode { id: "b".into(), dependencies: vec!["a".into()] },
        TaskNode { id: "c".into(), dependencies: vec!["a".into()] },
        TaskNode { id: "d".into(), dependencies: vec!["b".into(), "c".into()] },
    ];
    let layers = topological_sort(tasks).unwrap();
    assert_eq!(layers.len(), 3);
    // 第 0 层：[A]
    assert_eq!(layers[0].len(), 1);
    assert_eq!(layers[0][0].id, "a");
    // 第 1 层：[B, C]（顺序可能不同）
    assert_eq!(layers[1].len(), 2);
    let layer1_ids: Vec<&str> = layers[1].iter().map(|t| t.id.as_str()).collect();
    assert!(layer1_ids.contains(&"b"));
    assert!(layer1_ids.contains(&"c"));
    // 第 2 层：[D]
    assert_eq!(layers[2].len(), 1);
    assert_eq!(layers[2][0].id, "d");
}

#[test]
fn cycle_detected() {
    // A -> B -> C -> A（形成环）
    let tasks = vec![
        TaskNode { id: "a".into(), dependencies: vec!["c".into()] },
        TaskNode { id: "b".into(), dependencies: vec!["a".into()] },
        TaskNode { id: "c".into(), dependencies: vec!["b".into()] },
    ];
    let result = topological_sort(tasks);
    assert!(matches!(result, Err(DagError::CycleDetected(_))));
}

#[test]
fn missing_dependency_detected() {
    // a 依赖不存在的节点 x
    let tasks = vec![
        TaskNode { id: "a".into(), dependencies: vec!["x".into()] },
    ];
    let result = topological_sort(tasks);
    assert!(matches!(result, Err(DagError::MissingDependency { .. })));
}

// ============================================
// Proptest 属性测试
// 生成策略：N 个节点，节点 i 只能依赖编号 0..i 的节点，保证无环
// ============================================

/// 生成随机 DAG：节点数在 1..=20 之间
/// 每个节点的 ID 为 "t{i}"，依赖项从编号更小的节点中随机选取
fn random_dag_strategy() -> impl Strategy<Value = Vec<TaskNode>> {
    // 节点数 1 到 20
    (1usize..=20).prop_flat_map(|n| {
        // 对每个节点 i (1..n)，从 0..i 的子集中随机选取依赖
        let dep_strategies: Vec<_> = (0..n)
            .map(|i| {
                if i == 0 {
                    // 第 0 个节点没有依赖
                    Just(vec![]).boxed()
                } else {
                    // 从 0..i 范围内随机选取子集作为依赖
                    proptest::collection::vec(0usize..i, 0..=i)
                        .prop_map(|mut deps| {
                            // 去重
                            deps.sort_unstable();
                            deps.dedup();
                            deps
                        })
                        .boxed()
                }
            })
            .collect();
        dep_strategies.prop_map(move |dep_lists| {
            dep_lists
                .into_iter()
                .enumerate()
                .map(|(i, deps)| TaskNode {
                    id: format!("t{}", i),
                    dependencies: deps.into_iter().map(|d| format!("t{}", d)).collect(),
                })
                .collect::<Vec<TaskNode>>()
        })
    })
}

proptest! {
    /// 属性测试 1：完备性 - flatten 后的结果长度等于输入长度
    #[test]
    fn prop_completeness(tasks in random_dag_strategy()) {
        let n: usize = tasks.len();
        let result = topological_sort(tasks).expect("随机 DAG 不应有环或缺失依赖");
        let total: usize = result.iter().map(|layer: &Vec<TaskNode>| layer.len()).sum();
        prop_assert_eq!(total, n);
    }

    /// 属性测试 2：拓扑序有效性 - 每个任务的依赖必须出现在更早的层
    #[test]
    fn prop_topological_order(tasks in random_dag_strategy()) {
        // 先记录依赖关系（id -> dependencies）
        let dep_map: std::collections::HashMap<String, Vec<String>> = tasks
            .iter()
            .map(|t: &TaskNode| (t.id.clone(), t.dependencies.clone()))
            .collect();

        let result = topological_sort(tasks).expect("随机 DAG 不应有环或缺失依赖");

        // 建立 id -> 层索引的映射
        let mut id_to_layer: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (layer_idx, layer) in result.iter().enumerate() {
            for task in layer {
                id_to_layer.insert(task.id.clone(), layer_idx);
            }
        }

        // 验证每个任务的依赖都在更早的层
        for (task_id, deps) in &dep_map {
            let task_layer = id_to_layer[task_id];
            for dep_id in deps {
                let dep_layer = id_to_layer[dep_id];
                prop_assert!(
                    dep_layer < task_layer,
                    "任务 {} 在第 {} 层，但其依赖 {} 在第 {} 层（应更早）",
                    task_id, task_layer, dep_id, dep_layer
                );
            }
        }
    }
}
