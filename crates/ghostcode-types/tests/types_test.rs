//! ghostcode-types 集成测试
//!
//! 覆盖 T02 TDD 规范定义的所有测试用例
//! - EventKind 序列化格式验证
//! - Event 往返性（PBT）
//! - Event.id / Event.ts 格式验证
//!
//! @author Atlas.oi
//! @date 2026-02-28

use ghostcode_types::event::{Event, EventKind};
use proptest::prelude::*;

// ============================================
// 单元测试：EventKind 序列化格式
// ============================================

#[test]
fn event_kind_serialize_format() {
    // 验证每种 EventKind 序列化为正确的点分隔 snake_case 格式
    let cases = vec![
        (EventKind::GroupCreate, "\"group.create\""),
        (EventKind::GroupUpdate, "\"group.update\""),
        (EventKind::GroupStart, "\"group.start\""),
        (EventKind::GroupStop, "\"group.stop\""),
        (EventKind::GroupSetState, "\"group.set_state\""),
        (EventKind::ActorAdd, "\"actor.add\""),
        (EventKind::ActorUpdate, "\"actor.update\""),
        (EventKind::ActorStart, "\"actor.start\""),
        (EventKind::ActorStop, "\"actor.stop\""),
        (EventKind::ActorRemove, "\"actor.remove\""),
        (EventKind::ChatMessage, "\"chat.message\""),
        (EventKind::ChatRead, "\"chat.read\""),
        (EventKind::ChatAck, "\"chat.ack\""),
        (EventKind::SystemNotify, "\"system.notify\""),
    ];

    for (kind, expected) in cases {
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, expected, "EventKind::{:?} 序列化格式错误", kind);
    }
}

// ============================================
// 单元测试：Event.id 格式验证
// ============================================

#[test]
fn event_id_format() {
    // 验证 id 是 32 字符十六进制字符串（uuid v4 simple 格式）
    let event = Event::new(
        EventKind::ChatMessage,
        "group-1",
        "default",
        "user",
        serde_json::json!({}),
    );

    assert_eq!(event.id.len(), 32, "id 应为 32 字符");
    assert!(
        event.id.chars().all(|c| c.is_ascii_hexdigit()),
        "id 应全部为十六进制字符, 实际: {}",
        event.id
    );
}

// ============================================
// 单元测试：Event.ts 格式验证
// ============================================

#[test]
fn event_ts_format() {
    // 验证 ts 是 ISO 8601 UTC 微秒精度格式
    // 格式示例: "2026-02-28T17:30:00.123456Z"
    let event = Event::new(
        EventKind::SystemNotify,
        "group-1",
        "default",
        "user",
        serde_json::Value::Null,
    );

    // 以 Z 结尾（UTC 标识）
    assert!(event.ts.ends_with('Z'), "ts 应以 Z 结尾: {}", event.ts);

    // 包含 T 分隔符
    assert!(event.ts.contains('T'), "ts 应包含 T 分隔符: {}", event.ts);

    // 小数点后有 6 位（微秒精度）
    let dot_pos = event.ts.rfind('.').expect("ts 应包含小数点");
    let frac_len = event.ts.len() - dot_pos - 2; // 减去 '.' 和 'Z'
    assert_eq!(frac_len, 6, "ts 应有 6 位微秒精度: {}", event.ts);
}

// ============================================
// 单元测试：DaemonRequest / DaemonResponse 基本功能
// ============================================

#[test]
fn daemon_request_new() {
    use ghostcode_types::ipc::DaemonRequest;

    let req = DaemonRequest::new("ping", serde_json::json!({}));
    assert_eq!(req.v, 1);
    assert_eq!(req.op, "ping");
}

#[test]
fn daemon_response_ok_and_err() {
    use ghostcode_types::ipc::DaemonResponse;

    let ok = DaemonResponse::ok(serde_json::json!("pong"));
    assert!(ok.ok);
    assert!(ok.error.is_none());

    let err = DaemonResponse::err("NOT_FOUND", "Actor not found");
    assert!(!err.ok);
    assert_eq!(err.error.as_ref().unwrap().code, "NOT_FOUND");
}

// ============================================
// 单元测试：AddrDescriptor 创建
// ============================================

#[test]
fn addr_descriptor_new() {
    use ghostcode_types::addr::AddrDescriptor;

    let addr = AddrDescriptor::new("/tmp/ghostcode.sock", 12345, "0.1.0");
    assert_eq!(addr.v, 1);
    assert_eq!(addr.transport, "unix");
    assert_eq!(addr.path, "/tmp/ghostcode.sock");
    assert_eq!(addr.pid, 12345);
    assert_eq!(addr.version, "0.1.0");
    assert!(addr.ts.ends_with('Z'));
}

// ============================================
// PBT 属性测试：Event 往返性
// ============================================

fn arb_event_kind() -> impl Strategy<Value = EventKind> {
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

fn arb_event() -> impl Strategy<Value = Event> {
    (
        arb_event_kind(),
        "[a-z0-9]{1,32}",
        "[a-z0-9]{1,32}",
        "[a-z0-9]{1,32}",
        prop_oneof![
            Just(serde_json::Value::Null),
            any::<bool>().prop_map(serde_json::Value::Bool),
            any::<i64>().prop_map(|n| serde_json::Value::Number(n.into())),
            "[a-zA-Z0-9 ]{0,50}".prop_map(|s| serde_json::Value::String(s)),
        ],
    )
        .prop_map(|(kind, group_id, scope_key, by, data)| {
            Event::new(kind, group_id, scope_key, by, data)
        })
}

proptest! {
    /// PBT: Event -> JSON -> Event 往返性
    /// 序列化后再反序列化应得到语义等价的 Event（id 和 ts 保持不变）
    #[test]
    fn roundtrip_event(event in arb_event()) {
        let json = serde_json::to_string(&event).unwrap();
        let restored: Event = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(event.v, restored.v);
        prop_assert_eq!(event.id, restored.id);
        prop_assert_eq!(event.ts, restored.ts);
        prop_assert_eq!(event.kind, restored.kind);
        prop_assert_eq!(event.group_id, restored.group_id);
        prop_assert_eq!(event.scope_key, restored.scope_key);
        prop_assert_eq!(event.by, restored.by);
        prop_assert_eq!(event.data, restored.data);
    }

    /// PBT: 所有 EventKind 都能序列化/反序列化
    #[test]
    fn roundtrip_all_event_kinds(kind in arb_event_kind()) {
        let json = serde_json::to_string(&kind).unwrap();
        let restored: EventKind = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(kind, restored);
    }
}
