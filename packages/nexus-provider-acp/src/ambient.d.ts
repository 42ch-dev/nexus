/// <reference lib="dom" />

declare module 'node:crypto' {
  export function randomUUID(): string;
}

declare module 'node:child_process' {
  import type { Readable, Writable } from 'node:stream';

  export interface ChildProcessWithoutNullStreams {
    stdin: Writable | null;
    stdout: Readable | null;
    stderr: Readable | null;
    exitCode: number | null;
    signalCode: string | null;
    kill(signal?: string): boolean;
    once(event: 'exit', listener: () => void): void;
  }

  export function spawn(
    command: string,
    args: readonly string[],
    options: {
      cwd?: string;
      env?: Record<string, string | undefined>;
      stdio?: ('pipe' | 'ignore' | 'inherit')[];
    },
  ): ChildProcessWithoutNullStreams;
}

declare module 'node:stream' {
  export class Readable {
    on(event: 'data', listener: (chunk: Buffer) => void): this;
    static toWeb(stream: Readable): ReadableStream<Uint8Array>;
  }

  export class Writable {
    end(): void;
    static toWeb(stream: Writable): WritableStream<Uint8Array>;
  }
}

declare const process: {
  env: Record<string, string | undefined>;
};

declare function setTimeout(handler: () => void, timeout?: number): unknown;
declare function clearTimeout(handle: unknown): void;

declare module 'node:buffer' {
  export class Buffer extends Uint8Array {}
}
