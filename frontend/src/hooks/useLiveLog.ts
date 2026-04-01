import { useEffect, useRef, useState } from 'react';
import { useAuthStore } from '../stores/auth';

export function useLiveLog(jobId: string | null, enabled: boolean) {
  const [chunks, setChunks] = useState<string[]>([]);
  const [isComplete, setIsComplete] = useState(false);
  const sourceRef = useRef<EventSource | null>(null);

  useEffect(() => {
    if (!jobId || !enabled) return;

    const token = useAuthStore.getState().token;
    const url = `/api/v1/jobs/${jobId}/logs/stream?token=${encodeURIComponent(token || '')}`;
    const source = new EventSource(url);
    sourceRef.current = source;

    // Default message events (no event: prefix from server)
    source.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.data) {
          setChunks((prev) => [...prev, data.data]);
        }
      } catch {
        // Raw text fallback
        setChunks((prev) => [...prev, event.data]);
      }
    };

    // Named 'complete' event from server
    source.addEventListener('complete', () => {
      setIsComplete(true);
      source.close();
    });

    source.onerror = () => {
      source.close();
    };

    return () => {
      source.close();
    };
  }, [jobId, enabled]);

  return { chunks, isComplete, text: chunks.join('') };
}
