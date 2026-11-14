# wgw for R: interactive 3D figures of arrays and wgw widgets, in a window.
#
# A thin head: layers become the patches wgw's page code reads, and wgw-window (a small program in wgw's
# repository, found as WGW_WINDOW or on the PATH) shows them, with the page code and its WebAssembly core in a
# webview, calling your function when a slider moves or a point is picked. Drawing, turning, the controls and
# widgets are wgw's own.
#
#   wgw_show(wgw_figure("torus.wgw"))                                  # a widget (Rust, compiled to wasm), or a mesh
#   wgw_figure(wgw_mesh(vertices, faces, values = k))                  # arrays: vertices n x 3, faces m x 3 (1-based)
#   wgw_figure(function(s) list(mesh = wgw_mesh(vertices * s$t, faces)), list(t = wgw_slider(0.5, 2, 1)))
#
# A figure shows when printed, and showing waits until its window is closed. Arrays with one more leading axis
# than usual (frames x n x 3 vertices, frames x n values) are an animation.

# ---- layers

wgw_layer <- function(type, arrays, values = NULL, range = NULL, ...) {
  if (!is.null(values)) arrays$values <- values
  params <- list(...)
  if (!is.null(range)) { params$min <- range[1]; params$max <- range[2] }
  structure(list(type = type, arrays = arrays[!vapply(arrays, is.null, TRUE)], params = params), class = "wgw_layer")
}

#' A triangle mesh: n x 3 vertices, m x 3 faces (1-based). Parameters: values (per vertex), color, colors,
#' range = c(min, max), flat, pane, pickable, visible.
wgw_mesh <- function(vertices, faces, ...) wgw_layer("mesh", list(vertices = vertices, faces = faces - 1), ...)
#' Points, drawn as dots: n x 3. Parameters: values, color, colors, range, size, pane, pickable, visible.
wgw_points <- function(vertices, ...) wgw_layer("points", list(vertices = vertices), ...)
#' A curve through n x 3 vertices, or the k x 2 segments given (1-based).
wgw_lines <- function(vertices, segments = NULL, ...)
  wgw_layer("lines", list(vertices = vertices, segments = if (!is.null(segments)) segments - 1), ...)
#' Arrows from n x 3 origins along n x 3 vectors.
wgw_vectors <- function(origins, vectors, ...) wgw_layer("vectors", list(vertices = origins, vectors = vectors), ...)

# A scene as layers by name: a named list (NULL removes a layer), or one layer, named by its type.
wgw_layers <- function(scene) if (inherits(scene, "wgw_layer")) setNames(list(scene), scene$type) else scene

#' A scene as the patch bytes wgw's page reads: "WGW1", the header's length, a JSON header, the arrays (row-major).
wgw_encode <- function(scene) {
  scene <- wgw_layers(scene)
  data <- rawConnection(raw(0), "wb")
  on.exit(close(data))
  header <- list()
  for (name in names(scene)) {
    l <- scene[[name]]
    if (is.null(l)) { header[[length(header) + 1]] <- list(name, NULL); next }
    arrays <- list()
    for (key in names(l$arrays)) {
      a <- l$arrays[[key]]
      shape <- if (is.null(dim(a))) length(a) else dim(a)
      rowmajor <- if (is.null(dim(a))) as.vector(a) else as.vector(aperm(a, rev(seq_along(dim(a)))))
      ints <- key %in% c("faces", "segments")
      arrays[[key]] <- list(dtype = if (ints) "u4" else "f4", shape = I(shape), offset = seek(data))
      writeBin(if (ints) as.integer(rowmajor) else as.numeric(rowmajor), data, size = 4, endian = "little")
    }
    header[[length(header) + 1]] <- list(name, list(type = l$type, params = l$params, remove = I(list()), arrays = arrays))
  }
  text <- charToRaw(wgw_json(list(layers = header)))
  text <- c(text, rep(charToRaw(" "), (-(8 + length(text))) %% 4))  # the arrays start 4-byte aligned
  c(charToRaw("WGW1"), u32(length(text)), text, rawConnectionValue(data))
}

u32 <- function(n) writeBin(as.integer(n), raw(), size = 4, endian = "little")

# ---- controls and state

#' A slider from min to max, starting at value; log moves evenly in powers of ten.
wgw_slider <- function(min, max, value, step = NULL, log = FALSE)
  structure(list(spec = c(list(min, max, value), if (!is.null(step)) list(step), if (log) list("log"))), class = "wgw_slider")

