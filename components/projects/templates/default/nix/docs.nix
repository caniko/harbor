# Replace `my-project` / `caniko/my-project` throughout after init.
{
  pkgs,
  harborProjects,
}: {
  docs = harborProjects.mkDocs {
    inherit pkgs;
    src = ../docs;
    pname = "my-project-docs";
  };
}
