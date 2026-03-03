//! @file cost.rs
//! @description 成本追踪引擎
//!              记录 Agent API 调用成本，支持按 group_id/task_id/model 三维聚合
//!              内部使用 u64 micro-cents 避免浮点误差，支持 PBT 代数验证
//! @author Atlas.oi
//! @date 2026-03-03

use std::collections::HashMap;

// ============================================================
// 成本来源精度标注
// 三级精度：精确值 / 厂商上报 / 估算值
// ============================================================

/// 成本数据来源精度
///
/// 用于标注每条使用记录的成本数据可信程度：
/// - Exact: 本地精确计算（如通过 token 计数器）
/// - VendorReported: 由模型厂商 API 返回
/// - Estimated: 未知模型，使用默认价格估算
#[derive(Debug, Clone, PartialEq)]
pub enum CostSource {
    /// 本地精确计算
    Exact,
    /// 厂商 API 返回值
    VendorReported,
    /// 未知模型，使用默认价格估算
    Estimated,
}

// ============================================================
// 单次使用记录
// ============================================================

/// 单次 API 调用的使用记录
///
/// 包含完整的三维标识（group/task/model）和 token 计数
#[derive(Debug, Clone)]
pub struct UsageRecord {
    /// 所属 Agent 组 ID
    pub group_id: String,
    /// 所属任务 ID
    pub task_id: String,
    /// 使用的模型名称
    pub model: String,
    /// 输入 token 数量
    pub prompt_tokens: u32,
    /// 输出 token 数量
    pub completion_tokens: u32,
    /// 成本来源精度标注
    pub source: CostSource,
}

// ============================================================
// 模型价格配置
// 单位：micro-cents per token（1 美元 = 100 美分 = 100_000_000 micro-cents）
// ============================================================

/// 模型单 token 价格（micro-cents per token）
///
/// 使用整数避免浮点误差，1 美分 = 1_000_000 micro-cents
/// 故 $15/M tokens = 15_000 micro-cents/token
#[derive(Debug, Clone)]
pub struct ModelPricing {
    /// 每 token 输入价格（micro-cents）
    pub prompt_price: u64,
    /// 每 token 输出价格（micro-cents）
    pub completion_price: u64,
}

/// 全局模型价格表
///
/// 内置已知模型价格，未知模型使用 default_pricing 并标记为 Estimated
pub struct PricingTable {
    /// 已知模型价格映射
    prices: HashMap<String, ModelPricing>,
    /// 未知模型的默认估算价格
    default_pricing: ModelPricing,
}

impl PricingTable {
    /// 创建内置价格表
    ///
    /// 内置价格（micro-cents per token）：
    /// - claude-opus:   prompt=15_000, completion=75_000
    /// - claude-sonnet: prompt=3_000,  completion=15_000
    /// - codex:         prompt=0,      completion=0（免费）
    /// - gemini-pro:    prompt=1_250,  completion=10_000
    /// - default:       prompt=5_000,  completion=15_000
    pub fn new() -> Self {
        let mut prices = HashMap::new();

        // Claude Opus: $15/M input, $75/M output
        prices.insert(
            "claude-opus".to_string(),
            ModelPricing { prompt_price: 15_000, completion_price: 75_000 },
        );

        // Claude Sonnet: $3/M input, $15/M output
        prices.insert(
            "claude-sonnet".to_string(),
            ModelPricing { prompt_price: 3_000, completion_price: 15_000 },
        );

        // Codex: 免费
        prices.insert(
            "codex".to_string(),
            ModelPricing { prompt_price: 0, completion_price: 0 },
        );

        // Gemini Pro: $1.25/M input, $10/M output
        prices.insert(
            "gemini-pro".to_string(),
            ModelPricing { prompt_price: 1_250, completion_price: 10_000 },
        );

        Self {
            prices,
            // 未知模型默认价格：$5/M input, $15/M output
            default_pricing: ModelPricing { prompt_price: 5_000, completion_price: 15_000 },
        }
    }

    /// 查询模型价格
    ///
    /// 已知模型返回精确价格，未知模型返回默认估算价格
    pub fn get(&self, model: &str) -> (&ModelPricing, bool) {
        match self.prices.get(model) {
            Some(pricing) => (pricing, true),
            None => (&self.default_pricing, false),
        }
    }
}

