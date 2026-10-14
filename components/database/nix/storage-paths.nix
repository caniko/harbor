{lib}: {
  normalizedAbsolute = path:
    path
    == "/"
    || (lib.hasPrefix "/" path
      && lib.all (part: !(lib.elem part ["" "." ".."])) (lib.tail (lib.splitString "/" path)));
  disjoint = paths: let
    contains = root: path: root == "/" || root == path || lib.hasPrefix "${root}/" path;
  in
    lib.length paths
    == lib.length (lib.unique paths)
    && lib.all (root: lib.all (other: root == other || !(contains root other || contains other root)) paths) paths;
}
