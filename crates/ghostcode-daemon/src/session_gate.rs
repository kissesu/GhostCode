//! Session Gate 存储器
//!
//! 实现 SessionGateStore：多模型审查强制门控的核心存储层。
//! 通过 Mutex<HashMap> 保证并发安全，通过 state file 支持 Stop Hook 检测。
//!
//! 设计原则：
//! - open() 创建 session + 写 state file
//! - submit() 记录模型输出 + 自动检测完成状态 + 自动关闭并返回 CombinedOutput
//! - close() 手动关闭（可选，submit 已自动处理）
//! - abort() 无条件终止 session + 清理 state file
//!
//! 自动关闭机制：
//! submit() 在记录模型输出后，检测所有 required_models 是否都已提交。
//! 若完整（或有 bypass）→ 自动关闭 session 并返回 SubmitResult::Complete(CombinedOutput)。
//! 若尚不完整 → 返回 SubmitResult::Pending，session 保持打开。
//! 调用方无需手动调用 close()，除非需要强制关闭未完成的 session。
//!
//! @author Atlas.oi
//! @date 2026-03-07

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;
use uuid::Uuid;

// ============================================
// 错误类型定义
// ============================================

/// Session Gate 操作错误
///
/// 覆盖四种失败场景：
/// - NotFound: session 不存在或已被清理
/// - SessionIncomplete: 尚有模型未提交，不允许 close
/// - AlreadyClosed: session 已经关闭（保留，当前测试未覆盖）
/// - InvalidModel: 提交的模型不在 required_models 列表中
#[derive(Debug)]
pub enum SessionGateError {
    /// session 不存在
    NotFound,
    /// session 尚不完整，缺少指定模型的提交
    SessionIncomplete {
        /// 尚未提交的模型名列表
        missing_models: Vec<String>,
    },
    /// session 已经关闭
    AlreadyClosed(String),
    /// 提交的模型不在 required_models 列表中（W2 修复）
    InvalidModel {
        /// 被拒绝的模型名
        model: String,
        /// 该 session 允许的模型列表
        allowed: Vec<String>,
    },
    /// required_models 为空，无法创建有意义的 session（W3 修复）
    EmptyRequiredModels,
}

// ============================================
// 提交记录与合并输出
// ============================================

/// 单个模型的提交记录
///
/// 记录模型输出数据及 bypass 状态
#[derive(Debug)]
pub struct GateSubmission {
    /// 模型提交的输出数据（JSON 格式）
    pub data: Value,
    /// 是否为 bypass 提交（额度用完等异常情况）
    pub bypass: bool,
    /// bypass 原因说明（仅 bypass=true 时可能有值）
    pub bypass_reason: Option<String>,
}

/// 合并后的 session 输出
///
/// close() 成功后返回，包含所有模型的提交数据
#[derive(Debug)]
pub struct CombinedOutput {
    /// 各模型的提交数据，key 为模型名
    pub submissions: HashMap<String, GateSubmission>,
    /// 是否为部分 session（有模型使用了 bypass）
    pub partial: bool,
    /// 缺失的模型列表（bypass 模式下可能非空）
    pub missing_models: Vec<String>,
}

// ============================================
// submit 返回类型
// ============================================

/// submit() 操作的返回结果
///
/// 自动关闭机制的核心：submit 检测到所有 required_models 都已提交后，
/// 自动关闭 session 并返回 Complete(CombinedOutput)，无需调用方手动 close()。
#[derive(Debug)]
pub enum SubmitResult {
    /// 提交成功，但 session 尚未完成（还有模型未提交）
    Pending,
    /// 提交成功，所有模型已提交（或有 bypass），session 已自动关闭
    /// 包含合并后的所有模型输出
    Complete(CombinedOutput),
}

// ============================================
// 内部 Session 状态
// ============================================

