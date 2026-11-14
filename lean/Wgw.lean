import Lean.Data.Json

/-!
# wgw for Lean 4

Interactive 3D figures of arrays and wgw widgets, in a window. A thin head: layers become the patches wgw's page
code reads, and wgw-window (a small program in wgw's repository, found as `WGW_WINDOW` or on the PATH) shows them,
with the page code and its WebAssembly core in a webview, calling your function when a slider moves or a point is
picked. Drawing, turning, the controls and widgets are wgw's own.

```lean
def main : IO Unit := do
  (← Wgw.Figure.file "torus.wgw").display                    -- a widget (Rust, compiled to wasm), or a mesh file
  (Wgw.Figure.layers [("mesh", Wgw.mesh vertices faces |>.values k)]).display
  (Wgw.Figure.callback (fun s => do
      let t ← s.value "t"
      pure [("mesh", Wgw.mesh (vertices.map (scale t)) faces)])
    |>.slider "t" 0.5 2 1).display
```

`display` waits until the figure's window is closed; the window closes when the program ends.
-/
namespace Wgw

open Lean (Json)

/-- A point or vector. -/
abbrev Vec3 := Float × Float × Float

/-- An array for a layer: float32 or uint32, with its shape (row-major). -/
inductive Arr where
  | f32 (shape : Array Nat) (data : Array Float)
  | u32 (shape : Array Nat) (data : Array UInt32)

private def flat (rows : Array Vec3) : Array Float :=
  rows.foldl (fun out (x, y, z) => out.push x |>.push y |>.push z) #[]

/-- A layer: its type ("mesh", "points", "lines" or "vectors"), arrays and parameters (see wgw's docs). -/
structure Layer where
  type : String
  arrays : Array (String × Arr)
  params : Array (String × Json) := #[]

/-- A triangle mesh: vertices, and faces as triples of vertex indices (from 0). -/
def mesh (vertices : Array Vec3) (faces : Array (Nat × Nat × Nat)) : Layer :=
  { type := "mesh", arrays := #[("vertices", .f32 #[vertices.size, 3] (flat vertices)),
      ("faces", .u32 #[faces.size, 3] (faces.foldl (fun o (a, b, c) => o.push a.toUInt32 |>.push b.toUInt32 |>.push c.toUInt32) #[]))] }

/-- Points, drawn as dots. -/
def points (vertices : Array Vec3) : Layer :=
  { type := "points", arrays := #[("vertices", .f32 #[vertices.size, 3] (flat vertices))] }

/-- A curve through the vertices (see `segments` for other pairs). -/
def lines (vertices : Array Vec3) : Layer :=
  { type := "lines", arrays := #[("vertices", .f32 #[vertices.size, 3] (flat vertices))] }

/-- Arrows from the origins along the vectors. -/
def vectors (origins vectors : Array Vec3) : Layer :=
  { type := "vectors", arrays := #[("vertices", .f32 #[origins.size, 3] (flat origins)), ("vectors", .f32 #[vectors.size, 3] (flat vectors))] }

namespace Layer
/-- Any parameter: a number, boolean or CSS color, as JSON. -/
def param (l : Layer) (key : String) (value : Json) : Layer := { l with params := l.params.push (key, value) }
/-- Per-vertex values, colored through the layer's colors. -/
def values (l : Layer) (vs : Array Float) : Layer := { l with arrays := l.arrays.push ("values", .f32 #[vs.size] vs) }
/-- Which vertices to join, in pairs (from 0). -/
def segments (l : Layer) (pairs : Array (Nat × Nat)) : Layer :=
  { l with arrays := l.arrays.push ("segments", .u32 #[pairs.size, 2] (pairs.foldl (fun o (a, b) => o.push a.toUInt32 |>.push b.toUInt32) #[])) }
/-- One CSS color for the whole layer. -/
def color (l : Layer) (css : String) : Layer := l.param "color" css
/-- The colors values run through. -/
def colors (l : Layer) (css : List String) : Layer := l.param "colors" (Json.arr (css.map Json.str).toArray)
/-- The range of values the colors span (0 to 1 by default). -/
def range (l : Layer) (lo hi : Float) : Layer := l.param "min" (Lean.toJson lo) |>.param "max" (Lean.toJson hi)
def width (l : Layer) (px : Float) : Layer := l.param "width" (Lean.toJson px)
def size (l : Layer) (px : Float) : Layer := l.param "size" (Lean.toJson px)
/-- The pane it's drawn in: 1 for a second view beside the first. -/
def pane (l : Layer) (n : Nat) : Layer := l.param "pane" n
def flat (l : Layer) (on : Bool := true) : Layer := l.param "flat" on
end Layer

/-- A scene: layers by name. -/
abbrev Scene := List (String × Layer)

private def u32le (n : UInt32) : ByteArray :=
  ⟨#[n.toUInt8, (n >>> 8).toUInt8, (n >>> 16).toUInt8, (n >>> 24).toUInt8]⟩

/-- A scene as the patch bytes wgw's page reads: "WGW1", the header's length, a JSON header, the arrays. -/
def encode (scene : Scene) : ByteArray := Id.run do
  let mut data := ByteArray.empty
  let mut layers : Array Json := #[]
  for (name, l) in scene do
    let mut arrays : Array (String × Json) := #[]
    for (key, a) in l.arrays do
      let (dtype, shape, bytes) := match a with
        | .f32 s d => ("f4", s, d.foldl (fun b (x : Float) => b ++ u32le x.toFloat32.toBits) ByteArray.empty)
        | .u32 s d => ("u4", s, d.foldl (fun b (x : UInt32) => b ++ u32le x) ByteArray.empty)
      arrays := arrays.push (key, Json.mkObj [("dtype", dtype), ("shape", Lean.toJson shape), ("offset", data.size)])
      data := data ++ bytes
    layers := layers.push (Json.arr #[name, Json.mkObj [("type", l.type), ("params", Json.mkObj l.params.toList),
      ("remove", Json.arr #[]), ("arrays", Json.mkObj arrays.toList)]])
  let mut text := (Json.mkObj [("layers", Json.arr layers)]).compress.toUTF8
  while (8 + text.size) % 4 != 0 do text := text.push ' '.toNat.toUInt8  -- the arrays start 4-byte aligned
  return "WGW1".toUTF8 ++ u32le text.size.toUInt32 ++ text ++ data

/-- A pick: the vertex (or element, from 0), its layer, the point, and the frame on screen. -/
structure Pick where
  vertex : Nat
  layer : String
  point : Vec3
  frame : Nat

/-- What a scene is for: the controls' values by name, and the picks so far, the latest last. -/
structure State where
  values : List (String × Json)
  picks : Array Pick

namespace State
private def find (s : State) (name : String) : IO Json :=
  match s.values.lookup name with
  | some v => pure v
  | none => throw <| IO.userError s!"no control named {repr name}; the controls are {", ".intercalate (s.values.map (repr ·.1 |>.pretty))}"
/-- A slider's value. -/
def value (s : State) (name : String) : IO Float := do
  match (← s.find name).getNum? with
  | .ok n => pure n.toFloat
  | .error _ => throw <| IO.userError s!"the control {repr name} is a checkbox: use State.checked"
/-- A checkbox's state. -/
def checked (s : State) (name : String) : IO Bool := do
  match (← s.find name).getBool? with
  | .ok b => pure b
  | .error _ => throw <| IO.userError s!"the control {repr name} is a slider: use State.value"
/-- The latest pick, if any. -/
def pick (s : State) : Option Pick := s.picks.back?
end State

/-- What a figure shows: a .wgw file (a widget or a mesh), layers, or a function of the state. -/
inductive Source where
  | file (bytes : ByteArray)
  | layers (scene : Scene)
  | callback (scene : State → IO Scene)

/-- A figure, with its controls (for a function) and options: up is "x", "y" or "z"; view is turn and tilt in
degrees; pick is "click", "hover" or "none"; height is in CSS pixels. -/
structure Figure where
  source : Source
  title : String := "wgw"
  controls : List (String × Json) := []
  up : String := "z"
  view : Float × Float := (-60, 20)
  pick : String := "click"
  height : Nat := 420

namespace Figure
def layers (scene : Scene) : Figure := { source := .layers scene }
def callback (scene : State → IO Scene) : Figure := { source := .callback scene }
/-- A .wgw file: a widget (Rust, compiled to WebAssembly), or a mesh. -/
def file (path : System.FilePath) : IO Figure := do
  pure { source := .file (← IO.FS.readBinFile path), title := path.fileName.getD "wgw" }
/-- A slider from lo to hi, starting at value. -/
def slider (f : Figure) (name : String) (lo hi value : Float) : Figure :=
  { f with controls := f.controls ++ [(name, Lean.toJson [lo, hi, value])] }
/-- A checkbox. -/
def checkbox (f : Figure) (name : String) (on : Bool) : Figure :=
  { f with controls := f.controls ++ [(name, Lean.toJson on)] }

private def start (c : Json) : Json := match c with
  | .arr a => a[2]!
  | other => other

/-- A state from wgw-window's lines: `control <value> <name>`, `pick <vertex> <frame> <x> <y> <z> <layer>`. -/
def state (f : Figure) (lines : String) : State := Id.run do
  let num (w : String) : Float := ((Json.parse w).toOption.bind (·.getNum?.toOption)).map (·.toFloat) |>.getD 0
  let mut values := f.controls.map fun (k, c) => (k, start c)
  let mut picks := #[]
  for line in lines.splitOn "\n" do
    match line.splitOn " " with
    | "control" :: v :: name =>
      let value := if v == "true" then Json.bool true else if v == "false" then Json.bool false else (Json.parse v).toOption.getD Json.null
      values := (" ".intercalate name, value) :: values.filter (·.1 != " ".intercalate name)
    | "pick" :: v :: frame :: x :: y :: z :: layer =>
      picks := picks.push { vertex := v.toNat!, frame := frame.toNat!, point := (num x, num y, num z), layer := " ".intercalate layer }
    | _ => pure ()
  return { values, picks }

private def readExact (h : IO.FS.Handle) (n : Nat) : IO ByteArray := do
  let mut out := ByteArray.empty
  while out.size < n do
    let chunk ← h.read (n - out.size).toUSize
    if chunk.isEmpty then throw <| IO.userError "wgw-window stopped"
    out := out ++ chunk
  return out

private def readFrame (h : IO.FS.Handle) : IO (String × ByteArray) := do
  let part : IO ByteArray := do
    let n ← readExact h 4
    readExact h (n[0]!.toNat + n[1]!.toNat * 256 + n[2]!.toNat * 65536 + n[3]!.toNat * 16777216)
  let header ← part
  return (String.fromUTF8! header, ← part)

private def writeFrame (h : IO.FS.Handle) (header : Json) (body : ByteArray := .empty) : IO Unit := do
  let text := header.compress.toUTF8
  h.write (u32le text.size.toUInt32 ++ text ++ u32le body.size.toUInt32 ++ body)
  h.flush

private def spawn (args : Array String := #[]) : IO (IO.Process.Child ⟨.piped, .piped, .inherit⟩) := do
  let program := (← IO.getEnv "WGW_WINDOW").getD "wgw-window"
  -- the protocol this head speaks to wgw-window (docs/heads.typ)
  IO.Process.spawn { cmd := program, args := #["--protocol", "1"] ++ args, stdin := .piped, stdout := .piped }

/-- Save the figure, headless: an image (.png) or a video (.gif, .apng; .mp4, .webm or .mov with ffmpeg on the
PATH), drawn on the CPU by wgw's own renderer. width and height are in CSS pixels, scale pixels to each. A
function's figure (or a widget's) is drawn with its controls as set up, or as `controls` gives. A video plays the
animation through, or, for a figure that doesn't animate, turns it `spin` degrees (360) in `seconds` (4). -/
def save (f : Figure) (path : System.FilePath) (width : Nat := 800) (height : Nat := f.height) (scale : Float := 2)
    (background : Option String := none) (controls : List (String × Json) := []) (seconds : Option Float := none)
    (spin : Option Float := none) (fps : Option Float := none) : IO Unit := do
  let (kind, body) ← match f.source with
    | .file bytes => pure ("src", bytes)
    | .layers scene => pure ("scene", encode scene)
    | .callback scene =>
      let values := controls ++ (f.controls.map fun (k, c) => (k, start c)).filter (fun (k, _) => (controls.lookup k).isNone)
      pure ("scene", encode (← scene { values, picks := #[] }))
  let figure := Json.mkObj [("width", width), ("height", height), ("scale", Lean.toJson scale),
    ("background", match background with | some c => Json.str c | none => Json.null),
    ("up", f.up), ("view", Lean.toJson [f.view.1, f.view.2]), ("flat", false)]
  let video := Json.mkObj <| [("seconds", seconds), ("spin", spin), ("fps", fps)].filterMap fun (k, v) => v.map (k, Lean.toJson ·)
  let child ← spawn #["--serve"]
  writeFrame child.stdin (Json.mkObj [("type", "save"), ("request", 1), ("path", (← IO.FS.realPath ".") / path |>.toString),
    ("figure", figure), ("video", video), ("controls", Json.mkObj controls), ("body", kind)]) body
  let result ← (do
    let (header, _) ← readFrame child.stdout
    match header.splitOn " " with
    | "saved" :: _ => pure none
    | "failed" :: _ :: why => pure (some (" ".intercalate why))
    | _ => pure (some s!"unexpected {header}"))
  let (_, child) ← child.takeStdin
  discard child.wait
  if let some why := result then throw <| IO.userError s!"wgw: can't save {path}: {why}"

/-- Show the figure in a window, and wait until it's closed (answering its function meanwhile). -/
def display (f : Figure) : IO Unit := do
  let child ← spawn
  let (kind, body) := match f.source with
    | .file bytes => ("src", bytes)
    | .layers scene => ("scene", encode scene)
    | .callback _ => ("callback", .empty)
  let setup := Json.mkObj [("controls", Json.mkObj f.controls), ("frames", 1), ("fps", 30), ("loop", true),
    ("grid", false), ("keepPicks", false)]
  let options := Json.mkObj [("up", f.up), ("view", Lean.toJson [f.view.1, f.view.2]), ("pick", f.pick)]
  writeFrame child.stdin (Json.mkObj [("type", "figure"), ("id", "1"), ("title", f.title), ("options", options),
    ("setup", setup), ("height", f.height), ("body", kind)]) body
  repeat
    let (header, body) ← readFrame child.stdout
    match header.splitOn " " with
    | ["closed", _] => break
    | ["scene", request, _] =>
      let request := request.toNat!
      let .callback scene := f.source | pure ()
      try
        let patch := encode (← scene (f.state (String.fromUTF8! body)))
        writeFrame child.stdin (Json.mkObj [("type", "scene"), ("request", request)]) patch
      catch e =>
        IO.eprintln s!"wgw: {e}"
        writeFrame child.stdin (Json.mkObj [("type", "error"), ("request", request), ("message", toString e)])
    | _ => pure ()
  let (_, child) ← child.takeStdin  -- closing its stdin ends it
  discard child.wait
end Figure

end Wgw
