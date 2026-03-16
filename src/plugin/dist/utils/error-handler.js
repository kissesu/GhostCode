import {
  ERROR_TEMPLATES,
  UNKNOWN_ERROR_TEMPLATE
} from "../templates/error-messages";
function matchErrorTemplate(error) {
  if (typeof error !== "string") {
    const errnoCode = error.code;
    if (errnoCode && Object.prototype.hasOwnProperty.call(ERROR_TEMPLATES, errnoCode)) {
      return ERROR_TEMPLATES[errnoCode];
    }
  }
  const message = typeof error === "string" ? error : error.message;
  const messageUpper = message.toUpperCase();
  for (const key of Object.keys(ERROR_TEMPLATES)) {
    if (messageUpper.includes(key.toUpperCase())) {
      return ERROR_TEMPLATES[key];
    }
  }
  return null;
}
function formatErrorWithFix(error) {
  const errorObj = typeof error === "string" ? new Error(error) : error;
  const template = matchErrorTemplate(errorObj) ?? UNKNOWN_ERROR_TEMPLATE;
  return {
    code: template.code,
    title: template.title,
    description: template.description,
    suggestion: template.suggestion,
    ...template.fixCommand !== void 0 && { fixCommand: template.fixCommand },
    // 保留原始错误对象，方便上层记录日志或调试
    originalError: errorObj
  };
}
function formatErrorAsMarkdown(error) {
  const lines = [];
  lines.push(`**[${error.code}] ${error.title}**`);
  lines.push("");
  lines.push(error.description);
  lines.push("");
  lines.push(`**\u4FEE\u590D\u5EFA\u8BAE\uFF1A** ${error.suggestion}`);
  if (error.fixCommand) {
    lines.push("");
    lines.push("**\u4FEE\u590D\u547D\u4EE4\uFF1A**");
    lines.push("```sh");
    lines.push(error.fixCommand);
    lines.push("```");
  }
  return lines.join("\n");
}
export {
  formatErrorAsMarkdown,
  formatErrorWithFix
};
//# sourceMappingURL=error-handler.js.map