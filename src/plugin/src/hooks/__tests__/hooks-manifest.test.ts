/**
 * @file hooks-manifest.test.ts
 * @description hooks.json 清单完整性测试
 *              验证 hooks.json 包含足够的事件类型且所有 command 路径有效：
 *              1. hooks.json 包含至少 8 个事件类型
 *              2. 每个事件的 command 路径指向存在的脚本文件
 *              3. 所有 timeout 值在合理范围（3-15 秒）
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { describe, it, expect } from "vitest";
import { readFileSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";

// Plugin 根目录（src/plugin/）
// 测试文件路径: src/plugin/src/hooks/__tests__/hooks-manifest.test.ts
// 向上路径: __tests__ -> hooks -> src(plugin内) -> src/plugin
const PLUGIN_ROOT = join(
  import.meta.dirname ?? new URL(".", import.meta.url).pathname,
  "..",  // __tests__ -> src/plugin/src/hooks
  "..",  // src/plugin/src/hooks -> src/plugin/src
  "..",  // src/plugin/src -> src/plugin
);

// hooks.json 路径（相对于 src/plugin/hooks/hooks.json）
const HOOKS_JSON_PATH = join(PLUGIN_ROOT, "hooks", "hooks.json");

// ============================================
// 工具函数：从 command 字符串中提取脚本路径
// 支持两种格式：
//   - node "${CLAUDE_PLUGIN_ROOT}/scripts/xxx.mjs"
//   - node "${CLAUDE_PLUGIN_ROOT}/scripts/run.mjs" "${CLAUDE_PLUGIN_ROOT}/scripts/xxx.mjs"
// ============================================

/**
 * 从 command 字符串中提取所有 .mjs 脚本路径（去掉 ${CLAUDE_PLUGIN_ROOT} 前缀）
 *
 * @param {string} command - hooks.json 中的 command 字符串
 * @returns {string[]} 提取到的脚本相对路径列表（相对于 PLUGIN_ROOT）
 */
function extractScriptPaths(command: string): string[] {
  // 匹配 ${CLAUDE_PLUGIN_ROOT}/xxx 格式的路径
  const pattern = /\$\{CLAUDE_PLUGIN_ROOT\}(\/[^"'\s]+)/g;
  const paths: string[] = [];
  let match;
  while ((match = pattern.exec(command)) !== null) {
    // match[1] 对应捕获组 (\/[^"'\s]+)，正则结构保证存在时非 undefined
    if (match[1] !== undefined) {
      paths.push(match[1]);
    }
  }
  return paths;
}

// ============================================
// 测试套件：hooks.json 清单完整性
// ============================================

describe("hooks.json 清单完整性验证", () => {
  // 读取 hooks.json 文件
  const hooksJsonContent = readFileSync(HOOKS_JSON_PATH, "utf-8");
  const hooksJson = JSON.parse(hooksJsonContent);
  const hooks = hooksJson.hooks as Record<string, unknown[]>;
  const eventNames = Object.keys(hooks);

  it("hooks.json 应包含至少 8 个事件类型", () => {
    expect(eventNames.length).toBeGreaterThanOrEqual(8);
  });

  it("每个事件类型的 command 路径应指向存在的脚本文件", () => {
    const missingFiles: string[] = [];

    for (const [eventName, eventList] of Object.entries(hooks)) {
      for (const eventEntry of eventList as Array<{ hooks: Array<{ type: string; command: string }> }>) {
        for (const hook of eventEntry.hooks) {
          if (hook.type === "command" && hook.command) {
            const scriptPaths = extractScriptPaths(hook.command);
            for (const scriptPath of scriptPaths) {
              const absolutePath = join(PLUGIN_ROOT, scriptPath);
              if (!existsSync(absolutePath)) {
                missingFiles.push(`${eventName}: ${absolutePath}`);
              }
            }
          }
        }
      }
    }

    if (missingFiles.length > 0) {
      expect.fail(`以下脚本文件不存在：\n${missingFiles.join("\n")}`);
    }
  });

  it("所有 timeout 值应在合理范围（3-15 秒）", () => {
    const invalidTimeouts: string[] = [];

    for (const [eventName, eventList] of Object.entries(hooks)) {
      for (const eventEntry of eventList as Array<{ hooks: Array<{ type: string; timeout?: number }> }>) {
        for (const hook of eventEntry.hooks) {
          if (hook.type === "command" && hook.timeout !== undefined) {
            if (hook.timeout < 3 || hook.timeout > 15) {
              invalidTimeouts.push(`${eventName}: timeout=${hook.timeout}（应在 3-15 秒范围内）`);
            }
          }
        }
      }
    }

    if (invalidTimeouts.length > 0) {
      expect.fail(`以下事件的 timeout 超出合理范围：\n${invalidTimeouts.join("\n")}`);
    }
  });
});
