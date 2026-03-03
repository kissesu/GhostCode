/**
 * @file main.tsx
 * @description React 应用入口，挂载根组件到 DOM
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import App from './App';

// 获取根 DOM 节点并挂载 React 应用
const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('找不到根元素 #root，请检查 index.html');
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
