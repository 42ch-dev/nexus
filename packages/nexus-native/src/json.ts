export function parseJsonBuffer<T>(buffer: Uint8Array): T {
  const text = new TextDecoder('utf-8', { fatal: true }).decode(buffer);
  return JSON.parse(text) as T;
}

export function stringifyToBuffer(value: unknown): Uint8Array {
  const text = JSON.stringify(value);
  return new TextEncoder().encode(text);
}
