import Wgw
open Wgw Lean

/-- The Lean head against tests/heads/expected.json: `lake exe wgw-test <out dir>`. -/
def main (args : List String) : IO Unit := do
  let out : System.FilePath := args.headD "."
  let scene : Scene := [("m", mesh #[(0, 0, 0), (1, 0, 0), (0, 1, 0)] #[(0, 1, 2)] |>.values #[0, 0.5, 1] |>.color "red"),
    ("p", points #[(0, 0, 1)] |>.size 4)]
  IO.FS.writeBinFile (out / "lean.patch") (encode scene)

  let fig := (Figure.callback (fun _ => pure scene)).slider "t" 0 5 1 |>.checkbox "my box" false
  let s := fig.state "control 2.5 t\ncontrol true my box\npick 3 1 0.5 0 1 the mesh\n"
  unless (← s.value "t") == 2.5 && (← s.checked "my box") do throw <| IO.userError "the controls"
  let some p := s.pick | throw <| IO.userError "no pick"
  unless p.vertex == 3 && p.frame == 1 && p.point == (0.5, 0, 1) && p.layer == "the mesh" do
    throw <| IO.userError "the pick"

  fig.save (out / "lean.png") (width := 100) (height := 80) (scale := 1)
  let png ← IO.FS.readBinFile (out / "lean.png")
  unless png.extract 0 4 == ⟨#[0x89, 0x50, 0x4e, 0x47]⟩ do throw <| IO.userError "not a PNG"
