import * as fs from "node:fs/promises";
import * as net from "node:net";
import * as os from "node:os";
import * as path from "node:path";
const DEFAULT_BINARY_PATH = path.join(os.homedir(), ".ghostcode", "bin", "ghostcoded");
async function checkBinaryPath(binaryPath) {
  const targetPath = binaryPath ?? DEFAULT_BINARY_PATH;
  try {
    await fs.access(targetPath);
    return {
      name: "binary",
      status: "PASS",
      message: `ghostcoded \u4E8C\u8FDB\u5236\u5B58\u5728\u4E8E ${targetPath}`
    };
  } catch {
    return {
      name: "binary",
      status: "FAIL",
      message: `\u627E\u4E0D\u5230 ghostcoded \u4E8C\u8FDB\u5236\u6587\u4EF6\uFF0C\u8DEF\u5F84\uFF1A${targetPath}`,
      suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u91CD\u65B0\u5B89\u88C5 Daemon"
    };
  }
}
function checkNodeVersion(version) {
  const versionStr = version ?? process.version;
  const majorStr = versionStr.replace(/^v/, "").split(".")[0];
  const major = majorStr !== void 0 ? parseInt(majorStr, 10) : 0;
  if (major >= 20) {
    return {
      name: "node-version",
      status: "PASS",
      message: `Node.js \u7248\u672C ${versionStr} \u6EE1\u8DB3\u6700\u4F4E\u8981\u6C42 (>= 20)`
    };
  }
  return {
    name: "node-version",
    status: "FAIL",
    message: `Node.js \u7248\u672C ${versionStr} \u4F4E\u4E8E\u6700\u4F4E\u8981\u6C42\uFF0C\u9700\u8981 >= 20`,
    suggestion: "\u8BF7\u5347\u7EA7 Node.js \u5230 20 \u6216\u66F4\u9AD8\u7248\u672C\uFF0C\u63A8\u8350\u4F7F\u7528 volta \u7BA1\u7406\u7248\u672C"
  };
}
async function checkDaemonReachable() {
  const addrPath = path.join(os.homedir(), ".ghostcode", "daemon", "ghostcoded.addr.json");
  let socketPath;
  try {
    const content = await fs.readFile(addrPath, "utf-8");
    const parsed = JSON.parse(content);
    const rawPath = parsed["path"] ?? parsed["socket_path"];
    if (typeof rawPath !== "string" || rawPath.length === 0) {
      return {
        name: "daemon-reachable",
        status: "FAIL",
        message: "addr.json \u4E2D path \u5B57\u6BB5\u65E0\u6548\u6216\u7F3A\u5931\uFF08\u5B57\u6BB5\u5951\u7EA6\uFF1A\u9700\u8981 path \u6216 socket_path\uFF09",
        suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u91CD\u542F Daemon \u4EE5\u5199\u5165\u6B63\u786E\u7684 addr.json"
      };
    }
    socketPath = rawPath;
  } catch {
    return {
      name: "daemon-reachable",
      status: "FAIL",
      message: "\u65E0\u6CD5\u8BFB\u53D6 addr.json\uFF0CDaemon \u53EF\u80FD\u672A\u542F\u52A8",
      suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u542F\u52A8 Daemon"
    };
  }
  return new Promise((resolve) => {
    const socket = net.createConnection(socketPath);
    let resolved = false;
    const timeout = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        socket.destroy();
        resolve({
          name: "daemon-reachable",
          status: "FAIL",
          message: `\u8FDE\u63A5 Daemon socket \u8D85\u65F6\uFF08\u8DEF\u5F84\uFF1A${socketPath}\uFF09`,
          suggestion: "\u8BF7\u8FD0\u884C `ghostcode doctor` \u68C0\u67E5 Daemon \u72B6\u6001"
        });
      }
    }, 2e3);
    socket.on("connect", () => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        socket.destroy();
        resolve({
          name: "daemon-reachable",
          status: "PASS",
          message: `\u6210\u529F\u8FDE\u63A5\u5230 Daemon\uFF08\u8DEF\u5F84\uFF1A${socketPath}\uFF09`
        });
      }
    });
    socket.on("error", (err) => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        resolve({
          name: "daemon-reachable",
          status: "FAIL",
          message: `\u65E0\u6CD5\u8FDE\u63A5\u5230 Daemon\uFF1A${err.message}`,
          suggestion: "\u8BF7\u8FD0\u884C `ghostcode doctor` \u68C0\u67E5 Daemon \u72B6\u6001"
        });
      }
    });
  });
}
async function checkVersionMatch(pluginVersion, daemonVersion) {
  let pVer;
  if (pluginVersion !== void 0) {
    pVer = pluginVersion;
  } else {
    try {
      const pkgPath = path.join(path.dirname(new URL(import.meta.url).pathname), "..", "..", "package.json");
      const pkgContent = await fs.readFile(pkgPath, "utf-8");
      const pkg = JSON.parse(pkgContent);
      pVer = pkg.version ?? "unknown";
    } catch {
      pVer = "unknown";
    }
  }
  let dVer;
  if (daemonVersion !== void 0) {
    dVer = daemonVersion;
  } else {
    const addrPath = path.join(os.homedir(), ".ghostcode", "daemon", "ghostcoded.addr.json");
    try {
      const content = await fs.readFile(addrPath, "utf-8");
      const parsed = JSON.parse(content);
      const rawVersion = parsed["version"];
      if (typeof rawVersion !== "string" || rawVersion.length === 0) {
        return {
          name: "version-match",
          status: "WARN",
          message: "addr.json \u4E2D\u7F3A\u5C11 version \u5B57\u6BB5\uFF0C\u8DF3\u8FC7\u7248\u672C\u5339\u914D\u68C0\u67E5",
          suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u786E\u4FDD Daemon \u6B63\u786E\u5B89\u88C5"
        };
      }
      dVer = rawVersion;
    } catch {
      return {
        name: "version-match",
        status: "WARN",
        message: "\u65E0\u6CD5\u8BFB\u53D6 Daemon \u7248\u672C\u4FE1\u606F\uFF08Daemon \u53EF\u80FD\u672A\u542F\u52A8\uFF09\uFF0C\u8DF3\u8FC7\u7248\u672C\u5339\u914D\u68C0\u67E5",
        suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u786E\u4FDD Daemon \u6B63\u786E\u5B89\u88C5"
      };
    }
  }
  if (pVer === dVer) {
    return {
      name: "version-match",
      status: "PASS",
      message: `Plugin \u7248\u672C (${pVer}) \u4E0E Daemon \u7248\u672C (${dVer}) \u5339\u914D`
    };
  }
  return {
    name: "version-match",
    status: "FAIL",
    message: `\u7248\u672C\u4E0D\u5339\u914D\uFF1APlugin ${pVer} vs Daemon ${dVer}`,
    suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u66F4\u65B0 Daemon \u5230\u5339\u914D\u7248\u672C"
  };
}
async function checkConfigValid() {
  const configPath = path.join(os.homedir(), ".ghostcode", "config.toml");
  try {
    const stat = await fs.stat(configPath);
    if (stat.size === 0) {
      return {
        name: "config",
        status: "WARN",
        message: `\u914D\u7F6E\u6587\u4EF6\u5B58\u5728\u4F46\u4E3A\u7A7A\uFF1A${configPath}`,
        suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u751F\u6210\u9ED8\u8BA4\u914D\u7F6E"
      };
    }
    return {
      name: "config",
      status: "PASS",
      message: `\u914D\u7F6E\u6587\u4EF6\u5B58\u5728\u4E14\u975E\u7A7A\uFF1A${configPath}`
    };
  } catch {
    return {
      name: "config",
      status: "FAIL",
      message: `\u914D\u7F6E\u6587\u4EF6\u4E0D\u5B58\u5728\uFF1A${configPath}`,
      suggestion: "\u8BF7\u8FD0\u884C `ghostcode init` \u521D\u59CB\u5316\u914D\u7F6E"
    };
  }
}
export {
  checkBinaryPath,
  checkConfigValid,
  checkDaemonReachable,
  checkNodeVersion,
  checkVersionMatch
};
//# sourceMappingURL=checkers.js.map