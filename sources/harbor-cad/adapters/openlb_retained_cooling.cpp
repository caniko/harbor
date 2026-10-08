// Thin stationary retained-distribution driver over pinned OpenLB enthalpy APIs.
// Per-cell parameter routing preserves original water amount; upstream owns
// collision, streaming, temperature boundaries and phase/enthalpy evolution.
#include <olb.h>
#include <nlohmann/json.hpp>
#include <algorithm>
#include <cmath>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <set>
#include <sstream>
#include <stdexcept>

using namespace olb;
using namespace olb::names;
using Json = nlohmann::json;
using CoolingCase = Case<
  NavierStokes, Lattice<double,descriptors::D2Q9<descriptors::POROSITY,descriptors::VELOCITY_SOLID,descriptors::FORCE,descriptors::OMEGA>>,
  Temperature, Lattice<double,descriptors::D2Q5<descriptors::VELOCITY,descriptors::TEMPERATURE,TotalEnthalpy::L,descriptors::BOUZIDI_DISTANCE,descriptors::BOUZIDI_ADE_DIRICHLET>>
>;
using HeatDescriptor = CoolingCase::descriptor_t_of<Temperature>;
using FlowDescriptor = CoolingCase::descriptor_t_of<NavierStokes>;
using HeatDynamics = dynamics::ParameterFromCell<TotalEnthalpy::L,TotalEnthalpyAdvectionDiffusionTRTdynamics<double,HeatDescriptor>>;

struct OriginalLatentCoupling : TotalEnthalpyPhaseChangeCoupling {
  template<typename CELLS,typename PARAMETERS>
  void apply(CELLS& cells,PARAMETERS& parameters) any_platform {
    parameters.template set<TotalEnthalpyPhaseChangeCoupling::L>(
      cells.template get<Temperature>().template getField<TotalEnthalpy::L>());
    TotalEnthalpyPhaseChangeCoupling::apply(cells,parameters);
  }
};

static double finite(const Json& value) {
  if (!value.is_number()) throw std::runtime_error("explicit numeric input required");
  double v=value.get<double>();
  if (!std::isfinite(v)) throw std::runtime_error("finite input required");
  return v;
}
static void close_field(std::ofstream& out,const std::string& name) {
  out.flush(); if (!out) throw std::runtime_error("complete scientific output write required");
  out.close(); if (std::filesystem::exists(name)) throw std::runtime_error("immutable closed observation required");
  std::filesystem::rename(name+".partial",name);
}

