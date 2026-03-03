/**
 * @file setup.ts
 * @description Vitest 测试环境初始化，引入 jest-dom 断言扩展，修补 jsdom 缺失的 API
 * @author Atlas.oi
 * @date 2026-03-03
 */

import '@testing-library/jest-dom';

// jsdom 不支持 scrollIntoView，在测试环境中 mock 为空函数
// 避免 Timeline 组件中的 scrollIntoView 调用导致测试失败
Element.prototype.scrollIntoView = () => {};
