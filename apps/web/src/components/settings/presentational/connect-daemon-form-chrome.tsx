import { useState } from 'react';

import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  cn,
  Input,
  Label,
} from '@42ch/nexus-ui';
import { Fingerprint, Wifi } from 'lucide-react';

export type ConnectDaemonFormState =
  | 'firstUse'
  | 'reconnectMatch'
  | 'fingerprintMismatch'
  | 'loopbackOnly';

export interface ConnectDaemonFormChromeProps {
  state: ConnectDaemonFormState;
  daemonUrl?: string;
  apiKey?: string;
  fingerprint?: string;
  desktopAvailable?: boolean;
  onFetchFingerprint?: () => void;
  onConnect?: () => void;
  onUseLocal?: () => void;
  'data-testid'?: string;
}

const CONNECTION_FORM_DESCRIPTION =
  'Enter the remote daemon URL and API key. Local mode remains available — you can revert here at any time.';

const CONNECTION_URL_HELPER =
  'The full HTTPS address of the daemon, including port.';

const CONNECTION_API_KEY_HELPER_PREFIX =
  'The API key from the daemon machine (';
const CONNECTION_API_KEY_HELPER_COMMAND = 'nexus42 daemon api-key';
const CONNECTION_API_KEY_HELPER_SUFFIX = ' on that host).';

const CONNECTION_FINGERPRINT_HELPER =
  'Confirm the certificate fingerprint matches what you see on the daemon machine before connecting.';

const FINGERPRINT_MISMATCH_HELPER =
  'The fingerprint does not match a previously trusted certificate. Verify the daemon identity before continuing.';

const LOOPBACK_ONLY_HELPER =
  'Remote connections are disabled for this build. Only the local daemon is available.';

const DEFAULT_DAEMON_URL = 'https://192.168.1.42:8420';
const DEFAULT_API_KEY = '••••••••••••••••';
const DEFAULT_FINGERPRINT = 'SHA256:aa:bb:cc:dd:ee:ff';

/**
 * Presentational Connect-to-Daemon form chrome — four-state matrix for Studio.
 *
 * No daemon client, no IPC, no certificate validation. The host owns the actions.
 *
 * # Prop contract reconciliation (R-V1107QC1-W002)
 *
 * This chrome takes a `state` prop of type `ConnectDaemonFormState`
 * (`firstUse` | `reconnectMatch` | `fingerprintMismatch` | `loopbackOnly`).
 * The V1.107 delivery compass / studio-ui-tune spec named this concept
 * `matrixState`; the implementation uses `state`. The two refer to the same
 * four-state matrix — the naming drift is intentional and stays until the live
 * `ConnectDaemonForm` (`../connect-daemon-form.tsx`) delegates rendering to
 * this chrome. At that adoption point the prop is renamed in one move and the
 * spec is updated to match; until then renaming the presentational-only prop
 * would churn Studio fixtures for no behavioral gain.
 *
 * Supporting props the live form derives from daemon state — `savedConfig`,
 * `fingerprintValue`, `fetchStatus`, `fetchErrorMessage`, `hasSavedConfig` —
 * are intentionally NOT surfaced on this chrome. The chrome is a static fixture
 * surface (Studio gallery) driven entirely by `state` + display defaults; the
 * live form keeps those derived values internal. They will be threaded through
 * when the live form adopts the chrome as its presentational layer.
 */
