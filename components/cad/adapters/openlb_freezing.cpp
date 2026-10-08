// Thin conduction driver over pinned public OpenLB total-enthalpy dynamics.
// Upstream collision, streaming, boundaries and phase-change coupling own the
// numerical model. SI conversion and closed observations belong to this driver.
#include <olb.h>
#include <nlohmann/json.hpp>
#include <algorithm>
#include <cmath>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <set>
#include <stdexcept>

using namespace olb;
using namespace olb::names;
using Json = nlohmann::json;
using FreezingCase = Case<
  NavierStokes, Lattice<double,descriptors::D2Q9<descriptors::POROSITY,descriptors::VELOCITY_SOLID,descriptors::FORCE,descriptors::OMEGA>>,
  Temperature, Lattice<double,descriptors::D2Q5<descriptors::VELOCITY,descriptors::TEMPERATURE>>
>;
using HeatDescriptor = FreezingCase::descriptor_t_of<Temperature>;
using FlowDescriptor = FreezingCase::descriptor_t_of<NavierStokes>;
using EnthalpyDynamics = TotalEnthalpyAdvectionDiffusionBGKdynamics<double,HeatDescriptor>;

static double number(const Json& value) {
  if (!value.is_number()) throw std::runtime_error("finite explicit numeric input required");
  const double v=value.get<double>();
  if (!std::isfinite(v)) throw std::runtime_error("nonfinite input rejected");
  return v;
}

static void fresh_output() {
  for (const auto& entry : std::filesystem::directory_iterator(".")) {
    const auto name=entry.path().filename();
    if ((name!="process.log" && name!="freezing.log")
        || !std::filesystem::is_regular_file(entry.symlink_status())
        || std::filesystem::hard_link_count(entry.path())!=1
        || (name=="freezing.log" && std::filesystem::file_size(entry.path())!=0))
      throw std::runtime_error("fresh freezing stage with only regular native log and empty worker log required");
  }
}

static void close_field(std::ofstream& stream, const std::string& path) {
  stream.flush();
  if (!stream) throw std::runtime_error("native scientific output write failed");
  stream.close();
  if (std::filesystem::exists(path)) throw std::runtime_error("new immutable scientific output required");
  std::filesystem::rename(path+".partial",path);
}