# A state: the controls' values by name (s$t, s[["t"]]), and its picks (wgw_pick).
`[[.wgw_state` <- function(x, name) {
  values <- unclass(x)
  if (!name %in% names(values))
    stop(sprintf("no control named \"%s\"; the controls are %s", name, paste(sprintf("\"%s\"", names(values)), collapse = ", ")))
  values[[name]]
}
`$.wgw_state` <- function(x, name) x[[name]]

#' The latest pick (a list of vertex, 1-based, frame, point and layer), or NULL.
wgw_pick <- function(s) { picks <- attr(s, "picks"); if (length(picks)) picks[[length(picks)]] }

# ---- figures

#' A figure: a .wgw file's path (a widget, or a mesh); its layers (a named list, or one layer); or a function of a
#' state returning them, called again whenever a control moves or a point is picked, with `controls` (sliders, and
#' TRUE or FALSE for checkboxes) by name. up is "x", "y" or "z"; view is c(turn, tilt) in degrees; pick is "click",
#' "hover" or "none"; height is in CSS pixels.
wgw_figure <- function(scene, controls = list(), title = "wgw", up = "z", view = c(-60, 20), pick = "click", height = 420) {
  if (is.character(scene)) {
    title <- basename(scene)
    scene <- readBin(scene, "raw", file.size(scene))
  }
  structure(list(scene = scene, controls = controls, title = title, height = height,
                 options = list(up = up, view = I(view), pick = pick)), class = "wgw_figure")
}

print.wgw_figure <- function(x, ...) { wgw_show(x); invisible(x) }

# wgw-window's state lines: `control <value> <name>`, `pick <vertex> <frame> <x> <y> <z> <layer>`.
wgw_state <- function(fig, lines) {
  values <- lapply(fig$controls, function(c) if (inherits(c, "wgw_slider")) c$spec[[3]] else isTRUE(c))
  picks <- list()
  for (line in strsplit(lines, "\n", fixed = TRUE)[[1]]) {
    word <- strsplit(line, " ", fixed = TRUE)[[1]]
    if (length(word) >= 3 && word[1] == "control") {
      values[[paste(word[-(1:2)], collapse = " ")]] <- if (word[2] %in% c("true", "false")) word[2] == "true" else as.numeric(word[2])
    } else if (length(word) >= 7 && word[1] == "pick") {
      picks[[length(picks) + 1]] <- list(vertex = as.integer(word[2]) + 1L, frame = as.integer(word[3]),
                                         point = as.numeric(word[4:6]), layer = paste(word[-(1:6)], collapse = " "))
    }
  }
  structure(values, picks = picks, class = "wgw_state")
}

#' Show a figure in a window, and wait until it's closed, answering its function meanwhile.
wgw_show <- function(fig) {
  con <- wgw_connect()
  on.exit(close(con))  # closing it ends wgw-window

  kind <- if (is.raw(fig$scene)) "src" else if (is.function(fig$scene)) "callback" else "scene"
  body <- switch(kind, src = fig$scene, scene = wgw_encode(fig$scene), callback = raw(0))
  setup <- list(controls = setNames(lapply(fig$controls, function(c) if (inherits(c, "wgw_slider")) c$spec else isTRUE(c)), names(fig$controls)),
                frames = 1, fps = 30, loop = TRUE, grid = FALSE, keepPicks = FALSE)
  if (!length(fig$controls)) setup$controls <- structure(list(), names = character())
  write_frame(con, list(type = "figure", id = "1", title = fig$title, options = fig$options, setup = setup,
                        height = fig$height, body = kind), body)
  repeat {
    frame <- read_frame(con)
    if (is.null(frame)) break
    word <- strsplit(frame$header, " ", fixed = TRUE)[[1]]
    if (word[1] == "closed") break
    if (word[1] == "scene") {
      request <- as.numeric(word[2])
      patch <- tryCatch(wgw_encode(fig$scene(wgw_state(fig, rawToChar(frame$body)))), error = function(e) e)
      if (inherits(patch, "error")) {
        message("wgw: ", conditionMessage(patch))
        write_frame(con, list(type = "error", request = request, message = conditionMessage(patch)))
      } else {
        write_frame(con, list(type = "scene", request = request), patch)
      }
    }
  }
  invisible(NULL)
}