export function ConnectDaemonFormChrome({
  state,
  daemonUrl = DEFAULT_DAEMON_URL,
  apiKey = DEFAULT_API_KEY,
  fingerprint = DEFAULT_FINGERPRINT,
  desktopAvailable = true,
  onFetchFingerprint,
  onConnect,
  onUseLocal,
  'data-testid': dataTestId,
}: ConnectDaemonFormChromeProps) {
  const [showKey, setShowKey] = useState(false);
  const isLoopback = state === 'loopbackOnly';
  const isMismatch = state === 'fingerprintMismatch';
  const isFirstUse = state === 'firstUse';

  const disabled = isLoopback || !desktopAvailable;
  const showFingerprint = !isFirstUse || fingerprint !== DEFAULT_FINGERPRINT;

  return (
    <Card
      className={cn('shadow-card', isMismatch && 'border-warning-700')}
      data-testid={dataTestId}
    >
      <CardHeader>
        <div className="flex items-center gap-2">
          <Wifi
            className={cn('h-5 w-5', isMismatch ? 'text-warning-700' : 'text-blue-1000 dark:text-blue-700')}
            aria-hidden="true"
          />
          <CardTitle>Connect to Daemon</CardTitle>
        </div>
        <CardDescription>{CONNECTION_FORM_DESCRIPTION}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="space-y-2">
          <Label htmlFor="studio-daemon-url">Daemon URL</Label>
          <Input
            id="studio-daemon-url"
            type="url"
            defaultValue={isFirstUse ? '' : daemonUrl}
            placeholder="https://192.168.1.42:8420"
            data-testid="daemon-url-input"
            readOnly
            disabled={disabled}
          />
          <p className="text-copy-13 text-gray-700">{CONNECTION_URL_HELPER}</p>
        </div>

        <div className="space-y-2">
          <Label htmlFor="studio-api-key">API Key</Label>
          <Input
            id="studio-api-key"
            type={showKey ? 'text' : 'password'}
            defaultValue={isFirstUse ? '' : apiKey}
            placeholder="Enter the API key from the daemon machine"
            data-testid="api-key-input"
            readOnly
            disabled={disabled}
          />
          <p className="text-copy-13 text-gray-700">
            {CONNECTION_API_KEY_HELPER_PREFIX}
            <code className="rounded-control bg-background-200 px-1 py-0.5 font-mono text-copy-13">
              {CONNECTION_API_KEY_HELPER_COMMAND}
            </code>
            {CONNECTION_API_KEY_HELPER_SUFFIX}
          </p>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="tertiary"
              size="small"
              disabled={disabled}
              onClick={() => {
                setShowKey((s) => !s);
                onFetchFingerprint?.();
              }}
            >
              {showKey ? 'Hide key' : 'Show key'}
            </Button>
          </div>
        </div>

        {showFingerprint ? (
          <div className="space-y-2">
            <p
              className={cn(
                'text-copy-13',
                isMismatch ? 'text-warning-700' : 'text-gray-700',
              )}
              data-testid={
                isMismatch
                  ? 'fingerprint-mismatch-warning'
                  : state === 'reconnectMatch'
                    ? 'fingerprint-match-hint'
                    : undefined
              }
            >
              {isMismatch ? FINGERPRINT_MISMATCH_HELPER : CONNECTION_FINGERPRINT_HELPER}
            </p>
            <div
              className={cn(
                'rounded-control border bg-background-200 p-3 font-mono text-copy-13 font-normal leading-relaxed text-gray-1000',
                isMismatch ? 'border-warning-700' : 'border-gray-alpha-400',
              )}
              data-testid="fingerprint-block"
            >
              {fingerprint}
            </div>
          </div>
        ) : null}

        {isLoopback ? (
          <p className="text-copy-13 text-gray-700" data-testid="loopback-info-note">
            {LOOPBACK_ONLY_HELPER}
          </p>
        ) : null}

        <div className="flex flex-wrap items-center gap-3 pt-2">
          <Button
            type="button"
            variant="secondary"
            size="default"
            disabled={disabled}
            data-testid="fetch-fingerprint-button"
            onClick={onFetchFingerprint}
          >
            <Fingerprint className="h-4 w-4" aria-hidden="true" />
            Fetch
          </Button>
          <Button
            type="button"
            variant={isMismatch ? 'destructive' : 'primary'}
            size="default"
            disabled={disabled}
            data-testid="trust-connect-button"
            onClick={onConnect}
          >
            {state === 'reconnectMatch'
              ? 'Reconnect With These Settings'
              : 'Trust This Certificate and Connect'}
          </Button>
          <Button
            type="button"
            variant="tertiary"
            size="default"
            data-testid="revert-local-button"
            onClick={onUseLocal}
          >
            Use Local Daemon
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
