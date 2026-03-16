import { execFile } from "child_process";
function getPlatformCommand() {
  const platform = process.platform;
  if (platform === "darwin") {
    return "open";
  } else if (platform === "linux") {
    return "xdg-open";
  } else if (platform === "win32") {
    return "start";
  } else {
    throw new Error(`\u4E0D\u652F\u6301\u7684\u64CD\u4F5C\u7CFB\u7EDF: ${platform}`);
  }
}
function openURLWithExec(url, execFn = execFile) {
  return new Promise((resolve, reject) => {
    const command = getPlatformCommand();
    execFn(command, [url], (error) => {
      if (error !== null) {
        reject(
          new Error(`\u6D4F\u89C8\u5668\u542F\u52A8\u5931\u8D25 [${command}]: ${error.message}`)
        );
      } else {
        resolve();
      }
    });
  });
}
async function openURL(url) {
  await openURLWithExec(url);
}
export {
  openURL,
  openURLWithExec
};
//# sourceMappingURL=browser.js.map