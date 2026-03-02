//! 测试工具模块
//!
//! 提供 proptest 策略生成器，供 ghostcode-types 和其他 crate 的测试复用
//! 包含 arb_event_kind()、arb_event() 等 Arbitrary 策略
//!
//! @author Atlas.oi
//! @date 2026-02-28

use proptest::prelude::*;

use crate::event::{Event, EventKind};

/// 生成随机 EventKind 的策略
pub fn arb_event_kind() -> impl Strategy<Value = EventKind> {
    prop_oneof![
        Just(EventKind::GroupCreate),
        Just(EventKind::GroupUpdate),
        Just(EventKind::GroupStart),
        Just(EventKind::GroupStop),
        Just(EventKind::GroupSetState),
        Just(EventKind::ActorAdd),
        Just(EventKind::ActorUpdate),
        Just(EventKind::ActorStart),
        Just(EventKind::ActorStop),
        Just(EventKind::ActorRemove),
        Just(EventKind::ChatMessage),
        Just(EventKind::ChatRead),
        Just(EventKind::ChatAck),
        Just(EventKind::SystemNotify),
    ]
}

/// 生成随机 Event 的策略
///
/// 生成具有随机字段的合法 Event 对象
/// group_id、scope_key、by 使用 1-32 字符的字母数字串
/// data 使用简单的 JSON 对象
pub fn arb_event() -> impl Strategy<Value = Event> {
    (
        arb_event_kind(),
        "[a-z0-9]{1,32}",  // group_id
        "[a-z0-9]{1,32}",  // scope_key
        "[a-z0-9]{1,32}",  // by
        arb_json_value(),  // data
    )
        .prop_map(|(kind, group_id, scope_key, by, data)| {
            Event::new(kind, group_id, scope_key, by, data)
        })
}

/// 生成简单 JSON 值的策略
///
/// 用于填充 Event.data 和 DaemonRequest.args 等字段
fn arb_json_value() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(|n| serde_json::Value::Number(n.into())),
        "[a-zA-Z0-9 ]{0,50}".prop_map(|s| serde_json::Value::String(s)),
    ]
}
