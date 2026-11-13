// Meshes: smooth or flat shaded, colored by values; their triangle edges; and their ids for picking.

struct Uniforms {
  uModelView: mat4x4<f32>,
  uProj: mat4x4<f32>,
  uColor: vec4<f32>,
  uRange: vec2<f32>,
  uFlat: i32,
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
fn vs(@location(0) aPos: vec3<f32>, @location(1) aNormal: vec3<f32>, @location(2) aValue: f32,
      @builtin(vertex_index) vertex: u32) -> Varyings {
  let p = u.uModelView * vec4<f32>(aPos, 1.0);
  var out: Varyings;
  out.position = u.uProj * p;
  out.pos = p.xyz;
  out.normal = mat3_of(u.uModelView) * aNormal;
  out.value = aValue;
  out.id = vertex;
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let n = normalize(select(in.normal, cross(dpdx(in.pos), dpdy(in.pos)), u.uFlat != 0));
  return vec4<f32>(shade(colormap(in.value, u.uRange), n, in.pos), 1.0);
}

// Each triangle in its provoking (last) vertex's id, refined to the nearest vertex on the CPU.
@fragment
fn fs_ids(in: Varyings) -> @location(0) vec4<f32> {
  return encode_id(u.uLayer, in.id);
}

@fragment
fn fs_edges(in: Varyings) -> @location(0) vec4<f32> {
  return u.uColor;
}