int main(int argc,char** argv) {
  try {
    if (argc!=3 || std::string(argv[1])!="reference") throw std::runtime_error("usage: harbor-cad-openlb-retained-cooling reference request.json");
    if (!std::filesystem::is_regular_file(std::filesystem::symlink_status(argv[2])) || std::filesystem::file_size(argv[2])>65536) throw std::runtime_error("bounded regular explicit request required");
    Json spec; std::ifstream(argv[2])>>spec;
    const std::set<std::string> required={"schema_version","synthetic","formulation","source_shape","spacing_m","extrusion_m","destination_origin_m","thermal","steps","observation_steps","integration_substeps","spatial_refinement"};
    std::set<std::string> keys; for (auto i=spec.begin();i!=spec.end();++i) keys.insert(i.key());
    if (keys!=required || spec.at("schema_version")!=1 || spec.at("synthetic")!=true || spec.at("formulation")!="stationary_equal_property_retained_phase_conduction") throw std::runtime_error("strict explicitly synthetic stationary retained cooling request required");
    const auto shape=spec.at("source_shape").get<std::vector<unsigned>>();
    const auto origin=spec.at("destination_origin_m").get<std::vector<double>>();
    if (shape.size()!=2 || shape[0]<3 || shape[1]<4 || shape[0]>241 || shape[1]>145 || origin.size()!=3 || std::any_of(origin.begin(),origin.end(),[](double v){return !std::isfinite(v) || std::abs(v)>1e6;})) throw std::runtime_error("bounded original complete grid with distinct cold/insulated boundary controls and translation required");
    const int source_nx=shape[0],source_ny=shape[1];
    if (!spec.at("spatial_refinement").is_number_unsigned()) throw std::runtime_error("integer conservative spatial refinement required");
    const unsigned refinement=spec.at("spatial_refinement");
    if (refinement!=1 && refinement!=2 && refinement!=3 && refinement!=4) throw std::runtime_error("explicit conservative original-parent subcontrols required");
    const int nx=source_nx*refinement,ny=(source_ny-2)*refinement+2;
    const double dx=finite(spec.at("spacing_m")),depth=finite(spec.at("extrusion_m"));
    const auto& material=spec.at("thermal");
    const double rho=finite(material.at("density_kg_m3")),cp=finite(material.at("specific_heat_j_kg_k")),k=finite(material.at("conductivity_w_m_k")),latent=finite(material.at("latent_heat_j_kg")),tm=finite(material.at("melting_temperature_k")),cold=finite(material.at("cold_wall_temperature_k")),initial=finite(material.at("initial_temperature_k"));
    const auto domain=material.at("material_temperature_domain_k").get<std::vector<double>>();
    const double span=tm-cold,h_scale=cp*span,stefan=h_scale/latent;
    if (dx<=0. || dx>1. || depth<=0. || depth>1. || rho<=0. || cp<=0. || k<=0. || latent<=0. || initial!=tm || cold<100. || tm>1000. || cold>=tm || domain.size()!=2 || domain[0]>cold || domain[1]<tm || stefan<.05 || stefan>.2) throw std::runtime_error("explicit equal thermal properties, melting initial state and original Stefan applicability required");
    if (!spec.at("steps").is_number_unsigned() || !spec.at("integration_substeps").is_number_unsigned()) throw std::runtime_error("integer native integration budget required");
    const unsigned substeps=spec.at("integration_substeps");
    const std::size_t steps=spec.at("steps");
    if ((substeps!=1 && substeps!=2 && substeps!=4) || steps<1 || steps>1000000) throw std::runtime_error("bounded explicit same-grid temporal integration required");
    const double spacing=dx/refinement,alpha=1./(6.*substeps),dt=spacing*spacing*rho*cp/(6.*k*substeps),cell_mass=rho*spacing*spacing*depth;
    if (!std::isfinite(dt) || !std::isfinite(cell_mass) || dt<=0. || cell_mass<=0.) throw std::runtime_error("finite explicit SI conversion required");
    for (const auto& v:spec.at("observation_steps")) if (!v.is_number_unsigned()) throw std::runtime_error("integer original observation required");
    const auto retained=spec.at("observation_steps").get<std::vector<std::size_t>>();
    if (retained.size()<2 || retained.size()>16 || retained.front()!=0 || retained.back()!=steps || !std::is_sorted(retained.begin(),retained.end()) || std::adjacent_find(retained.begin(),retained.end())!=retained.end()) throw std::runtime_error("bounded ordered complete original observations required");
    for (const auto& entry:std::filesystem::directory_iterator(".")) {
      const auto name=entry.path().filename();
      if ((name!="process.log" && name!="retained-cooling.log") || !std::filesystem::is_regular_file(entry.symlink_status()) || std::filesystem::hard_link_count(entry.path())!=1) throw std::runtime_error("fresh native scientific output directory required");
    }
    const std::string original="/inputs/wetting-original.csv";
    if (!std::filesystem::is_regular_file(std::filesystem::symlink_status(original)) || std::filesystem::file_size(original)>16*1024*1024) throw std::runtime_error("bounded regular original wetting field required");
    std::ifstream source(original); std::string line; std::getline(source,line);
    if (line!="x_m,y_m,material,phi,u_lattice,v_lattice") throw std::runtime_error("unchanged original wetting field header required");
    std::vector<double> phase(source_nx*source_ny),source_x(source_nx*source_ny),source_y(source_nx*source_ny); std::size_t cursor=0;
    while (std::getline(source,line)) {
      if (cursor>=phase.size()) throw std::runtime_error("no extra original controls permitted");
      std::vector<double> row; std::istringstream stream(line); std::string field;
      while (std::getline(stream,field,',')) {std::size_t used{};double v=std::stod(field,&used);if (used!=field.size() || !std::isfinite(v)) throw std::runtime_error("finite complete unchanged original values required");row.push_back(v);}
      const auto x=cursor%source_nx,y=cursor/source_nx;
      if (row.size()!=6 || std::abs(row[0]-x*dx)>dx*1e-10 || std::abs(row[1]-y*dx)>dx*1e-10 || row[2]!=(y==0 || y==std::size_t(source_ny-1)?2:1) || row[4]!=0. || row[5]!=0.) throw std::runtime_error("complete original geometry/material and exactly zero original velocity required");
      const double f=1.-row[3];
      if (row[2]==1 && (f<0. || f>1.)) throw std::runtime_error("original bounded phase required without clipping");
      phase[cursor]=row[2]==1?f:0.;source_x[cursor]=row[0];source_y[cursor]=row[1];++cursor;
    }
    if (cursor!=phase.size()) throw std::runtime_error("complete original source grid required");
    initialize(&argc,&argv); singleton::directories().setOutputDir("./tmp/");
    CoolingCase::ParametersD params;params.set<parameters::RESOLUTION>(nx);params.set<parameters::DOMAIN_EXTENT>({double(nx-1),double(ny-1)});params.set<parameters::OVERLAP>(2);
    Mesh<double,2> mesh(Vector<double,2>{0.,0.},1.,Vector<int,2>{nx,ny},1);mesh.setOverlap(2);mesh.getCuboidDecomposition().setPeriodicity({true,false});
    CoolingCase experiment(params,mesh);auto& geometry=experiment.getGeometry();
    geometry.rename(0,2);geometry.rename(2,1,{0,1});
    IndicatorCuboid2D<double> bottom({double(nx+2),1.},{-1.,-1.});geometry.rename(2,3,1,bottom);
    geometry.clean();geometry.innerClean();geometry.checkForErrors();
    auto& heat=experiment.getLattice(Temperature{});auto& flow=experiment.getLattice(NavierStokes{});
    flow.setUnitConverter<ThermalUnitConverter<double,FlowDescriptor,HeatDescriptor>>(1.,1./6.,1.,1.,1.,1.,1.,1.,1.,0.,1.);heat.setUnitConverter(flow.getUnitConverter());
    dynamics::set<ForcedPSMBGKdynamics>(flow,geometry.getMaterialIndicator({1,2,3}));
    dynamics::set<HeatDynamics>(heat,geometry.getMaterialIndicator(1));boundary::set<boundary::BounceBack>(heat,geometry,2);boundary::set<boundary::BounceBack>(heat,geometry,3);
    heat.setParameter<descriptors::OMEGA>(1./(3.*alpha+.5));flow.setParameter<descriptors::OMEGA>(1.);
    heat.setParameter<collision::TRT::MAGIC>(.25);
    heat.setParameter<TotalEnthalpy::T_S>(1.);heat.setParameter<TotalEnthalpy::T_L>(1.);heat.setParameter<TotalEnthalpy::CP_S>(1.);heat.setParameter<TotalEnthalpy::CP_L>(1.);heat.setParameter<TotalEnthalpy::LAMBDA_S>(alpha);heat.setParameter<TotalEnthalpy::LAMBDA_L>(alpha);heat.setParameter<TotalEnthalpy::L>(0.);
    AnalyticalConst2D<double,double> zero(0.,0.),one(1.);fields::set<descriptors::OMEGA>(flow,geometry.getMaterialIndicator({1,2,3}),one);fields::set<descriptors::VELOCITY>(heat,geometry.getMaterialIndicator({1,2,3}),zero);flow.iniEquilibrium(geometry.getMaterialIndicator({1,2,3}),one,zero);
    const auto& load=heat.getLoadBalancer();if (load.size()!=1 || load.platform(0)!=Platform::CPU_SISD) throw std::runtime_error("single explicit Float64 CPU block required");
    const int global=load.glob(0);
    if (geometry.getBlockGeometry(0).getCuboid().getNx()!=nx || geometry.getBlockGeometry(0).getCuboid().getNy()!=ny || geometry.getStatistics().getNvoxel(1)!=std::size_t(nx)*(ny-2)) throw std::runtime_error("unchanged original active nodal controls required");
    for (int y=0;y<ny;++y) for (int x=0;x<nx;++x) {
      auto cell=heat.get(global,x,y);const auto index=((y-1)/int(refinement)+1)*source_nx+x/refinement;const double l=(y==0 || y==ny-1)?0.:phase[index]*latent/h_scale;cell.setField<TotalEnthalpy::L>(l);
      for (int p=0;p<HeatDescriptor::q;++p) cell[p]=p==0?l:0.;
    }
    int cold_outgoing=-1;
    for (int p=1;p<HeatDescriptor::q;++p) if (descriptors::c<HeatDescriptor>(p,1)==-1) cold_outgoing=p;
    if (cold_outgoing<0) throw std::runtime_error("native downward thermal link required");
    for (int x=0;x<nx;++x) {auto cell=heat.get(global,x,1);cell.setFieldComponent<descriptors::BOUZIDI_DISTANCE>(cold_outgoing,.5);cell.setFieldComponent<descriptors::BOUZIDI_ADE_DIRICHLET>(cold_outgoing,0.);}
    const int insulated_outgoing=descriptors::opposite<HeatDescriptor>(cold_outgoing);
    for (int x=0;x<nx;++x) {auto cell=heat.get(global,x,ny-2);cell.setFieldComponent<descriptors::BOUZIDI_DISTANCE>(insulated_outgoing,.5);}
    // Invoke the pinned native half-link Dirichlet postprocessor explicitly after
    // streaming to retain its observed replacement energy, without duplicating it.
    BouzidiAdeDirichletPostProcessor cold_boundary;
    BouzidiPostProcessor insulated_boundary;
    auto& coupling=experiment.setCouplingOperator("retained-phase",OriginalLatentCoupling{},NavierStokes{},flow,Temperature{},heat);coupling.restrictTo(geometry.getMaterialIndicator(1));
    coupling.setParameter<OriginalLatentCoupling::T_S>(1.);coupling.setParameter<OriginalLatentCoupling::T_L>(1.);coupling.setParameter<OriginalLatentCoupling::CP_S>(1.);coupling.setParameter<OriginalLatentCoupling::CP_L>(1.);coupling.setParameter<OriginalLatentCoupling::L>(0.);coupling.setParameter<OriginalLatentCoupling::FORCE_PREFACTOR>({0.,0.});coupling.setParameter<OriginalLatentCoupling::T_COLD>(0.);coupling.setParameter<OriginalLatentCoupling::DELTA_T>(1.);
    flow.initialize();heat.initialize();
    double cold_exchange=0.,reflecting_exchange=0.;Json snapshots=Json::array();
    std::ofstream ledger("heat-exchange.csv.partial");ledger<<std::setprecision(17)<<"step,time_s,cold_exchange_j,reflecting_exchange_j\n";
    for (std::size_t step=0;step<=steps;++step) {
      if (std::binary_search(retained.begin(),retained.end(),step)) {
        coupling.apply();heat.setProcessingContext(ProcessingContext::Evaluation);flow.setProcessingContext(ProcessingContext::Evaluation);
        const auto name="cooling-"+std::to_string(step)+".csv";std::ofstream out(name+".partial");out<<std::setprecision(17)<<"i,j,parent_i,parent_j,x_m,y_m,water_fraction,specific_enthalpy_j_kg,temperature_k,liquid_fraction\n";
        double energy=0.,water_mass=0.,liquid_mass=0.;
        for (int y=1;y<ny-1;++y) for (int x=0;x<nx;++x) {
          const int parent_i=x/refinement,parent_j=(y-1)/refinement+1;const auto index=parent_j*source_nx+parent_i;auto cell=heat.get(global,x,y);auto fluid=flow.get(global,x,y);const double h=cell.computeRho()*h_scale,t=cold+span*cell.getField<descriptors::TEMPERATURE>(),f=fluid.getField<descriptors::POROSITY>();
          const double xx=origin[0]+source_x[index]+((x%refinement+.5)/refinement-.5)*dx,yy=origin[1]+source_y[index]+(((y-1)%refinement+.5)/refinement-.5)*dx;
          if (!std::isfinite(h) || !std::isfinite(t) || !std::isfinite(f)) throw std::runtime_error("finite complete original thermal observation required");
          out<<x<<','<<y<<','<<parent_i<<','<<parent_j<<','<<xx<<','<<yy<<','<<phase[index]<<','<<h<<','<<t<<','<<f<<'\n';energy+=cell_mass*h;water_mass+=cell_mass*phase[index];liquid_mass+=cell_mass*phase[index]*f;
        }
        close_field(out,name);snapshots.push_back({{"path",name},{"step",step},{"time_s",step*dt},{"energy_j",energy},{"water_mass_kg",water_mass},{"liquid_water_mass_kg",liquid_mass},{"cold_exchange_j",cold_exchange},{"reflecting_exchange_j",reflecting_exchange}});
      }
      if (step==steps) break;
      heat.setProcessingContext(ProcessingContext::Simulation);heat.collide();double cold_delta=0.,reflecting_delta=0.;
      for (int y=1;y<ny-1;++y) for (int x=0;x<nx;++x) {auto cell=heat.get(global,x,y);for (int p=1;p<HeatDescriptor::q;++p) {const int xx=(x-descriptors::c<HeatDescriptor>(p,0)+nx)%nx,yy=y-descriptors::c<HeatDescriptor>(p,1),m=geometry.get(global,xx,yy);if (m!=1) {const double exchange=(heat.get(global,xx,yy)[p]-cell[descriptors::opposite<HeatDescriptor>(p)])*h_scale*cell_mass;if (m==3) cold_delta+=exchange;else if (m==2) reflecting_delta+=exchange;else throw std::runtime_error("undeclared native boundary exchange");}}}
      heat.AndStream();
      for (int x=0;x<nx;++x) {auto cell=heat.get(global,x,1);const int incoming=descriptors::opposite<HeatDescriptor>(cold_outgoing);const double before=cell[incoming];cold_boundary.apply(cell);cold_delta+=(cell[incoming]-before)*h_scale*cell_mass;}
      // Native half-link reflection returns this step's outgoing population.
      // Full-way ghost reflection otherwise stores undeclared boundary energy.
      for (int x=0;x<nx;++x) {auto cell=heat.get(global,x,ny-2);const double before=cell[cold_outgoing];insulated_boundary.apply(cell);reflecting_delta+=(cell[cold_outgoing]-before)*h_scale*cell_mass;}
      cold_exchange+=cold_delta;reflecting_exchange+=reflecting_delta;ledger<<step+1<<','<<(step+1)*dt<<','<<cold_delta<<','<<reflecting_delta<<'\n';
    }
    close_field(ledger,"heat-exchange.csv");
    Json receipt={{"schema_version",1},{"adapter","OpenLB"},{"backend","cpu"},{"precision","float64"},{"source_revision","145cd54810b468f4b6fd3ed86b10644264841578"},{"collision","native_total_enthalpy_trt"},{"trt_magic",.25},{"executed",true},{"software_fallback",false},{"request",spec},{"source_spacing_m",dx},{"spacing_m",spacing},{"physical_step_s",dt},{"cell_mass_kg",cell_mass},{"source_shape",shape},{"native_shape",{nx,ny}},{"boundary","native_half_link_cold_ymin; native_half_link_insulated_ymax; periodic_x"},{"snapshots",snapshots},{"physical_validation","unqualified"},{"scope","synthetic stationary original-control equal-property conduction; water amount conserved, per-cell latent heat; no flow, physical water-air, pressure or expansion"}};
    std::ofstream out("retained-cooling-receipt.json.partial");out<<receipt.dump(2);close_field(out,"retained-cooling-receipt.json");return 0;
  } catch (const std::exception& e) {std::cerr<<e.what()<<std::endl;return 1;}
}
