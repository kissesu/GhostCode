/**
 * @file parser.property.test.ts
 * @description Magic Keywords 解析器的属性测试（PBT）
 * 使用 fast-check 验证解析器的不变量和稳定性
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect } from "vitest";
import * as fc from "fast-check";
import { detectMagicKeywords, resolveKeywordPriority } from "../parser.js";
import { sanitizeForKeywordDetection } from "../sanitize.js";

describe("PBT: 解析器属性测试", () => {
  // ============================================
  // 属性 1：解析稳定性
  // parse(x) === parse(x)（同输入多次解析结果一致）
  // ============================================
  it("解析稳定性：同输入两次解析结果一致", () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        const result1 = detectMagicKeywords(input);
        const result2 = detectMagicKeywords(input);
        // 两次解析结果的关键词类型必须完全一致
        const types1 = result1.map((m) => m.type).sort();
        const types2 = result2.map((m) => m.type).sort();
        expect(types1).toEqual(types2);
      }),
      { numRuns: 200 }
    );
  });

  // ============================================
  // 属性 2：sanitize 幂等性
  // sanitize(sanitize(x)) === sanitize(x)
  // ============================================
  it("sanitize 幂等性：二次清理结果与一次清理相同", () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        const once = sanitizeForKeywordDetection(input);
        const twice = sanitizeForKeywordDetection(once);
        // 二次 sanitize 结果与一次相同（已清理的内容不会再变）
        expect(twice).toBe(once);
      }),
      { numRuns: 200 }
    );
  });

  // ============================================
  // 属性 3：非关键字输入无关键词检出
  // 纯数字/随机 Unicode 文本无关键词检出
  // ============================================
  it("纯数字输入不产生关键词匹配", () => {
    fc.assert(
      fc.property(fc.nat({ max: 999999 }), (num) => {
        const input = String(num);
        const matches = detectMagicKeywords(input);
        expect(matches).toHaveLength(0);
      }),
      { numRuns: 100 }
    );
  });

  it("随机 Unicode 文本（不含关键词）不产生误匹配", () => {
    // 生成仅包含汉字范围的随机字符串（不含 ASCII 关键词字符）
    // 使用 stringMatching 匹配汉字 Unicode 范围 \u4e00-\u9fff
    fc.assert(
      fc.property(
        fc.stringMatching(/^[\u4e00-\u9fff]{5,50}$/),
        (input) => {
          const matches = detectMagicKeywords(input);
          // 纯汉字文本不应触发任何关键词
          expect(matches).toHaveLength(0);
        }
      ),
      { numRuns: 100 }
    );
  });

  // ============================================
  // 属性 4：噪声注入稳定性
  // 随机字符串生成，不 crash
  // fast-check v4 已移除 unicodeString，使用 string 代替
  // ============================================
  it("随机字符串输入不导致崩溃", () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        // 不应抛出异常
        expect(() => {
          detectMagicKeywords(input);
          sanitizeForKeywordDetection(input);
        }).not.toThrow();
      }),
      { numRuns: 500 }
    );
  });

  // ============================================
  // 属性 5：关键词注入必定被检出
  // 向随机文本中插入已知关键词，必定被检出
  // ============================================
  it("注入已知关键词后必定被检出", () => {
    // 已知关键词及其预期类型映射
    const knownKeywords = [
      { keyword: "ralph", type: "ralph" },
      { keyword: "autopilot", type: "autopilot" },
      { keyword: "ultrawork", type: "ultrawork" },
    ] as const;

    fc.assert(
      fc.property(
        // 生成随机前缀（不含关键词特殊字符）
        fc
          .string()
          .filter(
            (s) =>
              !/(ralph|autopilot|ultrawork|ulw|auto-pilot|full\s+auto|cancel|team)\b/i.test(
                s
              )
          ),
        // 从已知关键词中随机选一个
        fc.constantFrom(...knownKeywords),
        (prefix, { keyword, type }) => {
          // 将关键词用空格包裹插入随机文本中，确保词边界
          const input = `${prefix} ${keyword} 结束标记`;
          const matches = detectMagicKeywords(input);
          const types = matches.map((m) => m.type);
          expect(types).toContain(type);
        }
      ),
      { numRuns: 200 }
    );
  });

  // ============================================
  // 附加属性：resolveKeywordPriority 稳定性
  // 多次调用优先级解析结果一致
  // ============================================
  it("优先级解析稳定性：同输入多次结果一致", () => {
    const knownKeywords = ["ralph", "autopilot", "ultrawork", "ulw"];
    fc.assert(
      fc.property(
        fc.subarray(knownKeywords, { minLength: 1, maxLength: 3 }),
        (keywords) => {
          const input = keywords.join(" ");
          const matches1 = detectMagicKeywords(input);
          const matches2 = detectMagicKeywords(input);
          const result1 = resolveKeywordPriority(matches1);
          const result2 = resolveKeywordPriority(matches2);
          expect(result1?.type).toBe(result2?.type);
        }
      ),
      { numRuns: 100 }
    );
  });
});
