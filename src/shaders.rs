//! GLSL for every layer kind, in pairs: a lit color pass and an id pass that writes (layer, element) for picking.

use crate::gl::Program;

/// Shared by every fragment shader: lighting in view space (a key light from the upper left, a weak fill from the
/// lower right and a sky/ground ambient, both sides lit), the colormap lookup, and the picking id encoding.
const COMMON: &str = r#"#version 300 es
precision highp float;
precision highp int;
vec3 shade(vec3 base, vec3 n, vec3 pos) {
  vec3 v = normalize(-pos);
  if (dot(n, v) < 0.0) n = -n;
  base = pow(base, vec3(2.2));
  vec3 key = normalize(vec3(-0.35, 0.55, 0.75)), fill = normalize(vec3(0.6, -0.3, 0.5));
  float light = 0.16 + 0.2 * (0.5 + 0.5 * n.y) + 0.75 * max(dot(n, key), 0.0) + 0.15 * max(dot(n, fill), 0.0);
  float spec = pow(max(dot(n, normalize(key + v)), 0.0), 40.0);
  return pow(base * light + 0.1 * spec, vec3(1.0 / 2.2));
}
vec3 colormap(sampler2D map, float value, vec2 range) {
  return texture(map, vec2(clamp((value - range.x) / (range.y - range.x), 0.0, 1.0), 0.5)).rgb;
}
// Element id in rgb (24 bits), layer + 1 in alpha, so a clear pixel reads as "nothing".
vec4 encodeId(uint layer, uint id) {
  return vec4(uvec4(id, id >> 8, id >> 16, layer + 1u) & 255u) / 255.0;
}
"#;

// ---- meshes

const MESH_VS: &str = r#"
layout(location=0) in vec3 aPos; layout(location=1) in vec3 aNormal; layout(location=2) in float aValue;
uniform mat4 uModelView, uProj;
out vec3 vPos; out vec3 vNormal; out float vValue; flat out uint vId;
void main() {
  vec4 p = uModelView * vec4(aPos, 1.0);
  vPos = p.xyz; vNormal = mat3(uModelView) * aNormal; vValue = aValue; vId = uint(gl_VertexID);
  gl_Position = uProj * p;
}"#;

const MESH_FS: &str = r#"
in vec3 vPos; in vec3 vNormal; in float vValue; flat in uint vId;
uniform sampler2D uMap; uniform vec2 uRange; uniform bool uFlat;
out vec4 color;
void main() {
  vec3 n = normalize(uFlat ? cross(dFdx(vPos), dFdy(vPos)) : vNormal);
  color = vec4(shade(colormap(uMap, vValue, uRange), n, vPos), 1.0);
}"#;

/// The id pass of a mesh: each triangle in its provoking (last) vertex's id, refined to the nearest vertex on the CPU.
const MESH_IDS_FS: &str = r#"
in vec3 vPos; in vec3 vNormal; in float vValue; flat in uint vId;
uniform uint uLayer;
out vec4 color;
void main() { color = encodeId(uLayer, vId); }"#;

const EDGES_FS: &str = r#"
in vec3 vPos; in vec3 vNormal; in float vValue; flat in uint vId;
uniform vec4 uColor;
out vec4 color;
void main() { color = uColor; }"#;

// ---- points: round sprites, shaded as little spheres

const POINTS_VS: &str = r#"
layout(location=0) in vec3 aPos; layout(location=2) in float aValue;
uniform mat4 uModelView, uProj; uniform float uSize, uLift;
out vec3 vPos; out float vValue; flat out uint vId;
void main() {
  vec4 p = uModelView * vec4(aPos, 1.0);
  p.xyz += normalize(-p.xyz) * uLift;  // toward the camera, so a point on a surface isn't half buried in it
  vPos = p.xyz; vValue = aValue; vId = uint(gl_VertexID);
  gl_Position = uProj * p;
  gl_PointSize = uSize;
}"#;

const POINTS_FS: &str = r#"
in vec3 vPos; in float vValue; flat in uint vId;
uniform sampler2D uMap; uniform vec2 uRange;
out vec4 color;
void main() {
  vec2 c = gl_PointCoord * 2.0 - 1.0;
  float r2 = dot(c, c);
  if (r2 > 1.0) discard;
  vec3 n = vec3(c.x, -c.y, sqrt(1.0 - r2));
  color = vec4(shade(colormap(uMap, vValue, uRange), n, vec3(0.0, 0.0, 1.0)), 1.0);
}"#;

const POINTS_IDS_FS: &str = r#"
in vec3 vPos; in float vValue; flat in uint vId;
uniform uint uLayer;
out vec4 color;
void main() {
  vec2 c = gl_PointCoord * 2.0 - 1.0;
  if (dot(c, c) > 1.0) discard;
  color = encodeId(uLayer, vId);
}"#;

// ---- lines: each segment an instanced screen-space quad, uWidth device pixels wide

