// @vitest-environment node
import { once } from 'node:events';
import {
  createServer,
  request as httpRequest,
  type IncomingMessage,
  type ServerResponse,
} from 'node:http';
import { Socket, createServer as createNetServer } from 'node:net';
import { describe, expect, it } from 'vitest';

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

  it('maps proxy connect refusal to 503 JSON instead of Vite default 500', async () => {
    const observed = await withHttpServer((req, res) => {
      handleDaemonProxyError({ code: 'ECONNREFUSED' }, req, res);
    });
    expect(observed.status).toBe(DAEMON_PROXY_UNAVAILABLE_STATUS);
    expect(observed.contentType).toBe('application/json');
    const body = JSON.parse(observed.body) as { error: { code: string } };
    expect(body.error.code).toBe('daemon_unavailable');
  });

  it('maps non-connect proxy errors to 502 bad_gateway', async () => {
    const observed = await withHttpServer((req, res) => {
      handleDaemonProxyError(new Error('boom'), req, res);
    });
    expect(observed.status).toBe(502);
    const body = JSON.parse(observed.body) as { error: { code: string } };
    expect(body.error.code).toBe('bad_gateway');
  });

  it('does not write when HTTP headers were already sent', async () => {
    const observed = await withHttpServer((req, res) => {
      res.writeHead(200, { 'Content-Type': 'text/plain' });
      res.write('upstream');
      handleDaemonProxyError({ code: 'ECONNREFUSED' }, req, res);
      res.end();
    });
    expect(observed.status).toBe(200);
    expect(observed.body).toBe('upstream');
  });

  it('does not write when the HTTP response already ended', async () => {
    const observed = await withHttpServer((req, res) => {
      res.writeHead(200, { 'Content-Type': 'text/plain' });
      res.end('upstream');
      handleDaemonProxyError({ code: 'ECONNREFUSED' }, req, res);
    });
    expect(observed.status).toBe(200);
    expect(observed.body).toBe('upstream');
  });

  it('ends a raw websocket socket without writing an HTTP response', async () => {
    const ended = await withSocketServer((socket) => {
      handleDaemonProxyError({ code: 'ECONNREFUSED' }, {}, socket);
    });
    expect(ended.data).toBe('');
    expect(ended.closed).toBe(true);
  });
});

interface HttpObservation {
  status: number;
  contentType: string | undefined;
  body: string;
}

/** Run `handler` inside a real loopback HTTP server and observe the wire result. */
async function withHttpServer(
  handler: (req: IncomingMessage, res: ServerResponse) => void,
): Promise<HttpObservation> {
  const server = createServer(handler);
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (address === null || typeof address === 'string') throw new Error('no bound address');
  const { port } = address;

  try {
    const req = httpRequest({ host: '127.0.0.1', port, path: '/' });
    req.on('error', () => {});
    req.end();
    const [res] = (await once(req, 'response')) as [IncomingMessage];
    const chunks: Buffer[] = [];
    res.on('data', (chunk: Buffer) => chunks.push(chunk));
    await once(res, 'end');
    return {
      status: res.statusCode ?? 0,
      contentType: res.headers['content-type'],
      body: Buffer.concat(chunks).toString('utf8'),
    };
  } finally {
    const done = once(server, 'close');
    server.close();
    await done;
  }
}

/** Run `handler` against a real loopback socket and observe stream completion. */
async function withSocketServer(
  handler: (socket: Socket) => void,
): Promise<{ data: string; closed: boolean }> {
  const server = createNetServer((socket) => handler(socket));
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (address === null || typeof address === 'string') throw new Error('no bound address');
  const { port } = address;

  try {
    const client = new Socket();
    const chunks: Buffer[] = [];
    client.on('data', (chunk: Buffer) => chunks.push(chunk));
    client.on('error', () => {});
    client.connect(port, '127.0.0.1');
    await once(client, 'close');
    return { data: Buffer.concat(chunks).toString('utf8'), closed: true };
  } finally {
    const done = once(server, 'close');
    server.close();
    await done;
  }
}