impl Default for PricingTable {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// 聚合快照
// ============================================================

/// 某维度下的成本聚合快照
///
/// 包含 token 总量、成本总量和请求次数
/// 所有字段均为 u64，使用 saturating_add 防止溢出
#[derive(Debug, Clone, PartialEq)]
pub struct CostSnapshot {
    /// 输入 token 总数
    pub total_prompt_tokens: u64,
    /// 输出 token 总数
    pub total_completion_tokens: u64,
    /// 总成本（micro-cents）
    pub total_cost_micro: u64,
    /// 请求总次数
    pub request_count: u64,
}

impl CostSnapshot {
    /// 创建零值快照（代数零元）
    pub fn zero() -> Self {
        Self {
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            total_cost_micro: 0,
            request_count: 0,
        }
    }
}

// ============================================================
// 核心纯函数（便于 PBT 代数属性验证）
// ============================================================

/// 计算单条 usage 的成本（micro-cents）
///
/// 业务逻辑：
/// 1. 查找模型价格（未知模型使用默认价格）
/// 2. 使用 saturating_mul 计算各 token 成本，防止 u64 溢出
/// 3. 使用 saturating_add 合并两部分成本
///
/// @param model - 模型名称
/// @param usage - 使用记录（取 token 数量）
/// @param pricing - 全局价格表
/// @returns 成本（micro-cents）
pub fn compute_cost(model: &str, usage: &UsageRecord, pricing: &PricingTable) -> u64 {
    let (model_pricing, _is_known) = pricing.get(model);

    // 使用 saturating_mul/add 防止溢出
    let prompt_cost = (usage.prompt_tokens as u64).saturating_mul(model_pricing.prompt_price);
    let completion_cost = (usage.completion_tokens as u64).saturating_mul(model_pricing.completion_price);
    prompt_cost.saturating_add(completion_cost)
}

/// 将单条 usage 累加到快照（纯函数，返回新快照）
///
/// 业务逻辑：
/// 1. 计算本次 usage 的成本
/// 2. 用 saturating_add 更新所有累计字段
/// 3. 请求数量 +1
///
/// @param snapshot - 当前快照（不可变引用）
/// @param usage - 待累加的使用记录
/// @param pricing - 全局价格表
/// @returns 新的累加快照
pub fn apply_usage(snapshot: &CostSnapshot, usage: &UsageRecord, pricing: &PricingTable) -> CostSnapshot {
    let cost = compute_cost(&usage.model, usage, pricing);

    CostSnapshot {
        total_prompt_tokens: snapshot.total_prompt_tokens.saturating_add(usage.prompt_tokens as u64),
        total_completion_tokens: snapshot.total_completion_tokens.saturating_add(usage.completion_tokens as u64),
        total_cost_micro: snapshot.total_cost_micro.saturating_add(cost),
        request_count: snapshot.request_count.saturating_add(1),
    }
}

/// 合并两个快照（满足结合律和交换律）
///
/// 业务逻辑：
/// - 所有数值字段使用 saturating_add 防溢出
/// - 满足代数性质：结合律、交换律、零元
///
/// @param lhs - 左侧快照
/// @param rhs - 右侧快照
/// @returns 合并后的新快照
pub fn merge(lhs: &CostSnapshot, rhs: &CostSnapshot) -> CostSnapshot {
    CostSnapshot {
        total_prompt_tokens: lhs.total_prompt_tokens.saturating_add(rhs.total_prompt_tokens),
        total_completion_tokens: lhs.total_completion_tokens.saturating_add(rhs.total_completion_tokens),
        total_cost_micro: lhs.total_cost_micro.saturating_add(rhs.total_cost_micro),
        request_count: lhs.request_count.saturating_add(rhs.request_count),
    }
}

// ============================================================
// CostStore：成本存储（有状态版本）
// ============================================================

/// 成本存储
///
/// 管理所有 API 调用的使用记录，支持三维聚合（group_id / task_id / model）
/// 内部使用 Vec<UsageRecord> 存储原始记录，按需聚合
pub struct CostStore {
    /// 所有使用记录（追加写入）
    records: Vec<UsageRecord>,
    /// 全局价格表
    pricing: PricingTable,
}

impl CostStore {
    /// 创建新的成本存储实例
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            pricing: PricingTable::default(),
        }
    }

    /// 记录一次 API 使用
    ///
    /// 业务逻辑：
    /// 1. 如果是未知模型，自动将 source 标记为 Estimated（若调用方传入 Exact）
    /// 2. 追加到内部记录列表
    ///
    /// @param usage - 使用记录
    pub fn record_usage(&mut self, usage: UsageRecord) {
        // ============================================
        // 未知模型自动标记为 Estimated
        // 确保 source 字段精度标注的一致性
        // ============================================
        let (_pricing, is_known) = self.pricing.get(&usage.model);
        let usage = if !is_known {
            UsageRecord {
                source: CostSource::Estimated,
                ..usage
            }
        } else {
            usage
        };

        self.records.push(usage);
    }

    /// 按 group_id / task_id 过滤并聚合快照
    ///
    /// 业务逻辑：
    /// 1. 按 group_id 过滤（None 表示不过滤）
    /// 2. 按 task_id 过滤（None 表示不过滤）
    /// 3. 折叠所有匹配记录为单个快照
    ///
    /// @param group_id - 可选 group 过滤条件
    /// @param task_id - 可选 task 过滤条件
    /// @returns 聚合快照
    pub fn get_summary(&self, group_id: Option<&str>, task_id: Option<&str>) -> CostSnapshot {
        let zero = CostSnapshot::zero();
        self.records
            .iter()
            .filter(|r| group_id.map_or(true, |g| r.group_id == g))
            .filter(|r| task_id.map_or(true, |t| r.task_id == t))
            .fold(zero, |acc, r| apply_usage(&acc, r, &self.pricing))
    }

    /// 按模型聚合统计（限定 group_id 下，可选 task_id 过滤）
    ///
    /// 业务逻辑：
    /// 1. 按 group_id 过滤
    /// 2. 按 task_id 过滤（None 表示不过滤）
    /// 3. 按 model 分组，各模型独立聚合
    /// 4. 返回 model -> CostSnapshot 的 HashMap
    ///
    /// @param group_id - 所属 Agent 组 ID
    /// @param task_id - 可选 task 过滤条件
    /// @returns 各模型的聚合快照映射
    pub fn get_summary_by_model(&self, group_id: &str, task_id: Option<&str>) -> HashMap<String, CostSnapshot> {
        let mut result: HashMap<String, CostSnapshot> = HashMap::new();

        for record in self.records
            .iter()
            .filter(|r| r.group_id == group_id)
            .filter(|r| task_id.map_or(true, |t| r.task_id == t))
        {
            let entry = result.entry(record.model.clone()).or_insert_with(CostSnapshot::zero);
            // 就地更新该模型的快照
            *entry = apply_usage(entry, record, &self.pricing);
        }

        result
    }
}

impl Default for CostStore {
    fn default() -> Self {
        Self::new()
    }
}
