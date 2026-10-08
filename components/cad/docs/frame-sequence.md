# Rendered frame and video binding

The renderer publishes a version-1 `frame-sequence.json` after all approved
frames close. Each entry contains the exact consecutive filename, byte size,
SHA-256, requested/observed physical times and native-step mapping. The completed
render receipt binds both the manifest's byte digest and its exact entries.

For worker-managed B1, the completed solver first publishes
`retained-fields/snapshot.json` and verified copies of the relative PVD → VTM →
VTI graph. The snapshot binds science, approved execution, execution binding,
exact file descriptors, explicit units and requested/observed physical times.
Its descriptors are registered together in one SQLite transaction. Presentation
requires that committed snapshot digest, verifies each file before and after
execution, and mounts the retained fields and solver receipt read-only. Frame
sequences and both presentation receipts carry the same snapshot byte digest,
artifact ID, science ID and execution ID.

The baseline accepts the pinned OpenLB driver's single-block Float64 point
arrays (`physVelocity`, `physPressure`, `geometry`) without conversion. DTDs,
namespaces, external/escaping references, mismatched clocks, missing times,
symlinks, raw appended arrays and unsupported topology/partitions reject.
Bounded XML parsing uses pinned [roxmltree 0.21.1 parsing options](https://docs.rs/roxmltree/0.21.1/roxmltree/struct.ParsingOptions.html);
array association and ImageData coordinates follow the
[VTK XML format](https://docs.vtk.org/en/latest/vtk_file_formats/vtkxml_file_format.html).
Capturing unchanged bytes does not independently establish pressure accuracy,
convergence or physical validation.

The encoder checks the sequence before initiating hardware encoding:

- Every approved presentation time has exactly one frame, in approved order.
- Missing trailing frames, extra frames, gaps, changed bytes and symlinks reject.
- Physical times are finite and the approved sequence is strictly increasing.
- Native frames match the completed render receipt and its manifest digest.
- A synthetic encoder fixture is explicitly labelled, cannot impersonate a
  render stage, and receives no scientific time-label qualification.

For the existing plan format, `observation.retained_times_s` is the approved
rendered sequence. A presentation can select an ordered subset of an available
native time collection; the manifest/video must match that approved selection,
not every available source time. Standalone retained-field job input binding is
a separate subsequent lifecycle capability.

Video playback remains 24 fps, independently of physical-time spacing. Actual
decode/FFprobe verifies frame count, dimensions and finite presentation
timestamps. The video receipt exports requested and observed physical times,
the frame-manifest digest and independent playback timestamps. Scientific
arrays remain in their lossless native artifacts.
