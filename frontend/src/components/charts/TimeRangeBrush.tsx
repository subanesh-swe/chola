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
}

/**
 * Renders a Recharts <Brush> and commits the selected date range to the
 * caller on mouseup (not on every drag tick).
 *
 * Listens on `window` so traveller drags that overshoot the container
 * boundary still fire the commit.
 *
 * Usage:
 *   <TimeRangeBrush data={data} onCommit={handleCommit} />
 */
export function TimeRangeBrush({ data, onCommit }: TimeRangeBrushProps) {
  const pendingRef = useRef<{ startIndex: number; endIndex: number } | null>(null);

  const handleBrushChange = (payload: BrushChangePayload) => {
    if (payload.startIndex == null || payload.endIndex == null) return;
    pendingRef.current = {
      startIndex: payload.startIndex,
      endIndex: payload.endIndex,
    };
  };

  useEffect(() => {
    const handleMouseUp = () => {
      const p = pendingRef.current;
      if (!p) return;
      pendingRef.current = null;
      const from = data[p.startIndex]?.date;
      const to = data[p.endIndex]?.date;
      if (from && to) {
        onCommit(from, to);
      }
    };

    window.addEventListener('mouseup', handleMouseUp, { passive: true });
    return () => {
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [data, onCommit]);

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
