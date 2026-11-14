import Wgw
open Wgw Lean

/-- A torus with tube radius r, n×n vertices. -/
def torus (n : Nat) (r : Float) : Array Vec3 × Array (Nat × Nat × Nat) := Id.run do
  let tau := 2 * 3.141592653589793
  let mut vs := #[]
  for i in [0:n] do
    for j in [0:n] do
      let u := tau * i.toFloat / n.toFloat
      let v := tau * j.toFloat / n.toFloat
      vs := vs.push ((1 + r * v.cos) * u.cos, (1 + r * v.cos) * u.sin, r * v.sin)
  let idx (i j : Nat) := (i % n) * n + j % n
  let mut fs := #[]
  for i in [0:n] do
    for j in [0:n] do
      fs := fs.push (idx i j, idx (i + 1) j, idx (i + 1) (j + 1)) |>.push (idx i j, idx (i + 1) (j + 1), idx i (j + 1))
  return (vs, fs)

/-- A torus whose tube the slider sets, colored by height. Run with `lake exe torus`. -/
def main : IO Unit := do
  let fig := Figure.callback (fun s => do
    let (vs, fs) := torus 60 (← s.value "r")
    pure [("mesh", mesh vs fs |>.values (vs.map (·.2.2)) |>.range (-0.5) 0.5)])
  let fig := { fig.slider "r" 0.1 0.9 0.4 with title := "torus from Lean" }
  if (← IO.getEnv "WGW_SAVE").isSome then  -- headless: a still with a thinner tube, and a turning video
    fig.save "torus-lean.png" (controls := [("r", Lean.toJson 0.2)])
    fig.save "torus-lean.gif" (width := 300) (height := 200) (seconds := some 1)
  else
    fig.display
