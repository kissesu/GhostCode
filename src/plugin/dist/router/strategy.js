const FRONTEND_KEYWORDS = [
  "css",
  "html",
  "ui",
  "ux",
  "style",
  "component",
  "layout",
  "responsive",
  "design",
  "animation"
];
const BACKEND_KEYWORDS = [
  "api",
  "database",
  "db",
  "sql",
  "algorithm",
  "server",
  "backend",
  "logic",
  "auth",
  "middleware"
];
const FORCED_PREFIX = /^\/(codex|claude|gemini)\b/i;
function resolveRoute(taskText) {
  const prefixMatch = taskText.match(FORCED_PREFIX);
  if (prefixMatch && prefixMatch[1]) {
    const matched = prefixMatch[1];
    return {
      backend: matched.toLowerCase(),
      reason: `\u5F3A\u5236\u524D\u7F00 /${matched}`,
      confidence: 1
    };
  }
  const lower = taskText.toLowerCase();
  const frontendScore = FRONTEND_KEYWORDS.filter((kw) => lower.includes(kw)).length;
  const backendScore = BACKEND_KEYWORDS.filter((kw) => lower.includes(kw)).length;
  if (frontendScore > 0 && frontendScore >= backendScore) {
    return {
      backend: "gemini",
      reason: `\u524D\u7AEF\u5173\u952E\u8BCD\u5339\u914D (${frontendScore} \u4E2A)`,
      confidence: Math.min(frontendScore * 0.3, 0.9)
    };
  }
  if (backendScore > 0) {
    return {
      backend: "codex",
      reason: `\u540E\u7AEF\u5173\u952E\u8BCD\u5339\u914D (${backendScore} \u4E2A)`,
      confidence: Math.min(backendScore * 0.3, 0.9)
    };
  }
  return {
    backend: "claude",
    reason: "\u65E0\u5339\u914D\u5173\u952E\u8BCD\uFF0C\u9ED8\u8BA4\u8DEF\u7531",
    confidence: 0
  };
}
export {
  resolveRoute
};
//# sourceMappingURL=strategy.js.map