// Vector fields: an instanced arrow (shaft, cone, cone base) per vector. Arrow vertices are (cos, sin, part):
// 0, 1 the shaft's bottom and top rings; 2, 3 the cone's base ring and tip; 4, 5 the cone's back ring and center.

struct Uniforms {
  uModelView: mat4x4<f32>,
  uProj: mat4x4<f32>,
  uRange: vec2<f32>,
  uScale: f32,
  uWidth: f32,
  uLayer: u32,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Varyings {
  @builtin(position) position: vec4<f32>,
  @location(0) pos: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) value: f32,
  @location(3) @interpolate(flat) id: u32,
}

@vertex
fn vs(@location(0) aCorner: vec3<f32>, @location(1) aOrigin: vec3<f32>, @location(2) aVec: vec3<f32>,
      @location(3) aValue: f32, @builtin(instance_index) instance: u32) -> Varyings {
  let v = aVec * u.uScale;
  let len = length(v);
  let z = select(vec3<f32>(0.0, 0.0, 1.0), v / len, len > 1e-12);
  let x = normalize(select(cross(z, vec3<f32>(1.0, 0.0, 0.0)), cross(z, vec3<f32>(0.0, 0.0, 1.0)), abs(z.z) < 0.9));
  let y = cross(z, x);
  let r = u.uWidth;
  let R = 2.5 * u.uWidth;
  let h = min(0.4 * len, 6.0 * u.uWidth);
  let cs = aCorner.xy;
  let part = aCorner.z;
  var local: vec3<f32>;
  var n: vec3<f32>;
  if (part < 0.5)      { local = vec3<f32>(cs * r, 0.0);     n = vec3<f32>(cs, 0.0); }
  else if (part < 1.5) { local = vec3<f32>(cs * r, len - h); n = vec3<f32>(cs, 0.0); }
  else if (part < 2.5) { local = vec3<f32>(cs * R, len - h); n = normalize(vec3<f32>(cs * h, R)); }
  else if (part < 3.5) { local = vec3<f32>(0.0, 0.0, len);   n = normalize(vec3<f32>(cs * h, R)); }
  else if (part < 4.5) { local = vec3<f32>(cs * R, len - h); n = vec3<f32>(0.0, 0.0, -1.0); }
  else                 { local = vec3<f32>(0.0, 0.0, len - h); n = vec3<f32>(0.0, 0.0, -1.0); }
  let frame = mat3x3<f32>(x, y, z);
  let p = u.uModelView * vec4<f32>(aOrigin + frame * local, 1.0);
  var out: Varyings;
  out.position = u.uProj * p;
  out.pos = p.xyz;
  out.normal = mat3_of(u.uModelView) * (frame * n);
  out.value = aValue;
  out.id = instance;
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  return vec4<f32>(shade(colormap(in.value, u.uRange), normalize(in.normal), in.pos), 1.0);
}

@fragment
fn fs_ids(in: Varyings) -> @location(0) vec4<f32> {
  return encode_id(u.uLayer, in.id);
}
