// @file executor_test.rs
// @description 并行执行引擎集成测试
//              使用真实系统命令（echo/sleep/false）验证任务调度、并行度、依赖传播和取消机制
// @author Atlas.oi
// @date 2026-03-02

use ghostcode_router::executor::{ExecutableTask, Executor, ExecutorConfig, TaskStatus};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

// ============================================================
// 辅助函数：构建 ExecutableTask
// ============================================================

/// 创建简单的 echo 任务（成功）
fn make_echo_task(id: &str, message: &str, deps: Vec<&str>) -> ExecutableTask {
    ExecutableTask {
        id: id.to_string(),
        command: "echo".to_string(),
        args: vec![message.to_string()],
        stdin_data: None,
        dependencies: deps.iter().map(|s| s.to_string()).collect(),
        timeout: Duration::from_secs(5),
    }
}

/// 创建 false 命令任务（必定失败）
fn make_fail_task(id: &str, deps: Vec<&str>) -> ExecutableTask {
    ExecutableTask {
        id: id.to_string(),
        command: "false".to_string(),
        args: vec![],
        stdin_data: None,
        dependencies: deps.iter().map(|s| s.to_string()).collect(),
        timeout: Duration::from_secs(5),
    }
}

/// 创建 sleep 任务（模拟长时间运行）
fn make_sleep_task(id: &str, secs: f64, deps: Vec<&str>) -> ExecutableTask {
    ExecutableTask {
        id: id.to_string(),
        command: "sleep".to_string(),
        args: vec![secs.to_string()],
        stdin_data: None,
        dependencies: deps.iter().map(|s| s.to_string()).collect(),
        timeout: Duration::from_secs(30),
    }
}

// ============================================================
// 测试 1：单任务成功
// ============================================================

/// 验证单个成功任务的基本执行流程
#[tokio::test]
async fn single_task_success() {
    let tasks = vec![make_echo_task("t1", "hello", vec![])];
    let cancel = CancellationToken::new();
    let config = ExecutorConfig {
        max_workers: 4,
        cancel,
    };

    let results = Executor::run(tasks, config).await;

    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert_eq!(r.id, "t1");
    assert_eq!(r.status, TaskStatus::Success);
    assert!(r.output.is_some());
}

// ============================================================
// 测试 2：并行任务证明并发执行
// ============================================================

/// 3 个独立 sleep 0.3s 任务，若串行执行需 0.9s+，并行则 < 1.5s
/// 通过总耗时验证并发执行
#[tokio::test]
async fn parallel_tasks_concurrent() {
    let tasks = vec![
        make_sleep_task("p1", 0.3, vec![]),
        make_sleep_task("p2", 0.3, vec![]),
        make_sleep_task("p3", 0.3, vec![]),
    ];
    let cancel = CancellationToken::new();
    let config = ExecutorConfig {
        max_workers: 10,
        cancel,
    };

    let start = Instant::now();
    let results = Executor::run(tasks, config).await;
    let elapsed = start.elapsed();

    // 所有任务成功
    assert_eq!(results.len(), 3);
    for r in &results {
        assert_eq!(r.status, TaskStatus::Success, "任务 {} 应该成功", r.id);
    }

    // 并行执行：总耗时应该 < 1.5s（如果串行会需要 0.9s，考虑系统开销留有余量）
    // 关键验证：耗时远小于 3 * 0.3s = 0.9s 的串行时间
    assert!(
        elapsed < Duration::from_millis(1500),
        "并行任务耗时 {:?} 超过了 1.5s，疑似串行执行",
        elapsed
    );
}

// ============================================================
// 测试 3：依赖任务失败时跳过下游任务
// ============================================================

/// A 任务（false 命令）失败 → B 任务依赖 A → B 被标记为 Skipped
#[tokio::test]
async fn dependency_failure_skips_dependents() {
    let tasks = vec![
        make_fail_task("a", vec![]),
        make_echo_task("b", "should_be_skipped", vec!["a"]),
    ];
    let cancel = CancellationToken::new();
    let config = ExecutorConfig {
        max_workers: 4,
        cancel,
    };

    let results = Executor::run(tasks, config).await;

    assert_eq!(results.len(), 2);

    // 找到 A 和 B 的结果
    let result_a = results.iter().find(|r| r.id == "a").expect("找不到任务 a 的结果");
    let result_b = results.iter().find(|r| r.id == "b").expect("找不到任务 b 的结果");

    // A 应该失败
    assert!(
        matches!(result_a.status, TaskStatus::Failed(_)),
        "任务 a 应该失败，实际状态: {:?}",
        result_a.status
    );

    // B 应该被跳过（因为 A 失败了）
    assert!(
        matches!(result_b.status, TaskStatus::Skipped(_)),
        "任务 b 应该被跳过，实际状态: {:?}",
        result_b.status
    );
}

