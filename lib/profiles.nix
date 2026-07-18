{
  # CourseOfLife's small CV documents. Keep this profile deliberately small
  # because it is also used by personal workstation builds.
  cv = {
    scheme = "scheme-basic";
    packages = [
      "latex-bin"
      "geometry"
      "titlesec"
      "enumitem"
      "hyperref"
      "xcolor"
      "pgf"
      "tools"
      "needspace"
      "microtype"
      "charter"
      "background"
      "xkeyval"
      "everypage"
      "bookmark"
    ];
  };

  # Shared scientific-article surface used by nix-article and SynDB.
  article = {
    scheme = "scheme-medium";
    packages = [
      "biblatex"
      "biber"
      "natbib"
      "amsmath"
      "amsfonts"
      "mathtools"
      "xcolor"
      "booktabs"
      "lm"
      "lm-math"
      "fontspec"
      "hyperref"
      "bookmark"
      "xurl"
      "microtype"
      "etoolbox"
      "soul"
      "lua-ul"
      "latexmk"
      "placeins"
      "cleveref"
      "standalone"
      "svg"
      "trimspaces"
      "transparent"
      "catchfile"
      "pgf"
      "siunitx"
      "caption"
      "geometry"
      "parskip"
      "setspace"
      "lineno"
      "tools"
      "babel"
      "csquotes"
      "psnfss"
      "times"
      "helvetic"
      "courier"
      "graphics"
      "algorithms"
      "algorithmicx"
      "newfloat"
      "eso-pic"
      "fancyhdr"
      "forloop"
      "inconsolata"
      "mathpazo"
      "todonotes"
    ];
  };

  # Conference templates are intentionally curated instead of pulling the
  # multi-gigabyte scheme-full distribution into every paper shell.
  conference = {
    scheme = "scheme-medium";
    packages = [
      "natbib"
      "amsmath"
      "amsfonts"
      "mathtools"
      "microtype"
      "booktabs"
      "hyperref"
      "url"
      "caption"
      "lineno"
      "xcolor"
      "geometry"
      "fancyhdr"
      "algorithmicx"
      "algorithms"
      "newfloat"
      "listings"
      "eso-pic"
      "forloop"
      "times"
      "helvetic"
      "courier"
      "inconsolata"
      "mathpazo"
      "psnfss"
      "todonotes"
    ];
  };

  # Interactive editor tooling plus the packages already present in Canix's
  # Home Manager profile.
  editor = {
    scheme = "scheme-medium";
    packages = [
      "latexmk"
      "latexindent"
      "chktex"
      "mathspec"
      "unicode-math"
      "biblatex"
      "natbib"
      "microtype"
      "parskip"
      "selnolig"
      "setspace"
      "soul"
      "ulem"
      "adjustbox"
      "collectbox"
      "enumitem"
      "geometry"
      "titling"
      "bookmark"
      "caption"
      "cleveref"
      "hyperref"
      "xurl"
      "fancyvrb"
      "listings"
      "upquote"
      "babel"
      "etoolbox"
      "footnotehyper"
      "mdwtools"
      "xcolor"
    ];
  };
}
