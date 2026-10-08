// Thin OpenLB 1.9 driver: the library owns collision, streaming and VTK output.
// API evidence: release 145cd548, examples/laminar/poiseuille3d and cylinder3d.
#include <olb.h>
#include <nlohmann/json.hpp>
#include <algorithm>
#include <cctype>
#include <fstream>
#include <iomanip>
#include <cstdlib>
#include <sstream>
#include <stdexcept>
#ifdef PLATFORM_GPU_CUDA
#include <cuda_runtime.h>
#endif
#ifdef PLATFORM_GPU_HIP
#include "hip_identity.hpp"

static nlohmann::json hip_select(const nlohmann::json& plan) {
  nlohmann::json selection;
  bool found = false;
  for (const auto& stage : plan.at("stages")) {
    if (stage.at("operation") == "openlb") {
      if (found) throw std::runtime_error("ambiguous OpenLB stage");
      found = true;
      selection = stage.at("selection");
    }
  }
  return hip_select_device(selection);
}
#endif

using namespace olb;
using namespace olb::names;
using T = double;
using D = descriptors::D3Q19<descriptors::FORCE>;
using Flow = Case<NavierStokes, Lattice<T,D>>;

class SteadyChannel : public AnalyticalF3D<T,T> {
  T height, acceleration, viscosity;
public:
  SteadyChannel(T h, T a, T nu) : AnalyticalF3D<T,T>(3), height(h), acceleration(a), viscosity(nu) {
    getName()="independent parallel-plate solution";
  }
  bool operator()(T output[], const T input[]) override {
    output[0]=acceleration*input[1]*(height-input[1])/(2*viscosity);
    output[1]=0; output[2]=0; return true;
  }
};

