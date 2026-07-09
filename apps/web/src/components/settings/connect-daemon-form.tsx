/**
 * Connect-to-Daemon form — extracted from legacy ConnectDaemonPage (V1.103 P2).
 *
 * Hosted under Settings → Connection. Implements the four author-visible
 * states locked in daemon-runtime.md §16.2. Post activate/revert stays on
 * `/settings/connection` (toast only — no navigate away).
 *
 * Author-facing copy: settings-connection-section.md.
 */

import { useEffect, useMemo, useState } from 'react';
import { AlertCircle, CheckCircle, Fingerprint, Info, Shield, Wifi } from 'lucide-react';

import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useToast } from '@/lib/use-toast';
import {
  normalizeEndpointUrl,
  endpointLabel,
  type ConnectionConfig,
} from '@/lib/nexus/connection-storage';
import { useConnectionConfig, useSetConnectionConfig } from '@/lib/client-context';
import { useFingerprint } from '@/lib/nexus/use-fingerprint';

/** Locked by settings-connection-section.md */
const FORM_CARD_DESCRIPTION =
  'Enter the remote daemon URL and API key. Local mode remains available — you can revert here at any time.';

const URL_FIELD_HELPER =
  'The full HTTPS address of the daemon, including port.';

const API_KEY_HELPER_PREFIX = 'The API key from the daemon machine (';
const API_KEY_HELPER_COMMAND = 'nexus42 daemon api-key';
const API_KEY_HELPER_SUFFIX = ' on that host).';

const FINGERPRINT_TRUST_HELPER =
  'Confirm the certificate fingerprint matches what you see on the daemon machine before connecting.';