# wgw-window, started and connected back to us (R can't pipe both ways to a program); serving only, it opens no
# windows.
wgw_connect <- function(serve = FALSE) {
  program <- Sys.getenv("WGW_WINDOW", unset = Sys.which("wgw-window"))
  if (!nzchar(program)) stop("wgw needs wgw-window: in wgw's repository, run ./build.sh, then cargo install --path window")
  for (try in 1:20) {
    port <- sample(20000:60000, 1)
    server <- tryCatch(serverSocket(port), error = function(e) NULL)
    if (!is.null(server)) break
  }
  system2(program, c("--protocol", "1", if (serve) "--serve", "--connect", port), wait = FALSE)  # protocol: docs/heads.typ
  con <- socketAccept(server, blocking = TRUE, open = "r+b", timeout = 30)
  close(server)
  con
}

#' Save a figure, headless: an image (.png) or a video (.gif, .apng; .mp4, .webm or .mov with ffmpeg on the PATH),
#' drawn on the CPU by wgw's own renderer. width and height are in CSS pixels, scale pixels to each. A function's
#' figure (or a widget's) is drawn with its controls as set up, or as `controls` (a named list) gives. A video plays
#' the animation through, or, for a figure that doesn't animate, turns it `spin` degrees (360) in `seconds` (4).
wgw_save <- function(fig, path, width = 800, height = fig$height, scale = 2, background = NULL, controls = list(),
                     fps = NULL, frames = NULL, seconds = NULL, spin = NULL) {
  values <- lapply(fig$controls, function(c) if (inherits(c, "wgw_slider")) c$spec[[3]] else isTRUE(c))
  values[names(controls)] <- controls
  kind <- if (is.raw(fig$scene)) "src" else "scene"
  body <- if (is.raw(fig$scene)) fig$scene else if (is.function(fig$scene))
    wgw_encode(fig$scene(structure(values, picks = list(), class = "wgw_state"))) else wgw_encode(fig$scene)
  figure <- list(width = width, height = height, scale = scale, background = background, up = fig$options$up,
                 view = fig$options$view, flat = FALSE)
  video <- Filter(Negate(is.null), list(fps = fps, frames = frames, seconds = seconds, spin = spin))
  if (!length(video)) video <- structure(list(), names = character())
  if (!length(controls)) controls <- structure(list(), names = character())
  con <- wgw_connect(serve = TRUE)
  on.exit(close(con))
  write_frame(con, list(type = "save", request = 1, path = normalizePath(path, mustWork = FALSE), figure = figure,
                        video = video, controls = controls, body = kind), body)
  repeat {
    frame <- read_frame(con)
    if (is.null(frame)) stop("wgw-window stopped")
    word <- strsplit(frame$header, " ", fixed = TRUE)[[1]]
    if (word[1] == "saved") return(invisible(path))
    if (word[1] == "failed") stop("wgw: can't save ", path, ": ", paste(word[-(1:2)], collapse = " "))
  }
}

write_frame <- function(con, header, body = raw(0)) {
  text <- charToRaw(enc2utf8(wgw_json(header)))
  writeBin(c(u32(length(text)), text, u32(length(body)), body), con)
  flush(con)
}

read_frame <- function(con) {
  part <- function() {
    n <- readBin(con, "integer", size = 4, endian = "little")
    if (!length(n)) return(NULL)
    out <- raw(0)
    while (length(out) < n) {
      got <- readBin(con, "raw", n - length(out))
      if (!length(got)) return(NULL)
      out <- c(out, got)
    }
    out
  }
  header <- part()
  if (is.null(header)) return(NULL)
  body <- part()
  list(header = rawToChar(header), body = if (is.null(body)) raw(0) else body)
}

# ---- JSON, written (wgw-window's own messages are lines), so the head needs no packages. I() keeps a vector of
# one as an array.

wgw_json <- function(x) {
  if (is.null(x)) return("null")
  if (is.list(x)) {
    if (!is.null(names(x)) || (length(x) == 0 && !inherits(x, "AsIs") && !is.null(attr(x, "names"))))
      return(paste0("{", paste(sprintf("%s:%s", vapply(names(x), wgw_json, ""), vapply(unclass(x), wgw_json, "")), collapse = ","), "}"))
    return(paste0("[", paste(vapply(unclass(x), wgw_json, ""), collapse = ","), "]"))
  }
  one <- function(v) {
    if (is.character(v)) return(paste0("\"", gsub("([\"\\\\])", "\\\\\\1", gsub("\n", "\\\\n", v)), "\""))
    if (is.logical(v)) return(if (is.na(v)) "null" else if (v) "true" else "false")
    if (!is.finite(v)) return("null")
    sprintf("%.17g", v)
  }
  items <- vapply(as.list(x), one, "")
  if (length(x) == 1 && !inherits(x, "AsIs")) items else paste0("[", paste(items, collapse = ","), "]")
}
