// Allowlisted Float64 point-gradient operation, independently packaged from EGL.
// Exact API sources and qualification constraints: docs/filter-compatibility.md.
#include "hip_identity.hpp"
#include <Kokkos_Core.hpp>
#include <impl/Kokkos_Profiling.hpp>
#include <viskores/cont/RuntimeDeviceTracker.h>
#include <viskores/cont/kokkos/DeviceAdapterKokkos.h>
#include <vtkmGradient.h>
#include <vtkGradientFilter.h>
#include <vtkCellData.h>
#include <vtkDataArray.h>
#include <vtkImageData.h>
#include <vtkMatrix3x3.h>
#include <vtkNew.h>
#include <vtkPointData.h>
#include <vtkSmartPointer.h>
#include <vtkXMLImageDataReader.h>
#include <vtkXMLImageDataWriter.h>
#include <openssl/evp.h>
#include <tinyxml2.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <chrono>
#include <cmath>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <mutex>
#include <memory>
#include <set>
#include <type_traits>

using json = nlohmann::json;
namespace fs = std::filesystem;
static_assert(std::is_same_v<Kokkos::DefaultExecutionSpace, Kokkos::HIP>, "HIP execution space required");

static void require(bool condition, const char* message) {
  if (!condition) throw std::runtime_error(message);
}
static void keys(const json& value, const std::set<std::string>& expected) {
  require(value.is_object() && value.size() == expected.size(), "exact versioned filter descriptor required");
  for (const auto& entry : value.items()) require(expected.count(entry.key()), "unknown filter descriptor field");
}
static std::string read(const fs::path& path, uint64_t maximum) {
  const int fd = open(path.c_str(), O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC);
  require(fd >= 0, "closed nonsymlink native input required");
  const std::unique_ptr<FILE, decltype(&fclose)> file(fdopen(fd, "rb"), &fclose);
  if (!file) {close(fd); throw std::runtime_error("native input read failed");}
  struct stat metadata{};
  require(fstat(fd, &metadata) == 0 && S_ISREG(metadata.st_mode) && metadata.st_size > 0 && uint64_t(metadata.st_size) <= maximum, "bounded regular native input required");
  std::string bytes(metadata.st_size, '\0');
  require(fread(bytes.data(), 1, bytes.size(), file.get()) == bytes.size() && fgetc(file.get()) == EOF && !ferror(file.get()), "native input changed during inspection");
  return bytes;
}
static std::string sha256(const std::string& data) {
  unsigned char result[EVP_MAX_MD_SIZE];
  unsigned int length = 0;
  require(EVP_Digest(data.data(), data.size(), result, &length, EVP_sha256(), nullptr) == 1 && length == 32, "SHA256 failed");
  std::ostringstream encoded;
  encoded << std::hex << std::setfill('0');
  for (unsigned int i = 0; i < length; ++i) encoded << std::setw(2) << static_cast<unsigned int>(result[i]);
  return encoded.str();
}
static bool hash(const json& value) {
  if (!value.is_string()) return false;
  const auto text = value.get<std::string>();
  return text.size() == 64 && std::all_of(text.begin(), text.end(), [](char c) {return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f');});
}
static fs::path input_path(const std::string& name) {
  require(!name.empty() && name.size() <= 4096, "bounded relative source path required");
  fs::path path("/inputs/fields");
  for (const auto& component : fs::path(name)) {
    require(component != "." && component != ".." && component != "/" && !component.empty(), "source path traversal rejected");
    path /= component;
    require(!fs::is_symlink(fs::symlink_status(path)), "source component symlink rejected");
  }
  require(path.extension() == ".vti", "single image-data shard required");
  return path;
}

// Header checks precede VTK allocation. A source snapshot is independently
// verified and mounted read-only by the submitter/worker before this operation.
static void image_header(const std::string& xml, uint64_t maximum) {
  require(xml.find("<!DOCTYPE") == std::string::npos, "VTK DTD rejected");
  tinyxml2::XMLDocument document;
  require(document.Parse(xml.data(), xml.size()) == tinyxml2::XML_SUCCESS, "well-formed VTI required");
  auto* root = document.FirstChildElement("VTKFile");
  require(root && root->Attribute("type", "ImageData"), "image-data topology required");
  auto* image = root->FirstChildElement("ImageData");
  require(image && image->Attribute("WholeExtent"), "image extent required");
  std::istringstream extent(image->Attribute("WholeExtent"));
  int64_t bounds[6];
  uint64_t points = 1;
  for (auto& bound : bounds) require(bool(extent >> bound) && bound >= 0 && bound <= INT32_MAX, "bounded image extent required");
  std::string extra;
  require(!(extent >> extra), "six-dimensional extent descriptor required");
  for (int i = 0; i < 3; ++i) {
    require(bounds[2*i+1] > bounds[2*i], "three-dimensional nonempty image required");
    const auto count = uint64_t(bounds[2*i+1] - bounds[2*i] + 1);
    require(count <= maximum && points <= maximum / count, "image point budget exhausted");
    points *= count;
  }
  auto* piece = image->FirstChildElement("Piece");
  require(piece && !piece->NextSiblingElement("Piece") && piece->Attribute("Extent") &&
    std::string(piece->Attribute("Extent")) == image->Attribute("WholeExtent"), "single full image piece required");
}

static std::mutex telemetry_mutex;
static uint64_t dispatches = 0, wrong_dispatches = 0, live_hip_bytes = 0, peak_hip_bytes = 0;
static int selected_ordinal = -1;
static bool measuring = false;
static json dispatch_labels = json::array();
static std::map<const void*, uint64_t> allocations;
static void begin_dispatch(const char* name, const uint32_t id, uint64_t* kernel_id) {
  std::lock_guard<std::mutex> guard(telemetry_mutex);
  *kernel_id = ++dispatches;
  if (!measuring) return;
  const auto device = Kokkos::Tools::Experimental::identifier_from_devid(id);
  if (device.type != Kokkos::Tools::Experimental::DeviceType::HIP || int(device.device_id) != selected_ordinal) ++wrong_dispatches;
  if (dispatch_labels.size() < 64) dispatch_labels.push_back({{"label", std::string(name).substr(0, 256)}, {"device_id", id}});
}
static void allocated(Kokkos::Tools::SpaceHandle space, const char*, const void* pointer, const uint64_t size) {
  if (std::string(space.name) != "HIPSpace") return;
  std::lock_guard<std::mutex> guard(telemetry_mutex);
  allocations[pointer] = size;
  live_hip_bytes += size;
  peak_hip_bytes = std::max(peak_hip_bytes, live_hip_bytes);
}
static void deallocated(Kokkos::Tools::SpaceHandle space, const char*, const void* pointer, const uint64_t) {
  if (std::string(space.name) != "HIPSpace") return;
  std::lock_guard<std::mutex> guard(telemetry_mutex);
  const auto entry = allocations.find(pointer);
  if (entry != allocations.end()) {live_hip_bytes -= entry->second; allocations.erase(entry);}
}

static void preserved(vtkImageData* source, vtkImageData* result, bool gradient) {
  require(source && result && source->GetNumberOfPoints() == result->GetNumberOfPoints() &&
    source->GetNumberOfCells() == result->GetNumberOfCells(), "filter topology changed");
  require(std::memcmp(source->GetExtent(), result->GetExtent(), 6*sizeof(int)) == 0 &&
    std::memcmp(source->GetOrigin(), result->GetOrigin(), 3*sizeof(double)) == 0 &&
    std::memcmp(source->GetSpacing(), result->GetSpacing(), 3*sizeof(double)) == 0 &&
    std::memcmp(source->GetDirectionMatrix()->GetData(), result->GetDirectionMatrix()->GetData(), 9*sizeof(double)) == 0,
    "filter coordinates changed");
  for (const bool point : {true, false}) {
    vtkDataSetAttributes* a = point ? static_cast<vtkDataSetAttributes*>(source->GetPointData()) : source->GetCellData();
    vtkDataSetAttributes* b = point ? static_cast<vtkDataSetAttributes*>(result->GetPointData()) : result->GetCellData();
    require(b->GetNumberOfArrays() == a->GetNumberOfArrays() + (point && gradient ? 1 : 0), "filter array inventory changed");
    for (int i = 0; i < a->GetNumberOfArrays(); ++i) {
      auto* x = a->GetArray(i); auto* y = x && x->GetName() ? b->GetArray(x->GetName()) : nullptr;
      require(x && y && x->GetDataType() == y->GetDataType() && x->GetNumberOfComponents() == y->GetNumberOfComponents() &&
        x->GetNumberOfTuples() == y->GetNumberOfTuples(), "filter association or precision changed");
      require(std::memcmp(x->GetVoidPointer(0), y->GetVoidPointer(0), x->GetDataSize()*x->GetDataTypeSize()) == 0, "original field bytes changed");
    }
  }
}

static json execute(const json& request, bool reference) {
  keys(request, {"schema_version", "selection", "field", "source_file", "source_sha256", "source_snapshot_sha256", "science_id", "execution_id", "max_input_bytes", "max_points", "max_output_bytes"});
  require(request.at("schema_version") == 1, "unsupported numerical-filter version");
  const std::string field = request.at("field");
  require(field == "physVelocity" || field == "physPressure", "unsupported gradient field");
  for (const auto& id : {"source_sha256", "source_snapshot_sha256", "science_id", "execution_id"}) require(hash(request.at(id)), "exact source identities required");
  for (const auto& budget : {"max_input_bytes", "max_points", "max_output_bytes"}) require(request.at(budget).is_number_unsigned(), "integer filter budgets required");
  const uint64_t maximum_input = request.at("max_input_bytes"), maximum_points = request.at("max_points"), maximum_output = request.at("max_output_bytes");
  require(maximum_input > 0 && maximum_input <= 64*1024*1024 && maximum_points > 0 && maximum_points <= 1000000 && maximum_output > 0 && maximum_output <= 256*1024*1024, "bounded numerical-filter budgets required");
  const auto path = input_path(request.at("source_file"));
  const auto bytes = read(path, maximum_input);
  require(sha256(bytes) == request.at("source_sha256"), "approved source bytes changed");
  image_header(bytes, maximum_points);
  json device;
  if (!reference) device = hip_select_device(request.at("selection"));
  else require(request.at("selection").is_null(), "CPU reference must be explicitly CPU-only");
  vtkNew<vtkXMLImageDataReader> reader;
  reader->SetFileName(path.c_str()); reader->Update();
  auto* input = reader->GetOutput();
  require(reader->GetErrorCode() == 0 && input->GetNumberOfPoints() > 0 && uint64_t(input->GetNumberOfPoints()) <= maximum_points, "closed bounded VTI read failed");
  require(!input->GetPointData()->GetGhostArray() && !input->GetCellData()->GetGhostArray(), "ghost arrays require separate qualification");
  auto* source_field = input->GetPointData()->GetArray(field.c_str());
  require(source_field && source_field->GetDataType() == VTK_DOUBLE && source_field->GetNumberOfTuples() == input->GetNumberOfPoints() &&
    source_field->GetNumberOfComponents() == (field == "physVelocity" ? 3 : 1), "Float64 point field with exact extent required");
  require(!input->GetPointData()->HasArray("gradient"), "gradient output name collision");
  for (vtkIdType i = 0; i < source_field->GetNumberOfValues(); ++i) require(std::isfinite(static_cast<double*>(source_field->GetVoidPointer(0))[i]), "nonfinite input field rejected");
  vtkSmartPointer<vtkGradientFilter> filter;
  const auto start = std::chrono::steady_clock::now();
  std::chrono::steady_clock::time_point initialized;
  if (reference) filter = vtkSmartPointer<vtkGradientFilter>::New();
  else {
    selected_ordinal = device.at("ordinal_diagnostic_only");
    require(!std::getenv("KOKKOS_TOOLS_LIBS") && !std::getenv("KOKKOS_PROFILE_LIBRARY"), "external Kokkos tooling is not an approved runtime input");
    Kokkos::initialize(Kokkos::InitializationSettings().set_device_id(selected_ordinal));
    Kokkos::Tools::Experimental::set_begin_parallel_for_callback(begin_dispatch);
    Kokkos::Tools::Experimental::set_begin_parallel_reduce_callback(begin_dispatch);
    Kokkos::Tools::Experimental::set_begin_parallel_scan_callback(begin_dispatch);
    Kokkos::Tools::Experimental::set_allocate_data_callback(allocated);
    Kokkos::Tools::Experimental::set_deallocate_data_callback(deallocated);
    // Force the Viskores device as well as vtkmGradient's no-fallback switch.
    viskores::cont::GetRuntimeDeviceTracker().ForceDevice(viskores::cont::DeviceAdapterTagKokkos{});
    auto accelerated = vtkSmartPointer<vtkmGradient>::New();
    accelerated->SetForceVTKm(true); filter = accelerated;
  }
  filter->SetInputData(input);
  filter->SetInputScalars(vtkDataObject::FIELD_ASSOCIATION_POINTS, field.c_str());
  filter->SetResultArrayName("gradient");
  filter->SetComputeGradient(true); filter->SetFasterApproximation(false);
  filter->SetComputeDivergence(false); filter->SetComputeVorticity(false); filter->SetComputeQCriterion(false);
  initialized = std::chrono::steady_clock::now();
  {std::lock_guard<std::mutex> guard(telemetry_mutex); measuring = !reference; dispatches = 0;}
  filter->Update();
  if (!reference) {Kokkos::fence("harbor-cad completed gradient"); hip_check(hipDeviceSynchronize());}
  {std::lock_guard<std::mutex> guard(telemetry_mutex); measuring = false;}
  require(filter->GetErrorCode() == 0, "numerical filter failed");
  if (!reference) require(dispatches > 0 && wrong_dispatches == 0, "HIP dispatch evidence absent or foreign execution space observed");
  auto* output = vtkImageData::SafeDownCast(filter->GetOutput());
  preserved(input, output, true);
  auto* gradient = output->GetPointData()->GetArray("gradient");
  require(gradient && gradient->GetDataType() == VTK_DOUBLE && gradient->GetNumberOfTuples() == input->GetNumberOfPoints() &&
    gradient->GetNumberOfComponents() == 3*source_field->GetNumberOfComponents(), "Float64 point gradient required");
  for (vtkIdType i = 0; i < gradient->GetNumberOfValues(); ++i) require(std::isfinite(static_cast<double*>(gradient->GetVoidPointer(0))[i]), "nonfinite gradient rejected");
  const auto elapsed = std::chrono::duration<double>(std::chrono::steady_clock::now()-start).count();
  const auto initialized_elapsed = std::chrono::duration<double>(std::chrono::steady_clock::now()-initialized).count();
  require(!fs::exists("gradient.vti") && !fs::exists("gradient.vti.partial"), "new stage-local gradient destination required");
  vtkNew<vtkXMLImageDataWriter> writer;
  writer->SetFileName("gradient.vti.partial"); writer->SetInputData(output); writer->SetDataModeToBinary(); writer->SetCompressorTypeToNone();
  require(writer->Write() && writer->GetErrorCode() == 0, "gradient write failed");
  const auto serialized = read("gradient.vti.partial", maximum_output);
  vtkNew<vtkXMLImageDataReader> roundtrip;
  roundtrip->SetFileName("gradient.vti.partial"); roundtrip->Update();
  require(roundtrip->GetErrorCode() == 0, "gradient round trip failed");
  preserved(output, roundtrip->GetOutput(), false);
  require(sha256(read(path, maximum_input)) == request.at("source_sha256"), "source changed during filter execution");
  struct rusage usage{}; require(getrusage(RUSAGE_SELF, &usage) == 0, "process RAM measurement failed");
  json receipt = {{"schema_version",1}, {"operation","float64_image_point_gradient"}, {"backend",reference ? "cpu_reference" : "hip"},
    {"device",device}, {"source_sha256",request.at("source_sha256")}, {"source_snapshot_sha256",request.at("source_snapshot_sha256")},
    {"science_id",request.at("science_id")}, {"execution_id",request.at("execution_id")}, {"source_field",field},
    {"output_field","gradient"}, {"association","point"}, {"precision","float64"}, {"gradient_ordering","du/dx,du/dy,du/dz,dv/dx,dv/dy,dv/dz,dw/dx,dw/dy,dw/dz"},
    {"source_unit",field == "physVelocity" ? "m/s" : "Pa"}, {"output_unit",field == "physVelocity" ? "1/s" : "Pa/m"},
    {"coordinate_and_source_array_roundtrip","exact_bytes"}, {"points",input->GetNumberOfPoints()}, {"cells",input->GetNumberOfCells()},
    {"output_sha256",sha256(serialized)}, {"output_bytes",serialized.size()}, {"filter_wall_seconds_including_initialization",elapsed},
    {"initialized_filter_wall_seconds",initialized_elapsed},
    {"peak_process_rss_bytes",uint64_t(usage.ru_maxrss)*1024}, {"kokkos_hip_space_peak_tracked_bytes",peak_hip_bytes},
    {"memory_measurement_scope","whole process RSS; Kokkos HIPSpace allocations after initialization, excludes driver and non-Kokkos device allocations"},
    {"hip_dispatches",dispatches}, {"dispatch_labels",dispatch_labels}, {"kernel_evidence","Kokkos execution-space callbacks; instruction trace qualification separate"},
    {"physical_validation","unqualified"}};
  receipt["adapter"] = reference ? "VTKReference" : "Viskores";
  receipt["executed"] = true;
  receipt["software_fallback"] = false;
  if (!reference) {
    const auto final_device = hip_select_device(request.at("selection"));
    require(final_device == device, "actual HIP identity changed");
    int runtime=0, driver=0; hip_check(hipRuntimeGetVersion(&runtime)); hip_check(hipDriverGetVersion(&driver));
    receipt["hip_versions"] = {{"compiled",HIP_VERSION}, {"runtime",runtime}, {"driver",driver}};
    require(runtime == HIP_VERSION && driver == HIP_VERSION, "loaded HIP ABI differs from compiled runtime");
  }
  fs::rename("gradient.vti.partial", "gradient.vti");
  return receipt;
}

int main(int argc, char** argv) {
  try {
    if (argc == 2 && std::string(argv[1]) == "--gpu-inventory") {
      std::cout << json({{"backend","hip"},{"executed",false},{"devices",hip_inventory()}}).dump(2) << std::endl;
      return 0;
    }
    require(argc == 3 && (std::string(argv[1]) == "gradient" || std::string(argv[1]) == "gradient-cpu-reference"), "usage: harbor-cad-filter gradient|gradient-cpu-reference request.json");
    const auto receipt = execute(json::parse(read(argv[2], 1024*1024)), std::string(argv[1]) == "gradient-cpu-reference");
    require(!fs::exists("filter-receipt.json"), "new stage-local filter receipt required");
    std::ofstream file("filter-receipt.json.partial", std::ios::binary); file << receipt.dump(2); file.close();
    require(bool(file), "filter receipt write failed");
    fs::rename("filter-receipt.json.partial", "filter-receipt.json");
    std::cout << receipt.dump(2) << std::endl;
    if (Kokkos::is_initialized()) Kokkos::finalize();
    return 0;
  } catch (const std::exception& error) {
    std::cerr << error.what() << std::endl;
    if (Kokkos::is_initialized()) Kokkos::finalize();
    return 1;
  }
}
