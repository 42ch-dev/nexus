/**
 * Connect-to-Daemon form — extracted from legacy ConnectDaemonPage (V1.103 P2).
 *
 * Hosted under Settings → Advanced (Connection section; route consolidated to
 * `/settings/advanced#connection` in V1.106 — legacy `/settings/connection`
 * now redirects here). Implements the four author-visible states locked in
 * daemon-runtime.md §16.2. Post activate/revert stays on
 * `/settings/advanced#connection` (toast only — no navigate away).
 *
 * Author-facing copy: settings-connection-section.md.
 */

import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
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
import { TransportErrorBlock } from '@42ch/nexus-ui';
import { useToast } from '@/lib/use-toast';
import { errorMessage } from '@/lib/error-message';
import {
  normalizeEndpointUrl,
  endpointLabel,
  type ConnectionConfig,
} from '@/lib/nexus/connection-storage';
import { useConnectionConfig, useSetConnectionConfig } from '@/lib/client-context';
import { useFingerprint } from '@/lib/nexus/use-fingerprint';

export function ConnectDaemonForm() {
  const { t } = useTranslation('settings');
  const { t: commonT } = useTranslation('common');
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
      toast({ variant: 'error', title: commonT('toast.enterDaemonUrl') });
      return;
    }
    resetFp();
    await fetchFingerprint(normalizedUrl);
  }

  async function activateConfig(nextFingerprint?: string) {
    if (!normalizedUrl || !apiKey) {
      toast({ variant: 'error', title: commonT('toast.enterDaemonUrlAndApiKey') });
      return;
    }
    const next: ConnectionConfig = {
      endpointUrl: normalizedUrl,
      apiKey,
      label: label.trim() || endpointLabel(normalizedUrl),
      active: true,
      pinnedFingerprint: nextFingerprint,
    };
    try {
      await setConfig(next);
      toast({
        variant: 'success',
        title: commonT('toast.connectedToDaemon'),
        description: t('connection.connectedDescription', { url: next.endpointUrl }),
      });
      // Stay on /settings/advanced#connection — no navigate away (V1.103 lock).
    } catch (err) {
      const description = errorMessage(err) || commonT('error.couldNotSaveConnection');
      toast({
        variant: 'error',
        title: commonT('toast.couldNotConnectToDaemon'),
        description,
      });
    }
  }

  async function handleRevertToLocal() {
    try {
      if (savedConfig) {
        // De-activate without deleting, so the saved entry can be re-activated later.
        await setConfig({ ...savedConfig, active: false });
      }
      toast({
        variant: 'info',
        title: commonT('toast.usingLocalDaemon'),
        description: commonT('toast.usingLocalDaemonDescription'),
      });
      // Stay on /settings/advanced#connection — no navigate away (V1.103 lock).
    } catch (err) {
      const description = errorMessage(err) || commonT('error.couldNotSwitchToLocalDaemon');
      toast({
        variant: 'error',
        title: commonT('toast.couldNotSwitchToLocalDaemon'),
        description,
      });
    }
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
            <p className="text-copy-14">{t('connection.loopback')}</p>
          </div>
        </div>
      );
    }
    return (
      <div className="space-y-4">
        <p className="text-copy-13 text-gray-700">{t('connection.fingerprintTrust')}</p>
        <div
          className="rounded-control border border-gray-alpha-400 bg-background-200 p-3 font-mono text-copy-13 font-normal leading-relaxed text-gray-1000"
          data-testid="fingerprint-block"
        >
          {fpState.response.fingerprint}
        </div>
        <div className="rounded-card border border-blue-700/20 bg-blue-700/10 p-4 text-gray-900">
          <div className="flex items-start gap-3">
            <Shield className="mt-0.5 h-5 w-5 flex-shrink-0 text-blue-700" aria-hidden />
            <p className="text-copy-14">{t('connection.fingerprintTrustDescription')}</p>
          </div>
        </div>
        {reconnectWithMatch && (
          <div
            className="rounded-card border border-blue-700/20 bg-blue-700/10 p-4 text-gray-900"
            data-testid="fingerprint-match-hint"
          >
            <div className="flex items-start gap-3">
              <CheckCircle className="mt-0.5 h-5 w-5 flex-shrink-0 text-blue-700" aria-hidden />
              <p className="text-copy-14">{t('connection.fingerprintMatch')}</p>
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
            <p className="text-copy-14">{t('connection.fingerprintMismatch')}</p>
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
                {t('connection.trustNewCertificate')}
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="small"
                onClick={() => resetFp()}
              >
                {t('connection.keepOldCertificate')}
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
          {t('connection.useLocalDaemon')}
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
          ? t('connection.reconnectWithSettings')
          : t('connection.trustAndConnect')}
      </Button>
    );
  }

  return (
    <Card className="shadow-card" data-testid="connect-daemon-form">
      <CardHeader>
        <div className="flex items-center gap-2">
          <Wifi className="h-5 w-5 text-blue-700" aria-hidden />
          <CardTitle>{t('connection.title')}</CardTitle>
        </div>
        <CardDescription>{t('connection.description')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="space-y-2">
          <Label htmlFor="daemon-url">{t('connection.urlLabel')}</Label>
          <Input
            id="daemon-url"
            type="url"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder={t('connection.urlPlaceholder')}
            data-testid="daemon-url-input"
          />
          <p className="text-copy-13 text-gray-700">{t('connection.urlHelper')}</p>
        </div>

        <div className="space-y-2">
          <Label htmlFor="api-key">{t('connection.apiKeyLabel')}</Label>
          <Input
            id="api-key"
            type={showKey ? 'text' : 'password'}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={t('connection.apiKeyPlaceholder')}
            data-testid="api-key-input"
          />
          <p className="text-copy-13 text-gray-700">
            {t('connection.apiKeyHelperPrefix')}
            <code className="rounded-control bg-background-200 px-1 py-0.5 font-mono text-copy-13">
              {t('connection.apiKeyCommand')}
            </code>
            {t('connection.apiKeyHelperSuffix')}
          </p>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => setShowKey((s) => !s)}
              aria-label={showKey ? t('connection.hideKeyAria') : t('connection.showKeyAria')}
              aria-pressed={showKey}
            >
              {showKey ? t('connection.hideKey') : t('connection.showKey')}
            </Button>
          </div>
        </div>

        <div className="space-y-2">
          <Label htmlFor="connection-label">{t('connection.labelLabel')}</Label>
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
              {t('connection.originNotePrefix')}
              <code className="rounded-control bg-background-200 px-1 py-0.5 font-mono text-copy-13">
                {t('connection.originSetting')}
              </code>
              {t('connection.originNoteSuffix')}
            </p>
          </div>
        </div>

        {fpState.status === 'error' && fpState.kind ? (
          // V1.129 P1: transport-classified failures (kind present) render
          // the promoted <TransportErrorBlock>. CTAs are intentionally
          // omitted — the author is already on the Connection settings page
          // (so "Open Connection Settings" is redundant), and the existing
          // Fetch Fingerprint button below serves as the retry affordance.
          // The classifier's long-form diagnostic rides the detail line so
          // the actionable text (e.g. "use the Nexus desktop app") survives.
          <TransportErrorBlock
            kind={fpState.kind}
            detail={fpState.message}
          />
        ) : fpState.status === 'error' ? (
          // HTTP errors (no kind) keep the legacy inline region — their
          // recovery is daemon-side (fix the 4xx/5xx), not transport.
          <div
            className="rounded-card border border-error-surface-border bg-error-surface p-4"
            role="alert"
            data-testid="fingerprint-error"
          >
            <div className="flex items-start gap-3">
              <AlertCircle className="mt-0.5 h-5 w-5 flex-shrink-0 text-red-700" aria-hidden />
              <div className="space-y-2">
                <p className="text-heading-16 font-heading text-red-1000">{fpState.message}</p>
                <p className="text-copy-14 text-red-900">{t('connection.fingerprintError')}</p>
              </div>
            </div>
          </div>
        ) : null}

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
            {fpState.status === 'loading' ? t('connection.fetchingFingerprint') : t('connection.fetchFingerprint')}
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
              {t('connection.useLocalDaemon')}
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
