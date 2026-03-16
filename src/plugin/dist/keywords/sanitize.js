function removeCodeBlocks(text) {
  let result = text.replace(/```[\s\S]*?```/g, "");
  result = result.replace(/~~~[\s\S]*?~~~/g, "");
  result = result.replace(/`[^`]+`/g, "");
  return result;
}
function sanitizeForKeywordDetection(input) {
  let result = input.replace(/<(\w[\w-]*)[\s>][\s\S]*?<\/\1>/g, "");
  result = result.replace(/<\w[\w-]*(?:\s[^>]*)?\s*\/>/g, "");
  result = result.replace(/https?:\/\/\S+/g, "");
  result = result.replace(
    /(^|[\s"'`(])(?:\.?\/(?:[\w.-]+\/)*[\w.-]+|(?:[\w.-]+\/)+[\w.-]+\.\w+)/gm,
    "$1"
  );
  result = removeCodeBlocks(result);
  return result;
}
export {
  sanitizeForKeywordDetection
};
//# sourceMappingURL=sanitize.js.map