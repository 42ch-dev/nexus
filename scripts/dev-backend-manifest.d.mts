export interface DaemonEndpointOptions {
  portEnv?: string;
  urlEnv?: string;
}

export interface DaemonEndpoint {
  baseUrl: string;
  port: number;
}

export function resolveDaemonEndpoint(options?: DaemonEndpointOptions): DaemonEndpoint;
