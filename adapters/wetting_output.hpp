// Shared filesystem precondition for standalone and worker-owned wetting runs.
#pragma once
#include <filesystem>
#include <stdexcept>

inline void verify_wetting_output_directory(const std::filesystem::path& root) {
  for (const auto& entry : std::filesystem::directory_iterator(root)) {
    const auto name = entry.path().filename();
    if ((name != "process.log" && name != "wetting.log")
        || !std::filesystem::is_regular_file(entry.symlink_status())
        || std::filesystem::hard_link_count(entry.path()) != 1
        || (name == "wetting.log" && std::filesystem::file_size(entry.path()) != 0)) {
      throw std::runtime_error("fresh isolated wetting directory with only distinct regular process log and empty worker log required");
    }
  }
}
