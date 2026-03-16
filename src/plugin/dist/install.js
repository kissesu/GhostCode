import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  chmodSync,
  unlinkSync
} from "node:fs";
import { dirname, join, basename } from "node:path";
import { homedir, tmpdir } from "node:os";
import { createRequire } from "node:module";
import { createGunzip } from "node:zlib";
import { createReadStream } from "node:fs";
import { downloadWithRetry } from "./utils/downloader.js";
import { verifyChecksum, parseSha256Sums } from "./utils/checksum.js";
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");
const GITHUB_REPO = "kissesu/GhostCode";
function detectPlatform() {
  const { platform, arch } = process;
  if (platform === "darwin" && arch === "arm64") {
    return "darwin-arm64";
  }
  if (platform === "darwin" && (arch === "x64" || arch === "ia32")) {
    return "darwin-x64";
  }
  if (platform === "linux" && arch === "x64") {
    return "linux-x64";
  }
  throw new Error(
    `\u4E0D\u652F\u6301\u7684\u5E73\u53F0: ${platform}/${arch}\u3002GhostCode \u5F53\u524D\u652F\u6301: macOS ARM64\u3001macOS x64\u3001Linux x64`
  );
}
function resolvePlatform(platform, arch) {
  if (platform === "darwin" && arch === "arm64") {
    return "darwin-arm64";
  }
  if (platform === "darwin" && (arch === "x64" || arch === "ia32")) {
    return "darwin-x64";
  }
  if (platform === "linux" && arch === "x64") {
    return "linux-x64";
  }
  throw new Error(
    `\u4E0D\u652F\u6301\u7684\u5E73\u53F0: ${platform}/${arch}\u3002GhostCode \u5F53\u524D\u652F\u6301: macOS ARM64\u3001macOS x64\u3001Linux x64`
  );
}
function platformToBinaryName(platform) {
  const mapping = {
    "darwin-arm64": "ghostcoded-darwin-arm64",
    "darwin-x64": "ghostcoded-darwin-x64",
    "linux-x64": "ghostcoded-linux-x64"
  };
  return mapping[platform];
}
function readPluginVersion() {
  const require2 = createRequire(import.meta.url);
  const pkg = require2("../package.json");
  if (typeof pkg.version !== "string" || !pkg.version) {
    throw new Error("package.json \u4E2D\u7F3A\u5C11\u6709\u6548\u7684 version \u5B57\u6BB5");
  }
  return pkg.version;
}
function isInstalledInDir(currentVersion, targetDir) {
  const markerPath = join(targetDir, ".installed");
  if (!existsSync(markerPath)) {
    return false;
  }
  if (!existsSync(join(targetDir, "ghostcoded"))) {
    return false;
  }
  if (!existsSync(join(targetDir, "ghostcode-mcp"))) {
    return false;
  }
  try {
    const content = readFileSync(markerPath, "utf-8");
    const marker = JSON.parse(content);
    return marker.version === currentVersion;
  } catch {
    return false;
  }
}
function writeInstalledMarkerToDir(version, platform, targetDir) {
  const marker = {
    version,
    installedAt: (/* @__PURE__ */ new Date()).toISOString(),
    platform
  };
  writeFileSync(join(targetDir, ".installed"), JSON.stringify(marker, null, 2), "utf-8");
}
function buildReleaseAssetUrl(version, platform, arch) {
  const supportedPlatform = resolvePlatform(platform, arch);
  const bundleName = `ghostcode-${supportedPlatform}.tar.gz`;
  return `https://github.com/${GITHUB_REPO}/releases/download/v${version}/${bundleName}`;
}
function buildChecksumUrl(version) {
  return `https://github.com/${GITHUB_REPO}/releases/download/v${version}/ghostcode_SHA256SUMS`;
}
async function extractBundle(bundlePath, targetDir) {
  await extractTarGz(bundlePath, targetDir);
  const daemonBin = join(targetDir, "ghostcoded");
  const mcpBin = join(targetDir, "ghostcode-mcp");
  if (!existsSync(daemonBin)) {
    throw new Error(`bundle \u89E3\u538B\u540E\u7F3A\u5C11 ghostcoded \u4E8C\u8FDB\u5236: ${daemonBin}`);
  }
  if (!existsSync(mcpBin)) {
    throw new Error(`bundle \u89E3\u538B\u540E\u7F3A\u5C11 ghostcode-mcp \u4E8C\u8FDB\u5236: ${mcpBin}`);
  }
  chmodSync(daemonBin, 493);
  chmodSync(mcpBin, 493);
}
async function extractTarGz(tarGzPath, destDir) {
  return new Promise((resolve, reject) => {
    const gunzip = createGunzip();
    const inputStream = createReadStream(tarGzPath);
    const chunks = [];
    gunzip.on("data", (chunk) => {
      chunks.push(chunk);
    });
    gunzip.on("end", () => {
      try {
        const tarBuffer = Buffer.concat(chunks);
        parseTarBuffer(tarBuffer, destDir);
        resolve();
      } catch (err) {
        reject(err);
      }
    });
    gunzip.on("error", reject);
    inputStream.on("error", reject);
    inputStream.pipe(gunzip);
  });
}
function parseTarBuffer(buffer, destDir) {
  const BLOCK_SIZE = 512;
  let offset = 0;
  while (offset < buffer.length) {
    if (offset + BLOCK_SIZE > buffer.length) {
      break;
    }
    const header = buffer.subarray(offset, offset + BLOCK_SIZE);
    if (header.every((byte) => byte === 0)) {
      break;
    }
    const nameRaw = header.subarray(0, 100);
    const nullIdx = nameRaw.indexOf(0);
    const name = nameRaw.subarray(0, nullIdx >= 0 ? nullIdx : 100).toString("utf-8");
    const sizeRaw = header.subarray(124, 136);
    const sizeStr = sizeRaw.toString("utf-8").replace(/\0/g, "").trim();
    const fileSize = parseInt(sizeStr, 8);
    const typeflag = header[156];
    const isRegularFile = typeflag === 48 || typeflag === 0;
    offset += BLOCK_SIZE;
    if (isRegularFile && fileSize > 0 && name) {
      const fileName = basename(name);
      const fileData = buffer.subarray(offset, offset + fileSize);
      if (fileName === "ghostcoded" || fileName === "ghostcode-mcp") {
        const destPath = join(destDir, fileName);
        writeFileSync(destPath, fileData);
      }
    }
    if (fileSize > 0) {
      const alignedSize = Math.ceil(fileSize / BLOCK_SIZE) * BLOCK_SIZE;
      offset += alignedSize;
    }
  }
}
async function installFromRelease(version, platform, arch, targetDir = join(GHOSTCODE_HOME, "bin")) {
  if (isInstalledInDir(version, targetDir)) {
    return;
  }
  mkdirSync(targetDir, { recursive: true });
  const supportedPlatform = resolvePlatform(platform, arch);
  const bundleName = `ghostcode-${supportedPlatform}.tar.gz`;
  const bundleUrl = buildReleaseAssetUrl(version, platform, arch);
  const checksumUrl = buildChecksumUrl(version);
  const tempDir = join(tmpdir(), `ghostcode-install-${Date.now()}`);
  mkdirSync(tempDir, { recursive: true });
  const bundleTempPath = join(tempDir, bundleName);
  const checksumTempPath = join(tempDir, "SHA256SUMS");
  try {
    await downloadWithRetry({
      url: bundleUrl,
      destPath: bundleTempPath
    });
    await downloadWithRetry({
      url: checksumUrl,
      destPath: checksumTempPath
    });
    const checksumContent = readFileSync(checksumTempPath, "utf-8");
    const expectedHash = parseSha256Sums(checksumContent, bundleName);
    if (!expectedHash) {
      throw new Error(
        `SHA256SUMS \u6587\u4EF6\u4E2D\u672A\u627E\u5230 ${bundleName} \u7684\u6821\u9A8C\u548C\u3002\u7248\u672C ${version} \u7684 Release \u6587\u4EF6\u53EF\u80FD\u4E0D\u5B8C\u6574\u3002`
      );
    }
    const checksumMatches = await verifyChecksum(bundleTempPath, expectedHash);
    if (!checksumMatches) {
      throw new Error(
        `Checksum \u6821\u9A8C\u5931\u8D25\uFF01bundle ${bundleName} \u7684 SHA256 \u4E0D\u5339\u914D\u3002
\u671F\u671B: ${expectedHash}
\u6587\u4EF6\u53EF\u80FD\u5DF2\u635F\u574F\u6216\u906D\u5230\u7BE1\u6539\u3002\u5B89\u88C5\u5DF2\u4E2D\u6B62\u3002`
      );
    }
    await extractBundle(bundleTempPath, targetDir);
    writeInstalledMarkerToDir(version, supportedPlatform, targetDir);
  } finally {
    try {
      if (existsSync(bundleTempPath)) {
        unlinkSync(bundleTempPath);
      }
      if (existsSync(checksumTempPath)) {
        unlinkSync(checksumTempPath);
      }
    } catch {
    }
  }
}
async function installGhostcode() {
  const currentVersion = readPluginVersion();
  const targetBinDir = join(GHOSTCODE_HOME, "bin");
  if (isInstalledInDir(currentVersion, targetBinDir)) {
    return;
  }
  const platform = detectPlatform();
  const daemonBinaryName = platformToBinaryName(platform);
  const pluginBinDir = join(dirname(new URL(import.meta.url).pathname), "..", "bin");
  const sourceDaemonPath = join(pluginBinDir, daemonBinaryName);
  if (!existsSync(sourceDaemonPath)) {
    throw new Error(
      `Plugin \u5305\u5185\u7F3A\u5C11\u5E73\u53F0\u5BF9\u5E94 Daemon \u4E8C\u8FDB\u5236: ${sourceDaemonPath}
\u8BF7\u91CD\u65B0\u5B89\u88C5 GhostCode Plugin \u6216\u4ECE GitHub Release \u624B\u52A8\u4E0B\u8F7D\u3002`
    );
  }
  const mcpBinaryName = `ghostcode-mcp-${platform}`;
  const sourceMcpPath = join(pluginBinDir, mcpBinaryName);
  if (!existsSync(sourceMcpPath)) {
    throw new Error(
      `Plugin \u5305\u5185\u7F3A\u5C11\u5E73\u53F0\u5BF9\u5E94 MCP \u4E8C\u8FDB\u5236: ${sourceMcpPath}
\u8BF7\u91CD\u65B0\u5B89\u88C5 GhostCode Plugin \u6216\u4ECE GitHub Release \u624B\u52A8\u4E0B\u8F7D\u3002`
    );
  }
  mkdirSync(targetBinDir, { recursive: true });
  const targetDaemonPath = join(targetBinDir, "ghostcoded");
  copyFileSync(sourceDaemonPath, targetDaemonPath);
  chmodSync(targetDaemonPath, 493);
  const targetMcpPath = join(targetBinDir, "ghostcode-mcp");
  copyFileSync(sourceMcpPath, targetMcpPath);
  chmodSync(targetMcpPath, 493);
  writeInstalledMarkerToDir(currentVersion, platform, targetBinDir);
}
export {
  buildReleaseAssetUrl,
  installFromRelease,
  installGhostcode
};
//# sourceMappingURL=install.js.map