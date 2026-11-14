"""
wgw for Julia: interactive 3D figures of arrays and wgw widgets, in a window (or inline in Jupyter and Pluto).

A thin head: layers become the patches wgw's page code reads, and wgw-window (a small program in wgw's repository)
shows them, with the page code and its WebAssembly core in a webview, calling your function when a slider moves or
a point is picked. Drawing, turning, the controls and widgets are wgw's own.

    using Wgw
    display(Figure("torus.wgw"))                                    # a widget (Rust, compiled to wasm), or a mesh file
    display(Figure(Mesh(vertices, faces; values = k)))              # arrays: vertices n×3, faces m×3 (1-based)
    display(Figure(s -> Dict("mesh" => Mesh(vertices .* s["t"], faces)), "t" => Slider(0.5, 2, 1)))

Arrays with one more leading axis than usual (frames×n×3 vertices, frames×n values) are an animation.
"""
module Wgw

export Figure, Mesh, Points, Lines, Vectors, Slider, State, pick, encode, savefig

# ---- layers

"""A layer: its type ("mesh", "points", "lines" or "vectors"), arrays and parameters (see wgw's docs)."""
struct Layer
    type::String
    arrays::Vector{Pair{String,AbstractArray}}
    params::Dict{String,Any}
end

function Layer(type, arrays; values = nothing, range = nothing, params...)
    arrays = Pair{String,AbstractArray}[k => a for (k, a) in arrays if a !== nothing]
    values === nothing || push!(arrays, "values" => values)
    p = Dict{String,Any}(string(k) => v for (k, v) in params)
    range === nothing || ((p["min"], p["max"]) = range)
    Layer(type, arrays, p)
end

"""A triangle mesh: n×3 vertices and m×3 faces (1-based). Keywords: values (per vertex), color, colors,
range = (min, max), flat, pane, pickable, visible."""
Mesh(vertices, faces; kw...) = Layer("mesh", ["vertices" => vertices, "faces" => faces .- 1]; kw...)

"""Points, drawn as dots: n×3. Keywords: values, color, colors, range, size, pane, pickable, visible."""
Points(vertices; kw...) = Layer("points", ["vertices" => vertices]; kw...)

"""A curve through n×3 vertices, or the k×2 segments given (1-based). Keywords: values, color, colors, range,
width, pane, pickable, visible."""
Lines(vertices, segments = nothing; kw...) =
    Layer("lines", ["vertices" => vertices, "segments" => segments === nothing ? nothing : segments .- 1]; kw...)

"""Arrows from n×3 origins along n×3 vectors. Keywords: values, color, colors, range, width, pane, pickable,
visible."""
Vectors(origins, vectors; kw...) = Layer("vectors", ["vertices" => origins, "vectors" => vectors]; kw...)

# A scene as layers by name: a Dict (nothing removes a layer), or one layer, named by its type.
layers(scene::Layer) = [scene.type => scene]
layers(scene::AbstractDict) = [string(k) => v for (k, v) in scene]
layers(::Nothing) = []

"""A scene as the patch bytes wgw's page reads: "WGW1", the header's length, a JSON header, the arrays (row-major:
each array as it reads in Julia, rows first)."""
function encode(scene)
    header, data = Any[], IOBuffer()
    for (name, layer) in layers(scene)
        if layer === nothing
            push!(header, Any[name, nothing])
            continue
        end
        arrays = Dict{String,Any}()
        for (key, a) in layer.arrays
            ints = key in ("faces", "segments")
            rowmajor = vec(permutedims(a, ndims(a):-1:1))
            arrays[key] = Dict("dtype" => ints ? "u4" : "f4", "shape" => collect(size(a)), "offset" => position(data))
            write(data, htol.(ints ? UInt32.(rowmajor) : Float32.(rowmajor)))
        end
        push!(header, Any[name, Dict("type" => layer.type, "params" => layer.params, "remove" => [], "arrays" => arrays)])
    end
    text = Vector{UInt8}(json(Dict("layers" => header)))
    append!(text, fill(UInt8(' '), mod(-(8 + length(text)), 4)))  # the arrays start 4-byte aligned
    vcat(Vector{UInt8}("WGW1"), reinterpret(UInt8, [htol(UInt32(length(text)))]), text, take!(data))
end

# ---- controls and state

"""A slider from min to max, starting at value; log moves evenly in powers of ten."""
struct Slider
    spec::Vector{Any}
end
Slider(min, max, value; step = nothing, log = false) =
    Slider(Any[min, max, value, (step === nothing ? [] : [step])..., (log ? ["log"] : [])...])

"""What a scene is for: the controls' values by name (state["t"]), and the picks so far, the latest last, each a
Dict with "vertex" and "frame" (both 1-based), "layer" and "point"."""
struct State
    values::Dict{String,Any}
    picks::Vector{Any}
end
Base.getindex(s::State, name) = haskey(s.values, string(name)) ? s.values[string(name)] :
    error("no control named $(repr(string(name))); the controls are $(join(repr.(sort(collect(keys(s.values)))), ", "))")

