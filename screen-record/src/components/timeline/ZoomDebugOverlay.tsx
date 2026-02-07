import React, { useMemo, useCallback, useState } from 'react';
import { VideoSegment } from '@/types/video';
import { videoRenderer } from '@/lib/videoRenderer';

interface ZoomDebugOverlayProps {
    segment: VideoSegment;
    duration: number;
}

export const ZoomDebugOverlay: React.FC<ZoomDebugOverlayProps> = ({ segment, duration }) => {
    const [copied, setCopied] = useState(false);

    const { zoomPath, speedPath, maxZoom } = useMemo(() => {
        if (!segment || duration <= 0) return { zoomPath: '', speedPath: '', maxZoom: 1 };

        const samples = videoRenderer.sampleZoomCurve(segment, 1920, 1080, 200);
        if (samples.length < 2) return { zoomPath: '', speedPath: '', maxZoom: 1 };

        let mxZ = 1;
        for (const s of samples) mxZ = Math.max(mxZ, s.zoom);

        const speeds: number[] = [];
        for (let i = 0; i < samples.length - 1; i++) {
            const dt = (samples[i + 1].time - samples[i].time) || 0.001;
            const dz = Math.abs(samples[i + 1].zoom - samples[i].zoom);
            const dx = Math.abs(samples[i + 1].posX - samples[i].posX);
            const dy = Math.abs(samples[i + 1].posY - samples[i].posY);
            speeds.push((dz + Math.sqrt(dx * dx + dy * dy) * 2) / dt);
        }
        speeds.push(speeds[speeds.length - 1] || 0);

        let mxS = 0;
        for (const s of speeds) mxS = Math.max(mxS, s);
        if (mxS < 0.001) mxS = 1;

        const W = 200, H = 36, PAD = 2;
        let zp = '', sp = '';
        for (let i = 0; i < samples.length; i++) {
            const x = (i / (samples.length - 1)) * W;
            const zy = PAD + (1 - (samples[i].zoom - 1) / (mxZ - 1 || 1)) * H;
            const sy = PAD + (1 - speeds[i] / mxS) * H;
            zp += `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${zy.toFixed(1)} `;
            sp += `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${sy.toFixed(1)} `;
        }

        return { zoomPath: zp, speedPath: sp, maxZoom: mxZ };
    }, [segment, duration, segment?.zoomKeyframes, segment?.smoothMotionPath, segment?.zoomInfluencePoints, segment?.trimStart, segment?.trimEnd]);

    const handleCopy = useCallback(() => {
        const samples = videoRenderer.sampleZoomCurve(segment, 1920, 1080, 200);
        const data = {
            keyframes: segment.zoomKeyframes.map(k => ({
                t: +k.time.toFixed(3), d: +k.duration.toFixed(3),
                z: +k.zoomFactor.toFixed(3), x: +k.positionX.toFixed(3), y: +k.positionY.toFixed(3)
            })),
            hasAutoPath: !!(segment.smoothMotionPath?.length),
            trimStart: +segment.trimStart.toFixed(3),
            trimEnd: +segment.trimEnd.toFixed(3),
            samples: samples.map(s => ({
                t: +s.time.toFixed(3), z: +s.zoom.toFixed(3),
                x: +s.posX.toFixed(3), y: +s.posY.toFixed(3)
            }))
        };
        navigator.clipboard.writeText(JSON.stringify(data));
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
    }, [segment]);

    if (!zoomPath) return null;

    return (
        <div className="zoom-debug-overlay relative h-10 rounded bg-[var(--surface-container)]/60 overflow-hidden">
            <svg
                className="w-full h-full"
                preserveAspectRatio="none"
                viewBox="0 0 200 40"
            >
                <line x1="0" y1="2" x2="200" y2="2" stroke="rgba(255,255,255,0.04)" vectorEffect="non-scaling-stroke" />
                <line x1="0" y1="20" x2="200" y2="20" stroke="rgba(255,255,255,0.04)" vectorEffect="non-scaling-stroke" />
                <line x1="0" y1="38" x2="200" y2="38" stroke="rgba(255,255,255,0.04)" vectorEffect="non-scaling-stroke" />

                <path d={speedPath} fill="none" stroke="#f59e0b" strokeWidth="1" vectorEffect="non-scaling-stroke" opacity="0.6" />
                <path d={zoomPath} fill="none" stroke="#3b82f6" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
            </svg>

            <div className="absolute top-0.5 left-1 flex gap-2 items-center">
                <span className="text-[8px] font-mono text-blue-400 pointer-events-none">zoom {maxZoom.toFixed(1)}x</span>
                <span className="text-[8px] font-mono text-amber-400 pointer-events-none">speed</span>
                <button
                    onClick={handleCopy}
                    className="text-[7px] font-mono px-1 rounded bg-white/10 hover:bg-white/20 text-white/60 hover:text-white transition-colors"
                >
                    {copied ? 'copied!' : 'copy'}
                </button>
            </div>
        </div>
    );
};