/// 单个门控 session 的内部状态
///
/// 跟踪 session 的生命周期：从 open 到 close/abort
struct GateSession {
    /// session 唯一标识符（UUID v4）
    session_id: String,
    /// 命令类型（如 "review"）
    command_type: String,
    /// 必须提交输出的模型列表
    required_models: Vec<String>,
    /// 已收到的模型提交，key 为模型名
    submitted: HashMap<String, GateSubmission>,
}

// ============================================
// SessionGateStore 核心实现
// ============================================

/// Session Gate 存储器
///
/// 管理多模型审查的门控 session 生命周期：
/// 1. open() 创建 session 并写 state file
/// 2. submit() 记录各模型的输出
/// 3. close() 验证完整性并返回合并结果
/// 4. abort() 无条件终止 session
///
/// 线程安全：通过 Mutex<HashMap> 保证并发安全
pub struct SessionGateStore {
    /// 活跃 session 映射表，key 为 session_id
    sessions: Mutex<HashMap<String, GateSession>>,
    /// state file 路径，用于 Stop Hook 检测
    state_file_path: PathBuf,
}

impl SessionGateStore {
    /// 构造 SessionGateStore 实例
    ///
    /// @param state_file_path - state file 的存储路径
    /// @return SessionGateStore 实例
    pub fn new(state_file_path: PathBuf) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            state_file_path,
        }
    }

    /// 创建新的门控 session
    ///
    /// 业务逻辑：
    /// 1. 生成 UUID v4 作为 session_id
    /// 2. 创建 GateSession 存入内存
    /// 3. 写 state file 到磁盘（供 Stop Hook 检测）
    ///
    /// @param command_type - 命令类型（如 "review"）
    /// @param required_models - 必须提交输出的模型列表
    /// @return session_id（UUID 字符串）
    /// W3 修复：required_models 为空时返回错误，防止创建"无需任何模型"的无意义 session
    pub fn open(&self, command_type: &str, required_models: Vec<&str>) -> Result<String, SessionGateError> {
        // W3: 校验 required_models 非空
        if required_models.is_empty() {
            return Err(SessionGateError::EmptyRequiredModels);
        }

        let session_id = Uuid::new_v4().to_string();

        let session = GateSession {
            session_id: session_id.clone(),
            command_type: command_type.to_string(),
            required_models: required_models.iter().map(|s| s.to_string()).collect(),
            submitted: HashMap::new(),
        };

        // 将 session 存入 Mutex 保护的 HashMap
        let mut sessions = self.sessions.lock().expect("Mutex 中毒");
        sessions.insert(session_id.clone(), session);

        // 写 state file
        self.write_state_file(&sessions);

        Ok(session_id)
    }

    /// 记录模型输出到指定 session，并自动检测完成状态
    ///
    /// 业务逻辑：
    /// 1. 查找目标 session（不存在则返回 NotFound）
    /// 2. 校验模型在 required_models 列表中（W2 修复）
    /// 3. 创建 GateSubmission 记录并存入 submitted 映射表
    /// 4. 检测所有 required_models 是否都已提交（或有 bypass）
    ///    - 全部完成 → 自动关闭 session，返回 Complete(CombinedOutput)
    ///    - 尚不完整 → 返回 Pending
    ///
    /// @param session_id - 目标 session ID
    /// @param model - 模型名称（如 "codex"、"gemini"）
    /// @param _output_type - 输出类型（如 "findings"），当前保留
    /// @param data - 模型输出数据（JSON）
    /// @param bypass - 是否为 bypass 提交
    /// @param bypass_reason - bypass 原因（仅 bypass=true 时有意义）
    /// @return Ok(SubmitResult) 或 Err(SessionGateError)
    pub fn submit(
        &self,
        session_id: &str,
        model: &str,
        _output_type: &str,
        data: Value,
        bypass: bool,
        bypass_reason: Option<&str>,
    ) -> Result<SubmitResult, SessionGateError> {
        let mut sessions = self.sessions.lock().expect("Mutex 中毒");

        // ============================================
        // 第一步：查找 session 并校验模型合法性
        // ============================================
        let session = sessions
            .get_mut(session_id)
            .ok_or(SessionGateError::NotFound)?;

        // W2 修复：校验提交的模型必须在 required_models 列表中
        // 防止非必需模型提交污染 session 或利用 bypass 绕过门控
        if !session.required_models.contains(&model.to_string()) {
            return Err(SessionGateError::InvalidModel {
                model: model.to_string(),
                allowed: session.required_models.clone(),
            });
        }

        // ============================================
        // 第二步：记录提交
        // ============================================
        let submission = GateSubmission {
            data,
            bypass,
            bypass_reason: bypass_reason.map(|s| s.to_string()),
        };

        session.submitted.insert(model.to_string(), submission);

        // ============================================
        // 第三步：检测是否所有 required_models 都已提交
        // 自动关闭机制核心：无需调用方手动 close()
        // ============================================
        let missing_models: Vec<String> = session
            .required_models
            .iter()
            .filter(|m| !session.submitted.contains_key(*m))
            .cloned()
            .collect();

        // C1 修复：bypass 判定收敛到 required_models 子集
        let has_required_bypass = session
            .required_models
            .iter()
            .any(|m| session.submitted.get(m).map_or(false, |s| s.bypass));

        // 判断是否可以自动关闭
        let should_auto_close = missing_models.is_empty() || has_required_bypass;

        if !should_auto_close {
            // 尚不完整，保持 session 打开
            return Ok(SubmitResult::Pending);
        }

        // ============================================
        // 第四步：自动关闭 session 并构造 CombinedOutput
        // ============================================
        let session = sessions
            .remove(session_id)
            .expect("session 刚检查过必须存在");

        let partial = session.submitted.values().any(|s| s.bypass);

        let missing_models: Vec<String> = session
            .required_models
            .iter()
            .filter(|m| !session.submitted.contains_key(*m))
            .cloned()
            .collect();

        let output = CombinedOutput {
            submissions: session.submitted,
            partial,
            missing_models,
        };

        // 更新或删除 state file
        self.write_or_remove_state_file(&sessions);

        Ok(SubmitResult::Complete(output))
    }

    /// 手动关闭 session 并返回合并输出（可选）
    ///
    /// 注意：通常不需要手动调用此方法。submit() 会在检测到所有模型都已提交后
    /// 自动关闭 session。此方法保留用于以下场景：
    /// - 需要在 bypass 模式下强制关闭未完成的 session
    /// - 需要检查 session 完整性而不依赖 submit 的自动关闭
    ///
    /// 业务逻辑：
    /// 1. 查找 session（不存在返回 NotFound）
    /// 2. 计算缺失模型：required_models 中完全没有提交记录的模型
    ///    - bypass=true 的提交也算作"已提交"，允许 close
    /// 3. 若有缺失模型且无 bypass，返回 SessionIncomplete 错误
    /// 4. 构造 CombinedOutput（partial = 是否有 bypass 提交）
    /// 5. 从 HashMap 中删除 session
    /// 6. 更新/删除 state file
    ///
    /// @param session_id - 目标 session ID
    /// @return Ok(CombinedOutput) 或 Err(SessionGateError)
    pub fn close(&self, session_id: &str) -> Result<CombinedOutput, SessionGateError> {
        let mut sessions = self.sessions.lock().expect("Mutex 中毒");

        // 先检查 session 是否存在，验证完整性
        {
            let session = sessions.get(session_id).ok_or(SessionGateError::NotFound)?;

            // 计算缺失模型：required_models 中没有出现在 submitted 中的模型
            let missing_models: Vec<String> = session
                .required_models
                .iter()
                .filter(|model| !session.submitted.contains_key(*model))
                .cloned()
                .collect();

            // C1 修复：bypass 判定收敛到 required_models 子集
            // 之前的 bug：遍历所有 submitted（含非 required 模型）检查 bypass，
            // 攻击者可通过提交非必需模型（bypass=true）绕过所有 required 模型的验证。
            // 修复后：仅检查 required_models 中已提交的模型是否有 bypass 标记
            let has_required_bypass = session
                .required_models
                .iter()
                .any(|m| session.submitted.get(m).map_or(false, |s| s.bypass));

            // 如果有缺失模型且没有 required model 的 bypass 提交，返回 SessionIncomplete 错误
            // bypass 模式下允许部分模型未提交（用于额度用完等场景）
            if !missing_models.is_empty() && !has_required_bypass {
                return Err(SessionGateError::SessionIncomplete { missing_models });
            }
        }

        // 验证通过，移除 session 并构造输出
        let session = sessions
            .remove(session_id)
            .expect("session 刚检查过必须存在");

        // 判断是否为部分 session：有任何 bypass 提交则为 partial
        let partial = session.submitted.values().any(|s| s.bypass);

        // 收集缺失模型列表（bypass 模式下可能有未提交的模型）
        let missing_models: Vec<String> = session
            .required_models
            .iter()
            .filter(|model| !session.submitted.contains_key(*model))
            .cloned()
            .collect();

        let output = CombinedOutput {
            submissions: session.submitted,
            partial,
            missing_models,
        };

        // 更新或删除 state file
        self.write_or_remove_state_file(&sessions);

        Ok(output)
    }

    /// 无条件终止 session
    ///
    /// 业务逻辑：
    /// 1. 查找 session（不存在返回 NotFound）
    /// 2. 从 HashMap 中删除 session
    /// 3. 更新/删除 state file
    ///
    /// @param session_id - 目标 session ID
    /// @return Ok(()) 或 Err(SessionGateError::NotFound)
    pub fn abort(&self, session_id: &str) -> Result<(), SessionGateError> {
        let mut sessions = self.sessions.lock().expect("Mutex 中毒");

        if sessions.remove(session_id).is_none() {
            return Err(SessionGateError::NotFound);
        }

        // 更新或删除 state file
        self.write_or_remove_state_file(&sessions);

        Ok(())
    }

    // ============================================
    // 内部辅助方法：State File 管理
    // ============================================

    /// 写 state file（JSON 格式）
    ///
    /// 将所有活跃 session 的状态序列化到磁盘，
    /// 供 Stop Hook 检测是否有未完成的审查 session
    fn write_state_file(&self, sessions: &HashMap<String, GateSession>) {
        let pending_sessions: Vec<serde_json::Value> = sessions
            .values()
            .map(|s| {
                // 收集已提交模型名列表
                let submitted_models: Vec<String> = s.submitted.keys().cloned().collect();

                serde_json::json!({
                    "session_id": s.session_id,
                    "command_type": s.command_type,
                    "required_models": s.required_models,
                    "submitted_models": submitted_models,
                    "opened_at": chrono::Utc::now().to_rfc3339(),
                })
            })
            .collect();

        let state = serde_json::json!({
            "pending_sessions": pending_sessions,
        });

        // 确保父目录存在
        if let Some(parent) = self.state_file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // 写入 state file（非关键路径：写失败只记录日志，不影响核心功能）
        match serde_json::to_string_pretty(&state) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&self.state_file_path, content) {
                    eprintln!("[session_gate] 写 state file 失败: {}", e);
                }
            }
            Err(e) => {
                eprintln!("[session_gate] state file JSON 序列化失败: {}", e);
            }
        }
    }

    /// 根据活跃 session 数量决定更新或删除 state file
    ///
    /// - 有活跃 session：更新 state file 内容
    /// - 无活跃 session：删除 state file（而非写空文件）
    fn write_or_remove_state_file(&self, sessions: &HashMap<String, GateSession>) {
        if sessions.is_empty() {
            // 无活跃 session，删除 state file
            let _ = std::fs::remove_file(&self.state_file_path);
        } else {
            // 还有活跃 session，更新 state file
            self.write_state_file(sessions);
        }
    }
}
