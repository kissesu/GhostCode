/**
 * @file sovereignty_test.rs
 * @description SovereigntyGuard 写入权限守卫单元测试
 *              覆盖：基本权限检查、自定义写入者、大小写不敏感、空字符串、输出审查
 *              以及基于属性的随机测试（proptest）
 * @author Atlas.oi
 * @date 2026-03-02
 */

use ghostcode_router::sovereignty::*;

// ============================================
// 基本权限检查
// ============================================

#[test]
fn claude_can_write() {
    let guard = SovereigntyGuard::new();
    assert!(guard.can_write("claude"));
}

#[test]
fn codex_cannot_write() {
    let guard = SovereigntyGuard::new();
    assert!(!guard.can_write("codex"));
}

#[test]
fn gemini_cannot_write() {
    let guard = SovereigntyGuard::new();
    assert!(!guard.can_write("gemini"));
}

// ============================================
// 自定义写入者
// ============================================

#[test]
fn custom_write_actor() {
    let guard = SovereigntyGuard::with_write_actor("codex");
    assert!(guard.can_write("codex"));
    assert!(!guard.can_write("claude"));
}

// ============================================
// 大小写不敏感
// ============================================

#[test]
fn case_insensitive() {
    let guard = SovereigntyGuard::new();
    assert!(guard.can_write("Claude"));
    assert!(guard.can_write("CLAUDE"));
    assert!(guard.can_write("claude"));
}

// ============================================
// 空字符串不允许写入
// ============================================

#[test]
fn empty_backend_cannot_write() {
    let guard = SovereigntyGuard::new();
    assert!(!guard.can_write(""));
}

// ============================================
// 输出审查
// ============================================

#[test]
fn codex_output_needs_review() {
    let guard = SovereigntyGuard::new();
    let result = guard.review_output("codex", "some generated code");
    assert!(matches!(result, ReviewResult::NeedsReview { .. }));
}

#[test]
fn claude_output_approved() {
    let guard = SovereigntyGuard::new();
    let result = guard.review_output("claude", "some code");
    assert!(matches!(result, ReviewResult::Approved));
}

#[test]
fn dangerous_output_rejected() {
    let guard = SovereigntyGuard::new();
    let result = guard.review_output("codex", "let me rm -rf / for you");
    assert!(matches!(result, ReviewResult::Rejected { .. }));
}

#[test]
fn dangerous_drop_table_rejected() {
    let guard = SovereigntyGuard::new();
    let result = guard.review_output("codex", "DROP TABLE users;");
    assert!(matches!(result, ReviewResult::Rejected { .. }));
}

// ============================================
// 属性测试：非写入者永远返回 false
// ============================================

use proptest::prelude::*;

proptest! {
    #[test]
    fn non_write_actor_always_false(
        name in "[a-z]{3,10}"
            .prop_filter("not claude", |s| s != "claude")
    ) {
        let guard = SovereigntyGuard::new();
        prop_assert!(!guard.can_write(&name));
    }
}
