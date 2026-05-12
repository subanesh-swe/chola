import { useRef, useEffect } from 'react';
import { Brush } from 'recharts';

interface DataPoint {
  date: string;
}

interface BrushChangePayload {
  startIndex?: number;
  endIndex?: number;
}

interface TimeRangeBrushProps {
  data: DataPoint[];
  onCommit: (from: string, to: string) => void;
  containerRef: React.RefObject<HTMLDivElement | null>;
}

/**
 * Renders a Recharts <Brush> and commits the selected date range to the
 * caller on mouseup (not on every drag tick).
 *
 * Usage:
 *   const containerRef = useRef<HTMLDivElement>(null);
 *   <div ref={containerRef}>
 *     <ResponsiveContainer ...>
 *       <SomeChart ...>
 *         <TimeRangeBrush data={data} onCommit={handleCommit} containerRef={containerRef} />
 *       </SomeChart>
 *     </ResponsiveContainer>
 *   </div>
 */
export function TimeRangeBrush({ data, onCommit, containerRef }: TimeRangeBrushProps) {
  const pendingRef = useRef<{ startIndex: number; endIndex: number } | null>(null);

  const handleBrushChange = (payload: BrushChangePayload) => {
    if (payload.startIndex == null || payload.endIndex == null) return;
    pendingRef.current = {
      startIndex: payload.startIndex,
      endIndex: payload.endIndex,
    };
  };

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const handleMouseUp = () => {
      const p = pendingRef.current;
      if (!p) return;
      const from = data[p.startIndex]?.date;
      const to = data[p.endIndex]?.date;
      if (from && to) {
        onCommit(from, to);
      }
      pendingRef.current = null;
    };

    el.addEventListener('mouseup', handleMouseUp);
    return () => {
      el.removeEventListener('mouseup', handleMouseUp);
    };
  }, [containerRef, data, onCommit]);

  return (
    <Brush
      dataKey="date"
      height={28}
      stroke="var(--color-accent)"
      fill="var(--color-surface-2)"
      travellerWidth={8}
      onChange={handleBrushChange}
    />
  );
}
