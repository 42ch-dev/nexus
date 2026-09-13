import { createHash, X509Certificate } from 'node:crypto';
import type { CertFingerprintResponse } from '@42ch/nexus-contracts';

export function fingerprintFromPem(pem: string): Pick<CertFingerprintResponse, 'fingerprint' | 'algorithm'> {
  const cert = new X509Certificate(pem);
  const digest = createHash('sha256').update(cert.raw).digest();
  const hex = [...digest].map((byte) => byte.toString(16).padStart(2, '0')).join(':');
  return {
    fingerprint: `SHA256:${hex}`,
    algorithm: 'sha256',
  };
}

export function certCreatedAtFromMtime(mtimeMs: number): string {
  return new Date(mtimeMs).toISOString();
}
