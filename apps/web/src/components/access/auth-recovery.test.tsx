import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { AUTHORIZED_EVENT, UNAUTHORIZED_EVENT } from '@/lib/nexus/errors';
import { readStoredApiKey } from '@/lib/client-context';
import { useHandlers } from '@/test/msw-server';

import { AuthRecoveryGate } from './auth-recovery';

const UNAUTHORIZED = () =>
  HttpResponse.json(
    { success: false, error: { code: 'auth_required', message: 'Authentication required' } },
    { status: 401 },
  );

const CREATORS_OK = () =>
  HttpResponse.json({ items: [], pagination: { limit: 50, has_more: false, next_cursor: null } });

function renderGate() {
  return render(
    <AuthRecoveryGate>
      <p>intended destination</p>
    </AuthRecoveryGate>,
  );
}

function signalUnauthorized(times = 1) {
  act(() => {
    for (let i = 0; i < times; i += 1) {
      window.dispatchEvent(new Event(UNAUTHORIZED_EVENT));
    }
  });
}

afterEach(cleanup);

describe('AuthRecoveryGate', () => {
  it('renders a single §6.1 surface for a concurrent 401 burst', () => {
    renderGate();
    expect(screen.getByText('intended destination')).toBeInTheDocument();

    signalUnauthorized(3);

    expect(screen.getAllByText('需要访问授权')).toHaveLength(1);
    expect(
      screen.getByText(
        '此浏览器还没有获得本 Nexus 服务的访问授权。Nexus 每次启动都会在服务终端打印一条带一次性令牌的访问链接——回到终端复制并打开它即可；也可以直接输入你的 API Key。',
      ),
    ).toBeInTheDocument();
    expect(screen.getByLabelText('输入 API Key…')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '授权' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '重新检测' })).toBeInTheDocument();
    expect(screen.queryByText('intended destination')).not.toBeInTheDocument();
  });

  it('keeps the surface and explains a rejected API key', async () => {
    useHandlers(http.get('/v1/daemon/creators', UNAUTHORIZED));
    renderGate();
    signalUnauthorized();

    fireEvent.change(screen.getByLabelText('输入 API Key…'), { target: { value: 'wrong-key' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '授权' }));
    });

    expect(await screen.findByText('API Key 无效或已失效，请检查后重试。')).toBeInTheDocument();
    expect(screen.getByText('需要访问授权')).toBeInTheDocument();
  });

  it('accepts a valid key, stores it, and restores the intended destination', async () => {
    useHandlers(http.get('/v1/daemon/creators', CREATORS_OK));
    const authorized = vi.fn();
    window.addEventListener(AUTHORIZED_EVENT, authorized);
    renderGate();
    signalUnauthorized();

    fireEvent.change(screen.getByLabelText('输入 API Key…'), { target: { value: 'good-key' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '授权' }));
    });

    expect(screen.getByText('intended destination')).toBeInTheDocument();
    expect(window.localStorage.getItem('nexus-api-key')).toBe('good-key');
    expect(authorized).toHaveBeenCalledTimes(1);
    window.removeEventListener(AUTHORIZED_EVENT, authorized);
  });

  it('re-detects access after the bootstrap finished in another tab', async () => {
    useHandlers(http.get('/v1/daemon/creators', CREATORS_OK));
    renderGate();
    signalUnauthorized();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '重新检测' }));
    });

    expect(screen.getByText('intended destination')).toBeInTheDocument();
  });

  it('drops a stale stored key when the cookie-only probe succeeds', async () => {
    window.localStorage.setItem('nexus-api-key', 'stale-key');
    const sentKeys: (string | null)[] = [];
    useHandlers(
      http.get('/v1/daemon/creators', ({ request }) => {
        sentKeys.push(request.headers.get('x-api-key'));
        return CREATORS_OK();
      }),
    );
    renderGate();
    signalUnauthorized();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '重新检测' }));
    });

    // The probe is cookie-only, and the stale key is gone, so the client that
    // ClientProvider rebuilds (which reads this store) sends no X-API-Key and
    // the valid session cookie wins service-side.
    expect(sentKeys).toEqual([null]);
    expect(window.localStorage.getItem('nexus-api-key')).toBeNull();
    expect(readStoredApiKey()).toBeNull();
    expect(screen.getByText('intended destination')).toBeInTheDocument();
  });
});
