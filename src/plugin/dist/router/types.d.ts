/**
 * @file 路由类型定义
 * @description 多模型路由的类型定义，包含路由配置、决策结果等
 * @author Atlas.oi
 * @date 2026-03-02
 */
/** 路由策略 */
type RoutingStrategy = 'parallel' | 'fallback' | 'round-robin';
/** 后端名称 */
type BackendName = 'codex' | 'claude' | 'gemini';
/** 模型路由配置 */
interface ModelRouting {
    frontend: {
        primary: BackendName;
        strategy: RoutingStrategy;
    };
    backend: {
        primary: BackendName;
        strategy: RoutingStrategy;
    };
    mode: 'smart' | 'parallel' | 'sequential';
}
/** 路由决策结果 */
interface RouteDecision {
    /** 目标后端 */
    backend: BackendName;
    /** 路由原因（用于透明度显示） */
    reason: string;
    /** 置信度 0-1 */
    confidence: number;
}

export type { BackendName, ModelRouting, RouteDecision, RoutingStrategy };
