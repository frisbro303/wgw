// Points: each an instanced square, uSize device pixels across, cut to a disc and shaded as a little sphere.

struct Uniforms {
  uModelView: mat4x4<f32>,
  uProj: mat4x4<f32>,
  uViewport: vec2<f32>,
  uRange: vec2<f32>,
  uSize: f32,
  uLift: f32,
  uLayer: u32,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Varyings {
  @builtin(position) position: vec4<f32>,
  @location(0) corner: vec2<f32>,
  @location(1) value: f32,
  @location(2) @interpolate(flat) id: u32,
}

@vertex
fn vs(@location(0) aPos: vec3<f32>, @location(2) aValue: f32,
      @builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> Varyings {
  var corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
                                     vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
  let c = corners[vertex];
  var p = u.uModelView * vec4<f32>(aPos, 1.0);
  p = vec4<f32>(p.xyz + normalize(-p.xyz) * u.uLift, 1.0);  // toward the camera, so a point on a surface isn't half buried in it
  var clip = u.uProj * p;
  clip = vec4<f32>(clip.xy + c * u.uSize / u.uViewport * clip.w, clip.zw);  // half the size each way, in device pixels
  var out: Varyings;
  out.position = clip;
  out.corner = c;
  out.value = aValue;
  out.id = instance;
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let r2 = dot(in.corner, in.corner);
  let n = vec3<f32>(in.corner, sqrt(max(1.0 - r2, 0.0)));
  let color = vec4<f32>(shade(colormap(in.value, u.uRange), n, vec3<f32>(0.0, 0.0, -1.0)), 1.0);  // seen from the camera, in front of it
  if (r2 > 1.0) { discard; }
  return color;
}

@fragment
fn fs_ids(in: Varyings) -> @location(0) vec4<f32> {
  if (dot(in.corner, in.corner) > 1.0) { discard; }
  return encode_id(u.uLayer, in.id);
}
