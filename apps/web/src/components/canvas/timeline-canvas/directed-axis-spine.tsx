import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';

export type BriefSpineConfig = {
  kind: 'brief';
  eraBounds: Array<{
    startHint: string;
    endHint: string | undefined;
    eraId: string | undefined;
    eraLabel: string;
  }>;
};

export type NarrativeSpineConfig = {
  kind: 'narrative';
  tickTimestamps: string[];
};

export type MomentSpineConfig = {
  kind: 'moment';
  chapterSegments: Array<{
    chapterId: number;
    chapterLabel: string;
    sceneCount: number;
    sceneTicks: string[];
  }>;
};

export type DirectedAxisSpineConfig =
  | BriefSpineConfig
  | NarrativeSpineConfig
  | MomentSpineConfig;

export interface DirectedAxisSpineNodeData {
  [key: string]: unknown;
  layer: 'brief' | 'narrative' | 'moment';
  spineConfig: DirectedAxisSpineConfig;
  accentColor: string;
}

const SPINE_HEIGHT = 32;
const ARROW_HEAD_SIZE = 10;
const TICK_LABEL_OFFSET = 14;

const SPINE_VIEWPORT_WIDTH = 4000;

function BriefSpine({
  config,
  accentColor,
}: {
  config: BriefSpineConfig;
  accentColor: string;
}) {
  const { eraBounds } = config;
  const segmentCount = eraBounds.length;
  const segmentWidth = segmentCount > 0
    ? Math.max(SPINE_VIEWPORT_WIDTH / segmentCount, 200)
    : SPINE_VIEWPORT_WIDTH;
  const totalWidth = segmentWidth * Math.max(segmentCount, 1);
  const spineY = SPINE_HEIGHT / 2;
  const arrowRightX = totalWidth + ARROW_HEAD_SIZE;

  return (
    <svg
      width={arrowRightX + 10}
      height={SPINE_HEIGHT + 24}
      className="pointer-events-none"
      aria-hidden
    >
      <defs>
        <linearGradient id="brief-arrow-grad" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0%" stopColor={accentColor} stopOpacity="0.4" />
          <stop offset="100%" stopColor={accentColor} stopOpacity="1" />
        </linearGradient>
      </defs>

      <line
        x1={0}
        y1={spineY}
        x2={totalWidth}
        y2={spineY}
        stroke="url(#brief-arrow-grad)"
        strokeWidth={4}
        strokeLinecap="round"
      />

      <polygon
        points={`${arrowRightX},${spineY} ${totalWidth},${spineY - ARROW_HEAD_SIZE} ${totalWidth},${spineY + ARROW_HEAD_SIZE}`}
        fill={accentColor}
      />

      {eraBounds.map((era, i) => {
        const x = i * segmentWidth;
        const nextX = (i + 1) * segmentWidth;
        const isLast = i === eraBounds.length - 1;
        const tickX = isLast ? nextX : x;
        return (
          <g key={era.eraId ?? i}>
            <line
              x1={tickX}
              y1={spineY - 6}
              x2={tickX}
              y2={spineY + 6}
              stroke={accentColor}
              strokeWidth={2}
              strokeLinecap="round"
            />
            <text
              x={tickX}
              y={spineY + TICK_LABEL_OFFSET + 10}
              textAnchor="middle"
              fill={accentColor}
              fontSize={10}
              fontFamily="var(--font-sans, ui-sans-serif, system-ui)"
            >
              {era.eraLabel}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

function NarrativeSpine({
  config,
  accentColor,
}: {
  config: NarrativeSpineConfig;
  accentColor: string;
}) {
  const { tickTimestamps } = config;
  const totalTicks = tickTimestamps.length;
  const tickSpacing = totalTicks > 1
    ? Math.min(SPINE_VIEWPORT_WIDTH / (totalTicks - 1), 280)
    : 200;
  const totalWidth = totalTicks > 1
    ? (totalTicks - 1) * tickSpacing + 40
    : 400;
  const spineY = SPINE_HEIGHT / 2;

  return (
    <svg
      width={Math.max(totalWidth + 40, 400)}
      height={SPINE_HEIGHT + 20}
      className="pointer-events-none"
      aria-hidden
    >
      <line
        x1={0}
        y1={spineY}
        x2={totalWidth}
        y2={spineY}
        stroke={accentColor}
        strokeWidth={1.5}
        strokeLinecap="round"
        strokeOpacity={0.6}
      />

      {tickTimestamps.map((ts, i) => {
        const x = totalTicks > 1
          ? 20 + i * tickSpacing
          : 20;
        const label = ts.length > 10 ? ts.slice(0, 10) : ts;
        return (
          <g key={i}>
            <line
              x1={x}
              y1={spineY - 4}
              x2={x}
              y2={spineY + 4}
              stroke={accentColor}
              strokeWidth={1.5}
              strokeLinecap="round"
            />
            {i % 3 === 0 ? (
              <text
                x={x}
                y={spineY + 16}
                textAnchor="middle"
                fill={accentColor}
                fontSize={9}
                fontFamily="var(--font-sans, ui-sans-serif, system-ui)"
                opacity={0.7}
              >
                {label}
              </text>
            ) : null}
          </g>
        );
      })}
    </svg>
  );
}

function MomentSpine({
  config,
  accentColor,
}: {
  config: MomentSpineConfig;
  accentColor: string;
}) {
  const { chapterSegments } = config;
  const MAX_SEGMENT_WIDTH = 280;
  const MIN_SEGMENT_WIDTH = 60;
  const spineY = SPINE_HEIGHT / 2;

  let totalWidth = 0;
  const segments = chapterSegments.map((ch) => {
    const width = Math.max(
      MIN_SEGMENT_WIDTH,
      Math.min(MAX_SEGMENT_WIDTH, ch.sceneCount * 80),
    );
    const seg = { ...ch, width };
    return seg;
  });

  totalWidth = segments.reduce((sum, s) => sum + s.width, 0);

  return (
    <svg
      width={Math.max(totalWidth + 40, 400)}
      height={SPINE_HEIGHT + 24}
      className="pointer-events-none"
      aria-hidden
    >
      {segments.map((seg, i) => {
        const x = segments.slice(0, i).reduce((sum, s) => sum + s.width, 0) + 20;
        const isLast = i === segments.length - 1;
        const segEndX = x + seg.width;

        if (seg.sceneCount === 0) {
          return (
            <g key={seg.chapterId}>
              <line
                x1={x}
                y1={spineY}
                x2={x + MIN_SEGMENT_WIDTH}
                y2={spineY}
                stroke={accentColor}
                strokeWidth={1.5}
                strokeOpacity={0.3}
                strokeLinecap="round"
                strokeDasharray="4 3"
              />
              {isLast ? (
                <polygon
                  points={`${x + MIN_SEGMENT_WIDTH + ARROW_HEAD_SIZE},${spineY} ${x + MIN_SEGMENT_WIDTH},${spineY - 6} ${x + MIN_SEGMENT_WIDTH},${spineY + 6}`}
                  fill={accentColor}
                  opacity={0.3}
                />
              ) : null}
              <text
                x={x + MIN_SEGMENT_WIDTH / 2}
                y={spineY + 16}
                textAnchor="middle"
                fill={accentColor}
                fontSize={9}
                fontFamily="var(--font-sans, ui-sans-serif, system-ui)"
                opacity={0.5}
              >
                {seg.chapterLabel}
              </text>
            </g>
          );
        }

        return (
          <g key={seg.chapterId}>
            <line
              x1={x}
              y1={spineY}
              x2={segEndX}
              y2={spineY}
              stroke={accentColor}
              strokeWidth={3}
              strokeLinecap="round"
            />

            {isLast ? (
              <polygon
                points={`${segEndX + ARROW_HEAD_SIZE},${spineY} ${segEndX},${spineY - 8} ${segEndX},${spineY + 8}`}
                fill={accentColor}
              />
            ) : null}

            {seg.sceneTicks.map((_tick, tickIdx) => {
              const tickX = x + ((tickIdx + 1) / (seg.sceneTicks.length + 1)) * seg.width;
              return (
                <line
                  key={tickIdx}
                  x1={tickX}
                  y1={spineY - 3}
                  x2={tickX}
                  y2={spineY + 3}
                  stroke={accentColor}
                  strokeWidth={1}
                  strokeLinecap="round"
                  opacity={0.7}
                />
              );
            })}

            <text
              x={x + seg.width / 2}
              y={spineY + 16}
              textAnchor="middle"
              fill={accentColor}
              fontSize={9}
              fontFamily="var(--font-sans, ui-sans-serif, system-ui)"
            >
              {seg.chapterLabel}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

export const DirectedAxisSpine = memo(function DirectedAxisSpine({
  data,
}: NodeProps) {
  const d = data as DirectedAxisSpineNodeData;
  const { spineConfig, accentColor } = d;

  const spine = (() => {
    switch (spineConfig.kind) {
      case 'brief':
        return <BriefSpine config={spineConfig} accentColor={accentColor} />;
      case 'narrative':
        return <NarrativeSpine config={spineConfig} accentColor={accentColor} />;
      case 'moment':
        return <MomentSpine config={spineConfig} accentColor={accentColor} />;
      default:
        return null;
    }
  })();

  return (
    <div
      className="flex items-center justify-start"
      style={{ minWidth: 400, minHeight: SPINE_HEIGHT + 24 }}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!h-0 !w-0 !border-0 !bg-transparent"
      />
      {spine}
      <Handle
        type="source"
        position={Position.Right}
        className="!h-0 !w-0 !border-0 !bg-transparent"
      />
    </div>
  );
});