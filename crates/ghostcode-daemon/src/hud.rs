//! HUD 状态存储与快照聚合模块
//!
//! 管理 HUD（头显状态栏）的缓存数据，提供快照聚合功能：
//! - 从 verification 状态机获取验证摘要
//! - 从 CostStore 获取成本汇总
//! - 根据 used_tokens/max_tokens 计算上下文压力级别
//! - 统计活跃 Agent 数量
//!
//! 参考: oh-my-claudecode 的 HUD 状态栏概念
//!
//! @author Atlas.oi
//! @date 2026-03-03

use serde::{Deserialize, Serialize};

use crate::server::AppState;
use crate::verification::RunStatus;

// ============================================
// 数据结构定义
// ============================================

/// 验证运行状态摘要
///
/// 从 VerificationStateStore 中提取的精简视图，供 HUD 展示
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationSummary {
    /// 运行 ID
    pub run_id: String,
    /// 所属 Group ID
    pub group_id: String,
    /// 当前状态字符串（"Running"/"Approved"/"Rejected"/"Cancelled"）
    pub status: String,
    /// 当前迭代次数
    pub iteration: u32,
    /// 最大允许迭代次数
    pub max_iterations: u32,
    /// 当前轮已通过的检查数量
    pub checks_passed: u32,
    /// 当前轮检查总数
    pub checks_total: u32,
}

/// 成本汇总视图
///
/// 从 CostStore 聚合的成本快照，用于 HUD 展示
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostSummaryView {
    /// 总成本（micro-cents）
    pub total_cost_micro: u64,
    /// 总输入 token 数
    pub total_prompt_tokens: u64,
    /// 总输出 token 数
    pub total_completion_tokens: u64,
    /// 请求总次数
    pub request_count: u64,
}

impl CostSummaryView {
    /// 创建零值视图
    pub fn zero() -> Self {
        Self {
            total_cost_micro: 0,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            request_count: 0,
        }
    }
}

/// 上下文压力视图
///
/// 描述当前 Agent 上下文使用情况，用于 HUD 展示
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextPressure {
    /// 已使用 token 数
    pub used_tokens: u64,
    /// 最大 token 数
    pub max_tokens: u64,
    /// 使用百分比（0.0-100.0）
    pub percentage: f64,
    /// 压力级别："green"/"yellow"/"red"
    pub level: String,
}

/// HUD 快照
///
/// 聚合了验证状态、成本、上下文压力和活跃 Agent 的完整视图
/// 由 build_hud_snapshot 函数同步构建，供 hud_snapshot handler 返回
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HudSnapshot {
    /// 验证状态摘要（仅在 args 中提供 group_id + run_id 时填充）
    pub verification: Option<VerificationSummary>,
    /// 成本汇总
    pub cost: CostSummaryView,
    /// 上下文压力
    pub context_pressure: ContextPressure,
    /// 活跃 Agent 数量（从 sessions 中统计）
    /// None 表示当前快照无法获取准确值（RwLock 被占用），而非真实为 0
    pub active_agents: Option<u32>,
}

// ============================================
// 核心纯函数
// ============================================

/// 根据使用百分比计算上下文压力级别（纯函数）
///
/// 业务逻辑：
/// - < 70.0  → "green"（正常使用）
/// - 70.0..=85.0 → "yellow"（需要注意）
/// - > 85.0  → "red"（已接近上限）
///
/// @param percentage - 上下文使用百分比（0.0-100.0，但接受任意 f64）
/// @returns 压力级别字符串 "green"/"yellow"/"red"
pub fn compute_context_level(percentage: f64) -> &'static str {
    if percentage < 70.0 {
        "green"
    } else if percentage <= 85.0 {
        "yellow"
    } else {
        "red"
    }
}

/// 将 RunStatus 转换为字符串表示
///
/// @param status - 运行状态枚举
/// @returns 状态字符串
fn status_to_str(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "Running",
        RunStatus::Approved => "Approved",
        RunStatus::Rejected => "Rejected",
        RunStatus::Cancelled => "Cancelled",
    }
}

// ============================================
// 主要聚合函数
// ============================================

