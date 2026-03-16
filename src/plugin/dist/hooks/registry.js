const registeredHooks = /* @__PURE__ */ new Map();
function registerHook(eventType, handler) {
  const existing = registeredHooks.get(eventType) ?? [];
  registeredHooks.set(eventType, [...existing, handler]);
}
function getHooks(eventType) {
  return registeredHooks.get(eventType) ?? [];
}
function clearHooks() {
  registeredHooks.clear();
}
export {
  clearHooks,
  getHooks,
  registerHook
};
//# sourceMappingURL=registry.js.map