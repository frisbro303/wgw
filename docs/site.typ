// wgw's docs: a tdg site (github.com/frisbro303/tdg), built by Typst alone. From the repository's root:
//
//   typst compile --root . --features html,bundle --format bundle docs/site.typ docs/_site
#import "@local/tdg:0.2.0": *

#show: site.with(
  title: "wgw",
  theme: "docs",
  url: "https://frisbro303.github.io/wgw/",
  icon: path("../assets/logo.svg"),
  nav: (("GitHub", "https://github.com/frisbro303/wgw"),),
  search: true,
)

#include "index.typ"
#include "figures.typ"
#include "widgets.typ"
#include "reference.typ"
#include "javascript.typ"
#include "internals.typ"
