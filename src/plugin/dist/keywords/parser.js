import { sanitizeForKeywordDetection } from "./sanitize.js";
const KEYWORD_PATTERNS = {
  // cancel：匹配 cancel 关键词（精确词边界）
  cancel: {
    regex: /\b(cancel)\b/i,
    priority: 1
  },
  // ralph：匹配 ralph，排除 ralph-xxx 连字符形式（如 ralph-mode）
  // 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:48
  ralph: {
    regex: /\b(ralph)\b(?!-)/i,
    priority: 2
  },
  // autopilot：匹配多种变体
  // - autopilot（基本形式）
  // - auto-pilot（连字符形式）
  // - auto pilot（空格形式）
  // - full auto（全自动表达）
  // 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:49
  autopilot: {
    regex: /\b(autopilot|auto[\s-]?pilot|full\s+auto)\b/i,
    priority: 3
  },
  // team：匹配 team，但排除冠词/代词修饰的常见表达
  // 排除：my team, the team, our team, his team, her team, their team, a team, its team
  // 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:53
  team: {
    regex: /(?<!\b(?:my|the|our|a|his|her|their|its)\s)\bteam\b/i,
    priority: 4
  },
  // ultrawork：匹配 ultrawork 及其缩写 ulw
  // 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:51
  ultrawork: {
    regex: /\b(ultrawork|ulw)\b/i,
    priority: 5
  }
};
const KEYWORD_PRIORITY_ORDER = [
  "cancel",
  "ralph",
  "autopilot",
  "team",
  "ultrawork"
];
function detectMagicKeywords(input) {
  const cleanedText = sanitizeForKeywordDetection(input);
  const matches = [];
  for (const type of KEYWORD_PRIORITY_ORDER) {
    const { regex, priority } = KEYWORD_PATTERNS[type];
    const matchResult = cleanedText.match(regex);
    if (matchResult !== null) {
      matches.push({
        type,
        priority,
        match: matchResult[0]
      });
    }
  }
  return matches;
}
function resolveKeywordPriority(matches) {
  if (matches.length === 0) {
    return null;
  }
  const sorted = [...matches].sort((a, b) => a.priority - b.priority);
  return sorted[0] ?? null;
}
export {
  KEYWORD_PATTERNS,
  detectMagicKeywords,
  resolveKeywordPriority
};
//# sourceMappingURL=parser.js.map