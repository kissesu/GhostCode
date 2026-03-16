import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
async function computeSha256(filePath) {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    const fileStream = createReadStream(filePath);
    fileStream.on("data", (chunk) => {
      hash.update(chunk);
    });
    fileStream.on("end", () => {
      resolve(hash.digest("hex"));
    });
    fileStream.on("error", (err) => {
      reject(new Error(`\u8BA1\u7B97\u6587\u4EF6 SHA256 \u5931\u8D25: ${filePath} - ${err.message}`));
    });
  });
}
function parseSha256Sums(content, fileName) {
  const lines = content.split("\n");
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }
    const match = /^([0-9a-f]{64})\s+(.+)$/i.exec(trimmed);
    if (!match) {
      continue;
    }
    const [, hash, name] = match;
    const parsedFileName = name?.trim().split("/").pop() ?? "";
    if (parsedFileName === fileName || name?.trim() === fileName) {
      return hash ?? null;
    }
  }
  return null;
}
async function verifyChecksum(filePath, expectedHash) {
  const actualHash = await computeSha256(filePath);
  return actualHash.toLowerCase() === expectedHash.toLowerCase();
}
export {
  computeSha256,
  parseSha256Sums,
  verifyChecksum
};
//# sourceMappingURL=checksum.js.map