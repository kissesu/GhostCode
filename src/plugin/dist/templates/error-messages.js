var ErrorCategory = /* @__PURE__ */ ((ErrorCategory2) => {
  ErrorCategory2["IPC"] = "IPC";
  ErrorCategory2["CONFIG"] = "CONFIG";
  ErrorCategory2["BINARY"] = "BINARY";
  ErrorCategory2["RUNTIME"] = "RUNTIME";
  ErrorCategory2["NETWORK"] = "NETWORK";
  ErrorCategory2["UNKNOWN"] = "UNKNOWN";
  return ErrorCategory2;
})(ErrorCategory || {});
const ERROR_TEMPLATES = {
  // ----------------------------------------
  // IPC 通信错误（GC_IPC_xxx）
  // 对应 Daemon 与 Plugin 之间的 Unix socket 通信故障
  // ----------------------------------------
  /** connect ECONNREFUSED：Daemon 未启动或 socket 不存在 */
  ECONNREFUSED: {
    code: "GC_IPC_001",
    category: "IPC" /* IPC */,
    title: "Daemon \u8FDE\u63A5\u5931\u8D25",
    description: "\u65E0\u6CD5\u8FDE\u63A5\u5230 GhostCode Daemon\uFF0C\u53EF\u80FD\u662F Daemon \u672A\u542F\u52A8\u6216 socket \u6587\u4EF6\u4E0D\u5B58\u5728",
    suggestion: "\u8BF7\u8FD0\u884C `ghostcode doctor` \u68C0\u67E5 Daemon \u72B6\u6001",
    fixCommand: "ghostcode doctor"
  },
  /** socket 文件不存在：IPC 通道尚未建立 */
  SOCKET_NOT_FOUND: {
    code: "GC_IPC_002",
    category: "IPC" /* IPC */,
    title: "Socket \u6587\u4EF6\u4E0D\u5B58\u5728",
    description: "GhostCode Daemon \u7684 Unix socket \u6587\u4EF6\u672A\u627E\u5230",
    suggestion: "\u8BF7\u8FD0\u884C `ghostcode doctor` \u91CD\u65B0\u542F\u52A8 Daemon",
    fixCommand: "ghostcode doctor"
  },
  /** socket 超时：Daemon 无响应 */
  SOCKET_TIMEOUT: {
    code: "GC_IPC_003",
    category: "IPC" /* IPC */,
    title: "IPC \u8FDE\u63A5\u8D85\u65F6",
    description: "\u8FDE\u63A5 Daemon \u8D85\u65F6\uFF0CDaemon \u53EF\u80FD\u65E0\u54CD\u5E94\u6216\u8D1F\u8F7D\u8FC7\u9AD8",
    suggestion: "\u8BF7\u8FD0\u884C `ghostcode doctor` \u68C0\u67E5 Daemon \u72B6\u6001\u6216\u91CD\u542F",
    fixCommand: "ghostcode doctor"
  },
  // ----------------------------------------
  // 配置错误（GC_CONFIG_xxx）
  // TOML 配置文件读取或解析失败
  // ----------------------------------------
  /** TOML 格式错误 */
  CONFIG_PARSE_ERROR: {
    code: "GC_CONFIG_001",
    category: "CONFIG" /* CONFIG */,
    title: "\u914D\u7F6E\u6587\u4EF6\u89E3\u6790\u5931\u8D25",
    description: "GhostCode \u914D\u7F6E\u6587\u4EF6\u683C\u5F0F\u9519\u8BEF\uFF0C\u65E0\u6CD5\u89E3\u6790 TOML",
    suggestion: "\u8BF7\u68C0\u67E5 ~/.ghostcode/config.toml \u7684\u683C\u5F0F\u662F\u5426\u6B63\u786E"
  },
  /** 配置文件不存在 */
  CONFIG_NOT_FOUND: {
    code: "GC_CONFIG_002",
    category: "CONFIG" /* CONFIG */,
    title: "\u914D\u7F6E\u6587\u4EF6\u4E0D\u5B58\u5728",
    description: "\u672A\u627E\u5230 GhostCode \u914D\u7F6E\u6587\u4EF6",
    suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u521D\u59CB\u5316\u914D\u7F6E",
    fixCommand: "ghostcode init"
  },
  // ----------------------------------------
  // 二进制错误（GC_BINARY_xxx）
  // ghostcode-daemon 可执行文件相关故障
  // ----------------------------------------
  /** Daemon 可执行文件未找到 */
  BINARY_NOT_FOUND: {
    code: "GC_BINARY_001",
    category: "BINARY" /* BINARY */,
    title: "Daemon \u4E8C\u8FDB\u5236\u6587\u4EF6\u4E0D\u5B58\u5728",
    description: "\u627E\u4E0D\u5230 ghostcode-daemon \u53EF\u6267\u884C\u6587\u4EF6\uFF0C\u53EF\u80FD\u672A\u5B89\u88C5\u6216\u8DEF\u5F84\u914D\u7F6E\u9519\u8BEF",
    suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u91CD\u65B0\u5B89\u88C5 Daemon",
    fixCommand: "ghostcode init"
  },
  /** Daemon 权限不足 */
  BINARY_PERMISSION: {
    code: "GC_BINARY_002",
    category: "BINARY" /* BINARY */,
    title: "Daemon \u6743\u9650\u4E0D\u8DB3",
    description: "ghostcode-daemon \u7F3A\u5C11\u6267\u884C\u6743\u9650",
    suggestion: "\u8BF7\u8FD0\u884C `chmod +x` \u8D4B\u4E88\u6267\u884C\u6743\u9650\uFF0C\u6216\u91CD\u65B0\u8FD0\u884C `ghostcode init`",
    fixCommand: "ghostcode init"
  },
  /** 版本不匹配 */
  VERSION_MISMATCH: {
    code: "GC_BINARY_003",
    category: "BINARY" /* BINARY */,
    title: "\u7248\u672C\u4E0D\u5339\u914D",
    description: "Plugin \u7248\u672C\u4E0E Daemon \u7248\u672C\u4E0D\u517C\u5BB9",
    suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u66F4\u65B0 Daemon \u5230\u5339\u914D\u7248\u672C",
    fixCommand: "ghostcode init"
  },
  // ----------------------------------------
  // 运行时错误（GC_RUNTIME_xxx）
  // Daemon 运行期间发生的故障
  // ----------------------------------------
  /** Daemon 崩溃 */
  DAEMON_CRASHED: {
    code: "GC_RUNTIME_001",
    category: "RUNTIME" /* RUNTIME */,
    title: "Daemon \u610F\u5916\u5D29\u6E83",
    description: "GhostCode Daemon \u8FDB\u7A0B\u5DF2\u5D29\u6E83\u6216\u610F\u5916\u9000\u51FA",
    suggestion: "\u8BF7\u8FD0\u884C `ghostcode doctor` \u68C0\u67E5\u65E5\u5FD7\u5E76\u91CD\u542F Daemon",
    fixCommand: "ghostcode doctor"
  },
  /** 会话过期 */
  SESSION_EXPIRED: {
    code: "GC_RUNTIME_002",
    category: "RUNTIME" /* RUNTIME */,
    title: "\u4F1A\u8BDD\u5DF2\u8FC7\u671F",
    description: "\u5F53\u524D GhostCode \u4F1A\u8BDD\u79DF\u7EA6\u5DF2\u8FC7\u671F\uFF0C\u9700\u8981\u91CD\u65B0\u5EFA\u7ACB\u8FDE\u63A5",
    suggestion: "\u8BF7\u91CD\u65B0\u542F\u52A8 Claude Code \u6216\u8FD0\u884C `ghostcode doctor`",
    fixCommand: "ghostcode doctor"
  },
  // ----------------------------------------
  // 网络错误（GC_NETWORK_xxx）
  // 下载 Daemon 二进制或远端资源时的网络故障
  // ----------------------------------------
  /** 下载失败 */
  DOWNLOAD_FAILED: {
    code: "GC_NETWORK_001",
    category: "NETWORK" /* NETWORK */,
    title: "\u4E0B\u8F7D\u5931\u8D25",
    description: "\u4E0B\u8F7D GhostCode Daemon \u4E8C\u8FDB\u5236\u6587\u4EF6\u65F6\u7F51\u7EDC\u8BF7\u6C42\u5931\u8D25",
    suggestion: "\u8BF7\u68C0\u67E5\u7F51\u7EDC\u8FDE\u63A5\u540E\u91CD\u8BD5\uFF0C\u6216\u624B\u52A8\u4E0B\u8F7D\u5E76\u653E\u7F6E\u5230\u6B63\u786E\u8DEF\u5F84"
  },
  /** Checksum 校验不通过 */
  CHECKSUM_MISMATCH: {
    code: "GC_NETWORK_002",
    category: "NETWORK" /* NETWORK */,
    title: "\u6587\u4EF6\u6821\u9A8C\u5931\u8D25",
    description: "\u4E0B\u8F7D\u7684 Daemon \u6587\u4EF6 checksum \u6821\u9A8C\u4E0D\u901A\u8FC7\uFF0C\u53EF\u80FD\u4E0B\u8F7D\u4E0D\u5B8C\u6574\u6216\u6587\u4EF6\u88AB\u7BE1\u6539",
    suggestion: "\u8BF7\u91CD\u65B0\u8FD0\u884C `ghostcode init` \u91CD\u65B0\u4E0B\u8F7D",
    fixCommand: "ghostcode init"
  }
};
const UNKNOWN_ERROR_TEMPLATE = {
  code: "GC_UNKNOWN_000",
  category: "UNKNOWN" /* UNKNOWN */,
  title: "\u672A\u77E5\u9519\u8BEF",
  description: "\u53D1\u751F\u4E86\u672A\u9884\u671F\u7684\u9519\u8BEF",
  suggestion: "\u8BF7\u8FD0\u884C `ghostcode doctor` \u67E5\u770B\u8BE6\u7EC6\u8BCA\u65AD\u4FE1\u606F\uFF0C\u6216\u5411\u5F00\u53D1\u8005\u62A5\u544A\u6B64\u95EE\u9898",
  fixCommand: "ghostcode doctor"
};
export {
  ERROR_TEMPLATES,
  ErrorCategory,
  UNKNOWN_ERROR_TEMPLATE
};
//# sourceMappingURL=error-messages.js.map