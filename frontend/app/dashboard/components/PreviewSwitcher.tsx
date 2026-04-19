'use client';
import { useState, useEffect, lazy, Suspense } from 'react';

const ModelPreview = lazy(() => import('./ModelPreview'));
const AliceViewPreview = lazy(() => import('./AliceViewPreview'));

const STORAGE_KEY = '3dvbgaran-preview-mode';

type PreviewMode = 'standard' | 'alice-view';

interface PreviewSwitcherProps {
  blob: Blob | null;
  lolSource: string | null;
}

export default function PreviewSwitcher({ blob, lolSource }: PreviewSwitcherProps) {
  const [mode, setMode] = useState<PreviewMode>('standard');

  useEffect(() => {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === 'alice-view') setMode('alice-view');
  }, []);

  const toggle = (m: PreviewMode) => {
    setMode(m);
    localStorage.setItem(STORAGE_KEY, m);
  };

  const fallback = (
    <div className="w-full h-64 border border-dashed border-border rounded-lg flex items-center justify-center text-sm text-muted-foreground">
      Loading viewer...
    </div>
  );

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <span className="text-xs text-muted-foreground">Viewer:</span>
        <button
          onClick={() => toggle('standard')}
          className={`px-2 py-0.5 text-xs rounded ${mode === 'standard' ? 'bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground'}`}
        >
          Standard
        </button>
        <button
          onClick={() => toggle('alice-view')}
          className={`px-2 py-0.5 text-xs rounded ${mode === 'alice-view' ? 'bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground'}`}
        >
          ALICE-View
        </button>
        {mode === 'alice-view' && (
          <span className="text-xs text-yellow-600">GPU intensive</span>
        )}
      </div>
      <Suspense fallback={fallback}>
        {mode === 'alice-view' ? (
          <AliceViewPreview lolSource={lolSource} />
        ) : (
          <ModelPreview blob={blob} />
        )}
      </Suspense>
    </div>
  );
}
