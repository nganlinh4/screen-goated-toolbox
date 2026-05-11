import React, { useEffect, useMemo, useRef, useState } from "react";
import type { AudioGainPoint } from "@/types/video";
import { getAudioWaveform, type AudioWaveformResponse } from "@/lib/audioWaveform";

interface AudioWaveformLayerProps {
  sourcePath?: string | null;
  duration: number;
  gainPoints?: AudioGainPoint[];
  getVolumeAtTime: (
    time: number,
    points: AudioGainPoint[] | undefined | null,
  ) => number;
  colorVariable: string;
  topPx: number;
  bottomPx: number;
  offsetSec?: number;
  sourceInSec?: number;
  sourceOutSec?: number;
  playbackRate?: number;
  gainTimeOffsetSec?: number;
}

const TARGET_BIN_MIN = 128;
const TARGET_BIN_MAX = 2048;
const TARGET_PIXELS_PER_BIN = 2;
const REQUEST_DEBOUNCE_MS = 120;
const MAX_SMOOTH_RADIUS = 2;

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function getTargetBinCount(widthPx: number) {
  return clamp(
    Math.round(widthPx / TARGET_PIXELS_PER_BIN),
    TARGET_BIN_MIN,
    TARGET_BIN_MAX,
  );
}

function getSmoothingRadius(widthPx: number, binCount: number) {
  if (binCount <= 0) return 0;
  const pixelsPerBin = widthPx / binCount;
  if (pixelsPerBin >= 6) return 2;
  if (pixelsPerBin >= 2.5) return 1;
  return 0;
}

function smoothEnvelopeValues(
  values: number[],
  radius: number,
) {
  if (radius <= 0 || values.length <= 2) {
    return values;
  }

  const safeRadius = clamp(radius, 0, MAX_SMOOTH_RADIUS);
  const smoothed = new Array<number>(values.length);
  for (let index = 0; index < values.length; index += 1) {
    let total = 0;
    let weightTotal = 0;
    for (let offset = -safeRadius; offset <= safeRadius; offset += 1) {
      const neighborIndex = index + offset;
      if (neighborIndex < 0 || neighborIndex >= values.length) continue;
      const weight = safeRadius + 1 - Math.abs(offset);
      total += values[neighborIndex] * weight;
      weightTotal += weight;
    }
    smoothed[index] = weightTotal > 0 ? total / weightTotal : values[index];
  }
  return smoothed;
}

