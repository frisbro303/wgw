# The Julia head against expected.json: julia julia.jl <out dir>. Faces, and picks' vertices and frames, count from 1.
push!(LOAD_PATH, joinpath(@__DIR__, "..", "..", "heads", "julia"))
using Wgw
out = ARGS[1]
scene = Dict("m" => Mesh([0 0 0; 1 0 0; 0 1 0], [1 2 3]; values = [0, 0.5, 1], color = "red"),
             "p" => Points([0 0 1]; size = 4))
write(joinpath(out, "julia.patch"), encode(scene))

seen = []
fig = Figure(s -> (push!(seen, s); scene), "t" => Slider(0, 5, 1), "my box" => false)
Wgw.render(fig, "control 2.5 t\ncontrol true my box\npick 3 1 0.5 0 1 the mesh\n")
s = seen[1]
@assert s["t"] == 2.5 && s["my box"] == true
@assert pick(s) == Dict("vertex" => 4, "frame" => 2, "point" => [0.5, 0, 1], "layer" => "the mesh") pick(s)

savefig(fig, joinpath(out, "julia.png"); width = 100, height = 80, scale = 1)
@assert read(joinpath(out, "julia.png"))[1:4] == UInt8[0x89, 0x50, 0x4e, 0x47]