const LINES_VS: &str = r#"
layout(location=0) in vec3 aA; layout(location=1) in vec3 aB; layout(location=2) in float aVa; layout(location=3) in float aVb;
uniform mat4 uModelView, uProj; uniform vec2 uViewport; uniform float uWidth;
out float vValue; flat out uint vId;
const vec2 corners[6] = vec2[](vec2(0, -1), vec2(1, -1), vec2(1, 1), vec2(0, -1), vec2(1, 1), vec2(0, 1));
void main() {
  vec2 c = corners[gl_VertexID];
  vec4 pa = uProj * uModelView * vec4(aA, 1.0), pb = uProj * uModelView * vec4(aB, 1.0);
  vec2 d = pb.xy / pb.w * uViewport - pa.xy / pa.w * uViewport;
  vec2 dir = length(d) > 1e-6 ? normalize(d) : vec2(1.0, 0.0), normal = vec2(-dir.y, dir.x);
  vec4 p = mix(pa, pb, c.x);
  // Out to the side by half the width, and past each end by half the width so joints overlap.
  p.xy += (normal * c.y + dir * (c.x * 2.0 - 1.0)) * uWidth / uViewport * p.w;
  p.z -= 0.0004 * p.w;  // just in front of a surface the curve lies on
  gl_Position = p;
  vValue = mix(aVa, aVb, c.x); vId = uint(gl_InstanceID);
}"#;

const LINES_FS: &str = r#"
in float vValue; flat in uint vId;
uniform sampler2D uMap; uniform vec2 uRange;
out vec4 color;
void main() { color = vec4(colormap(uMap, vValue, uRange), 1.0); }"#;

const LINES_IDS_FS: &str = r#"
in float vValue; flat in uint vId;
uniform uint uLayer;
out vec4 color;
void main() { color = encodeId(uLayer, vId); }"#;

// ---- vectors: an instanced arrow (shaft, cone, cone base) per vector

/// Arrow vertices are (cos, sin, part): 0, 1 the shaft's bottom and top rings; 2, 3 the cone's base ring and tip;
/// 4, 5 the cone's back ring and center.
const VECTORS_VS: &str = r#"
layout(location=0) in vec3 aCorner; layout(location=1) in vec3 aOrigin; layout(location=2) in vec3 aVec; layout(location=3) in float aValue;
uniform mat4 uModelView, uProj; uniform float uScale, uWidth;
out vec3 vPos; out vec3 vNormal; out float vValue; flat out uint vId;
void main() {
  vec3 v = aVec * uScale;
  float len = length(v);
  vec3 z = len > 1e-12 ? v / len : vec3(0.0, 0.0, 1.0);
  vec3 x = normalize(abs(z.z) < 0.9 ? cross(z, vec3(0.0, 0.0, 1.0)) : cross(z, vec3(1.0, 0.0, 0.0)));
  vec3 y = cross(z, x);
  float r = uWidth, R = 2.5 * uWidth, h = min(0.4 * len, 6.0 * uWidth);
  vec2 cs = aCorner.xy; float part = aCorner.z;
  vec3 local, n;
  if (part < 0.5)      { local = vec3(cs * r, 0.0);     n = vec3(cs, 0.0); }
  else if (part < 1.5) { local = vec3(cs * r, len - h); n = vec3(cs, 0.0); }
  else if (part < 2.5) { local = vec3(cs * R, len - h); n = normalize(vec3(cs * h, R)); }
  else if (part < 3.5) { local = vec3(0.0, 0.0, len);   n = normalize(vec3(cs * h, R)); }
  else if (part < 4.5) { local = vec3(cs * R, len - h); n = vec3(0.0, 0.0, -1.0); }
  else                 { local = vec3(0.0, 0.0, len - h); n = vec3(0.0, 0.0, -1.0); }
  mat3 frame = mat3(x, y, z);
  vec4 p = uModelView * vec4(aOrigin + frame * local, 1.0);
  vPos = p.xyz; vNormal = mat3(uModelView) * (frame * n); vValue = aValue; vId = uint(gl_InstanceID);
  gl_Position = uProj * p;
}"#;

const VECTORS_FS: &str = r#"
in vec3 vPos; in vec3 vNormal; in float vValue; flat in uint vId;
uniform sampler2D uMap; uniform vec2 uRange;
out vec4 color;
void main() { color = vec4(shade(colormap(uMap, vValue, uRange), normalize(vNormal), vPos), 1.0); }"#;

pub struct Shaders {
    pub mesh: Program,
    pub mesh_ids: Program,
    pub edges: Program,
    pub points: Program,
    pub points_ids: Program,
    pub lines: Program,
    pub lines_ids: Program,
    pub vectors: Program,
    pub vectors_ids: Program,
}

impl Shaders {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let vs = |body: &str| format!("{COMMON}{body}");
        let p = |v: &str, f: &str| Program::new(gl, &vs(v), &vs(f));
        Ok(Shaders {
            mesh: p(MESH_VS, MESH_FS)?,
            mesh_ids: p(MESH_VS, MESH_IDS_FS)?,
            edges: p(MESH_VS, EDGES_FS)?,
            points: p(POINTS_VS, POINTS_FS)?,
            points_ids: p(POINTS_VS, POINTS_IDS_FS)?,
            lines: p(LINES_VS, LINES_FS)?,
            lines_ids: p(LINES_VS, LINES_IDS_FS)?,
            vectors: p(VECTORS_VS, VECTORS_FS)?,
            vectors_ids: p(VECTORS_VS, MESH_IDS_FS)?,
        })
    }
}