"""The latest pick, or nothing."""
pick(s::State) = isempty(s.picks) ? nothing : s.picks[end]

# ---- figures

"""
    Figure(scene, controls...; title, up, view, pick, height, fps, loop, grid, keep_picks, flat)

A figure. `scene` is a .wgw file's path or bytes (a widget, or a mesh); its layers (a Dict by name, or one layer);
or a function of a State returning them, called again whenever a control moves or a point is picked. `controls`
are name => Slider or name => true/false (a checkbox). up is "x", "y" or "z"; view is (turn, tilt) in degrees, or
(turn, tilt, scale); pick is "click", "hover" or "none"; height is in CSS pixels.
"""
mutable struct Figure
    scene::Any
    controls::Vector{Pair{String,Any}}
    title::String
    options::Dict{String,Any}
    setup::Dict{String,Any}
    height::Int
    urls::Dict{Bool,String}
    closed::Base.Event
end

function Figure(scene, controls::Pair...; title = "wgw", up = "z", view = (-60, 20), pick = "click", height = 420,
                fps = 30, loop = true, grid = false, keep_picks = false, flat = false)
    if scene isa AbstractString
        title, scene = basename(scene), read(scene)
    end
    controls = Pair{String,Any}[string(k) => c for (k, c) in controls]
    setup = Dict{String,Any}("controls" => Dict(k => c isa Slider ? c.spec : Bool(c) for (k, c) in controls),
        "frames" => 1, "fps" => fps, "loop" => loop, "grid" => grid, "keepPicks" => keep_picks)
    options = Dict{String,Any}("up" => up, "view" => collect(view), "pick" => pick, "flat" => flat)
    Figure(scene, controls, title, options, setup, height, Dict(), Base.Event())
end

"""The patch's bytes for a state, as wgw-window sends it: `control <value> <name>` and
`pick <vertex> <frame> <x> <y> <z> <layer>` lines."""
function render(fig::Figure, lines::AbstractString)
    values = Dict{String,Any}(k => c isa Slider ? c.spec[3] : Bool(c) for (k, c) in fig.controls)
    picks = Any[]
    for line in split(lines, '\n'; keepempty = false)
        word = split(line, ' '; limit = 7)
        if word[1] == "control"
            values[join(word[3:end], ' ')] = word[2] in ("true", "false") ? word[2] == "true" : parse(Float64, word[2])
        elseif word[1] == "pick"
            push!(picks, Dict("vertex" => parse(Int, word[2]) + 1, "frame" => parse(Int, word[3]) + 1,
                              "point" => parse.(Float64, word[4:6]), "layer" => String(word[7])))
        end
    end
    encode(fig.scene(State(values, picks)))
end

function send(fig::Figure, serve::Bool)
    get!(fig.urls, serve) do
        kind, body = fig.scene isa AbstractVector{UInt8} ? ("src", fig.scene) :
                     fig.scene isa Function ? ("callback", UInt8[]) : ("scene", encode(fig.scene))
        header = Dict("type" => "figure", "title" => fig.title, "options" => fig.options, "setup" => fig.setup,
                      "height" => fig.height, "body" => kind)
        figure(window(serve), fig, header, body)
    end
end

"""
    savefig(fig, path; width = 800, height, scale = 2, background, controls, fps, frames, seconds, spin)

Save the figure, headless: an image (.png) or a video (.gif, .apng; .mp4, .webm or .mov with ffmpeg on the PATH),
drawn on the CPU by wgw's own renderer. width and height are in CSS pixels, scale pixels to each. A function's
figure (or a widget's) is drawn with its controls as set up, or as `controls` (name => value) gives. A video plays
the animation through, or, for a figure that doesn't animate, turns it `spin` degrees (360) in `seconds` (4).
"""
function savefig(fig::Figure, path; width = 800, height = fig.height, scale = 2, background = nothing,
                 controls = Dict(), fps = nothing, frames = nothing, seconds = nothing, spin = nothing)
    controls = Dict{String,Any}(string(k) => v for (k, v) in controls)
    kind, body = if fig.scene isa AbstractVector{UInt8}
        "src", fig.scene
    elseif fig.scene isa Function
        values = Dict{String,Any}(k => c isa Slider ? c.spec[3] : Bool(c) for (k, c) in fig.controls)
        "scene", encode(fig.scene(State(merge(values, controls), [])))
    else
        "scene", encode(fig.scene)
    end
    figure = Dict("width" => width, "height" => height, "scale" => scale, "background" => background,
                  "up" => fig.options["up"], "view" => fig.options["view"], "flat" => fig.options["flat"])
    video = Dict(k => v for (k, v) in ("fps" => fps, "frames" => frames, "seconds" => seconds, "spin" => spin) if v !== nothing)
    w = window(true)
    request = lock(() -> w.next += 1, w.lock)
    w.saves[request] = done = Channel{Union{Nothing,String}}(1)
    write_frame(w, Dict("type" => "save", "request" => request, "path" => abspath(path), "figure" => figure,
                        "video" => video, "controls" => controls, "setup" => fig.setup, "body" => kind), body)
    failed = take!(done)
    failed === nothing || error("wgw: can't save $path: $failed")
    path