int main(int argc, char** argv) {
  try {
#ifdef PLATFORM_GPU_HIP
    if (argc == 2 && std::string(argv[1]) == "--gpu-inventory") {
      std::cout << nlohmann::json({{"backend", "hip"}, {"executed", false},
        {"compiled_architecture", "@hip_architecture@"}, {"devices", hip_inventory()}}).dump(2) << std::endl;
      return 0;
    }
#endif
    if (argc != 3 || std::string(argv[1]) != "openlb") {
      throw std::runtime_error("usage: harbor-cad-openlb openlb plan.json");
    }
    nlohmann::json plan;
    std::ifstream(argv[2]) >> plan;
    const auto& c = plan.at("case");
    if (!c.at("geometry").at("synthetic").get<bool>() ||
        c.at("applicability").at("formulation") != "periodic_forced_channel") {
      throw std::runtime_error("driver applicability: synthetic periodic forced channel only");
    }
    for (const auto* key : {"length", "channel_height", "kinematic_viscosity", "acceleration"}) {
      const auto unit = c.at(key).at("unit").get<std::string>();
      const std::string expected = std::string(key)=="kinematic_viscosity" ? "m2/s" :
        std::string(key)=="acceleration" ? "m/s2" : "m";
      if (unit != expected) throw std::runtime_error("driver requires explicit SI quantities");
    }
    const int resolution = c.at("resolution");
    const T height = c.at("channel_height").at("value");
    const T viscosity = c.at("kinematic_viscosity").at("value");
    const T acceleration = c.at("acceleration").at("value");
    const T dx = height / resolution;
    const T length = c.at("length").at("value");
    if (resolution < 3 || !std::isfinite(dx) || dx <= 0 || !std::isfinite(viscosity) || viscosity <= 0 ||
        !std::isfinite(length) || length < dx || std::abs(length/dx - std::round(length/dx)) > 1e-8) {
      throw std::runtime_error("positive SI parameters and integral periodic lattice extent required");
    }
    nlohmann::json receipt = {{"adapter","OpenLB"}, {"source_revision","145cd54810b468f4b6fd3ed86b10644264841578"},
      {"precision","float64"}, {"software_fallback",false}, {"physical_validation","unqualified"}};
#ifdef PLATFORM_GPU_CUDA
    auto selection = nlohmann::json();
    for (const auto& stage : plan.at("stages")) if (stage.at("operation") == "openlb") selection = stage.at("selection");
    int count = 0; if (cudaGetDeviceCount(&count) != cudaSuccess) throw std::runtime_error("CUDA driver unavailable");
    int chosen = -1;
    for (int i=0; i<count; ++i) {
      char pci[64];
      if (cudaDeviceGetPCIBusId(pci,sizeof(pci),i) != cudaSuccess) throw std::runtime_error("CUDA PCI query failed");
      std::string id(pci); std::transform(id.begin(),id.end(),id.begin(),::tolower);
      if (id == selection.at("pci")) {if (chosen != -1) throw std::runtime_error("ambiguous CUDA partition/card"); chosen=i;}
    }
    if (chosen < 0 || cudaSetDevice(chosen) != cudaSuccess) throw std::runtime_error("selected CUDA PCI card missing");
    cudaDeviceProp props;
    if (cudaGetDeviceProperties(&props,chosen) != cudaSuccess) throw std::runtime_error("CUDA property query failed");
    receipt["backend"]="cuda"; receipt["pci"]=selection.at("pci"); receipt["device_name"]=props.name;
#elif defined(PLATFORM_GPU_HIP)
    receipt.update(hip_select(plan));
    receipt["backend"]="hip";
    receipt["compiled_architecture"]="@hip_architecture@";
    receipt["uuid_source"]="hipDeviceGetUuid; GPU- followed by 16 bytes in lowercase hex";
    int runtimeVersion = 0, driverVersion = 0;
    hip_check(hipRuntimeGetVersion(&runtimeVersion));
    hip_check(hipDriverGetVersion(&driverVersion));
    receipt["hip_runtime_version"]=runtimeVersion;
    receipt["hip_driver_version"]=driverVersion;
    receipt["compiled_hip_version"]=HIP_VERSION;
#else
    if (plan.contains("stages")) {
      for (const auto& stage : plan.at("stages")) {
        if (stage.at("operation") == "openlb" &&
            (!stage.at("selection").is_null() || stage.at("gpu") != "cpu_only")) {
          throw std::runtime_error("CPU driver cannot execute a GPU-required stage");
        }
      }
    }
    receipt["backend"]="cpu";
#endif
    initialize(&argc,&argv);
    singleton::directories().setOutputDir("./tmp/");
    // Consume CAD STL through OpenLB's own voxelizer. Intended gaps are never healed.
    STLreader<T> geometryInput("fluid.stl",dx,0.001);
    const Vector<T,3> expectedMax{length,height,height};
    for (int axis=0; axis<3; ++axis) {
      if (std::abs(geometryInput.getMesh().getMin()[axis]) > dx*1e-6 ||
          std::abs(geometryInput.getMesh().getMax()[axis]-expectedMax[axis]) > dx*1e-6) {
        throw std::runtime_error("synthetic channel STL bounds differ from approved SI geometry");
      }
    }
    // Cell-centred fluid spans the full periodic x/z extent. Only y has wall
    // layers; wrapping a six-sided wall box would block the driven channel.
    IndicatorCuboid3D<T> extent({length-dx,height+dx,height-dx},{dx/2,-dx/2,dx/2});
    Mesh<T,3> mesh(extent,dx,1);
    mesh.setOverlap(2);
    mesh.getCuboidDecomposition().setPeriodicity({true,false,true});
    Flow::ParametersD parameters;
    Flow flow(parameters,mesh);
    auto& geometry=flow.getGeometry();
    geometry.rename(0,2);
    geometry.rename(2,1,geometryInput);
    geometry.clean(); geometry.checkForErrors();
    const std::size_t expectedFluidCells=std::llround(length/dx)*resolution*resolution;
    if (geometry.getStatistics().getNvoxel(1)!=expectedFluidCells) {
      throw std::runtime_error("STL voxelization does not match the supported full periodic channel");
    }
    auto& lattice=flow.getLattice(NavierStokes{});
#ifdef PLATFORM_GPU_HIP
    const auto& load = lattice.getLoadBalancer();
    if (load.size() <= 0) throw std::runtime_error("no local HIP lattice blocks");
    for (int i=0; i<load.size(); ++i) {
      if (load.platform(i)!=Platform::GPU_HIP) throw std::runtime_error("silent CPU block assignment rejected");
    }
    receipt["gpu_blocks"]=load.size();
#endif
    const T characteristicVelocity=std::abs(acceleration)*height*height/(8*viscosity);
    if (characteristicVelocity <= 0) throw std::runtime_error("nonzero drive required");
    lattice.setUnitConverter<UnitConverterFromResolutionAndRelaxationTime<T,D>>(
      resolution,0.8,height,characteristicVelocity,viscosity,c.at("material").at("density").at("value"));
    const auto& converter=lattice.getUnitConverter();
    const T latticeMach=std::sqrt(3.)*converter.getCharLatticeVelocity();
    if (!std::isfinite(latticeMach) || latticeMach>0.1) {
      throw std::runtime_error("fixed BGK reference requires lattice Mach <= 0.1; refine/reapprove explicitly");
    }
    receipt["lattice_mach"]=latticeMach;
    receipt["relaxation_time"]=0.8;
    dynamics::set<ForcedBGKdynamics>(lattice,geometry,1);
    boundary::set<boundary::BounceBack>(lattice,geometry,2);
    lattice.setParameter<descriptors::OMEGA>(converter.getLatticeRelaxationFrequency());
    Vector<T,3> forcing{acceleration*converter.getPhysDeltaT()*converter.getPhysDeltaT()/dx,0,0};
    fields::set<descriptors::FORCE>(lattice,geometry.getMaterialIndicator({1,2}),forcing);
    lattice.initialize();
    const std::size_t steps=converter.getLatticeTime(c.at("max_time_s").get<T>());
    if (steps == 0) throw std::runtime_error("physical duration below one lattice time step");
    SuperLatticePhysVelocity3D<T,D> velocity(lattice,converter);
    SuperLatticePhysPressure3D<T,D> pressure(lattice,converter);
    SuperGeometryF3D<T> material(geometry);
    SuperVTMwriter3D<T,T> writer("channel"); writer.addFunctor(velocity); writer.addFunctor(pressure); writer.addFunctor(material); writer.createMasterFile();
    std::vector<std::size_t> outputs;
    receipt["retained_times"] = nlohmann::json::array();
    for (const auto& time : plan.at("observation").at("retained_times_s")) {
      const auto step=converter.getLatticeTime(time.get<T>());
      if (step>steps || std::find(outputs.begin(),outputs.end(),step)!=outputs.end()) {
        throw std::runtime_error("retained times exceed duration or collapse to the same lattice step");
      }
      outputs.push_back(step);
      receipt["retained_times"].push_back({{"requested_s",time},{"step",step},{"observed_s",converter.getPhysTime(step)}});
    }
    lattice.setProcessingContext(ProcessingContext::Simulation);
    for (std::size_t i=0; i<=steps; ++i) {
      if (std::find(outputs.begin(),outputs.end(),i)!=outputs.end()) {
        lattice.setProcessingContext(ProcessingContext::Evaluation); writer.write(i);
        // GPU Simulation uploads host mirrors. Re-enter only after observation,
        // never each step: repeated uploads erase the evolving device state.
        if (i<steps) lattice.setProcessingContext(ProcessingContext::Simulation);
      }
      if (i<steps) lattice.collideAndStream();
    }
    lattice.setProcessingContext(ProcessingContext::Evaluation);
    SteadyChannel analytical(height,acceleration,viscosity);
    auto fluid=geometry.getMaterialIndicator(1);
    SuperRelativeErrorL2Norm3D<T> relativeError(velocity,analytical,fluid);
    T error[2]={}; int reductionInput[1]={};
    relativeError(error,reductionInput);
    const T tolerance=c.at("applicability").at("numerical_tolerance");
    if (!std::isfinite(tolerance) || tolerance<=0) throw std::runtime_error("explicit positive numerical tolerance required");
    receipt["numerical_verification"]={{"reference","steady parallel-plate analytical velocity"},
      {"relative_l2_error",error[0]}, {"tolerance",tolerance},
      {"passed",std::isfinite(error[0]) && error[0]<=tolerance},
      {"scope","velocity only; pressure and physical validation separately unqualified"}};
#ifdef PLATFORM_GPU_CUDA
    if (cudaDeviceSynchronize()!=cudaSuccess || cudaGetLastError()!=cudaSuccess) throw std::runtime_error("CUDA kernel execution failure");
    // Build flags alone are insufficient: every local block must actually use CUDA.
    for (int i=0; i<lattice.getLoadBalancer().size(); ++i) {
      if (lattice.getLoadBalancer().platform(i)!=Platform::GPU_CUDA) throw std::runtime_error("silent CPU block assignment rejected");
    }
#endif
#ifdef PLATFORM_GPU_HIP
    hip_check(hipDeviceSynchronize());
    hip_check(hipGetLastError());
    int current = -1;
    hip_check(hipGetDevice(&current));
    if (current != receipt.at("ordinal_diagnostic_only")) throw std::runtime_error("HIP execution device changed");
    receipt["gpu_kernel_completion_verified"]=true;
#endif
    receipt["executed"]=true; receipt["lattice_steps"]=steps; receipt["cells"]=geometry.getStatistics().getNvoxel();
    receipt["fluid_cells"]=expectedFluidCells; receipt["spacing_m"]=dx; receipt["physical_step_s"]=converter.getPhysDeltaT();
    receipt["walls_m"]={0,height}; receipt["periodic_axes"]={"x","z"};
    receipt["field_units"]={{"physVelocity","m/s"},{"physPressure","Pa"},{"geometry","material ID"}};
    receipt["convergence"]="not assessed"; receipt["physical_times_are_lattice_quantized"]=true;
    receipt["processing_context_policy"]="device-resident stepping; host synchronization at retained observations";
    receipt["compiler"]=__VERSION__; receipt["mpi"]="none";
    std::ofstream("openlb-receipt.json.partial") << receipt.dump(2);
    std::rename("openlb-receipt.json.partial","openlb-receipt.json");
    if (!std::isfinite(error[0]) || error[0]>tolerance) throw std::runtime_error("approved numerical tolerance not met; fields retained");
    return 0;
  } catch (const std::exception& e) {std::cerr << e.what() << std::endl; return 1;}
}