/// 构建 HUD 快照（同步函数）
///
/// 业务逻辑：
/// 1. 从 args 中读取可选的 group_id + run_id，查询验证状态摘要
/// 2. 从 args 中读取可选的 group_id，聚合成本快照（无 group_id 则全局聚合）
/// 3. 从 args 中读取 used_tokens / max_tokens 计算上下文压力
/// 4. 通过 sessions 的 try_read 统计活跃 Agent 数量
///
/// 注意：此函数是同步函数（fn），不调用任何 async 方法
///
/// @param state - 共享应用状态
/// @param args - 请求参数（可选字段：group_id, run_id, used_tokens, max_tokens）
/// @returns HudSnapshot 聚合快照
pub fn build_hud_snapshot(state: &AppState, args: &serde_json::Value) -> HudSnapshot {
    // ============================================
    // 第一步：构建验证状态摘要
    // 仅在 args 中同时提供 group_id + run_id 时查询
    // ============================================
    let verification = if let (Some(group_id), Some(run_id)) = (
        args["group_id"].as_str(),
        args["run_id"].as_str(),
    ) {
        let store = state.verification.lock().unwrap_or_else(|e| e.into_inner());
        store.get_run(group_id, run_id).map(|run_state| {
            // 统计当前轮已通过的检查数
            use crate::verification::CheckStatus;
            let checks_passed = run_state
                .current_checks
                .iter()
                .filter(|(_, s)| matches!(s, CheckStatus::Passed))
                .count() as u32;
            let checks_total = run_state.current_checks.len() as u32;

            VerificationSummary {
                run_id: run_state.run_id.clone(),
                group_id: run_state.group_id.clone(),
                status: status_to_str(&run_state.status).to_string(),
                iteration: run_state.iteration,
                max_iterations: run_state.max_iterations,
                checks_passed,
                checks_total,
            }
        })
    } else {
        None
    };

    // ============================================
    // 第二步：构建成本汇总视图
    // 使用 args 中的可选 group_id 进行过滤聚合
    // ============================================
    let cost = {
        let group_id = args["group_id"].as_str();
        let store = state.costs.lock().unwrap_or_else(|e| e.into_inner());
        let snapshot = store.get_summary(group_id, None);
        CostSummaryView {
            total_cost_micro: snapshot.total_cost_micro,
            total_prompt_tokens: snapshot.total_prompt_tokens,
            total_completion_tokens: snapshot.total_completion_tokens,
            request_count: snapshot.request_count,
        }
    };

    // ============================================
    // 第三步：计算上下文压力
    // 从 args 中读取 used_tokens 和 max_tokens
    // ============================================
    let used_tokens = args["used_tokens"].as_u64().unwrap_or(0);
    let max_tokens = args["max_tokens"].as_u64().unwrap_or(0);

    let percentage = if max_tokens > 0 {
        (used_tokens as f64 / max_tokens as f64) * 100.0
    } else {
        0.0
    };

    let context_pressure = ContextPressure {
        used_tokens,
        max_tokens,
        percentage,
        level: compute_context_level(percentage).to_string(),
    };

    // ============================================
    // 第四步：统计活跃 Agent 数量
    // 使用 try_read 避免在同步函数中阻塞等待 async RwLock
    // 获取失败时返回 None（诚实表达"无法获取"而非伪造 0）
    // HUD 轮询机制会在下次请求时重新获取，瞬时不一致可接受
    // ============================================
    let active_agents = state
        .sessions
        .try_read()
        .ok()
        .map(|sessions| u32::try_from(sessions.len()).unwrap_or(u32::MAX));

    HudSnapshot {
        verification,
        cost,
        context_pressure,
        active_agents,
    }
}

// ============================================
// HUD 状态存储（保留空脚手架）
// 后续迭代可添加缓存功能
// ============================================

/// HUD 状态存储（空脚手架）
///
/// 负责缓存 HUD 显示所需的聚合状态
/// 后续迭代将添加：last_snapshot、update_time 等字段
pub struct HudStateStore;

impl HudStateStore {
    /// 创建新的 HUD 状态存储实例
    pub fn new() -> Self {
        Self
    }
}

impl Default for HudStateStore {
    fn default() -> Self {
        Self::new()
    }
}
