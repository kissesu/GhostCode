# GhostCode 构建自动化
#
# 统一管理 Rust 核心 + TypeScript Plugin + Dashboard 前端的构建与部署
# 解决二进制与源码不同步的历史问题（参见 2026-03-16 group_id 空值事故）
#
# 使用方法：
#   make build       - 编译所有 Rust crate（release）并同步二进制到 Plugin
#   make plugin      - 仅编译 MCP 二进制并同步到 Plugin 目录
#   make dashboard   - 构建 Dashboard 前端并部署
#   make deploy      - 全量构建 + 部署到 ~/.ghostcode/ + Plugin 缓存
#   make test        - 运行 Rust + Dashboard 测试
#   make check       - clippy + TypeScript 类型检查
#   make clean       - 清理构建产物
#
# @author Atlas.oi
# @date 2026-03-16

# ============================================
# 路径常量
# ============================================

# Rust 构建输出目录
RELEASE_DIR := target/release

# Plugin 二进制目录（提交到 Git，供 Claude Plugin 安装使用）
PLUGIN_BIN_DIR := src/plugin/bin

# Dashboard 源码与输出目录
DASHBOARD_SRC := src/dashboard
DASHBOARD_DIST := $(DASHBOARD_SRC)/dist

# 生产部署目录
GHOSTCODE_HOME := $(HOME)/.ghostcode
DEPLOY_BIN_DIR := $(GHOSTCODE_HOME)/bin
DEPLOY_WEB_DIR := $(GHOSTCODE_HOME)/web

# Plugin 缓存目录（Claude Code 插件运行时使用的二进制位置）
PLUGIN_CACHE_BASE := $(HOME)/.claude/plugins/cache/ghostcode/gc

# ============================================
# Rust 二进制产物清单
# ============================================

# Plugin 内嵌的二进制（通过 Git 分发，Claude Plugin 安装时直接使用）
PLUGIN_BINS := ghostcode-mcp

# 部署到 ~/.ghostcode/bin/ 的二进制（Daemon + Wrapper + Web Server）
DEPLOY_BINS := ghostcoded ghostcode-wrapper ghostcode-web

# 全部 Rust 二进制
ALL_BINS := $(PLUGIN_BINS) $(DEPLOY_BINS)

# ============================================
# 默认目标
# ============================================

.PHONY: all build plugin dashboard deploy test check clean help sync-plugin-cache

all: build  ## 默认：编译所有 Rust crate 并同步二进制

help:  ## 显示帮助信息
	@echo "GhostCode 构建自动化"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
	@echo ""

# ============================================
# 构建目标
# ============================================

build: build-rust sync-bins  ## 编译所有 Rust crate（release）并同步二进制
	@echo "[GhostCode] 构建完成"

build-rust:  ## 编译所有 Rust crate（release 模式）
	@echo "[GhostCode] 编译 Rust crate..."
	cargo build --release

plugin: build-mcp sync-plugin-bin  ## 仅编译 MCP 并同步到 Plugin 目录
	@echo "[GhostCode] MCP 二进制已同步到 Plugin"

build-mcp:  ## 编译 ghostcode-mcp
	@echo "[GhostCode] 编译 ghostcode-mcp..."
	cargo build --release -p ghostcode-mcp

dashboard:  ## 构建 Dashboard 前端并部署到生产目录
	@echo "[GhostCode] 构建 Dashboard..."
	cd $(DASHBOARD_SRC) && pnpm run build
	@mkdir -p $(DEPLOY_WEB_DIR)
	cp $(DASHBOARD_DIST)/* $(DEPLOY_WEB_DIR)/
	@echo "[GhostCode] Dashboard 已部署到 $(DEPLOY_WEB_DIR)"

deploy: build dashboard sync-deploy-bins sync-plugin-cache  ## 全量构建 + 部署
	@echo "[GhostCode] 全量部署完成"
	@echo "  Rust 二进制 -> $(DEPLOY_BIN_DIR)/"
	@echo "  MCP 二进制  -> $(PLUGIN_BIN_DIR)/"
	@echo "  Dashboard   -> $(DEPLOY_WEB_DIR)/"
	@echo ""
	@echo "  注意：已运行的 MCP 进程需要重启会话才能加载新二进制"

# ============================================
# 二进制同步目标
# ============================================

sync-bins: sync-plugin-bin sync-deploy-bins  ## 同步所有二进制到目标目录

sync-plugin-bin:  ## 同步 MCP 二进制到 Plugin 目录（提交到 Git）
	@mkdir -p $(PLUGIN_BIN_DIR)
	@for bin in $(PLUGIN_BINS); do \
		if [ -f "$(RELEASE_DIR)/$$bin" ]; then \
			cp "$(RELEASE_DIR)/$$bin" "$(PLUGIN_BIN_DIR)/$$bin"; \
			echo "[GhostCode] $$bin -> $(PLUGIN_BIN_DIR)/"; \
		else \
			echo "[GhostCode] 警告: $(RELEASE_DIR)/$$bin 不存在，跳过"; \
		fi; \
	done

sync-deploy-bins:  ## 同步 Daemon/Wrapper/Web 二进制到 ~/.ghostcode/bin/
	@mkdir -p $(DEPLOY_BIN_DIR)
	@for bin in $(DEPLOY_BINS); do \
		if [ -f "$(RELEASE_DIR)/$$bin" ]; then \
			cp "$(RELEASE_DIR)/$$bin" "$(DEPLOY_BIN_DIR)/$$bin"; \
			echo "[GhostCode] $$bin -> $(DEPLOY_BIN_DIR)/"; \
		else \
			echo "[GhostCode] 警告: $(RELEASE_DIR)/$$bin 不存在，跳过"; \
		fi; \
	done

sync-plugin-cache:  ## 同步 MCP 二进制到所有 Plugin 缓存版本目录
	@if [ -d "$(PLUGIN_CACHE_BASE)" ]; then \
		for version_dir in $(PLUGIN_CACHE_BASE)/*/; do \
			if [ -d "$$version_dir/bin" ]; then \
				for bin in $(PLUGIN_BINS); do \
					if [ -f "$(RELEASE_DIR)/$$bin" ]; then \
						cp "$(RELEASE_DIR)/$$bin" "$$version_dir/bin/$$bin"; \
						echo "[GhostCode] $$bin -> $$version_dir/bin/"; \
					fi; \
				done; \
			fi; \
		done; \
	else \
		echo "[GhostCode] Plugin 缓存目录不存在，跳过: $(PLUGIN_CACHE_BASE)"; \
	fi

# ============================================
# 质量检查目标
# ============================================

test: test-rust test-dashboard  ## 运行所有测试

test-rust:  ## 运行 Rust 测试
	@echo "[GhostCode] 运行 Rust 测试..."
	cargo test

test-dashboard:  ## 运行 Dashboard 前端测试
	@echo "[GhostCode] 运行 Dashboard 测试..."
	cd $(DASHBOARD_SRC) && pnpm test -- --run

check: check-rust check-dashboard  ## 运行所有静态检查

check-rust:  ## Rust clippy 检查
	@echo "[GhostCode] Clippy 检查..."
	cargo clippy -- -D warnings

check-dashboard:  ## Dashboard TypeScript 类型检查
	@echo "[GhostCode] TypeScript 类型检查..."
	cd $(DASHBOARD_SRC) && pnpm run typecheck

# ============================================
# 清理目标
# ============================================

clean:  ## 清理所有构建产物
	@echo "[GhostCode] 清理构建产物..."
	cargo clean
	rm -rf $(DASHBOARD_DIST)
	@echo "[GhostCode] 清理完成"
