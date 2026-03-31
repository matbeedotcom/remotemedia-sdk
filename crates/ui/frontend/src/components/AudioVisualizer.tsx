import { useEffect, useRef } from 'preact/hooks';

interface AudioVisualizerProps {
  stream: MediaStream | null;
  height?: number;
}

export function AudioVisualizer({ stream, height = 60 }: AudioVisualizerProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !stream) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const audioCtx = new AudioContext();
    const analyser = audioCtx.createAnalyser();
    analyser.fftSize = 2048;

    const source = audioCtx.createMediaStreamSource(stream);
    source.connect(analyser);

    const bufferLength = analyser.frequencyBinCount;
    const dataArray = new Uint8Array(bufferLength);

    let animFrameId: number;

    const draw = () => {
      animFrameId = requestAnimationFrame(draw);

      analyser.getByteTimeDomainData(dataArray);

      const style = getComputedStyle(canvas);
      const bgColor = style.getPropertyValue('--bg-input').trim() || '#1a1a3e';
      const accentColor = style.getPropertyValue('--accent').trim() || '#e94560';

      ctx.fillStyle = bgColor;
      ctx.fillRect(0, 0, canvas.width, canvas.height);

      ctx.lineWidth = 2;
      ctx.strokeStyle = accentColor;
      ctx.beginPath();

      const sliceWidth = canvas.width / bufferLength;
      let x = 0;

      for (let i = 0; i < bufferLength; i++) {
        const v = dataArray[i] / 128.0;
        const y = (v * canvas.height) / 2;

        if (i === 0) {
          ctx.moveTo(x, y);
        } else {
          ctx.lineTo(x, y);
        }
        x += sliceWidth;
      }

      ctx.lineTo(canvas.width, canvas.height / 2);
      ctx.stroke();
    };

    draw();

    return () => {
      cancelAnimationFrame(animFrameId);
      source.disconnect();
      audioCtx.close();
    };
  }, [stream]);

  return (
    <canvas
      ref={canvasRef}
      class="audio-visualizer-canvas"
      width={480}
      height={height}
      style={{ width: '100%' }}
    />
  );
}
