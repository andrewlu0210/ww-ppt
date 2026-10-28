import { useEffect, useRef } from "react";

/** Draws an RGBA thumbnail rendered by the engine. */
export function Thumb({ image, className }: { image?: ImageData; className?: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas || !image) return;
    canvas.width = image.width;
    canvas.height = image.height;
    canvas.getContext("2d")?.putImageData(image, 0, 0);
  }, [image]);
  return <canvas ref={ref} className={className} />;
}
