// Hardware-independent test of the exact header used by the native driver.
#include "../adapters/wetting_output.hpp"
#include <fstream>
#include <iostream>

int main(int argc, char** argv) {
  if (argc != 2) return 2;
  const std::filesystem::path root(argv[1]);
  std::filesystem::create_directory(root);
  const auto accepts = [&] {
    try { verify_wetting_output_directory(root); return true; }
    catch (const std::runtime_error&) { return false; }
  };
  if (!accepts()) return 3;
  std::ofstream(root / "process.log") << "native startup output\n";
  std::ofstream(root / "wetting.log");
  if (!accepts()) return 4;
  std::ofstream(root / "wetting.log") << "stale worker output";
  if (accepts()) return 5;
  std::ofstream(root / "wetting.log", std::ios::trunc);
  std::ofstream(root / "wetting-0.csv");
  if (accepts()) return 6;
  std::filesystem::remove(root / "wetting-0.csv");
  std::filesystem::create_hard_link(root / "wetting.log", root.parent_path() / "aliased-worker.log");
  if (accepts()) return 7;
  std::filesystem::remove(root.parent_path() / "aliased-worker.log");
  std::filesystem::remove(root / "wetting.log");
  std::filesystem::create_symlink("process.log", root / "wetting.log");
  if (accepts()) return 8;
  std::filesystem::remove(root / "wetting.log");
  std::filesystem::create_directory(root / "wetting.log");
  if (accepts()) return 9;
  std::filesystem::remove(root / "wetting.log");
  std::filesystem::create_hard_link(root / "process.log", root.parent_path() / "aliased-process.log");
  if (accepts()) return 10;
  std::filesystem::remove(root.parent_path() / "aliased-process.log");
  std::filesystem::remove(root / "process.log");
  std::filesystem::create_symlink("absent", root / "process.log");
  if (accepts()) return 11;
  std::filesystem::remove(root / "process.log");
  if (!accepts()) return 12;
  std::cout << "native directory guard: valid worker/standalone layouts and seven rejections passed\n";
}
