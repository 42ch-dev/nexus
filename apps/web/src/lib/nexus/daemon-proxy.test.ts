import { describe, expect, it, vi } from 'vitest';

import {
  DAEMON_PROXY_UNAVAILABLE_STATUS,
  buildDaemonProxyUnavailableBody,
  handleDaemonProxyError,
  isDaemonProxyConnectError,
} from './daemon-proxy';

describe('daemon-proxy', () => {
  it('classifies ECONNREFUSED as a connect error', () => {
    expect(isDaemonProxyConnectError({ code: 'ECONNREFUSED' })).toBe(true);
    expect(isDaemonProxyConnectError({ code: 'EINVAL' })).toBe(false);
  });

  it('builds the daemon error envelope for connect refusal', () => {
    const body = JSON.parse(
      buildDaemonProxyUnavailableBody({ code: 'ECONNREFUSED' }),
    ) as {
      success: boolean;
      error: { code: string; message: string };
    };
    expect(body).toEqual({
      success: false,
      error: {
        code: 'daemon_unavailable',
        message: 'Local daemon is not reachable on the configured port.',
      },
    });
  });

  it('maps proxy connect refusal to 503 instead of Vite default 500', () => {
    const writeHead = vi.fn();
    const end = vi.fn();
    const res = { headersSent: false, writeHead, end };

    handleDaemonProxyError({ code: 'ECONNREFUSED' }, {}, res);

    expect(writeHead).toHaveBeenCalledWith(
      DAEMON_PROXY_UNAVAILABLE_STATUS,
      { 'Content-Type': 'application/json' },
    );
    const body = JSON.parse(end.mock.calls[0]![0] as string) as {
      error: { code: string };
    };
    expect(body.error.code).toBe('daemon_unavailable');
    expect(writeHead.mock.calls[0]![0]).not.toBe(500);
  });

  it('does not write a response when headers were already sent', () => {
    const writeHead = vi.fn();
    const end = vi.fn();
    handleDaemonProxyError({ code: 'ECONNREFUSED' }, {}, { headersSent: true, writeHead, end });
    expect(writeHead).not.toHaveBeenCalled();
    expect(end).not.toHaveBeenCalled();
  });
});
