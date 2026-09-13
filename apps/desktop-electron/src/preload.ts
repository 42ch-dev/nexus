import { contextBridge, ipcRenderer } from 'electron';

/** Frozen preload surface for the private proof shell — no raw ipcRenderer/process/require. */
const api = {
  getLifecycle(): Promise<unknown> {
    return ipcRenderer.invoke('nexus-proof:lifecycle');
  },

  /** Proof-only harness command; restricted steps validated in main. */
  runProofStep(step: string, payload?: Record<string, unknown>): Promise<unknown> {
    return ipcRenderer.invoke('nexus-proof:proof-step', { step, payload });
  },

  onLifecycleChanged(listener: (status: unknown) => void): () => void {
    const channel = 'nexus-proof:lifecycle-changed';
    const handler = (_event: Electron.IpcRendererEvent, status: unknown) => listener(status);
    ipcRenderer.on(channel, handler);
    return () => ipcRenderer.removeListener(channel, handler);
  },

  /** Writes JSON into the DOM inspection harness for automated proof drivers. */
  publishHarnessResult(label: string, value: unknown): void {
    const root = document.getElementById('nexus-electron-proof-root');
    if (!root) return;
    root.dataset[label] = JSON.stringify(value);
    root.textContent = JSON.stringify({ label, value, at: new Date().toISOString() });
  },
};

contextBridge.exposeInMainWorld('nexusProof', api);

function ensureHarnessRoot(): void {
  if (document.getElementById('nexus-electron-proof-root')) return;
  const root = document.createElement('div');
  root.id = 'nexus-electron-proof-root';
  root.hidden = true;
  root.setAttribute('aria-hidden', 'true');
  document.documentElement.appendChild(root);
}

window.addEventListener('DOMContentLoaded', () => {
  ensureHarnessRoot();
  ipcRenderer.invoke('nexus-proof:renderer-ready').catch(() => undefined);
});
