function mapStatusToVerdict(status) {
  switch (status) {
    case "Approved":
      return "approved";
    case "Rejected":
      return "rejected";
    case "Cancelled":
      return "cancelled";
    default:
      return "in_progress";
  }
}
function formatCheckStatus(status) {
  if (status === "Pending") return "\u5F85\u68C0\u67E5";
  if (status === "Passed") return "\u901A\u8FC7";
  if (typeof status === "object" && "Failed" in status) {
    return `\u5931\u8D25: ${status.Failed}`;
  }
  return "\u672A\u77E5";
}
function getStatusIcon(status) {
  if (status === "Passed") return "[PASS]";
  if (status === "Pending") return "[WAIT]";
  if (typeof status === "object" && "Failed" in status) return "[FAIL]";
  return "[????]";
}
function formatReport(state, format = "markdown") {
  if (format === "json") {
    return formatJsonReport(state);
  }
  return formatMarkdownReport(state);
}
function formatJsonReport(state) {
  return JSON.stringify(
    {
      verdict: mapStatusToVerdict(state.status),
      iteration: state.iteration,
      max_iterations: state.max_iterations,
      checks: state.current_checks.map(([kind, status]) => ({
        kind,
        status: formatCheckStatus(status)
      })),
      history_count: state.history.length
    },
    null,
    2
  );
}
function formatMarkdownReport(state) {
  const verdict = mapStatusToVerdict(state.status);
  const lines = [];
  lines.push(`## Ralph \u9A8C\u8BC1\u62A5\u544A`);
  lines.push(``);
  lines.push(
    `**\u72B6\u6001**: ${verdict} | **\u8FED\u4EE3**: ${state.iteration + 1}/${state.max_iterations}`
  );
  lines.push(``);
  lines.push(`| \u68C0\u67E5\u9879 | \u72B6\u6001 |`);
  lines.push(`|--------|------|`);
  for (const [kind, status] of state.current_checks) {
    const icon = getStatusIcon(status);
    lines.push(`| ${kind} | ${icon} ${formatCheckStatus(status)} |`);
  }
  if (state.history.length > 0) {
    lines.push(``);
    lines.push(`### \u5386\u53F2\u8BB0\u5F55`);
    for (let i = 0; i < state.history.length; i++) {
      const iter = state.history[i];
      if (iter === void 0) continue;
      const failCount = iter.failure_reasons.length;
      const summary = failCount === 0 ? "\u5168\u90E8\u901A\u8FC7" : `${failCount} \u9879\u5931\u8D25`;
      lines.push(`- \u7B2C ${i + 1} \u8F6E: ${summary}`);
    }
  }
  return lines.join("\n");
}
export {
  formatReport,
  mapStatusToVerdict
};
//# sourceMappingURL=report.js.map