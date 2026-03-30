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

    source.addEventListener('log', (event) => {
      const data = JSON.parse(event.data);
      setChunks((prev) => [...prev, atob(data.data)]);
    });

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
