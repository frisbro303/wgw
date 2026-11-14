# The R head against expected.json: Rscript r.R <out dir>. Faces and picks count from 1.
args <- commandArgs(TRUE)
here <- dirname(normalizePath(sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE))))
source(file.path(here, "..", "..", "r", "wgw", "R", "wgw.R"))
out <- args[1]
scene <- list(m = wgw_mesh(matrix(c(0, 0, 0, 1, 0, 0, 0, 1, 0), 3, byrow = TRUE), matrix(1:3, 1), values = c(0, 0.5, 1), color = "red"),
              p = wgw_points(matrix(c(0, 0, 1), 1), size = 4))
writeBin(wgw_encode(scene), file.path(out, "r.patch"))

fig <- wgw_figure(function(s) scene, list(t = wgw_slider(0, 5, 1), `my box` = FALSE))
s <- wgw_state(fig, "control 2.5 t\ncontrol true my box\npick 3 1 0.5 0 1 the mesh\n")
stopifnot(s$t == 2.5, isTRUE(s[["my box"]]))
stopifnot(identical(wgw_pick(s), list(vertex = 4L, frame = 1L, point = c(0.5, 0, 1), layer = "the mesh")))

wgw_save(fig, file.path(out, "r.png"), width = 100, height = 80, scale = 1)
stopifnot(identical(readBin(file.path(out, "r.png"), "raw", 4), as.raw(c(0x89, 0x50, 0x4e, 0x47))))
