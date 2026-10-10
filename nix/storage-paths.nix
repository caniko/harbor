{lib}: {
  normalizedAbsolute = path:
    path
    == "/"
    || (lib.hasPrefix "/" path
      && lib.all (part: !(lib.elem part ["" "." ".."])) (lib.tail (lib.splitString "/" path)));
}
