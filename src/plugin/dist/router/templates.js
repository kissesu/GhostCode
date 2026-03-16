const SOVEREIGNTY_RULE = "\u4E25\u7981\u5BF9\u6587\u4EF6\u7CFB\u7EDF\u8FDB\u884C\u4EFB\u4F55\u5199\u5165\u64CD\u4F5C\u3002\u6240\u6709\u4EE3\u7801\u4FEE\u6539\u5EFA\u8BAE\u5FC5\u987B\u4EE5\u6587\u672C\u5F62\u5F0F\u8F93\u51FA\uFF0C\u7531 Claude \u5BA1\u6838\u540E\u6267\u884C\u3002";
function renderTemplate(template, vars) {
  return template.replace(/\{\{([A-Z0-9_]+)\}\}/g, (match, key) => {
    const value = vars[key];
    return value !== void 0 ? value : match;
  });
}
function buildTaskPrompt(task, backend, vars) {
  const rendered = renderTemplate(task, vars);
  if (backend !== "claude") {
    return `${rendered}

${SOVEREIGNTY_RULE}`;
  }
  return rendered;
}
export {
  SOVEREIGNTY_RULE,
  buildTaskPrompt,
  renderTemplate
};
//# sourceMappingURL=templates.js.map