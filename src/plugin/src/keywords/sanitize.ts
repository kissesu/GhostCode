/**
 * @file sanitize.ts
 * @description 用于关键词检测前的文本清理函数
 * 移除代码块、行内代码、XML 标签、URL、文件路径等噪声，防止误触发关键词检测
 *
 * 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:103-115
 * - sanitizeForKeywordDetection 函数逻辑
 * - removeCodeBlocks 辅助函数
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

/**
 * 移除代码块（围栏式和行内代码）
 *
 * 业务逻辑：
 * 1. 移除三反引号围栏式代码块（```...```）
 * 2. 移除波浪线围栏式代码块（~~~...~~~）
 * 3. 移除行内代码（`...`）
 */
function removeCodeBlocks(text: string): string {
  // 移除三反引号围栏式代码块（支持多行）
  let result = text.replace(/```[\s\S]*?```/g, "");
  // 移除波浪线围栏式代码块（支持多行）
  result = result.replace(/~~~[\s\S]*?~~~/g, "");
  // 移除行内代码（单反引号）
  result = result.replace(/`[^`]+`/g, "");
  return result;
}

/**
 * 对输入文本执行关键词检测前的清理
 *
 * 业务逻辑：
 * 1. 移除 XML 标签（含内容），避免注入攻击导致误触发
 * 2. 移除自闭合 XML 标签
 * 3. 移除 URL（https:// 或 http://），URL 路径可能含关键词片段
 * 4. 移除文件路径（绝对路径 /foo/bar 和相对路径 ./foo/bar）
 * 5. 移除代码块（围栏式和行内代码）
 *
 * @param input - 原始用户输入文本
 * @returns 清理后的文本，保留普通自然语言内容
 */
export function sanitizeForKeywordDetection(input: string): string {
  // ============================================
  // 第一步：移除 XML 标签及其内容
  // 匹配 <tag>...</tag> 格式（标签名必须一致）
  // ============================================
  let result = input.replace(/<(\w[\w-]*)[\s>][\s\S]*?<\/\1>/g, "");
  // 移除自闭合 XML 标签
  result = result.replace(/<\w[\w-]*(?:\s[^>]*)?\s*\/>/g, "");

  // ============================================
  // 第二步：移除 URL
  // 覆盖 https:// 和 http:// 开头的 URL
  // ============================================
  result = result.replace(/https?:\/\/\S+/g, "");

  // ============================================
  // 第三步：移除文件路径
  // 覆盖绝对路径（/foo/bar）和相对路径（./foo/bar）以及多段路径（dir/file.ext）
  // ============================================
  result = result.replace(
    /(^|[\s"'`(])(?:\.?\/(?:[\w.-]+\/)*[\w.-]+|(?:[\w.-]+\/)+[\w.-]+\.\w+)/gm,
    "$1"
  );

  // ============================================
  // 第四步：移除代码块（围栏式和行内代码）
  // ============================================
  result = removeCodeBlocks(result);

  return result;
}
