function extractSkillTemplate(pattern, skillId, skillName) {
  const triggers = pattern.suggestedTriggers.join(", ");
  const tags = pattern.suggestedTags.join(", ");
  const now = (/* @__PURE__ */ new Date()).toISOString();
  const frontmatter = [
    "---",
    `id: ${skillId}`,
    `name: "${skillName}"`,
    `description: "${pattern.problem.slice(0, 100).replace(/"/g, "'")}"`,
    `triggers: "${triggers}"`,
    `source: extracted`,
    `version: "1.0.0"`,
    `quality: "${pattern.confidence}"`,
    `usageCount: "0"`,
    `tags: ${tags}`,
    `createdAt: ${now}`,
    "---"
  ].join("\n");
  const body = [
    `# ${skillName}`,
    "",
    "## \u95EE\u9898",
    pattern.problem,
    "",
    "## \u89E3\u51B3\u65B9\u6848",
    pattern.solution,
    "",
    `> \u7F6E\u4FE1\u5EA6: ${pattern.confidence}/100 | \u89C2\u5BDF\u6B21\u6570: ${pattern.occurrences}`
  ].join("\n");
  return `${frontmatter}
${body}`;
}
export {
  extractSkillTemplate
};
//# sourceMappingURL=extractor.js.map