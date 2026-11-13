// The README's banner: typst compile assets/banner.typ assets/banner-light.svg, and with --input theme=dark,
// assets/banner-dark.svg. (Libertinus Serif ships with Typst, so the banner is the same wherever it's built.)
#let dark = sys.inputs.at("theme", default: "light") == "dark"
#let icon = read("logo.svg")
// The icon's colors for a dark page (an SVG image is drawn with its light colors).
#let icon = if dark { for (light, dark) in (("#af3029", "#d14d41"),) { icon = icon.replace(light, dark, count: 1) }; icon } else { icon }
#set page(width: auto, height: auto, margin: (x: 4pt, y: 8pt), fill: none)
#set text(font: "Libertinus Serif", size: 40pt, weight: "semibold", fill: if dark { rgb("#cecdc3") } else { rgb("#100f0f") })
#box(image(bytes(icon), height: 56pt), baseline: 22%)#h(10pt)Web Geometry Widgets
