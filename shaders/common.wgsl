// Shared by every layer's shaders (each file is compiled with this one prepended): lighting in view space, the
// colormap lookup, and the picking id encoding. The single source of wgw's look: the build translates these
// shaders to GLSL for WebGL, and the still (for PDFs) runs the same ones.

// The colors values run through, as a 256-texel strip.
@group(1) @binding(0) var uMap: texture_2d<f32>;
@group(1) @binding(1) var uMapSampler: sampler;

// A key light from the upper left, a weak fill from the lower right and a sky/ground ambient, both sides lit.
fn shade(base_color: vec3<f32>, normal: vec3<f32>, pos: vec3<f32>) -> vec3<f32> {
  let v = normalize(-pos);
  var n = normal;
  if (dot(n, v) < 0.0) { n = -n; }
  let base = pow(base_color, vec3<f32>(2.2));
  let key = normalize(vec3<f32>(-0.35, 0.55, 0.75));
  let fill = normalize(vec3<f32>(0.6, -0.3, 0.5));
  let light = 0.16 + 0.2 * (0.5 + 0.5 * n.y) + 0.75 * max(dot(n, key), 0.0) + 0.15 * max(dot(n, fill), 0.0);
  let spec = pow(max(dot(n, normalize(key + v)), 0.0), 40.0);
  return pow(base * light + 0.1 * spec, vec3<f32>(1.0 / 2.2));
}

fn colormap(value: f32, range: vec2<f32>) -> vec3<f32> {
  let t = clamp((value - range.x) / (range.y - range.x), 0.0, 1.0);
  return textureSample(uMap, uMapSampler, vec2<f32>(t, 0.5)).rgb;
}

// Element id in rgb (24 bits), layer + 1 in alpha, so a clear pixel reads as "nothing".
fn encode_id(layer: u32, id: u32) -> vec4<f32> {
  return vec4<f32>(vec4<u32>(id, id >> 8u, id >> 16u, layer + 1u) & vec4<u32>(255u)) / 255.0;
}

fn mat3_of(m: mat4x4<f32>) -> mat3x3<f32> {
  return mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz);
}
