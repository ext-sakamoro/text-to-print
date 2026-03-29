'use client';
import { useState, useEffect, lazy, Suspense } from 'react';
import { useGeneration } from '@/lib/hooks/use-generation';

const ModelPreview = lazy(() => import('./components/ModelPreview'));

type ServiceStatus = 'checking' | 'online' | 'offline';

export default function DashboardPage() {
  const [prompt, setPrompt] = useState('');
  const [quality, setQuality] = useState('high');
  const [lolMode, setLolMode] = useState(false);
  const [lolSource, setLolSource] = useState('');
  const [previewBlob, setPreviewBlob] = useState<Blob | null>(null);
  const [serviceStatus, setServiceStatus] = useState<ServiceStatus>('checking');

  const { loading, result, error, run } = useGeneration();

  useEffect(() => {
    (async () => {
      try {
        const workerUrl = process.env.NEXT_PUBLIC_WORKER_URL || '';
        if (!workerUrl) { setServiceStatus('offline'); return; }
        const resp = await fetch(`${workerUrl}/health`, { signal: AbortSignal.timeout(5000) });
        if (resp.ok) {
          const data = await resp.json();
          setServiceStatus(data.llm_endpoint && data.llm_endpoint !== 'http://localhost:8000' ? 'online' : 'offline');
        } else {
          setServiceStatus('offline');
        }
      } catch {
        setServiceStatus('offline');
      }
    })();
  }, []);

  const handleGenerate = async () => {
    setPreviewBlob(null);
    const res = await run({
      mode: lolMode ? 'lol' : 'natural',
      prompt,
      lolSource,
      quality,
    });
    if (res) {
      setPreviewBlob(res.blob);
    }
  };

  const handleDownload = () => {
    if (!previewBlob || !result) return;
    const url = URL.createObjectURL(previewBlob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${result.jobId}.3mf`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const isLlmError = error?.error?.includes('LLM error') || error?.error?.includes('request failed');

  return (
    <div className="p-6 max-w-4xl mx-auto space-y-6">
      <h1 className="text-2xl font-bold">3dvbgaran</h1>
      <p className="text-sm text-muted-foreground">
        Describe what you want to 3D print. The AI generates a mathematically precise .3mf file.
      </p>

      {serviceStatus === 'offline' && (
        <div className="border border-yellow-400 bg-yellow-50 dark:bg-yellow-950 rounded-lg p-4">
          <p className="font-medium text-yellow-700 dark:text-yellow-300">3D Generation Engine: Offline</p>
          <p className="text-sm text-yellow-600 dark:text-yellow-400 mt-1">
            The inference server is currently unavailable. LOL DSL direct input mode is available.
            Natural language generation requires the inference server.
          </p>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Left: Input */}
        <div className="space-y-4">
          {/* Mode toggle */}
          <div className="flex gap-2">
            <button
              onClick={() => setLolMode(false)}
              className={`px-3 py-1 text-sm rounded-md ${!lolMode ? 'bg-primary text-primary-foreground' : 'bg-muted'}`}
            >
              Natural Language
            </button>
            <button
              onClick={() => setLolMode(true)}
              className={`px-3 py-1 text-sm rounded-md ${lolMode ? 'bg-primary text-primary-foreground' : 'bg-muted'}`}
            >
              LOL DSL
            </button>
          </div>

          {lolMode ? (
            <textarea
              value={lolSource}
              onChange={(e) => setLolSource(e.target.value)}
              placeholder={'smooth_union {\n  sphere { radius: 20 }\n  translate {\n    cylinder { radius: 8, height: 30 }\n    offset: [0, 15, 0]\n  }\n  k: 5\n}'}
              rows={10}
              className="w-full px-3 py-2 border border-input rounded-md bg-background text-sm font-mono"
            />
          ) : (
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder="A phone stand with a 15 degree angle and a cable hole at the back"
              rows={4}
              className="w-full px-3 py-2 border border-input rounded-md bg-background text-sm"
            />
          )}

          {/* Quality */}
          <div className="flex items-center gap-3">
            <label className="text-sm font-medium">Quality:</label>
            {['preview', 'high', 'ultra'].map((q) => (
              <button
                key={q}
                onClick={() => setQuality(q)}
                className={`px-3 py-1 text-sm rounded-md capitalize ${quality === q ? 'bg-primary text-primary-foreground' : 'bg-muted'}`}
              >
                {q}
              </button>
            ))}
          </div>

          <button
            onClick={handleGenerate}
            disabled={loading || (!lolMode && !prompt.trim()) || (lolMode && !lolSource.trim())}
            className="w-full px-4 py-3 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90 disabled:opacity-50"
          >
            {loading ? 'Generating...' : 'Generate .3mf'}
          </button>
        </div>

        {/* Right: Preview */}
        <div className="space-y-4">
          <Suspense fallback={<div className="w-full h-80 border border-dashed border-border rounded-lg flex items-center justify-center text-sm text-muted-foreground">Loading 3D viewer...</div>}>
            <ModelPreview blob={previewBlob} />
          </Suspense>

          {result && (
            <div className="flex items-center justify-between">
              <div className="text-xs text-muted-foreground space-x-4">
                <span>{result.triangles.toLocaleString()} triangles</span>
                <span>{result.vertices.toLocaleString()} vertices</span>
              </div>
              <button
                onClick={handleDownload}
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90"
              >
                Download .3mf
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Result / Error */}
      {(result || error) && (
        <div className={`border rounded-lg p-4 ${error ? 'border-red-500 bg-red-50 dark:bg-red-950' : 'border-green-500 bg-green-50 dark:bg-green-950'}`}>
          {error ? (
            <div>
              <p className="font-medium text-red-700 dark:text-red-300">
                {isLlmError ? 'Inference Server Unavailable' : 'Error'}
              </p>
              <p className="text-sm mt-1">
                {isLlmError
                  ? 'The 3D generation engine is currently offline. Please try again later or use LOL DSL direct input mode.'
                  : error.error}
              </p>
            </div>
          ) : result ? (
            <div>
              <p className="font-medium text-green-700 dark:text-green-300">Generated successfully</p>
              {result.lolSource && (
                <details className="mt-2">
                  <summary className="text-xs cursor-pointer text-muted-foreground">LOL Source</summary>
                  <pre className="mt-1 text-xs bg-muted p-2 rounded overflow-x-auto max-h-32">{result.lolSource}</pre>
                </details>
              )}
            </div>
          ) : null}
        </div>
      )}

      <div className="text-xs text-muted-foreground border-t pt-4">
        Target: Bambu Lab H2D (315 x 310 x 315 mm) | Powered by 3dvbgaran Engine
      </div>
    </div>
  );
}
