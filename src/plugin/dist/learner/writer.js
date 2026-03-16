import { writeFile, mkdir } from "node:fs/promises";
import { join } from "node:path";
async function writeSkillFile(dir, filename, content) {
  if (!/^[a-zA-Z0-9._-]+$/.test(filename)) {
    throw new Error(`\u975E\u6CD5\u6587\u4EF6\u540D: "${filename}" \u5305\u542B\u4E0D\u5141\u8BB8\u7684\u5B57\u7B26`);
  }
  if (filename.includes("..")) {
    throw new Error(`\u975E\u6CD5\u6587\u4EF6\u540D: "${filename}" \u5305\u542B\u8DEF\u5F84\u904D\u5386\u5B57\u7B26`);
  }
  await mkdir(dir, { recursive: true });
  const filepath = join(dir, filename);
  await writeFile(filepath, content, "utf-8");
}
export {
  writeSkillFile
};
//# sourceMappingURL=writer.js.map