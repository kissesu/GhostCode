/**
 * @file writer.ts
 * @description Skill 文件写入器
 *              将 Skill 模板文本写入指定目录
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { writeFile, mkdir } from "node:fs/promises";
import { join } from "node:path";

/**
 * 将 Skill 内容写入指定目录的文件
 *
 * W8 修复：对 filename 进行安全校验，防止路径遍历攻击
 * 只允许字母数字、连字符、下划线、点号字符，并禁止 .. 路径组件
 *
 * @param dir - 目标目录路径（不存在时自动创建）
 * @param filename - 文件名（含 .md 后缀），只允许安全字符
 * @param content - 文件内容
 * @throws {Error} 当 filename 包含非法字符或路径遍历尝试时抛出
 */
export async function writeSkillFile(
  dir: string,
  filename: string,
  content: string,
): Promise<void> {
  // ============================================
  // W8 安全校验：阻止路径遍历攻击
  // 只允许字母数字、连字符(-)、下划线(_)、点号(.)字符
  // 明确禁止 .. 路径组件，防止越出目标目录
  // ============================================
  if (!/^[a-zA-Z0-9._-]+$/.test(filename)) {
    throw new Error(`非法文件名: "${filename}" 包含不允许的字符`);
  }
  if (filename.includes("..")) {
    throw new Error(`非法文件名: "${filename}" 包含路径遍历字符`);
  }

  // 确保目录存在
  await mkdir(dir, { recursive: true });
  const filepath = join(dir, filename);
  await writeFile(filepath, content, "utf-8");
}
