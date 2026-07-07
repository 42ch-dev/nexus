export function errorMessage(err: unknown): string {
  if (err && typeof err === 'object' && 'message' in err) {
    const msg = (err as { message: unknown }).message;
    if (typeof msg === 'string' && msg.length > 0) return msg;
  }
  if (typeof err === 'string') return err;
  return '';
}
