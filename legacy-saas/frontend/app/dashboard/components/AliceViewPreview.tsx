'use client';
import { useRef, useEffect, useCallback, useState } from 'react';

declare global {
  interface Navigator {
    gpu?: unknown;
  }
}

interface AliceViewPreviewProps {
  lolSource: string | null;
}

interface AliceViewHandle {
  render_frame(dt: number): void;
  resize(w: number, h: number): void;
  orbit(dTheta: number, dPhi: number): void;
  zoom(delta: number): void;
  set_sdf_shader(wgsl: string): void;
}

interface AliceViewWasm {
  default(): Promise<void>;
  alice_view_init(canvasId: string): Promise<AliceViewHandle>;
}

let wasmModule: AliceViewWasm | null = null;

async function loadWasm(): Promise<AliceViewWasm | null> {
  if (wasmModule) return wasmModule;
  try {
    // ALICE-View WASM built with wasm-pack and placed in public/wasm/
    const mod = await import('/wasm/alice_view_wasm.js' as string) as unknown as AliceViewWasm;
    await mod.default();
    wasmModule = mod;
    return mod;
  } catch {
    return null;
  }
}

export default function AliceViewPreview({ lolSource }: AliceViewPreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const handleRef = useRef<AliceViewHandle | null>(null);
  const rafRef = useRef<number>(0);
  const [supported, setSupported] = useState<boolean | null>(null);
  const dragging = useRef(false);
  const lastPos = useRef({ x: 0, y: 0 });

  // Initialize WASM + WebGPU
  useEffect(() => {
    let cancelled = false;

    (async () => {
      // Check WebGPU support
      if (!navigator.gpu) {
        setSupported(false);
        return;
      }

      const mod = await loadWasm();
      if (!mod || cancelled) {
        setSupported(false);
        return;
      }

      try {
        const handle = await mod.alice_view_init('alice-view-canvas');
        if (cancelled) return;
        handleRef.current = handle;
        setSupported(true);

        // Render loop
        let last = performance.now();
        const frame = () => {
          const now = performance.now();
          const dt = (now - last) * 0.001;
          last = now;
          handle.render_frame(dt);
          rafRef.current = requestAnimationFrame(frame);
        };
        rafRef.current = requestAnimationFrame(frame);
      } catch {
        setSupported(false);
      }
    })();

    return () => {
      cancelled = true;
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, []);

  // Update SDF when LOL source changes
  useEffect(() => {
    if (!handleRef.current || !lolSource) return;
    // LOL → WGSL transpile happens server-side; for now pass through
    // The WGSL shader is embedded in the LOL source as a comment or separate field
    // For MVP: use a default sphere shader if no WGSL provided
    handleRef.current.set_sdf_shader(lolSource);
  }, [lolSource]);

  // Resize handler
  useEffect(() => {
    const handle = handleRef.current;
    const canvas = canvasRef.current;
    if (!handle || !canvas) return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        handle.resize(Math.floor(width), Math.floor(height));
      }
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [supported]);

  // Mouse orbit + zoom
  const onMouseDown = useCallback((e: React.MouseEvent) => {
    dragging.current = true;
    lastPos.current = { x: e.clientX, y: e.clientY };
  }, []);

  const onMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current || !handleRef.current) return;
    const dx = e.clientX - lastPos.current.x;
    const dy = e.clientY - lastPos.current.y;
    handleRef.current.orbit(dx * 0.01, dy * 0.01);
    lastPos.current = { x: e.clientX, y: e.clientY };
  }, []);

  const onMouseUp = useCallback(() => {
    dragging.current = false;
  }, []);

  const onWheel = useCallback((e: React.WheelEvent) => {
    if (!handleRef.current) return;
    handleRef.current.zoom(e.deltaY * 0.01);
  }, []);

  if (supported === false) {
    // Fallback: WebGPU not supported
    return (
      <div className="w-full h-64 border border-dashed border-border rounded-lg flex items-center justify-center text-sm text-muted-foreground">
        WebGPU not supported — use Chrome 113+ for ALICE-View preview
      </div>
    );
  }

  return (
    <div className="w-full h-80 border border-border rounded-lg overflow-hidden bg-black relative">
      <canvas
        id="alice-view-canvas"
        ref={canvasRef}
        className="w-full h-full"
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={onMouseUp}
        onMouseLeave={onMouseUp}
        onWheel={onWheel}
      />
      {supported === null && (
        <div className="absolute inset-0 flex items-center justify-center text-sm text-muted-foreground">
          Initializing ALICE-View...
        </div>
      )}
    </div>
  );
}