int main(int argc,char** argv) {
  try {
    if (argc!=3 || std::string(argv[1])!="reference") throw std::runtime_error("usage: harbor-cad-openlb-freezing reference request.json");
    if (!std::filesystem::is_regular_file(std::filesystem::symlink_status(argv[2]))
        || std::filesystem::file_size(argv[2])>65536) throw std::runtime_error("bounded regular request required");
    Json spec; std::ifstream(argv[2])>>spec;
    const std::set<std::string> expected={"schema_version","synthetic","backend","formulation","size_m","resolution","density_kg_m3","specific_heat_j_kg_k","conductivity_w_m_k","latent_heat_j_kg","melting_temperature_k","initial_temperature_k","cold_wall_temperature_k","material_temperature_domain_k","steps","observation_steps","front_tolerance","temperature_tolerance","mass_tolerance","energy_tolerance","material_provenance","boundary_provenance","geometry_provenance","moisture_risk"};
    std::set<std::string> observed; for (auto i=spec.begin();i!=spec.end();++i) observed.insert(i.key());
    if (observed!=expected || spec.at("schema_version")!=1 || spec.at("synthetic")!=true
        || spec.at("backend")!="cpu" || spec.at("formulation")!="conduction_stefan_solidification_2d") throw std::runtime_error("strict synthetic CPU conduction solidification descriptor required");
    if (!spec.at("resolution").is_number_unsigned() || !spec.at("steps").is_number_unsigned()) throw std::runtime_error("integer grid/step counts required");
    const auto raw_resolution=spec.at("resolution").get<std::uint64_t>();
    if (raw_resolution<32 || raw_resolution>256) throw std::runtime_error("bounded resolution required before integer conversion");
    const int n=static_cast<int>(raw_resolution); const std::size_t steps=spec.at("steps");
    const auto size=spec.at("size_m").get<std::vector<double>>();
    const auto domain=spec.at("material_temperature_domain_k").get<std::vector<double>>();
    const double rho=number(spec.at("density_kg_m3")), cp=number(spec.at("specific_heat_j_kg_k")), k=number(spec.at("conductivity_w_m_k")), latent=number(spec.at("latent_heat_j_kg")), tm=number(spec.at("melting_temperature_k")), initial=number(spec.at("initial_temperature_k")), cold=number(spec.at("cold_wall_temperature_k"));
    if (n<32 || n>256 || n%8 || size.size()!=3 || domain.size()!=2
        || std::any_of(size.begin(),size.end(),[](double v){return !std::isfinite(v) || v<1e-6 || v>1.;})
        || size[1]!=size[0]/8. || rho<=0. || cp<=0. || k<=0. || latent<=0.
        || tm<100. || tm>1000. || cold<100. || cold>=tm || initial!=tm
        || !std::isfinite(domain[0]) || !std::isfinite(domain[1]) || domain[0]<100. || domain[1]>1000.
        || domain[0]>=domain[1] || cold<domain[0] || tm>domain[1]
        || steps<std::size_t(n)*n/2 || steps>std::size_t(n)*n) throw std::runtime_error("bounded equal-property fixed-volume conduction reference required");
    const double span=tm-cold, stefan=cp*span/latent, dx=size[0]/n, dt=dx*dx*rho*cp/(6.*k), h_scale=cp*span, latent_lattice=1./stefan;
    if (!std::isfinite(dt) || dt<=0. || !std::isfinite(h_scale) || h_scale<=0. || stefan<.05 || stefan>.2) throw std::runtime_error("finite independent SI conversion and Stefan 0.05..0.2 required");
    for (const char* name : {"front_tolerance","temperature_tolerance","mass_tolerance","energy_tolerance"}) {
      const double tolerance=number(spec.at(name));
      const double maximum=std::string(name)=="front_tolerance" || std::string(name)=="temperature_tolerance" ? .02 : 1e-10;
      if (tolerance<=0. || tolerance>maximum) throw std::runtime_error("unchanged acceptance gate required");
    }
    for (const char* name : {"material_provenance","boundary_provenance","geometry_provenance"})
      if (!spec.at(name).is_string() || spec.at(name).get<std::string>().empty() || spec.at(name).get<std::string>().size()>4096) throw std::runtime_error("explicit synthetic provenance required");
    if (!spec.at("observation_steps").is_array() || std::any_of(spec.at("observation_steps").begin(),spec.at("observation_steps").end(),[](const auto& v){return !v.is_number_unsigned();})) throw std::runtime_error("integer observation steps required");
    const auto retained=spec.at("observation_steps").get<std::vector<std::size_t>>();
    if (retained.size()<2 || retained.size()>16 || retained.front()!=0 || retained.back()!=steps
        || !std::is_sorted(retained.begin(),retained.end()) || std::adjacent_find(retained.begin(),retained.end())!=retained.end()
        || std::any_of(retained.begin(),retained.end(),[&](auto v){return v!=0 && (v<std::size_t(n)*n/2 || v>steps);})) throw std::runtime_error("ordered bounded original observations required");
    fresh_output(); initialize(&argc,&argv); singleton::directories().setOutputDir("./tmp/");
    FreezingCase::ParametersD params; params.set<parameters::RESOLUTION>(n); params.set<parameters::DOMAIN_EXTENT>({1.,.125}); params.set<parameters::OVERLAP>(2);
    // Exactly n/8 cell-centered periodic rows; no duplicated periodic endpoint.
    Mesh<double,2> mesh(Vector<double,2>{0.,.5/n},1./n,Vector<int,2>{n+1,n/8},1);
    mesh.setOverlap(2); mesh.getCuboidDecomposition().setPeriodicity({false,true});
    FreezingCase experiment(params,mesh); auto& geometry=experiment.getGeometry();
    geometry.rename(0,2); geometry.rename(2,1,{1,0});
    IndicatorCuboid2D<double> left({1./n,.125+2./n},{-1./n,0.}); geometry.rename(2,3,1,left);
    geometry.clean(); geometry.innerClean(); geometry.checkForErrors();
    auto& flow=experiment.getLattice(NavierStokes{}); auto& heat=experiment.getLattice(Temperature{});
    flow.setUnitConverter<ThermalUnitConverter<double,FlowDescriptor,HeatDescriptor>>(1./n,1./(6.*n*n),1.,1.,1.,1.,1.,1.,1.,0.,1.);
    heat.setUnitConverter(flow.getUnitConverter());
    dynamics::set<ForcedPSMBGKdynamics>(flow,geometry.getMaterialIndicator({1,2,3}));
    dynamics::set<EnthalpyDynamics>(heat,geometry.getMaterialIndicator({1,3}));
    boundary::set<boundary::BounceBack>(heat,geometry,2);
    boundary::set<boundary::RegularizedTemperature>(heat,geometry.getMaterialIndicator(3));
    heat.setParameter<descriptors::OMEGA>(1.); flow.setParameter<descriptors::OMEGA>(1.);
    heat.setParameter<TotalEnthalpy::T_S>(1.); heat.setParameter<TotalEnthalpy::T_L>(1.);
    heat.setParameter<TotalEnthalpy::CP_S>(1.); heat.setParameter<TotalEnthalpy::CP_L>(1.);
    heat.setParameter<TotalEnthalpy::LAMBDA_S>(1./6.); heat.setParameter<TotalEnthalpy::LAMBDA_L>(1./6.); heat.setParameter<TotalEnthalpy::L>(latent_lattice);
    AnalyticalConst2D<double,double> zero(0.,0.),one(1.),cold_native(0.);
    // ForcedPSM momenta reads the per-cell OMEGA field during coupling's
    // computeU, independently of the collision OMEGA parameter.
    fields::set<descriptors::OMEGA>(flow,geometry.getMaterialIndicator({1,2,3}),one);
    fields::set<descriptors::VELOCITY>(heat,geometry.getMaterialIndicator({1,2,3}),zero);
    flow.iniEquilibrium(geometry.getMaterialIndicator({1,2,3}),one,zero);
    // theta=1 equilibrium in shifted populations, with latent energy only in
    // the rest population. FirstOrder(total_enthalpy) warms the reflecting shell.
    std::vector<double> populations(HeatDescriptor::q,0.); populations[0]=latent_lattice;
    AnalyticalConst2D<double,double> initial_populations(populations);
    heat.definePopulations(geometry.getMaterialIndicator({1,2}),initial_populations);
    heat.iniEquilibrium(geometry.getMaterialIndicator(3),cold_native,zero);
    momenta::setTemperature(heat,geometry.getMaterialIndicator(3),0.);
    auto& coupling=experiment.setCouplingOperator("phase",TotalEnthalpyPhaseChangeCoupling{},NavierStokes{},flow,Temperature{},heat);
    coupling.restrictTo(geometry.getMaterialIndicator({1,3}));
    coupling.setParameter<TotalEnthalpyPhaseChangeCoupling::T_S>(1.); coupling.setParameter<TotalEnthalpyPhaseChangeCoupling::T_L>(1.);
    coupling.setParameter<TotalEnthalpyPhaseChangeCoupling::CP_S>(1.); coupling.setParameter<TotalEnthalpyPhaseChangeCoupling::CP_L>(1.);
    coupling.setParameter<TotalEnthalpyPhaseChangeCoupling::L>(latent_lattice);
    coupling.setParameter<TotalEnthalpyPhaseChangeCoupling::FORCE_PREFACTOR>({0.,0.});
    coupling.setParameter<TotalEnthalpyPhaseChangeCoupling::T_COLD>(0.); coupling.setParameter<TotalEnthalpyPhaseChangeCoupling::DELTA_T>(1.);
    flow.initialize(); heat.initialize();
    const auto& load=heat.getLoadBalancer();
    if (load.size()!=1 || load.platform(0)!=Platform::CPU_SISD) throw std::runtime_error("one explicit Float64 CPU block required");
    const int global=load.glob(0),nx=geometry.getBlockGeometry(0).getCuboid().getNx(),ny=geometry.getBlockGeometry(0).getCuboid().getNy();
    if (nx!=n+1 || ny!=n/8 || geometry.getStatistics().getNvoxel(1)!=std::size_t(n-1)*ny) throw std::runtime_error("exact complete active grid required");
    const double cell_mass=rho*dx*dx*size[2];
    Json snapshots=Json::array(); double cold_exchange=0.,reflecting_exchange=0.;
    std::ofstream ledger("heat-exchange.csv.partial"); ledger<<std::setprecision(17)<<"step,time_s,cold_exchange_j,reflecting_exchange_j\n";
    for (std::size_t step=0;step<=steps;++step) {
      if (std::binary_search(retained.begin(),retained.end(),step)) {
        coupling.apply(); heat.setProcessingContext(ProcessingContext::Evaluation); flow.setProcessingContext(ProcessingContext::Evaluation);
        const std::string name="freezing-"+std::to_string(step)+".csv"; std::ofstream out(name+".partial");
        out<<std::setprecision(17)<<"i,j,x_m,y_m,material,specific_enthalpy_j_kg,temperature_k,liquid_fraction\n";
        double energy=0.,liquid_mass=0.;
        for (int y=0;y<ny;++y) for (int x=0;x<nx-1;++x) {
          const int m=geometry.get(global,x,y); auto cell=heat.get(global,x,y); auto fluid=flow.get(global,x,y);
          const double h=cell.computeRho()*h_scale,t=cold+span*cell.getField<descriptors::TEMPERATURE>(),f=fluid.getField<descriptors::POROSITY>();
          if (m!=(x==0?3:1) || !std::isfinite(h) || !std::isfinite(t) || !std::isfinite(f)) throw std::runtime_error("finite original phase/temperature/enthalpy coverage required");
          out<<x<<','<<y<<','<<x*dx<<','<<(y+.5)*dx<<','<<m<<','<<h<<','<<t<<','<<f<<'\n';
          if (m==1) {energy+=cell_mass*h;liquid_mass+=cell_mass*f;}
        }
        close_field(out,name);
        snapshots.push_back({{"path",name},{"step",step},{"time_s",step*dt},{"energy_j",energy},{"mass_kg",cell_mass*(n-1)*ny},{"liquid_mass_kg",liquid_mass},{"cold_exchange_j",cold_exchange},{"reflecting_exchange_j",reflecting_exchange}});
      }
      if (step==steps) break;
      heat.setProcessingContext(ProcessingContext::Simulation); heat.collide();
      double cold_delta=0.,reflecting_delta=0.;
      for (int y=0;y<ny;++y) for (int x=1;x<nx-1;++x) {
        auto cell=heat.get(global,x,y);
        for (int p=1;p<HeatDescriptor::q;++p) {
          const int xx=x-descriptors::c<HeatDescriptor>(p,0),yy=(y-descriptors::c<HeatDescriptor>(p,1)+ny)%ny;
          const int m=geometry.get(global,xx,yy);
          if (m!=1) {
            const double exchange=(heat.get(global,xx,yy)[p]-cell[descriptors::opposite<HeatDescriptor>(p)])*h_scale*cell_mass;
            if (m==3) cold_delta+=exchange; else if (m==2) reflecting_delta+=exchange; else throw std::runtime_error("undeclared boundary exchange");
          }
        }
      }
      heat.AndStream(); cold_exchange+=cold_delta; reflecting_exchange+=reflecting_delta;
      ledger<<step+1<<','<<(step+1)*dt<<','<<cold_delta<<','<<reflecting_delta<<'\n';
    }
    close_field(ledger,"heat-exchange.csv");
    Json receipt={{"schema_version",1},{"adapter","OpenLB"},{"backend","cpu"},{"executed",true},{"software_fallback",false},{"precision","float64"},{"source_revision","145cd54810b468f4b6fd3ed86b10644264841578"},{"formulation",spec.at("formulation")},{"dimensionality",2},{"synthetic",true},{"request",spec},{"spacing_m",dx},{"physical_step_s",dt},{"stefan_number",stefan},{"cell_mass_kg",cell_mass},{"active_volume_m3",(n-1)*ny*dx*dx*size[2]},{"active_control_bounds_m",{{.5*dx,size[0]-.5*dx},{0.,size[1]},{0.,size[2]}}},{"shape",{nx,ny}},{"snapshots",snapshots},{"energy_zero","solid at prescribed cold wall temperature"},{"numerical_verification","independent complete-field Stefan/mass/energy gate required"},{"physical_validation","unqualified"},{"scope","synthetic equal-property conduction solidification; active nodal control volume excludes zero-measure boundary nodes; no retained-water transfer, expansion or pressure"}};
    std::ofstream result("freezing-receipt.json.partial"); result<<receipt.dump(2); close_field(result,"freezing-receipt.json");
    return 0;
  } catch (const std::exception& error) {std::cerr<<error.what()<<std::endl;return 1;}
}
