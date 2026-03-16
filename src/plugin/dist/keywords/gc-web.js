import { openURL } from "../utils/browser.js";
import { ensureWeb, getWebPort } from "../web.js";
function getDashboardUrl(port, token) {
  const baseUrl = `http://127.0.0.1:${port}`;
  if (token !== void 0 && token.length > 0) {
    return `${baseUrl}?token=${encodeURIComponent(token)}`;
  }
  return baseUrl;
}
async function handleGcWeb(options) {
  const port = options?.port ?? getWebPort();
  const token = options?.token;
  await ensureWeb();
  const url = getDashboardUrl(port, token);
  await openURL(url);
  return url;
}
export {
  getDashboardUrl,
  handleGcWeb
};
//# sourceMappingURL=gc-web.js.map