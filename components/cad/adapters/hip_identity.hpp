// Shared HIP identity checks for compute adapters. Host authorization and
// selected-node containment are enforced independently by the Rust worker.
#pragma once
#include <hip/hip_runtime.h>
#include <nlohmann/json.hpp>
#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <iomanip>
#include <sstream>
#include <stdexcept>

inline void hip_check(hipError_t status) {
  if (status != hipSuccess) throw std::runtime_error(hipGetErrorString(status));
}

inline nlohmann::json hip_inventory() {
  if (std::getenv("HSA_OVERRIDE_GFX_VERSION")) {
    throw std::runtime_error("HIP architecture spoofing is not an approved runtime");
  }
  int count = 0;
  hip_check(hipGetDeviceCount(&count));
  if (count <= 0 || count > 32) throw std::runtime_error("bounded HIP device inventory required");
  auto devices = nlohmann::json::array();
  for (int i = 0; i < count; ++i) {
    char pci[64]{};
    hipDeviceProp_t props{};
    hipUUID uuid{};
    hip_check(hipDeviceGetPCIBusId(pci, sizeof(pci), i));
    hip_check(hipGetDeviceProperties(&props, i));
    hip_check(hipDeviceGetUuid(&uuid, i));
    std::ostringstream encoded;
    encoded << "GPU-" << std::hex << std::setfill('0');
    bool nonzero = false;
    for (unsigned char byte : uuid.bytes) {
      nonzero |= byte != 0;
      encoded << std::setw(2) << static_cast<unsigned int>(byte);
    }
    if (!nonzero) throw std::runtime_error("HIP device UUID unavailable");
    std::string id(pci);
    std::transform(id.begin(), id.end(), id.begin(), [](unsigned char c) { return std::tolower(c); });
    devices.push_back({{"ordinal_diagnostic_only", i}, {"pci", id},
      {"backend_uuid", encoded.str()}, {"device_name", props.name},
      {"architecture", props.gcnArchName}, {"total_vram_bytes", props.totalGlobalMem}});
  }
  return devices;
}

inline nlohmann::json hip_select_device(const nlohmann::json& selection) {
  if (selection.is_null() || selection.at("backend") != "hip" || selection.at("role") != "compute") {
    throw std::runtime_error("explicit HIP compute PCI/UUID selection required");
  }
  nlohmann::json chosen;
  for (const auto& device : hip_inventory()) {
    if (device.at("pci") == selection.at("pci")) {
      if (!chosen.is_null()) throw std::runtime_error("ambiguous HIP partition/card");
      chosen = device;
    }
  }
  if (chosen.is_null()) throw std::runtime_error("selected HIP PCI card missing");
  if (chosen.at("backend_uuid") != selection.at("backend_uuid")) {
    throw std::runtime_error("selected HIP UUID/PCI mismatch");
  }
  const auto architecture = chosen.at("architecture").get<std::string>();
  if (architecture.substr(0, architecture.find(':')) != "@hip_architecture@") {
    throw std::runtime_error("HIP architecture differs from compiled target");
  }
  hip_check(hipSetDevice(chosen.at("ordinal_diagnostic_only").get<int>()));
  int current = -1;
  hip_check(hipGetDevice(&current));
  if (current != chosen.at("ordinal_diagnostic_only")) throw std::runtime_error("HIP selection did not take effect");
  return chosen;
}