export function ConnectDaemonForm() {
  const { toast } = useToast();
  const savedConfig = useConnectionConfig();
  const setConfig = useSetConnectionConfig();
  const { state: fpState, fetchFingerprint, reset: resetFp } = useFingerprint();

  const [url, setUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [label, setLabel] = useState('');
  const [showKey, setShowKey] = useState(false);

  useEffect(() => {
    if (savedConfig) {
      setUrl(savedConfig.endpointUrl);
      setApiKey(savedConfig.apiKey);
      setLabel(savedConfig.label ?? '');
    }
  }, [savedConfig]);

  const normalizedUrl = useMemo(() => normalizeEndpointUrl(url), [url]);

  const hasSavedConfig = Boolean(savedConfig);
  const savedEndpointMatches = savedConfig?.endpointUrl === normalizedUrl;
  const savedFingerprint = savedConfig?.pinnedFingerprint;
  const reconnectWithMatch =
    hasSavedConfig &&
    savedEndpointMatches &&
    savedFingerprint !== undefined &&
    fpState.status === 'success' &&
    savedFingerprint === fpState.response.fingerprint;

  const isLoopbackOnly =
    fpState.status === 'success' && fpState.response.fingerprint === '';
  const fingerprintMismatch =
    fpState.status === 'success' &&
    savedFingerprint !== undefined &&
    savedFingerprint !== fpState.response.fingerprint;

  async function handleFetchFingerprint() {
    if (!normalizedUrl) {
      toast({ variant: 'error', title: 'Enter a daemon URL' });
      return;
    }
    resetFp();
    await fetchFingerprint(normalizedUrl);
  }

  async function activateConfig(nextFingerprint?: string) {
    if (!normalizedUrl || !apiKey) {
      toast({ variant: 'error', title: 'Enter the daemon URL and API key' });
      return;
    }
    const next: ConnectionConfig = {
      endpointUrl: normalizedUrl,
      apiKey,
      label: label.trim() || endpointLabel(normalizedUrl),
      active: true,
      pinnedFingerprint: nextFingerprint,
    };
    await setConfig(next);
    toast({
      variant: 'success',
      title: 'Connected to daemon',
      description: `Using ${next.endpointUrl}`,
    });
    // Stay on /settings/connection — no navigate away (V1.103 lock).
  }

  async function handleRevertToLocal() {
    if (savedConfig) {
      // De-activate without deleting, so the saved entry can be re-activated later.
      await setConfig({ ...savedConfig, active: false });
    }
    toast({
      variant: 'info',
      title: 'Using local daemon',
      description: 'Remote settings are saved but inactive.',
    });
    // Stay on /settings/connection — no navigate away (V1.103 lock).
  }

  function renderFingerprintBlock() {
    if (fpState.status !== 'success') return null;
    if (isLoopbackOnly) {
      return (
        <div
          className="rounded-card border border-gray-alpha-300 bg-gray-alpha-100 p-4 text-gray-800"
          data-testid="loopback-info-note"
        >
          <div className="flex items-start gap-3">
            <Info className="mt-0.5 h-5 w-5 flex-shrink-0 text-gray-600" aria-hidden />
            <p className="text-copy-14">
              This daemon has no TLS certificate. It is running in loopback-only mode,
              so remote access is not available. Use local mode instead.
            </p>
          </div>
        </div>
      );
    }
    return (
      <div className="space-y-4">
        <p className="text-copy-13 text-gray-700">{FINGERPRINT_TRUST_HELPER}</p>
        <div
          className="rounded-control border border-gray-alpha-400 bg-background-200 p-3 font-mono text-[13px] font-normal leading-relaxed text-gray-1000"
          data-testid="fingerprint-block"
        >
          {fpState.response.fingerprint}
        </div>
        <div className="rounded-card border border-blue-700/20 bg-blue-700/10 p-4 text-gray-900">
          <div className="flex items-start gap-3">
            <Shield className="mt-0.5 h-5 w-5 flex-shrink-0 text-blue-700" aria-hidden />
            <p className="text-copy-14">
              This fingerprint is how your app makes sure it is talking to the real
              daemon and not someone pretending to be it. Compare it to the value
              printed on the daemon machine&apos;s screen. If they match, it is safe
              to trust.
            </p>
          </div>
        </div>
        {reconnectWithMatch && (
          <div
            className="rounded-card border border-blue-700/20 bg-blue-700/10 p-4 text-gray-900"
            data-testid="fingerprint-match-hint"
          >
            <div className="flex items-start gap-3">
              <CheckCircle className="mt-0.5 h-5 w-5 flex-shrink-0 text-blue-700" aria-hidden />
              <p className="text-copy-14">Fingerprint matches the trusted daemon.</p>
            </div>
          </div>
        )}
      </div>
    );
  }

  function renderMismatchWarning() {
    if (!fingerprintMismatch) return null;
    return (
      <div
        className="rounded-card border border-amber-700/20 bg-amber-700/10 p-4 text-gray-900"
        data-testid="fingerprint-mismatch-warning"
      >
        <div className="flex items-start gap-3">
          <AlertCircle className="mt-0.5 h-5 w-5 flex-shrink-0 text-amber-700" aria-hidden />
          <div className="space-y-3">
            <p className="text-copy-14">
              The certificate for this daemon has changed. This can happen if the
              daemon was reinstalled or its certificate was deliberately rotated. It
              can also mean someone is intercepting your connection.
            </p>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="primary"
                size="small"
                onClick={() =>
                  void activateConfig(
                    fpState.status === 'success' ? fpState.response.fingerprint : undefined,
                  )
                }
              >
                Trust the New Certificate and Continue
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="small"
                onClick={() => resetFp()}
              >
                Cancel and Keep Using the Old Certificate
              </Button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  function renderPrimaryAction() {
    if (fpState.status !== 'success' || fingerprintMismatch) return null;
    if (isLoopbackOnly) {
      return (
        <Button
          type="button"
          variant="secondary"
          size="default"
          onClick={() => void handleRevertToLocal()}
        >
          Use Local Daemon
        </Button>
      );
    }
    return (
      <Button
        type="button"
        variant="primary"
        size="default"
        onClick={() => void activateConfig(fpState.response.fingerprint)}
        data-testid="trust-connect-button"
      >
        {reconnectWithMatch
          ? 'Reconnect With These Settings'
          : 'Trust This Certificate and Connect'}
      </Button>
    );
  }

  return (
    <Card className="shadow-card" data-testid="connect-daemon-form">
      <CardHeader>
        <div className="flex items-center gap-2">
          <Wifi className="h-5 w-5 text-blue-700" aria-hidden />
          <CardTitle>Connect to Daemon</CardTitle>
        </div>
        <CardDescription>{FORM_CARD_DESCRIPTION}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="space-y-2">
          <Label htmlFor="daemon-url">Daemon URL</Label>
          <Input
            id="daemon-url"
            type="url"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://192.168.1.42:8420"
            data-testid="daemon-url-input"
          />
          <p className="text-copy-13 text-gray-700">{URL_FIELD_HELPER}</p>
        </div>

        <div className="space-y-2">
          <Label htmlFor="api-key">API Key</Label>
          <Input
            id="api-key"
            type={showKey ? 'text' : 'password'}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="Enter the API key from the daemon machine"
            data-testid="api-key-input"
          />
          <p className="text-copy-13 text-gray-700">
            {API_KEY_HELPER_PREFIX}
            <code className="rounded-control bg-background-200 px-1 py-0.5 font-mono text-[13px]">
              {API_KEY_HELPER_COMMAND}
            </code>
            {API_KEY_HELPER_SUFFIX}
          </p>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => setShowKey((s) => !s)}
            >
              {showKey ? 'Hide key' : 'Show key'}
            </Button>
          </div>
        </div>

        <div className="space-y-2">
          <Label htmlFor="connection-label">Label (optional)</Label>
          <Input
            id="connection-label"
            type="text"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder={endpointLabel(normalizedUrl)}
            data-testid="connection-label-input"
          />
        </div>

        <div className="rounded-card border border-gray-alpha-300 bg-gray-alpha-100 p-4 text-gray-800">
          <div className="flex items-start gap-3">
            <Info className="mt-0.5 h-5 w-5 flex-shrink-0 text-gray-600" aria-hidden />
            <p className="text-copy-14">
              If you are connecting from a browser tab to a remote daemon, your
              serving origin must be listed in the daemon&apos;s{' '}
              <code className="rounded-control bg-background-200 px-1 py-0.5 font-mono text-[13px]">
                NEXUS_DAEMON_ALLOWED_ORIGINS
              </code>{' '}
              setting. Desktop apps are allowed automatically.
            </p>
          </div>
        </div>

        {fpState.status === 'error' && (
          <div
            className="rounded-card border border-red-700/20 bg-red-700/10 p-4 text-gray-900"
            role="alert"
            data-testid="fingerprint-error"
          >
            <div className="flex items-start gap-3">
              <AlertCircle className="mt-0.5 h-5 w-5 flex-shrink-0 text-red-700" aria-hidden />
              <div className="space-y-2">
                <p className="text-copy-14">{fpState.message}</p>
                <p className="text-copy-14">
                  Browsers cannot reliably distinguish an unreachable daemon from a
                  rejected self-signed certificate, so fetching the certificate fingerprint
                  may fail even when the daemon is running. For remote daemons that use a
                  self-signed certificate, use the Nexus desktop app — it supports Trust On
                  First Use (TOFU) and can store the certificate in the OS keychain.
                </p>
              </div>
            </div>
          </div>
        )}

        {renderFingerprintBlock()}
        {renderMismatchWarning()}

        <div className="flex flex-wrap items-center gap-3 pt-2">
          <Button
            type="button"
            variant="secondary"
            size="default"
            onClick={() => void handleFetchFingerprint()}
            disabled={fpState.status === 'loading' || !normalizedUrl}
            data-testid="fetch-fingerprint-button"
          >
            <Fingerprint className="h-4 w-4" aria-hidden />
            {fpState.status === 'loading' ? 'Fetching fingerprint…' : 'Fetch fingerprint'}
          </Button>
          {renderPrimaryAction()}
          {hasSavedConfig && (
            <Button
              type="button"
              variant="tertiary"
              size="default"
              onClick={() => void handleRevertToLocal()}
              data-testid="revert-local-button"
            >
              Use Local Daemon
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