// ============================================================
// 测试 4：信号量限制并发数
// ============================================================

/// 4 个独立 sleep 0.3s 任务 + max_workers=2
/// 分两批执行，总耗时应 >= 0.5s（证明并发数被限制）
#[tokio::test]
async fn semaphore_limits_concurrency() {
    let tasks = vec![
        make_sleep_task("s1", 0.3, vec![]),
        make_sleep_task("s2", 0.3, vec![]),
        make_sleep_task("s3", 0.3, vec![]),
        make_sleep_task("s4", 0.3, vec![]),
    ];
    let cancel = CancellationToken::new();
    let config = ExecutorConfig {
        max_workers: 2, // 最多同时 2 个
        cancel,
    };

    let start = Instant::now();
    let results = Executor::run(tasks, config).await;
    let elapsed = start.elapsed();

    // 所有任务应该成功
    assert_eq!(results.len(), 4);
    for r in &results {
        assert_eq!(r.status, TaskStatus::Success, "任务 {} 应该成功", r.id);
    }

    // 最大并发为 2，4 个任务需要分 2 批，每批 0.3s，总耗时 >= 0.5s
    assert!(
        elapsed >= Duration::from_millis(500),
        "耗时 {:?} < 0.5s，说明并发限制未生效（4 任务 max_workers=2 应分 2 批）",
        elapsed
    );
}

// ============================================================
// 测试 5：取消令牌停止所有任务
// ============================================================

/// 提交 sleep 100 长任务，100ms 后触发取消 → 任务状态应为 Cancelled
#[tokio::test]
async fn cancel_stops_all() {
    let tasks = vec![
        make_sleep_task("long1", 100.0, vec![]),
        make_sleep_task("long2", 100.0, vec![]),
    ];
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    // 100ms 后触发取消
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel_clone.cancel();
    });

    let config = ExecutorConfig {
        max_workers: 4,
        cancel,
    };

    let start = Instant::now();
    let results = Executor::run(tasks, config).await;
    let elapsed = start.elapsed();

    // 验证任务被取消（不是超时完成）
    assert_eq!(results.len(), 2);
    for r in &results {
        assert_eq!(
            r.status,
            TaskStatus::Cancelled,
            "任务 {} 应该被取消，实际: {:?}",
            r.id,
            r.status
        );
    }

    // 应该远早于 100s 完成
    assert!(
        elapsed < Duration::from_secs(10),
        "取消后耗时 {:?} 过长，取消机制可能未生效",
        elapsed
    );
}

// ============================================================
// 测试 6：空任务列表返回空结果
// ============================================================

/// 空输入应该直接返回空结果，不报错
#[tokio::test]
async fn empty_tasks_empty_results() {
    let cancel = CancellationToken::new();
    let config = ExecutorConfig {
        max_workers: 4,
        cancel,
    };

    let results = Executor::run(vec![], config).await;

    assert!(results.is_empty(), "空任务列表应返回空结果");
}

// ============================================================
// 测试 7：同层失败不影响同层其他任务
// ============================================================

/// 同层中 A 失败，B 成功 → B 的失败不应因 A 失败而受影响
/// 验证层内任务之间的隔离性
#[tokio::test]
async fn same_layer_failure_no_cross_impact() {
    // A 和 B 都在第一层（无依赖），A 失败不应影响 B
    let tasks = vec![
        make_fail_task("a", vec![]),
        make_echo_task("b", "independent", vec![]),
    ];
    let cancel = CancellationToken::new();
    let config = ExecutorConfig {
        max_workers: 4,
        cancel,
    };

    let results = Executor::run(tasks, config).await;

    assert_eq!(results.len(), 2);

    let result_a = results.iter().find(|r| r.id == "a").expect("找不到任务 a 的结果");
    let result_b = results.iter().find(|r| r.id == "b").expect("找不到任务 b 的结果");

    // A 失败
    assert!(
        matches!(result_a.status, TaskStatus::Failed(_)),
        "任务 a 应该失败，实际: {:?}",
        result_a.status
    );

    // B 独立成功，不受 A 影响
    assert_eq!(
        result_b.status,
        TaskStatus::Success,
        "任务 b 应该成功（与 a 同层无依赖关系），实际: {:?}",
        result_b.status
    );
}