end

# Inline in Jupyter (IJulia) and Pluto: served, in an iframe.
function Base.show(io::IO, ::MIME"text/html", fig::Figure)
    bar = isempty(fig.controls) ? 0 : 40
    print(io, """<iframe src="$(send(fig, true))" style="width:100%;height:$(fig.height + bar)px;border:0" title="$(fig.title)"></iframe>""")
end
Base.show(io::IO, fig::Figure) = print(io, "Wgw.Figure(", repr(fig.title), ")")

"""
    display(fig::Figure; block)

Show the figure in a window, which closes when the program ends (in Jupyter or Pluto, inline). A script (not an
interactive session) waits here until the window is closed.
"""
function Base.display(fig::Figure; block = !isinteractive())
    if isdefined(Main, :IJulia) || isdefined(Main, :PlutoRunner)
        return display(MIME"text/html"(), sprint(show, MIME"text/html"(), fig))
    end
    send(fig, false)
    block && wait(fig.closed)
    nothing
end

# ---- wgw-window: started once (for windows, or serving for Jupyter), spoken to in frames over its stdin/stdout

function program()
    found = get(ENV, "WGW_WINDOW", nothing)
    found === nothing && (found = Sys.which("wgw-window"))
    built = joinpath(@__DIR__, "..", "..", "..", "target", "release", "wgw-window")  # in wgw's repository
    found !== nothing && return found
    isfile(built) && return built
    error("wgw needs wgw-window: in wgw's repository, run ./build.sh, then cargo install --path window (or set WGW_WINDOW)")
end

mutable struct Window
    process::Base.Process
    figures::Dict{String,Figure}
    urls::Dict{String,Channel{String}}
    saves::Dict{Int,Channel{Union{Nothing,String}}}
    lock::ReentrantLock
    next::Int
end

const windows = Dict{Bool,Window}()
const PROTOCOL = "1"  # what this head speaks to wgw-window (docs/heads.typ)

function window(serve::Bool)
    get!(windows, serve) do
        process = open(serve ? `$(program()) --protocol $PROTOCOL --serve` : `$(program()) --protocol $PROTOCOL`, "r+")
        w = Window(process, Dict(), Dict(), Dict(), ReentrantLock(), 0)
        errormonitor(@async listen(w))
        w
    end
end

function write_frame(w::Window, header, body = UInt8[])
    text = Vector{UInt8}(json(header))
    lock(w.lock) do
        write(w.process, htol(UInt32(length(text))), text, htol(UInt32(length(body))), body)
        flush(w.process)
    end
end

function read_frame(w::Window)
    part() = String(read(w.process, ltoh(read(w.process, UInt32))))
    split(part(), ' '; limit = 3), part()
end

function figure(w::Window, fig::Figure, header, body)
    id = lock(() -> string(w.next += 1), w.lock)
    w.figures[id] = fig
    w.urls[id] = got = Channel{String}(1)
    write_frame(w, merge(header, Dict("id" => id)), body)
    take!(got)
end

# "url <id> <url>", "closed <id>", "scene <request> <id>" with the state, "saved <request>", "failed <request> <why>".
function listen(w::Window)
    while !eof(w.process)
        words, body = read_frame(w)
        if words[1] == "url"
            put!(w.urls[words[2]], words[3])
        elseif words[1] in ("saved", "failed")
            put!(pop!(w.saves, parse(Int, words[2])), words[1] == "failed" ? String(words[3]) : nothing)
        elseif words[1] == "closed"
            notify(w.figures[words[2]].closed)
        elseif words[1] == "scene"
            @async answer(w, parse(Int, words[2]), String(words[3]), body)
        end
    end
end

function answer(w::Window, request, id, state)
    body = try
        render(w.figures[id], state)
    catch err
        showerror(stderr, err, catch_backtrace())
        println(stderr)
        return write_frame(w, Dict("type" => "error", "request" => request, "message" => sprint(showerror, err)))
    end
    write_frame(w, Dict("type" => "scene", "request" => request), body)
end

# ---- JSON, written (wgw-window's own messages are lines), so the head needs no packages

json(x) = sprint(writejson, x)
writejson(io, x::AbstractString) = (print(io, '"'); for c in x
    c == '"' ? print(io, "\\\"") : c == '\\' ? print(io, "\\\\") : c < ' ' ? print(io, "\\u", string(UInt16(c), base = 16, pad = 4)) : print(io, c)
end; print(io, '"'))
writejson(io, x::Bool) = print(io, x)
writejson(io, x::Real) = isfinite(x) ? print(io, isinteger(x) && abs(x) < 2^53 ? Int(x) : Float64(x)) : print(io, "null")
writejson(io, ::Nothing) = print(io, "null")
writejson(io, x::Union{AbstractVector,Tuple}) = (print(io, '['); join(io, (json(v) for v in x), ','); print(io, ']'))
writejson(io, x::AbstractDict) = (print(io, '{'); join(io, (json(string(k)) * ":" * json(v) for (k, v) in x), ','); print(io, '}'))
writejson(io, x::Symbol) = writejson(io, string(x))

end
