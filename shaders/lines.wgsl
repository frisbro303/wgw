// Curves: each segment an instanced screen-space quad, uWidth device pixels wide.

struct Uniforms {
  uModelView: mat4x4<f32>,
  uProj: mat4x4<f32>,
  uViewport: vec2<f32>,
  uRange: vec2<f32>,
  uWidth: f32,
  uLayer: u32,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Varyings {
  @builtin(position) position: vec4<f32>,
  @location(0) value: f32,
  @location(1) @interpolate(flat) id: u32,
}

@vertex
fn vs(@location(0) aA: vec3<f32>, @location(1) aB: vec3<f32>, @location(2) aVa: f32, @location(3) aVb: f32,
      @builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> Varyings {
  var corners = array<vec2<f32>, 6>(vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
                                     vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
  let c = corners[vertex];
  let pa = u.uProj * u.uModelView * vec4<f32>(aA, 1.0);
  let pb = u.uProj * u.uModelView * vec4<f32>(aB, 1.0);
  let d = pb.xy / pb.w * u.uViewport - pa.xy / pa.w * u.uViewport;
  let dir = select(vec2<f32>(1.0, 0.0), normalize(d), length(d) > 1e-6);
  let normal = vec2<f32>(-dir.y, dir.x);
  var p = mix(pa, pb, c.x);
  // Out to the side by half the width, and past each end by half the width so joints overlap.
  p = vec4<f32>(p.xy + (normal * c.y + dir * (c.x * 2.0 - 1.0)) * u.uWidth / u.uViewport * p.w, p.zw);
  p.z = p.z - 0.0015 * p.w;  // just in front of a surface the curve lies on
  var out: Varyings;
  out.position = p;
  out.value = mix(aVa, aVb, c.x);
  out.id = instance;
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  return vec4<f32>(colormap(in.value, u.uRange), 1.0);
}

@fragment
fn fs_ids(in: Varyings) -> @location(0) vec4<f32> {
  return encode_id(u.uLayer, in.id);
}
