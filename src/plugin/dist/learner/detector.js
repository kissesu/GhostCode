import { createHash } from "node:crypto";
const MIN_CONTENT_LENGTH = 100;
const HIGH_VALUE_KEYWORDS = [
  "\u9519\u8BEF",
  "error",
  "\u4FEE\u590D",
  "fix",
  "\u89E3\u51B3",
  "solved",
  "resolved",
  "\u65B9\u6848",
  "solution"
];
const ERROR_PATTERNS = [
  /(?:TypeError|ReferenceError|SyntaxError|Error)[:：]\s*([^\n]+)/gi,
  /(?:错误|问题|issue)[:：]\s*([^\n]+)/gi,
  /cannot find|找不到|not found|未找到/gi
];
function detectPatterns(content) {
  if (content.length < MIN_CONTENT_LENGTH) {
    return [];
  }
  const patterns = [];
  const lower = content.toLowerCase();
  let keywordCount = 0;
  for (const kw of HIGH_VALUE_KEYWORDS) {
    const matches = lower.split(kw).length - 1;
    keywordCount += matches;
  }
  let errorMatches = 0;
  for (const pattern of ERROR_PATTERNS) {
    const matches = content.match(pattern);
    if (matches) {
      errorMatches += matches.length;
    }
  }
  if (errorMatches > 0 && keywordCount >= 2) {
    const confidence = Math.min(50 + keywordCount * 5 + errorMatches * 10, 100);
    const now = (/* @__PURE__ */ new Date()).toISOString();
    const id = createHash("sha256").update(content.slice(0, 200)).digest("hex").slice(0, 16);
    patterns.push({
      id,
      problem: content.slice(0, 200).trim(),
      solution: content.slice(-200).trim(),
      confidence,
      occurrences: 1,
      firstSeen: now,
      lastSeen: now,
      suggestedTriggers: extractTriggers(content),
      suggestedTags: extractTags(content)
    });
  }
  return patterns;
}
function extractTriggers(content) {
  const triggers = [];
  if (/rust|cargo/i.test(content)) triggers.push("rust");
  if (/typescript|ts\b/i.test(content)) triggers.push("typescript");
  if (/python|pip/i.test(content)) triggers.push("python");
  if (/error|错误/i.test(content)) triggers.push("fix");
  return triggers.slice(0, 3);
}
function extractTags(content) {
  const tags = [];
  if (/rust/i.test(content)) tags.push("rust");
  if (/typescript/i.test(content)) tags.push("typescript");
  if (/fix|修复/i.test(content)) tags.push("bugfix");
  return tags.slice(0, 3);
}
export {
  detectPatterns
};
//# sourceMappingURL=detector.js.map