// A rippling sheet, built and animated in plain JavaScript: the same shape of widget as Python's, no Pyodide.
const N = 80, FRAMES = 60;

export function widget() {
  const faces = [];
  for (let i = 0; i < N - 1; i++) for (let j = 0; j < N - 1; j++) {
    const a = i * N + j, b = a + N;
    faces.push(a, b, b + 1, a, b + 1, a + 1);
  }
  const triangles = new Uint32Array(faces);  // the same object every time, so it's sent once

  function onChange({ frequency, damping }) {
    const vertices = new Float32Array(FRAMES * N * N * 3), values = new Float32Array(FRAMES * N * N);
    for (let f = 0; f < FRAMES; f++) {
      const phase = (2 * Math.PI * f) / FRAMES;
      for (let i = 0; i < N; i++) for (let j = 0; j < N; j++) {
        const x = (2 * i) / (N - 1) - 1, y = (2 * j) / (N - 1) - 1, r = Math.hypot(x, y);
        const z = 0.25 * Math.cos(frequency * Math.PI * r - phase) * Math.exp(-damping * 3 * r);
        const k = (f * N + i) * N + j;
        vertices.set([x, y, z], 3 * k);
        values[k] = 0.5 + 2 * z;
      }
    }
    return {
      ripple: {
        type: "mesh",
        vertices: { data: vertices, shape: [FRAMES, N * N, 3] },
        values: { data: values, shape: [FRAMES, N * N] },
        faces: triangles,
      },
    };
  }

  return { controls: { frequency: [1, 8, 4], damping: [0, 1, 0.4] }, onChange, loop: true };
}
