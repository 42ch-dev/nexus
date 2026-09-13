export function parseJsonBuffer<T>(buffer: Uint8Array): T {
  const text = new TextDecoder('utf-8', { fatal: true }).decode(buffer);
  return JSON.parse(text) as T;
}
