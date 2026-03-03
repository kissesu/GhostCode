// @file session_test.rs
// @description SESSION_ID 管理器测试套件（TDD Red 阶段）
//              测试 SessionStore 的 save/get/list/flush 及持久化功能
// @author Atlas.oi
// @date 2026-03-02

use ghostcode_router::session::{SessionStore, SessionKey};
use proptest::prelude::*;
use tempfile::tempdir;

/// 验证 save 后 get 能正确返回保存的 session_id
#[test]
fn save_and_get_session() {
    let dir = tempdir().unwrap();
    let store = SessionStore::new(dir.path().join("sessions.json")).unwrap();

    let key: SessionKey = ("g1".to_string(), "a1".to_string(), "codex".to_string());
    store.save(key.clone(), "sid1".to_string()).unwrap();

    assert_eq!(store.get(&key), Some("sid1".to_string()));
}

/// 验证对同一个 key 连续 save 两次，get 返回最新的 session_id
#[test]
fn save_overwrites_old() {
    let dir = tempdir().unwrap();
    let store = SessionStore::new(dir.path().join("sessions.json")).unwrap();

    let key: SessionKey = ("g1".to_string(), "a1".to_string(), "codex".to_string());
    store.save(key.clone(), "sid_old".to_string()).unwrap();
    store.save(key.clone(), "sid_new".to_string()).unwrap();

    assert_eq!(store.get(&key), Some("sid_new".to_string()));
}

/// 验证不同 backend 的 session 相互隔离，不会混淆
#[test]
fn different_backends_isolated() {
    let dir = tempdir().unwrap();
    let store = SessionStore::new(dir.path().join("sessions.json")).unwrap();

    let key_codex: SessionKey = ("g1".to_string(), "a1".to_string(), "codex".to_string());
    let key_gemini: SessionKey = ("g1".to_string(), "a1".to_string(), "gemini".to_string());

    store.save(key_codex.clone(), "sid1".to_string()).unwrap();
    store.save(key_gemini.clone(), "sid2".to_string()).unwrap();

    // 两个不同 backend 的 session 必须各自独立
    assert_eq!(store.get(&key_codex), Some("sid1".to_string()));
    assert_eq!(store.get(&key_gemini), Some("sid2".to_string()));
}

/// 验证 flush 持久化后，新建 SessionStore 能正确从文件加载并 get 一致
#[test]
fn flush_and_reload() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("sessions.json");

    // 第一个 store 写入并 flush
    {
        let store = SessionStore::new(file_path.clone()).unwrap();
        let key: SessionKey = ("g1".to_string(), "a1".to_string(), "codex".to_string());
        store.save(key, "sid_persisted".to_string()).unwrap();
        // save() 已经 flush，但显式调用确保文件已落盘
        store.flush().unwrap();
    }

    // 新建 store 从同一文件加载，验证持久化数据正确恢复
    let store2 = SessionStore::new(file_path).unwrap();
    let key: SessionKey = ("g1".to_string(), "a1".to_string(), "codex".to_string());
    assert_eq!(store2.get(&key), Some("sid_persisted".to_string()));
}

/// 验证 list 返回所有已保存的 session 条目数量正确
#[test]
fn list_all_sessions() {
    let dir = tempdir().unwrap();
    let store = SessionStore::new(dir.path().join("sessions.json")).unwrap();

    store.save(("g1".to_string(), "a1".to_string(), "codex".to_string()), "sid1".to_string()).unwrap();
    store.save(("g1".to_string(), "a2".to_string(), "claude".to_string()), "sid2".to_string()).unwrap();
    store.save(("g2".to_string(), "a1".to_string(), "gemini".to_string()), "sid3".to_string()).unwrap();

    let sessions = store.list();
    assert_eq!(sessions.len(), 3);
}

// proptest 属性测试：验证任意 session 数据经过 save → flush → reload 后全部 get 一致
proptest! {
    #[test]
    fn roundtrip_persistence(
        entries in prop::collection::vec(
            (
                // group_id: 非空字母数字字符串
                "[a-z][a-z0-9]{0,7}",
                // actor_id: 非空字母数字字符串
                "[a-z][a-z0-9]{0,7}",
                // backend_name: 固定几种后端名称
                prop::sample::select(vec!["codex".to_string(), "claude".to_string(), "gemini".to_string()]),
                // session_id: 非空字符串
                "[a-zA-Z0-9]{4,20}",
            ),
            1..=10
        )
    ) {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sessions.json");

        // save 阶段：将所有 entry 写入第一个 store
        let store = SessionStore::new(file_path.clone()).unwrap();
        // 用 HashMap 记录最终期望值（后写覆盖先写）
        let mut expected = std::collections::HashMap::new();
        for (g, a, b, sid) in &entries {
            let key: SessionKey = (g.clone(), a.clone(), b.clone());
            store.save(key.clone(), sid.clone()).unwrap();
            expected.insert(key, sid.clone());
        }
        store.flush().unwrap();

        // reload 阶段：从文件重新加载，验证所有 session 一致
        let store2 = SessionStore::new(file_path).unwrap();
        for (key, expected_sid) in &expected {
            prop_assert_eq!(store2.get(key), Some(expected_sid.clone()));
        }
    }
}
