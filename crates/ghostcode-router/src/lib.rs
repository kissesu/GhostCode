// @file lib.rs
// @description ghostcode-router crate 公开模块导出
//              提供 Backend trait 和三后端（Codex/Claude/Gemini）CLI 参数构建器
// @author Atlas.oi
// @date 2026-03-02

// 公开 backend 模块，暴露 Backend trait 和三个后端实现
pub mod backend;

// 公开 rolefile 模块，提供 ROLE_FILE 注入功能
pub mod rolefile;

// 公开 dag 模块，提供 DAG 拓扑排序用于多模型并行任务调度
pub mod dag;

// 公开 stream 模块，提供 JSON Stream 统一解析器
pub mod stream;

// 公开 task_format 模块，提供 ---TASK---/---CONTENT--- 格式解析
pub mod task_format;

// 公开 sovereignty 模块，提供代码主权（写入权限）控制
pub mod sovereignty;

// 公开 session 模块，提供 SESSION_ID 持久化管理
pub mod session;

// 公开 process 模块，提供异步子进程管理器（启动/监控/终止 AI CLI 工具）
pub mod process;

// 公开 executor 模块，提供并行执行引擎（基于 DAG 拓扑排序的层间串行+层内并行调度）
pub mod executor;

// 公开 runtime_probe 模块，提供 AI CLI 工具运行时探测（检测可用性和版本信息）
pub mod runtime_probe;