export const AudioWaveformLayer: React.FC<AudioWaveformLayerProps> = ({
  sourcePath,
  duration,
  gainPoints,
  getVolumeAtTime,
  colorVariable,
  topPx,
  bottomPx,
  offsetSec = 0,
  sourceInSec,
  sourceOutSec,
  playbackRate = 1,
  gainTimeOffsetSec = 0,
}) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const requestIdRef = useRef(0);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [isVisible, setIsVisible] = useState(true);
  const [waveform, setWaveform] = useState<AudioWaveformResponse | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      setSize({
        width: Math.max(0, Math.round(entry.contentRect.width)),
        height: Math.max(0, Math.round(entry.contentRect.height)),
      });
    });

    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || typeof IntersectionObserver === "undefined") {
      setIsVisible(true);
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        setIsVisible(Boolean(entry?.isIntersecting));
      },
      { root: null, rootMargin: "320px 960px" },
    );

    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  const targetBins = useMemo(() => {
    if (size.width <= 0) return 0;
    return getTargetBinCount(size.width);
  }, [size.width]);

  useEffect(() => {
    const trimmedPath = sourcePath?.trim() ?? "";
    if (!trimmedPath || targetBins <= 0) {
      setWaveform(null);
      return;
    }
    if (!isVisible) {
      return;
    }

    const requestId = ++requestIdRef.current;
    const timer = window.setTimeout(() => {
      void getAudioWaveform(trimmedPath, targetBins)
        .then((nextWaveform) => {
          if (requestIdRef.current !== requestId) return;
          setWaveform(nextWaveform);
        })
        .catch((error) => {
          if (requestIdRef.current !== requestId) return;
          console.warn("[AudioWaveform] Failed to load waveform", error);
          setWaveform(null);
        });
    }, REQUEST_DEBOUNCE_MS);

    return () => {
      window.clearTimeout(timer);
    };
  }, [isVisible, sourcePath, targetBins]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const cssWidth = Math.max(0, size.width);
    const cssHeight = Math.max(0, size.height);
    const dpr = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(cssWidth * dpr));
    const height = Math.max(1, Math.round(cssHeight * dpr));

    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, width, height);
    if (
      !waveform ||
      waveform.bins.length === 0 ||
      waveform.sourceDurationSec <= 0 ||
      duration <= 0 ||
      !isVisible ||
      cssWidth <= 0 ||
      cssHeight <= 0
    ) {
      return;
    }

    ctx.scale(dpr, dpr);
    const computedStyle = getComputedStyle(canvas);
    const color = computedStyle.getPropertyValue(colorVariable).trim() || "#5ca8ff";
    const centerY = (topPx + bottomPx) * 0.5;
    const halfRange = Math.max(1, (bottomPx - topPx) * 0.5 - 1);
    const sourceDuration = waveform.sourceDurationSec;
    const sourceStartSec = Math.max(0, sourceInSec ?? 0);
    const sourceEndSec = Math.max(
      sourceStartSec,
      Math.min(sourceDuration, sourceOutSec ?? sourceDuration),
    );
    const rate = playbackRate > 0 ? playbackRate : 1;
    const displaySourceDuration = Math.max(0.0001, (sourceEndSec - sourceStartSec) / rate);
    const smoothingRadius = getSmoothingRadius(cssWidth, waveform.bins.length);
    const drawPoints: Array<{ x: number; topY: number; bottomY: number }> = [];

    for (let index = 0; index < waveform.bins.length; index += 1) {
      const bin = waveform.bins[index];
      const rawStart = (index / waveform.bins.length) * sourceDuration;
      const rawEnd = ((index + 1) / waveform.bins.length) * sourceDuration;
      if (rawEnd <= sourceStartSec || rawStart >= sourceEndSec) continue;
      const clippedStart = Math.max(sourceStartSec, rawStart);
      const clippedEnd = Math.min(sourceEndSec, rawEnd);
      const sourceStart = (clippedStart - sourceStartSec) / rate + offsetSec;
      const sourceEnd = (clippedEnd - sourceStartSec) / rate + offsetSec;
      if (sourceEnd <= 0 || sourceStart >= displaySourceDuration) continue;

      const displayCenter = clamp((sourceStart + sourceEnd) * 0.5, 0, duration);
      const gain = clamp(getVolumeAtTime(displayCenter + gainTimeOffsetSec, gainPoints), 0, 1);
      const minAmplitude = clamp(bin.min * gain, -1, 1);
      const maxAmplitude = clamp(bin.max * gain, -1, 1);
      const x = (sourceStart / duration) * cssWidth;
      const nextX = (sourceEnd / duration) * cssWidth;
      const centerX = clamp((x + nextX) * 0.5, 0, cssWidth);
      const topY = clamp(centerY - maxAmplitude * halfRange, topPx, bottomPx);
      const bottomY = clamp(centerY - minAmplitude * halfRange, topPx, bottomPx);
      drawPoints.push({ x: centerX, topY, bottomY });
    }

    if (drawPoints.length === 0) {
      return;
    }

    const smoothedTop = smoothEnvelopeValues(
      drawPoints.map((point) => point.topY),
      smoothingRadius,
    );
    const smoothedBottom = smoothEnvelopeValues(
      drawPoints.map((point) => point.bottomY),
      smoothingRadius,
    );

    ctx.fillStyle = color;
    ctx.globalAlpha = 0.18;
    ctx.beginPath();
    ctx.moveTo(drawPoints[0].x, smoothedTop[0]);
    for (let index = 1; index < drawPoints.length; index += 1) {
      ctx.lineTo(drawPoints[index].x, smoothedTop[index]);
    }
    for (let index = drawPoints.length - 1; index >= 0; index -= 1) {
      ctx.lineTo(drawPoints[index].x, smoothedBottom[index]);
    }
    ctx.closePath();
    ctx.fill();

    ctx.globalAlpha = 0.32;
    ctx.strokeStyle = color;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(drawPoints[0].x, smoothedTop[0]);
    for (let index = 1; index < drawPoints.length; index += 1) {
      ctx.lineTo(drawPoints[index].x, smoothedTop[index]);
    }
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(drawPoints[0].x, smoothedBottom[0]);
    for (let index = 1; index < drawPoints.length; index += 1) {
      ctx.lineTo(drawPoints[index].x, smoothedBottom[index]);
    }
    ctx.stroke();
    ctx.globalAlpha = 1;
  }, [
    bottomPx,
    colorVariable,
    duration,
    gainTimeOffsetSec,
    gainPoints,
    getVolumeAtTime,
    isVisible,
    offsetSec,
    size.height,
    size.width,
    playbackRate,
    sourceInSec,
    sourceOutSec,
    topPx,
    waveform,
  ]);

  return (
    <canvas
      ref={canvasRef}
      className="audio-waveform-layer absolute inset-0 h-full w-full pointer-events-none"
      aria-hidden="true"
    />
  );
};
