/**
 * @file types.ts
 * @description TS 验证客户端类型定义
 *              与 Rust 侧 verification.rs 的类型保持一致
 *              参考: crates/ghostcode-daemon/src/verification.rs - RunState 结构
 * @author Atlas.oi
 * @date 2026-03-03
 */
/** 验证检查类型（7 种，与 Rust VerificationCheckKind 枚举对齐） */
type VerificationCheckKind = "Build" | "Test" | "Lint" | "Functionality" | "Architect" | "Todo" | "ErrorFree";
/** 单项检查状态（与 Rust CheckStatus 枚举对齐） */
type CheckStatus = "Pending" | "Passed" | {
    Failed: string;
};
/** 运行整体状态（与 Rust RunStatus 枚举对齐） */
type RunStatus = "Running" | "Approved" | "Rejected" | "Cancelled";
/** 单项检查结果 */
interface VerificationCheckResult {
    kind: VerificationCheckKind;
    status: CheckStatus;
}
/** 单轮迭代记录（记录每次 Ralph 循环的检查结果） */
interface VerificationIteration {
    checks: VerificationCheckResult[];
    failure_reasons: string[];
}
/** 运行状态快照（与 Rust RunState struct 完全对齐） */
interface RunState {
    /** 运行唯一标识符 */
    run_id: string;
    /** 所属 Agent 分组 */
    group_id: string;
    /** 当前整体运行状态 */
    status: RunStatus;
    /** 当前迭代轮次（从 0 开始） */
    iteration: number;
    /** 最大迭代次数上限 */
    max_iterations: number;
    /** 当前各项检查状态，格式为 [检查类型, 检查状态] 元组数组 */
    current_checks: [VerificationCheckKind, CheckStatus][];
    /** 历史迭代记录列表 */
    history: VerificationIteration[];
}
/** 验证报告输出格式 */
type ReportFormat = "markdown" | "json";
/** 验证判定结果（语义化，用于外部消费） */
type Verdict = "approved" | "rejected" | "cancelled" | "in_progress";

export type { CheckStatus, ReportFormat, RunState, RunStatus, Verdict, VerificationCheckKind, VerificationCheckResult, VerificationIteration };
