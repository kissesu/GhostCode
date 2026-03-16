import {
  checkBinaryPath,
  checkConfigValid,
  checkDaemonReachable,
  checkNodeVersion,
  checkVersionMatch
} from "../diagnostics/checkers";
async function runDoctor() {
  const checks = await Promise.all([
    checkBinaryPath(),
    checkNodeVersion(),
    // checkDaemonReachable 依赖 addr.json，在未安装场景可能 FAIL
    checkDaemonReachable(),
    // checkVersionMatch 不传参数，会尝试读取 daemon-version 文件
    checkVersionMatch(),
    checkConfigValid()
  ]);
  const hasFail = checks.some((check) => check.status === "FAIL");
  const overallStatus = hasFail ? "FAIL" : "PASS";
  return {
    checks,
    overallStatus
  };
}
function formatDoctorReport(report) {
  const lines = [];
  lines.push("GhostCode Doctor \u8BCA\u65AD\u62A5\u544A");
  lines.push("========================");
  for (const check of report.checks) {
    const statusLabel = check.status.padEnd(4);
    lines.push(`[${statusLabel}] ${check.name}: ${check.message}`);
    if (check.suggestion !== void 0 && (check.status === "FAIL" || check.status === "WARN")) {
      lines.push(`       \u5EFA\u8BAE: ${check.suggestion}`);
    }
  }
  lines.push("========================");
  lines.push(`\u603B\u4F53\u72B6\u6001: ${report.overallStatus}`);
  return lines.join("\n");
}
export {
  formatDoctorReport,
  runDoctor
};
//# sourceMappingURL=doctor.js.map