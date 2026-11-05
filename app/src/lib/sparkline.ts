/** SVG polyline points. The peak sits 10% below the top; all-zero samples lie on the bottom edge. */
export function sparkPoints(samples: number[], width: number, height: number): string {
  if (samples.length === 0) return '';
  const vals = samples.map((v) => (Number.isFinite(v) && v > 0 ? v : 0));
  const max = Math.max(...vals);
  const n = vals.length;
  return vals
    .map((v, i) => {
      const x = n === 1 ? 0 : (i / (n - 1)) * width;
      const y = max > 0 ? height - (v / max) * height * 0.9 : height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(' ');
}
